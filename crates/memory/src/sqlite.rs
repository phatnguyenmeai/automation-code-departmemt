//! SQLite-backed memory implementation, inspired by OpenClaw's default
//! SQLite persistence with memory management capabilities.

use crate::tokens::{estimate_json_tokens, estimate_tokens};
use crate::{Memory, MemoryBudget, MemoryEntry, MemoryError, MemorySource, Result};
use agent_core::{Role, TaskMessage};
use async_trait::async_trait;
use chrono::Utc;
use llm_claude::{ClaudeClient, ClaudeModel};
use std::sync::Arc;
use storage::Storage;
use uuid::Uuid;

const SUMMARIZE_SYSTEM: &str = "You are a concise summarizer. Given a sequence of \
task messages from a software engineering pipeline, produce a brief summary that \
captures the key decisions, outputs, and context. Focus on information that would \
be useful for downstream agents. Respond with plain text, no JSON wrapping.";

/// SQLite memory backend that wraps the existing Storage trait.
///
/// Provides recall/remember/compact operations on top of the session
/// message storage, adding sliding window and summarization support.
pub struct SqliteMemory {
    storage: Arc<dyn Storage>,
    llm: ClaudeClient,
    summary_model: ClaudeModel,
}

impl SqliteMemory {
    pub fn new(
        storage: Arc<dyn Storage>,
        llm: ClaudeClient,
        summary_model: ClaudeModel,
    ) -> Self {
        Self {
            storage,
            llm,
            summary_model,
        }
    }

    /// Format a TaskMessage into a human-readable string for summarization.
    fn format_message(msg: &TaskMessage) -> String {
        let payload_str =
            serde_json::to_string_pretty(&msg.payload).unwrap_or_else(|_| "{}".into());
        format!(
            "[{} → {}] ({:?}): {}",
            msg.from, msg.to, msg.kind, payload_str
        )
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    async fn recall(
        &self,
        session_id: Uuid,
        _agent_role: Role,
        budget: &MemoryBudget,
    ) -> Result<Vec<MemoryEntry>> {
        let mut entries = Vec::new();
        let mut remaining_budget = budget.history_budget;

        // 1. Load summaries first (compressed older context).
        let summaries = self.storage.load_summaries(session_id).await?;
        for summary in summaries {
            let tokens = summary.token_count;
            if tokens <= remaining_budget {
                entries.push(MemoryEntry {
                    source: MemorySource::Summary,
                    content: serde_json::json!(summary.content),
                    token_estimate: tokens,
                    relevance: 0.5, // Summaries have moderate relevance
                    created_at: summary.created_at,
                });
                remaining_budget -= tokens;
            }
        }

        // 2. Load active (non-compacted) messages — most recent context.
        let active_messages = self.storage.load_active_messages(session_id).await?;

        // Apply sliding window: take most recent messages that fit budget.
        // Iterate from newest to oldest to prioritize recent context.
        let mut recent_entries: Vec<MemoryEntry> = Vec::new();
        for msg in active_messages.iter().rev() {
            let tokens = estimate_json_tokens(&msg.payload) + 20; // overhead for role/kind
            if tokens <= remaining_budget {
                recent_entries.push(MemoryEntry {
                    source: MemorySource::ShortTerm,
                    content: serde_json::json!({
                        "from": msg.from,
                        "to": msg.to,
                        "kind": msg.kind,
                        "payload": msg.payload,
                    }),
                    token_estimate: tokens,
                    relevance: 1.0, // Recent messages have highest relevance
                    created_at: Utc::now(),
                });
                remaining_budget -= tokens;
            } else {
                break; // Budget exhausted
            }
        }

        // Reverse so entries are in chronological order.
        recent_entries.reverse();
        entries.extend(recent_entries);

        Ok(entries)
    }

    async fn remember(
        &self,
        session_id: Uuid,
        entry: &TaskMessage,
    ) -> Result<()> {
        self.storage
            .record_message(session_id, entry)
            .await
            .map_err(MemoryError::Storage)
    }

    async fn compact(
        &self,
        session_id: Uuid,
        compact_threshold: usize,
    ) -> Result<()> {
        // Load all active messages.
        let messages = self.storage.load_active_messages(session_id).await?;

        // Calculate total tokens.
        let total_tokens: usize = messages
            .iter()
            .map(|m| estimate_json_tokens(&m.payload) + 20)
            .sum();

        if total_tokens <= compact_threshold {
            tracing::debug!(
                session_id = %session_id,
                total_tokens,
                threshold = compact_threshold,
                "no compaction needed"
            );
            return Ok(());
        }

        // Take the oldest half of messages for compaction.
        let compact_count = messages.len() / 2;
        if compact_count == 0 {
            return Ok(());
        }

        let to_compact = &messages[..compact_count];

        // Build summarization prompt from messages to compact.
        let mut context = String::new();
        let mut ids_to_mark: Vec<String> = Vec::new();
        for msg in to_compact {
            context.push_str(&Self::format_message(msg));
            context.push_str("\n\n");
            ids_to_mark.push(msg.id.0.to_string());
        }

        let prompt = format!(
            "Summarize the following pipeline messages in 2-3 paragraphs:\n\n{}",
            context
        );

        // Call LLM for summarization.
        let summary_text = self
            .llm
            .complete(self.summary_model, Some(SUMMARIZE_SYSTEM), &prompt, 512)
            .await
            .map_err(|e| MemoryError::Llm(e.to_string()))?;

        let summary_tokens = estimate_tokens(&summary_text);

        // Store summary.
        self.storage
            .store_summary(session_id, &summary_text, summary_tokens)
            .await?;

        // Mark original messages as compacted.
        self.storage
            .mark_compacted(session_id, &ids_to_mark)
            .await?;

        tracing::info!(
            session_id = %session_id,
            compacted = compact_count,
            summary_tokens,
            "compacted session memory"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_message_basic() {
        let msg = TaskMessage::new(
            Role::PM,
            Role::BA,
            agent_core::TaskKind::Requirement,
            serde_json::json!({"text": "build login"}),
        );
        let formatted = SqliteMemory::format_message(&msg);
        assert!(formatted.contains("pm"));
        assert!(formatted.contains("ba"));
        assert!(formatted.contains("build login"));
    }
}
