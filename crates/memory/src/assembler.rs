//! Context assembler — builds optimal prompts with memory-aware context.
//!
//! Before each LLM call, the assembler partitions the token budget between
//! system prompt, current task payload, and recalled history/summaries,
//! then builds a complete prompt that fits within the budget.

use crate::tokens::{estimate_json_tokens, estimate_tokens, fits_budget};
use crate::{Memory, MemoryBudget};
use agent_core::{ContextAssembly, ContextBudget, Role, TaskMessage};
use async_trait::async_trait;
use llm_claude::PromptBuilder;
use std::sync::Arc;
use uuid::Uuid;

/// Result of context assembly: a ready-to-send prompt with metadata.
#[derive(Debug)]
pub struct AssembledContext {
    /// The system prompt (unchanged).
    pub system: String,
    /// The assembled user prompt with context.
    pub user_prompt: String,
    /// Estimated total tokens (system + user).
    pub total_tokens: usize,
    /// Number of memory entries included in the prompt.
    pub memory_entries_used: usize,
}

/// Builds optimal prompts by assembling context from memory within token budgets.
pub struct ContextAssembler {
    memory: Arc<dyn Memory>,
}

impl ContextAssembler {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }

    /// Assemble a complete prompt with memory-aware context.
    ///
    /// Priority order:
    /// 1. System prompt (always included, reserved space)
    /// 2. Current task payload (highest priority, reserved space)
    /// 3. Recalled memory entries (fill remaining budget)
    ///
    /// The task-specific instruction (`task_instruction`) is appended at the end
    /// of the user prompt to keep it prominent.
    pub async fn assemble_context(
        &self,
        session_id: Uuid,
        agent_role: Role,
        current_msg: &TaskMessage,
        system_prompt: &str,
        task_instruction: &str,
        budget: &MemoryBudget,
    ) -> AssembledContext {
        let system_tokens = estimate_tokens(system_prompt);
        let _task_tokens = estimate_json_tokens(&current_msg.payload);

        // Recall memory entries within the history budget.
        let entries = self
            .memory
            .recall(session_id, agent_role, budget)
            .await
            .unwrap_or_default();

        // Select entries that fit within the history budget.
        let selected = fits_budget(&entries, budget.history_budget);
        let memory_entries_used = selected.len();

        // Build the user prompt with context sections.
        let mut builder = PromptBuilder::new();

        // Add memory context if available.
        if !selected.is_empty() {
            let mut context_parts = Vec::new();
            for entry in &selected {
                let part = serde_json::to_string_pretty(&entry.content)
                    .unwrap_or_else(|_| "{}".into());
                let source_label = match entry.source {
                    crate::MemorySource::Summary => "[Summary]",
                    crate::MemorySource::ShortTerm => "[Recent]",
                };
                context_parts.push(format!("{} {}", source_label, part));
            }
            builder = builder.section(
                "Prior Context",
                &context_parts.join("\n\n"),
            );
        }

        // Add current task payload (highest priority).
        let kind_label = format!("{:?}", current_msg.kind);
        builder = builder.json_section(&kind_label, &current_msg.payload);

        // Add task instruction last (most visible to the model).
        builder = builder.section("Task", task_instruction);

        let user_prompt = builder.build();
        let user_tokens = estimate_tokens(&user_prompt);

        AssembledContext {
            system: system_prompt.to_string(),
            user_prompt,
            total_tokens: system_tokens + user_tokens,
            memory_entries_used,
        }
    }
}

/// Bridge implementation: adapts ContextAssembler to the ContextAssembly trait
/// defined in agent-core, avoiding circular dependencies.
#[async_trait]
impl ContextAssembly for ContextAssembler {
    async fn assemble(
        &self,
        session_id: Uuid,
        agent_role: Role,
        current_msg: &TaskMessage,
        system_prompt: &str,
        task_instruction: &str,
        budget: &ContextBudget,
    ) -> (String, String, usize, usize) {
        let mem_budget = MemoryBudget {
            total_context_tokens: budget.total_context_tokens,
            system_prompt_reserve: budget.system_prompt_reserve,
            current_task_reserve: budget.current_task_reserve,
            history_budget: budget.history_budget,
        };
        let result = self
            .assemble_context(session_id, agent_role, current_msg, system_prompt, task_instruction, &mem_budget)
            .await;
        (
            result.system,
            result.user_prompt,
            result.total_tokens,
            result.memory_entries_used,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryEntry;
    use async_trait::async_trait;

    /// A mock memory that always returns empty.
    struct EmptyMemory;

    #[async_trait]
    impl Memory for EmptyMemory {
        async fn recall(
            &self,
            _session_id: Uuid,
            _agent_role: Role,
            _budget: &MemoryBudget,
        ) -> crate::Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        async fn remember(
            &self,
            _session_id: Uuid,
            _entry: &TaskMessage,
        ) -> crate::Result<()> {
            Ok(())
        }
        async fn compact(
            &self,
            _session_id: Uuid,
            _compact_threshold: usize,
        ) -> crate::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn assemble_without_memory() {
        let memory = Arc::new(EmptyMemory);
        let assembler = ContextAssembler::new(memory);
        let msg = TaskMessage::new(
            Role::PM,
            Role::BA,
            agent_core::TaskKind::Requirement,
            serde_json::json!({"text": "build login page"}),
        );
        let budget = MemoryBudget::default();
        let result = assembler
            .assemble_context(
                Uuid::new_v4(),
                Role::BA,
                &msg,
                "You are a BA.",
                "Produce user stories.",
                &budget,
            )
            .await;

        assert!(result.user_prompt.contains("build login page"));
        assert!(result.user_prompt.contains("Produce user stories"));
        assert!(!result.user_prompt.contains("Prior Context"));
        assert_eq!(result.memory_entries_used, 0);
        assert_eq!(result.system, "You are a BA.");
    }
}
