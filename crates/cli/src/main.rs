use agent_core::{Agent, Role, TaskKind, TaskMessage};
use agents::{BaAgent, DevAgent, FrontendAgent, PmAgent, TestAgent};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gateway::storage::sqlite::SqliteStorage;
use gateway::storage::{SessionStatus, Storage};
use gateway::{Gateway, Workspace};
use llm_claude::{ClaudeClient, ClaudeModel};
use memory::assembler::ContextAssembler;
use memory::sqlite::SqliteMemory;
use plugin::builtin;
use plugin::ChannelPlugin;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "agentdept", version, about = "Virtual engineering department orchestrator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the full PM -> BA -> Dev+FE -> Test pipeline for one requirement.
    Run {
        #[arg(long)]
        requirement: String,
        #[arg(long, default_value = "config/workspace.toml")]
        config: PathBuf,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long, default_value_t = false)]
        no_playwright: bool,
        /// Path to the SQLite database for session persistence.
        /// Omit to run without persistence (ephemeral mode).
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Resume a previously interrupted session.
    Resume {
        /// Session UUID to resume.
        #[arg(long)]
        session_id: String,
        #[arg(long, default_value = "config/workspace.toml")]
        config: PathBuf,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long, default_value_t = false)]
        no_playwright: bool,
        /// Path to the SQLite database.
        #[arg(long, default_value = "data/sessions.db")]
        db: PathBuf,
    },
    /// List past sessions stored in the database.
    Sessions {
        /// Path to the SQLite database.
        #[arg(long, default_value = "data/sessions.db")]
        db: PathBuf,
        /// Maximum number of sessions to display.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Start the always-on gateway server (HTTP + WebSocket + Web UI).
    Serve {
        /// Port to listen on.
        #[arg(long, default_value_t = 18789)]
        port: u16,
        /// Path to the SQLite database.
        #[arg(long, default_value = "data/sessions.db")]
        db: PathBuf,
        /// Workspace config file (for Telegram and other integrations).
        #[arg(long, default_value = "config/workspace.toml")]
        config: PathBuf,
        /// Directory containing skill definitions (SKILL.md files).
        #[arg(long, default_value = "skills")]
        skills_dir: PathBuf,
        /// Admin API key to bootstrap. Enables authentication.
        /// If omitted, the server runs without auth (all requests are admin).
        /// Can also be set via AGENTDEPT_ADMIN_KEY env var.
        #[arg(long, env = "AGENTDEPT_ADMIN_KEY")]
        admin_key: Option<String>,
        /// Telegram bot token (overrides config file).
        /// Can also be set via TELEGRAM_BOT_TOKEN env var.
        #[arg(long, env = "TELEGRAM_BOT_TOKEN")]
        telegram_token: Option<String>,
        /// Default Telegram chat ID for reports.
        /// Can also be set via TELEGRAM_CHAT_ID env var.
        #[arg(long, env = "TELEGRAM_CHAT_ID")]
        telegram_chat_id: Option<i64>,
    },
}

#[derive(Deserialize)]
struct WorkspaceConfig {
    workspace: WorkspaceSection,
    models: ModelsSection,
    #[serde(default)]
    test: TestSection,
    /// Optional Telegram bot integration.
    telegram: Option<plugin::TelegramConfig>,
}

#[derive(Deserialize)]
struct WorkspaceSection {
    id: String,
}

#[derive(Deserialize)]
struct ModelsSection {
    pm: String,
    ba: String,
    dev: String,
    frontend: String,
    test_strategy: String,
    test_exec: String,
}

#[derive(Deserialize, Default)]
struct TestSection {
    #[serde(default = "default_base_url")]
    base_url: String,
    #[serde(default = "default_true")]
    enable_playwright: bool,
}

fn default_base_url() -> String {
    "http://localhost:3000".into()
}
fn default_true() -> bool {
    true
}

fn parse_model(s: &str) -> Result<ClaudeModel> {
    ClaudeModel::from_str(s)
        .with_context(|| format!("unknown model '{s}' (use opus|sonnet|haiku)"))
}

