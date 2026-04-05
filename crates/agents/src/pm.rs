use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use std::collections::HashMap;

/// Project Manager agent.
///
/// - Receives the raw `Requirement` from the CLI and forwards it to BA.
/// - Receives `TestReport` and produces `FinalReport` (Done).
/// - Receives `Blocker` and logs / retries (v1: just emits FinalReport with failure).
pub struct PmAgent {
    /// Track in-flight test reports keyed by the originating story id.
    collected: HashMap<String, serde_json::Value>,
}

impl PmAgent {
    pub fn new() -> Self {
        Self {
            collected: HashMap::new(),
        }
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

    async fn handle(&mut self, msg: TaskMessage, _ctx: &AgentCtx) -> Result<AgentOutput> {
        match msg.kind {
            TaskKind::Requirement => {
                // Kick off pipeline: forward to BA.
                let fwd = msg.reply(Role::PM, Role::BA, TaskKind::Requirement, msg.payload.clone());
                Ok(AgentOutput::Dispatch(vec![fwd]))
            }
            TaskKind::TestReport => {
                self.collected
                    .insert(msg.id.to_string(), msg.payload.clone());
                let report = serde_json::json!({
                    "status": "done",
                    "test_report": msg.payload,
                });
                Ok(AgentOutput::Done(report))
            }
            TaskKind::Blocker => {
                let report = serde_json::json!({
                    "status": "blocked",
                    "reason": msg.payload,
                });
                Ok(AgentOutput::Done(report))
            }
            other => Err(AgentError::Other(format!(
                "PM: unexpected task kind {:?}",
                other
            ))),
        }
    }
}
