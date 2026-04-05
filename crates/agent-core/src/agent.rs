use crate::{message::TaskMessage, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Shared context passed to every agent invocation.
///
/// Holds trace/workspace identity and a handle to dispatch follow-up
/// tasks back to the gateway (used by agents that need to emit mid-flight
/// side tasks; most agents just return `AgentOutput::Dispatch`).
#[derive(Clone)]
pub struct AgentCtx {
    pub workspace_id: String,
    pub dispatch: Arc<dyn Dispatcher>,
}

/// Handle to send a message back into the gateway's lane queue.
#[async_trait]
pub trait Dispatcher: Send + Sync {
    async fn dispatch(&self, msg: TaskMessage) -> Result<()>;
}

/// Output of a single agent invocation.
#[derive(Debug)]
pub enum AgentOutput {
    /// Forward zero or more new tasks to other agents.
    Dispatch(Vec<TaskMessage>),
    /// Task complete with a terminal payload (handled by PM).
    Done(serde_json::Value),
    /// Agent cannot proceed; PM must resolve.
    Blocked(String),
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn role(&self) -> crate::role::Role;

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput>;
}