fn open_storage(db_path: &PathBuf) -> Result<Arc<dyn Storage>> {
    let store = SqliteStorage::open(db_path)
        .with_context(|| format!("opening database at {}", db_path.display()))?;
    Ok(Arc::new(store))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,gateway=info,agents=info")))
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run {
            requirement,
            config,
            base_url,
            no_playwright,
            db,
        } => run(requirement, config, base_url, no_playwright, db).await,
        Cmd::Resume {
            session_id,
            config,
            base_url,
            no_playwright,
            db,
        } => resume(session_id, config, base_url, no_playwright, db).await,
        Cmd::Sessions { db, limit } => list_sessions(db, limit).await,
        Cmd::Serve {
            port,
            db,
            config,
            skills_dir,
            admin_key,
            telegram_token,
            telegram_chat_id,
        } => serve(port, db, config, skills_dir, admin_key, telegram_token, telegram_chat_id).await,
    }
}

async fn run(
    requirement: String,
    config_path: PathBuf,
    base_url_override: Option<String>,
    no_playwright: bool,
    db_path: Option<PathBuf>,
) -> Result<()> {
    let cfg_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let cfg: WorkspaceConfig = toml::from_str(&cfg_text).context("parsing config")?;

    let base_url = base_url_override.unwrap_or(cfg.test.base_url);
    let enable_pw = !no_playwright && cfg.test.enable_playwright;

    let claude = ClaudeClient::from_env()
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY env var required"))?;

    // Create workspace: persistent if --db is provided, ephemeral otherwise.
    // When storage is available, also create memory-aware context assembler.
    let mut gw = if let Some(ref db) = db_path {
        let storage = open_storage(db)?;
        let ws = Workspace::with_storage(cfg.workspace.id, storage.clone());
        let gw = Gateway::new(ws);

        // Wire in OpenClaw-style memory management.
        let mem = SqliteMemory::new(
            storage,
            claude.clone(),
            parse_model(&cfg.models.ba)?, // Use BA model for summarization (cost-effective)
        );
        let assembler = ContextAssembler::new(Arc::new(mem));
        gw.with_assembler(Arc::new(assembler))
    } else {
        Gateway::new(Workspace::new(cfg.workspace.id))
    };

    // Persist session creation if storage is available.
    let session = gw.session();
    session
        .persist_create(Some(&requirement))
        .await
        .map_err(|e| anyhow::anyhow!("persist session: {e}"))?;

    if db_path.is_some() {
        tracing::info!(session_id = %session.id, "session persisted — use --resume to continue later");
        eprintln!("Session ID: {}", session.id);
    }

    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(PmAgent::new()),
        Box::new(BaAgent::new(claude.clone(), parse_model(&cfg.models.ba)?)),
        Box::new(DevAgent::new(claude.clone(), parse_model(&cfg.models.dev)?)),
        Box::new(FrontendAgent::new(
            claude.clone(),
            parse_model(&cfg.models.frontend)?,
        )),
        Box::new(TestAgent::new(
            claude.clone(),
            parse_model(&cfg.models.test_strategy)?,
            parse_model(&cfg.models.test_exec)?,
            base_url,
            enable_pw,
        )),
    ];

    // Unused but forces pm model resolution to fail fast if misconfigured.
    let _ = parse_model(&cfg.models.pm)?;

    let handles = gw.spawn_workers(agents);
    let mut final_rx = gw.take_final_rx();

    // Inject initial Requirement to PM.
    let pm_sender = gw.sender(Role::PM);
    let initial = TaskMessage::new(
        Role::PM,
        Role::PM,
        TaskKind::Requirement,
        serde_json::json!({ "text": requirement }),
    );
    pm_sender.send(initial).map_err(|e| anyhow::anyhow!("dispatch initial: {e}"))?;

    // Wait for first final report.
    let report = final_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("final channel closed"))?;

    // Mark session completed.
    session
        .persist_status(SessionStatus::Completed)
        .await
        .map_err(|e| anyhow::anyhow!("persist status: {e}"))?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    // Drop senders to let workers exit; abort if they linger.
    for h in handles {
        h.abort();
    }
    Ok(())
}

