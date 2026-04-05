use agent_core::{Agent, Role, TaskKind, TaskMessage};
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
        requirement: String,
        #[arg(long, default_value = "config/workspace.toml")]
        config: PathBuf,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long, default_value_t = false)]
        no_playwright: bool,
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
        } => run(requirement, config, base_url, no_playwright).await,
    }
}

async fn run(
    requirement: String,
    config_path: PathBuf,
    base_url_override: Option<String>,
    no_playwright: bool,
) -> Result<()> {
    let cfg_text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let cfg: WorkspaceConfig = toml::from_str(&cfg_text).context("parsing config")?;

    let base_url = base_url_override.unwrap_or(cfg.test.base_url);
    let enable_pw = !no_playwright && cfg.test.enable_playwright;

    let claude = ClaudeClient::from_env()
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY env var required"))?;

    let mut gw = Gateway::new(Workspace::new(cfg.workspace.id));

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

    println!("{}", serde_json::to_string_pretty(&report)?);

    // Drop senders to let workers exit; abort if they linger.
    for h in handles {
        h.abort();
    }
    Ok(())
}
