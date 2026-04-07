use crate::{message::TaskMessage, Result};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// Shared context passed to every agent invocation.
///
/// Holds trace/workspace identity, a handle to dispatch follow-up tasks
/// back to the gateway, and optional memory-aware context assembly.
#[derive(Clone)]
pub struct AgentCtx {
    pub workspace_id: String,
    pub dispatch: Arc<dyn Dispatcher>,
    /// Session ID for memory recall (OpenClaw-style memory management).
    pub session_id: Uuid,
    /// Optional context assembler for memory-aware prompt building.
    /// When None, agents fall back to their original single-turn behavior.
    pub assembler: Option<Arc<dyn ContextAssembly>>,
}

/// Trait for memory-aware context assembly.
///
/// Defined here in agent-core to avoid circular dependencies between
/// the memory and agents crates. The actual implementation lives in
/// the `memory` crate's `ContextAssembler`.
#[async_trait]
pub trait ContextAssembly: Send + Sync {
    /// Assemble a prompt with memory context for the given agent role.
    ///
    /// Returns (system_prompt, user_prompt, total_tokens, entries_used).
    async fn assemble(
        &self,
        session_id: Uuid,
        agent_role: crate::role::Role,
        current_msg: &TaskMessage,
        system_prompt: &str,
        task_instruction: &str,
        budget: &ContextBudget,
    ) -> (String, String, usize, usize);
}

/// Budget configuration passed from agents to the assembler.
/// Mirrors `MemoryBudget` but lives in agent-core to avoid dependency.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    pub total_context_tokens: usize,
    pub system_prompt_reserve: usize,
    pub current_task_reserve: usize,
    pub history_budget: usize,
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
