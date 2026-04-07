//! Memory management layer inspired by OpenClaw's Memory Plugin architecture.
//!
//! Provides pluggable memory backends with context assembly, token budgeting,
//! and automatic summarization of older conversation history.

pub mod assembler;
pub mod sqlite;
pub mod tokens;

use agent_core::{Role, TaskMessage};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("storage error: {0}")]
    Storage(#[from] storage::StorageError),
    #[error("llm error: {0}")]
    Llm(String),
    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

/// Where a memory entry originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// Recent message from the current session (sliding window).
    ShortTerm,
    /// Compressed summary of older messages.
    Summary,
}

/// A scored memory entry ready for context assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub source: MemorySource,
    pub content: serde_json::Value,
    pub token_estimate: usize,
    pub relevance: f32,
    pub created_at: DateTime<Utc>,
}

/// Per-agent memory budget configuration.
///
/// Controls how the context window is partitioned between system prompt,
/// current task payload, and recalled history/summaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBudget {
    /// Total tokens available for the full prompt (system + user).
    pub total_context_tokens: usize,
    /// Tokens reserved for the system prompt.
    pub system_prompt_reserve: usize,
    /// Tokens reserved for the current task message (highest priority).
    pub current_task_reserve: usize,
    /// Remaining budget for history and summaries.
    pub history_budget: usize,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            total_context_tokens: 8000,
            system_prompt_reserve: 500,
            current_task_reserve: 3000,
            history_budget: 4500,
        }
    }
}

/// Core memory trait — modeled after OpenClaw's Memory Plugin interface.
///
/// Implementations provide recall (context retrieval), remember (storage),
/// and compact (summarization) operations.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Retrieve relevant context entries within a token budget.
    ///
    /// Returns entries ordered by relevance (most relevant first),
    /// fitting within the given budget's history allocation.
    async fn recall(
        &self,
        session_id: Uuid,
        agent_role: Role,
        budget: &MemoryBudget,
    ) -> Result<Vec<MemoryEntry>>;

    /// Store a new task message into memory.
    async fn remember(
        &self,
        session_id: Uuid,
        entry: &TaskMessage,
    ) -> Result<()>;

    /// Summarize and compress older entries when context exceeds threshold.
    ///
    /// Called periodically to keep memory within manageable bounds.
    async fn compact(
        &self,
        session_id: Uuid,
        compact_threshold: usize,
    ) -> Result<()>;
}
