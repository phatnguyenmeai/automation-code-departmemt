use agent_core::{Agent, Role, TaskKind, TaskMessage};
use agents::{BaAgent, DevAgent, FrontendAgent, PmAgent, TestAgent};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gateway::storage::sqlite::SqliteStorage;
use gateway::storage::{SessionStatus, Storage};
use gateway::{Gateway, Workspace};
use llm_claude::{ClaudeClient, ClaudeModel};
use serde::Deserialize;
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
}

#[derive(Deserialize)]
struct WorkspaceConfig {
    workspace: WorkspaceSection,
    models: ModelsSection,
    #[serde(default)]
    test: TestSection,
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
    let mut gw = if let Some(ref db) = db_path {
        let storage = open_storage(db)?;
        let ws = Workspace::with_storage(cfg.workspace.id, storage);
        Gateway::new(ws)
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
    let ws = Workspace::with_storage(record.workspace_id, storage.clone());
    let mut gw = Gateway::new(ws);

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
