use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, ContextBudget, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};

const SYSTEM: &str = "You are a senior Business Analyst on a software team. \
Given a raw product requirement, produce a concise set of user stories with \
acceptance criteria in Gherkin-lite form. Respond with ONLY a JSON object of \
shape {\"stories\":[{\"id\":\"S1\",\"title\":\"...\",\"as_a\":\"...\",\"i_want\":\"...\",\
\"so_that\":\"...\",\"acceptance_criteria\":[\"given/when/then\"]}]}. No prose.";

const TASK_INSTRUCTION: &str = "Produce user stories JSON as specified.";

pub struct BaAgent {
    llm: ClaudeClient,
    model: ClaudeModel,
    budget: ContextBudget,
}

impl BaAgent {
    pub fn new(llm: ClaudeClient, model: ClaudeModel) -> Self {
        Self {
            llm,
            model,
            budget: ContextBudget {
                total_context_tokens: 8000,
                system_prompt_reserve: 500,
                current_task_reserve: 3000,
                history_budget: 4500,
            },
        }
    }

    pub fn with_budget(mut self, budget: ContextBudget) -> Self {
        self.budget = budget;
        self
    }
}

#[async_trait]
impl Agent for BaAgent {
    fn role(&self) -> Role {
        Role::BA
    }

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Requirement) {
            return Err(AgentError::Other(format!(
                "BA: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let (system, prompt) = if let Some(assembler) = &ctx.assembler {
            let (sys, prompt, tokens, entries) = assembler
                .assemble(ctx.session_id, self.role(), &msg, SYSTEM, TASK_INSTRUCTION, &self.budget)
                .await;
            tracing::debug!(tokens, entries, "BA: assembled context with memory");
            (sys, prompt)
        } else {
            let prompt = PromptBuilder::new()
                .json_section("Requirement", &msg.payload)
                .section("Task", TASK_INSTRUCTION)
                .build();
            (SYSTEM.to_string(), prompt)
        };

        let text = self
            .llm
            .complete(self.model, Some(&system), &prompt, 2048)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let stories = parse_json(&text).map_err(|e| AgentError::Llm(e.to_string()))?;

        // Fan out to Dev (API design) and Frontend (UI spec) in parallel.
        let to_dev = msg.reply(Role::BA, Role::Dev, TaskKind::Story, stories.clone());
        let to_fe = msg.reply(Role::BA, Role::Frontend, TaskKind::Story, stories);
        Ok(AgentOutput::Dispatch(vec![to_dev, to_fe]))
    }
}
