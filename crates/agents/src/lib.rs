//! Concrete agent implementations: PM, BA, Dev, Frontend, Test.
//!
//! Each agent is a thin adapter that:
//!   1. looks up the upstream artifact via `ctx.artifacts.read(&msg.artifact)`
//!   2. checks if its own output artifact already exists (resume path)
//!   3. otherwise calls Claude, writes a `(json, md)` artifact pair
//!   4. emits follow-up `TaskMessage`s referencing its new artifact

pub mod ba;
pub mod dev;
pub mod frontend;
pub mod pm;
pub mod test;

pub use ba::BaAgent;
pub use dev::DevAgent;
pub use frontend::FrontendAgent;
pub use pm::PmAgent;
pub use test::TestAgent;

use agent_core::{AgentCtx, AgentError, ArtifactRef, Result, Role, TaskId};

/// Try to extract a JSON block from an LLM response. Accepts:
///   - raw JSON
///   - ```json ... ``` fenced blocks
pub(crate) fn parse_json(text: &str) -> anyhow::Result<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let body_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after[body_start..];
        if let Some(end) = body.find("```") {
            let json_str = body[..end].trim();
            return Ok(serde_json::from_str(json_str)?);
        }
    }
    anyhow::bail!("no JSON found in response: {}", trimmed)
}

/// Convenience: write a `(json, md)` artifact pair.
pub(crate) async fn write_pair(
    ctx: &AgentCtx,
    role: Role,
    kind: &str,
    parent_id: TaskId,
    data: &serde_json::Value,
    markdown: &str,
) -> Result<ArtifactRef> {
    let produced_by = TaskId::new();
    ctx.artifacts
        .write(role, kind, parent_id, produced_by, data, markdown)
        .await
}

/// Read the JSON content referenced by an `ArtifactRef`.
pub(crate) async fn read_ref(ctx: &AgentCtx, r: &ArtifactRef) -> Result<serde_json::Value> {
    ctx.artifacts.read(r).await
}

/// If role already produced an artifact of `kind` for input `parent_id`,
/// return it. Agents use this to short-circuit on resume.
pub(crate) async fn skip_if_exists(
    ctx: &AgentCtx,
    role: Role,
    kind: &str,
    parent_id: TaskId,
) -> Result<Option<ArtifactRef>> {
    ctx.artifacts.exists(role, kind, parent_id).await
}

/// Fallback markdown renderer: pretty-print JSON inside a fenced block.
pub(crate) fn render_fallback_md(title: &str, data: &serde_json::Value) -> String {
    let body = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".into());
    format!("# {title}\n\n```json\n{body}\n```\n")
}

/// Map a non-io error into `AgentError`.
pub(crate) fn other(e: impl std::fmt::Display) -> AgentError {
    AgentError::Other(e.to_string())
}
