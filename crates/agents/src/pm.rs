use crate::{read_ref, render_fallback_md, write_pair};
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;

/// Project Manager agent.
///
/// - Receives the initial `Requirement` artifact (written by the CLI) and
///   forwards it to BA.
/// - Receives `TestReport`, writes `FinalReport` artifact, emits Done.
/// - Receives `Blocker`, writes terminal `FinalReport` with failure.
pub struct PmAgent;

impl PmAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PmAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for PmAgent {
    fn role(&self) -> Role {
        Role::PM
    }

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        match msg.kind {
            TaskKind::Requirement => {
                // Just forward to BA: the requirement artifact already exists
                // on disk (written by CLI). Reuse the same artifact ref.
                let fwd = msg.reply(
                    Role::PM,
                    Role::BA,
                    TaskKind::Requirement,
                    msg.artifact.clone(),
                    msg.summary.clone(),
                );
                Ok(AgentOutput::Dispatch(vec![fwd]))
            }
            TaskKind::TestReport => {
                // Read test-report content, produce final-report artifact.
                let report = read_ref(ctx, &msg.artifact).await?;
                let final_data = serde_json::json!({
                    "status": "done",
                    "test_report_summary": msg.summary,
                    "test_report": report,
                });
                let md = render_final_md(&final_data);
                let _art = write_pair(
                    ctx,
                    Role::PM,
                    TaskKind::FinalReport.slug(),
                    msg.id,
                    &final_data,
                    &md,
                )
                .await?;
                Ok(AgentOutput::Done(final_data))
            }
            TaskKind::Blocker => {
                let blocker = read_ref(ctx, &msg.artifact).await.unwrap_or_default();
                let final_data = serde_json::json!({
                    "status": "blocked",
                    "blocker": blocker,
                });
                let md = render_fallback_md("Final Report (blocked)", &final_data);
                let _art = write_pair(
                    ctx,
                    Role::PM,
                    TaskKind::FinalReport.slug(),
                    msg.id,
                    &final_data,
                    &md,
                )
                .await?;
                Ok(AgentOutput::Done(final_data))
            }
            other => Err(AgentError::Other(format!(
                "PM: unexpected task kind {:?}",
                other
            ))),
        }
    }
}

fn render_final_md(data: &serde_json::Value) -> String {
    let mut out = String::new();
    out.push_str("# Final Report\n\n");
    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    out.push_str(&format!("**Status**: {status}\n\n"));
    if let Some(summary) = data.get("test_report_summary") {
        out.push_str("## Test Summary\n\n");
        out.push_str("```json\n");
        out.push_str(&serde_json::to_string_pretty(summary).unwrap_or_default());
        out.push_str("\n```\n");
    }
    out
}