async fn resume(
    session_id_str: String,
    config_path: PathBuf,
    base_url_override: Option<String>,
    no_playwright: bool,
    db_path: PathBuf,
) -> Result<()> {
    let session_id: uuid::Uuid = session_id_str
        .parse()
        .with_context(|| format!("invalid session UUID: {session_id_str}"))?;

    let storage = open_storage(&db_path)?;
    let record = storage
        .load_session(session_id)
        .await
        .map_err(|e| anyhow::anyhow!("load session: {e}"))?;

    let messages = storage
        .load_messages(session_id)
        .await
        .map_err(|e| anyhow::anyhow!("load messages: {e}"))?;

    eprintln!("Resuming session {} ({} prior messages)", session_id, messages.len());
    eprintln!("  Workspace: {}", record.workspace_id);
    eprintln!("  Status:    {}", record.status);
    if let Some(ref req) = record.requirement {
        eprintln!("  Requirement: {}", req);
    }

    // Show session transcript
    eprintln!("\n--- Session Transcript ({} messages) ---", messages.len());
    for (i, msg) in messages.iter().enumerate() {
        eprintln!(
            "  [{}] {} -> {} ({:?})",
            i + 1,
            msg.from,
            msg.to,
            msg.kind
        );
    }
    eprintln!("--- End Transcript ---\n");

    // If the session already completed, just print the transcript.
    if record.status == SessionStatus::Completed {
        eprintln!("Session already completed. Printing last message payload:");
        if let Some(last) = messages.last() {
            println!("{}", serde_json::to_string_pretty(&last.payload)?);
        }
        return Ok(());
    }

    // Otherwise, re-run the pipeline with the original requirement.
    let requirement = record
        .requirement
        .ok_or_else(|| anyhow::anyhow!("no requirement stored for session"))?;

    let cfg_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let cfg: WorkspaceConfig = toml::from_str(&cfg_text).context("parsing config")?;

    let base_url = base_url_override.unwrap_or(cfg.test.base_url);
    let enable_pw = !no_playwright && cfg.test.enable_playwright;

    let claude = ClaudeClient::from_env()
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY env var required"))?;

    // Create a fresh workspace with storage for the re-run.
    // Wire in OpenClaw-style memory management for context recall.
    let ws = Workspace::with_storage(record.workspace_id, storage.clone());
    let mem = SqliteMemory::new(
        storage.clone(),
        claude.clone(),
        parse_model(&cfg.models.ba)?,
    );
    let assembler = ContextAssembler::new(Arc::new(mem));
    let mut gw = Gateway::new(ws).with_assembler(Arc::new(assembler));

    let session = gw.session();
    session
        .persist_create(Some(&requirement))
        .await
        .map_err(|e| anyhow::anyhow!("persist session: {e}"))?;
    eprintln!("New session ID: {}", session.id);

    // Mark original session as interrupted.
    storage
        .update_session_status(session_id, SessionStatus::Interrupted)
        .await
        .map_err(|e| anyhow::anyhow!("update old session: {e}"))?;

    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(PmAgent::new()),
        Box::new(BaAgent::new(claude.clone(), parse_model(&cfg.models.ba)?)),
        Box::new(DevAgent::new(claude.clone(), parse_model(&cfg.models.dev)?)),
        Box::new(FrontendAgent::new(
            claude.clone(),
            parse_model(&cfg.models.frontend)?,
        )),
        Box::new(TestAgent::new(
            claude.clone(),
            parse_model(&cfg.models.test_strategy)?,
            parse_model(&cfg.models.test_exec)?,
            base_url,
            enable_pw,
        )),
    ];

    let _ = parse_model(&cfg.models.pm)?;

    let handles = gw.spawn_workers(agents);
    let mut final_rx = gw.take_final_rx();

    let pm_sender = gw.sender(Role::PM);
    let initial = TaskMessage::new(
        Role::PM,
        Role::PM,
        TaskKind::Requirement,
        serde_json::json!({ "text": requirement }),
    );
    pm_sender.send(initial).map_err(|e| anyhow::anyhow!("dispatch initial: {e}"))?;

    let report = final_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("final channel closed"))?;

    session
        .persist_status(SessionStatus::Completed)
        .await
        .map_err(|e| anyhow::anyhow!("persist status: {e}"))?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    for h in handles {
        h.abort();
    }
    Ok(())
}

