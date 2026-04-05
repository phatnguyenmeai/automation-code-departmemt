use agent_core::{Agent, ArtifactStore, Priority, Role, TaskId, TaskKind, TaskMessage};
use agents::{BaAgent, DevAgent, FrontendAgent, PmAgent, TestAgent};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gateway::{Gateway, Workspace};
use llm_claude::{ClaudeClient, ClaudeModel};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agentdept", version, about = "Virtual engineering department orchestrator")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the full PM → BA → Dev+FE → Test pipeline for one requirement.
    Run {
        #[arg(long)]
        requirement: Option<String>,
        #[arg(long, default_value = "config/workspace.toml")]
        config: PathBuf,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long, default_value_t = false)]
        no_playwright: bool,
        /// Root directory for session runs.
        #[arg(long, default_value = "./runs")]
        runs_dir: PathBuf,
        /// Resume an existing session by id (runs/<id>/ must exist).
        #[arg(long)]
        resume: Option<String>,
    },
}

#[derive(Deserialize)]
struct WorkspaceConfig {
    workspace: WorkspaceSection,
    models: ModelsSection,
    #[serde(default)]
    test: TestSection,
    #[serde(default)]
    runtime: RuntimeSection,
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

#[derive(Deserialize, Default)]
struct RuntimeSection {
    #[serde(default)]
    runs_dir: Option<String>,
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,gateway=info,agents=info")
            }),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run {
            requirement,
            config,
            base_url,
            no_playwright,
            runs_dir,
            resume,
        } => run(requirement, config, base_url, no_playwright, runs_dir, resume).await,
    }
}

async fn run(
    requirement: Option<String>,
    config_path: PathBuf,
    base_url_override: Option<String>,
    no_playwright: bool,
    runs_dir_flag: PathBuf,
    resume: Option<String>,
) -> Result<()> {
    let cfg_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let cfg: WorkspaceConfig = toml::from_str(&cfg_text).context("parsing config")?;

    let base_url = base_url_override.unwrap_or_else(|| cfg.test.base_url.clone());
    let enable_pw = !no_playwright && cfg.test.enable_playwright;
    let runs_dir = cfg
        .runtime
        .runs_dir
        .map(PathBuf::from)
        .unwrap_or(runs_dir_flag);

    let claude = ClaudeClient::from_env()
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY env var required"))?;

    // --- Bootstrap workspace (new or resume) ---
    let workspace = if let Some(id) = resume.as_ref() {
        let run_dir = runs_dir.join(id);
        if !run_dir.exists() {
            anyhow::bail!("resume dir not found: {}", run_dir.display());
        }
        Workspace::resume(cfg.workspace.id.clone(), run_dir).await?
    } else {
        tokio::fs::create_dir_all(&runs_dir).await.ok();
        Workspace::new_run(cfg.workspace.id.clone(), &runs_dir).await?
    };

    let session_id = workspace.session.id;
    let run_dir = workspace.run_dir.clone();
    let artifacts_store = workspace.artifacts.clone();
    let resume_entries: Vec<TaskMessage> = workspace
        .resume_state
        .as_ref()
        .map(|s| s.entries.iter().map(|e| e.msg.clone()).collect())
        .unwrap_or_default();
    let resume_in_flight: Vec<TaskMessage> = workspace
        .resume_state
        .as_ref()
        .map(|s| s.in_flight.clone())
        .unwrap_or_default();

    tracing::info!(%session_id, run_dir = %run_dir.display(), resumed = resume.is_some(), "session ready");

    let mut gw = Gateway::new(workspace);

    // --- Build agents ---
    let test_agent = TestAgent::new(
        claude.clone(),
        parse_model(&cfg.models.test_strategy)?,
        parse_model(&cfg.models.test_exec)?,
        base_url,
        enable_pw,
    );
    // Restore Test agent's buffer from transcript if resuming.
    if !resume_entries.is_empty() {
        test_agent.restore_buffer(&resume_entries).await;
    }

    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(PmAgent::new()),
        Box::new(BaAgent::new(claude.clone(), parse_model(&cfg.models.ba)?)),
        Box::new(DevAgent::new(claude.clone(), parse_model(&cfg.models.dev)?)),
        Box::new(FrontendAgent::new(
            claude.clone(),
            parse_model(&cfg.models.frontend)?,
        )),
        Box::new(test_agent),
    ];
    let _ = parse_model(&cfg.models.pm)?;

    let handles = gw.spawn_workers(agents);
    let mut final_rx = gw.take_final_rx();

    // --- Kick off: fresh requirement or replay in-flight ---
    if let Some(id) = resume.as_ref() {
        if resume_in_flight.is_empty() {
            anyhow::bail!("resume dir has no in-flight messages for session {id}");
        }
        tracing::info!(count = resume_in_flight.len(), "re-injecting in-flight messages");
        for m in resume_in_flight {
            let sender = gw.sender(m.to);
            let mut m = m;
            m.priority = Priority::Normal;
            sender.send(m).map_err(|e| anyhow::anyhow!("re-inject: {e}"))?;
        }
    } else {
        let req_text = requirement
            .ok_or_else(|| anyhow::anyhow!("--requirement required for new run"))?;
        let initial_task_id = TaskId::new();
        let req_payload = serde_json::json!({ "text": req_text });
        let req_md = format!("# Requirement\n\n{req_text}\n");
        let req_ref = artifacts_store
            .write(
                Role::PM,
                TaskKind::Requirement.slug(),
                initial_task_id,
                initial_task_id,
                &req_payload,
                &req_md,
            )
            .await?;
        let summary = serde_json::json!({ "text_len": req_text.len() });
        let initial =
            TaskMessage::new(Role::PM, Role::PM, TaskKind::Requirement, req_ref, summary);
        let pm_sender = gw.sender(Role::PM);
        pm_sender
            .send(initial)
            .map_err(|e| anyhow::anyhow!("dispatch initial: {e}"))?;
    }

    // --- Wait for completion ---
    let report = final_rx
        .recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("final channel closed"))?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    eprintln!("\n--- session {session_id} ---");
    eprintln!("run dir: {}", run_dir.display());

    for h in handles {
        h.abort();
    }
    // Flush transcript (drop gateway → drops transcript handle clones)
    drop(gw);
    Ok(())
}
