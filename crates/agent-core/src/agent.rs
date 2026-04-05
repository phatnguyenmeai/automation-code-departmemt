use crate::{artifact::ArtifactStore, message::TaskMessage, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Shared context passed to every agent invocation.
///
/// Holds trace/workspace identity, a dispatch handle for emitting side
/// tasks, and an artifact store for reading inputs + writing outputs.
#[derive(Clone)]
pub struct AgentCtx {
    pub workspace_id: String,
    pub session_id: Uuid,
    /// Absolute path to the session run directory (`runs/<session_id>/`).
    pub run_dir: PathBuf,
    pub dispatch: Arc<dyn Dispatcher>,
    pub artifacts: Arc<dyn ArtifactStore>,
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