async fn list_sessions(db_path: PathBuf, limit: usize) -> Result<()> {
    let storage = open_storage(&db_path)?;
    let sessions = storage
        .list_sessions(limit)
        .await
        .map_err(|e| anyhow::anyhow!("list sessions: {e}"))?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!(
        "{:<38} {:<12} {:<24} {}",
        "SESSION ID", "STATUS", "UPDATED", "REQUIREMENT"
    );
    println!("{}", "-".repeat(100));
    for s in &sessions {
        let req = s
            .requirement
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(40)
            .collect::<String>();
        println!(
            "{:<38} {:<12} {:<24} {}",
            s.id,
            s.status,
            s.updated_at.format("%Y-%m-%d %H:%M:%S"),
            req
        );
    }
    println!("\n{} session(s) total.", sessions.len());
    Ok(())
}

async fn serve(
    port: u16,
    db_path: PathBuf,
    config_path: PathBuf,
    skills_dir: PathBuf,
    admin_key: Option<String>,
    telegram_token: Option<String>,
    telegram_chat_id: Option<i64>,
) -> Result<()> {
    let storage = open_storage(&db_path)?;

    // Load workspace config (optional — server can run without it).
    let workspace_cfg: Option<WorkspaceConfig> = if config_path.exists() {
        let cfg_text = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading config {}", config_path.display()))?;
        Some(toml::from_str(&cfg_text).context("parsing config")?)
    } else {
        tracing::info!(path = %config_path.display(), "config not found, using defaults");
        None
    };

    // Initialize tool registry with built-in tools.
    let tool_registry = builtin::default_registry();
    tracing::info!(tools = ?tool_registry.list(), "tool registry initialized");

    // Load skills from directory.
    let mut skill_registry = plugin::SkillRegistry::new();
    if skills_dir.exists() {
        let count = skill_registry
            .load_dir(&skills_dir)
            .map_err(|e| anyhow::anyhow!("load skills: {e}"))?;
        tracing::info!(count, dir = %skills_dir.display(), "skills loaded");
    } else {
        tracing::info!(dir = %skills_dir.display(), "skills directory not found, skipping");
    }

    // Initialize channel plugins.
    let mut channels: HashMap<String, Arc<dyn ChannelPlugin>> = HashMap::new();
    let mut telegram_ref: Option<Arc<plugin::TelegramPlugin>> = None;

    // Set up Telegram if configured.
    let tg_config = build_telegram_config(
        workspace_cfg.as_ref().and_then(|c| c.telegram.clone()),
        telegram_token,
        telegram_chat_id,
    );

    if let Some(tg_cfg) = tg_config {
        let webhook_mode = tg_cfg.webhook_mode;
        let tg = Arc::new(plugin::TelegramPlugin::new(tg_cfg));

        // Verify bot token.
        match tg.get_me().await {
            Ok(bot) => {
                eprintln!(
                    "Telegram bot connected: @{} ({})",
                    bot.username.as_deref().unwrap_or("unknown"),
                    bot.first_name
                );
                tracing::info!(
                    bot_username = ?bot.username,
                    bot_id = bot.id,
                    "telegram bot verified"
                );
            }
            Err(e) => {
                eprintln!("WARNING: Telegram bot verification failed: {e}");
                tracing::warn!(error = %e, "telegram bot verification failed");
            }
        }

        channels.insert("telegram".into(), tg.clone());
        telegram_ref = Some(tg.clone());

        // Start long-polling if not in webhook mode.
        if !webhook_mode {
            let poll_tg = tg.clone();
            // Delete any existing webhook to enable long-polling.
            if let Err(e) = poll_tg.delete_webhook().await {
                tracing::warn!(error = %e, "failed to delete telegram webhook");
            }

            let (poll_handle, mut poll_rx) = plugin::telegram::spawn_polling(poll_tg, 30);

            // Spawn a task to forward polling events to the pipeline.
            let poll_storage = storage.clone();
            tokio::spawn(async move {
                while let Some(event) = poll_rx.recv().await {
                    let is_command = event
                        .metadata
                        .get("is_command")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if is_command {
                        tracing::info!(
                            command = %event.text,
                            sender = %event.sender,
                            "telegram command received"
                        );
                        // Commands are handled by the polling loop itself for now.
                        continue;
                    }

                    // Non-command messages create sessions (requirements).
                    let session_id = uuid::Uuid::new_v4();
                    if let Err(e) = poll_storage
                        .create_session(session_id, "telegram", Some(&event.text))
                        .await
                    {
                        tracing::error!(error = %e, "failed to create session for telegram message");
                        continue;
                    }

                    tracing::info!(
                        session_id = %session_id,
                        sender = %event.sender,
                        text = %event.text,
                        "telegram message -> session created"
                    );
                }
            });

            // The poll_handle will run until dropped.
            std::mem::forget(poll_handle);
        }
    }

    let mut state = server::state::AppState::new(storage.clone(), tool_registry, skill_registry, channels);

    if let Some(tg) = telegram_ref {
        state = state.with_telegram(tg);
    }

    // Bootstrap admin key and enable auth if provided.
    if let Some(ref key) = admin_key {
        server::auth::bootstrap_admin_key(&storage, key)
            .await
            .map_err(|e| anyhow::anyhow!("bootstrap admin key: {e}"))?;
        state = state.with_auth();
        eprintln!("Authentication ENABLED (admin key configured)");
    } else {
        eprintln!("Authentication DISABLED (no --admin-key provided)");
    }

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("AgentDept Gateway starting on http://0.0.0.0:{port}");
    eprintln!("  Dashboard:  http://localhost:{port}/");
    eprintln!("  API:        http://localhost:{port}/api/health");
    eprintln!("  WebSocket:  ws://localhost:{port}/ws");
    if state.telegram.is_some() {
        eprintln!("  Telegram:   http://localhost:{port}/api/telegram/status");
        eprintln!("  Webhook:    http://localhost:{port}/channels/telegram/webhook");
    }
    if state.auth_enabled {
        eprintln!("  Auth:       Bearer token required for /api/* and /ws");
        eprintln!("  Create keys: POST /api/keys (with admin key)");
    }

    server::serve(state, addr)
        .await
        .map_err(|e| anyhow::anyhow!("server: {e}"))
}

/// Build Telegram config by merging config file, env vars, and CLI args.
fn build_telegram_config(
    file_cfg: Option<plugin::TelegramConfig>,
    cli_token: Option<String>,
    cli_chat_id: Option<i64>,
) -> Option<plugin::TelegramConfig> {
    // CLI token takes precedence over file config.
    let token = cli_token.or_else(|| file_cfg.as_ref().map(|c| c.bot_token.clone()));

    let token = token?;

    let base = file_cfg.unwrap_or(plugin::TelegramConfig {
        bot_token: String::new(),
        default_chat_id: None,
        allowed_users: vec![],
        webhook_mode: false,
        webhook_url: None,
        parse_mode: "HTML".into(),
    });

    Some(plugin::TelegramConfig {
        bot_token: token,
        default_chat_id: cli_chat_id.or(base.default_chat_id),
        allowed_users: base.allowed_users,
        webhook_mode: base.webhook_mode,
        webhook_url: base.webhook_url,
        parse_mode: base.parse_mode,
    })
}
