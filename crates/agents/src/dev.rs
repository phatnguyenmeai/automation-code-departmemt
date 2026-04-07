use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, ContextBudget, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};

const SYSTEM: &str = "You are a senior backend engineer. Given user stories, \
design the minimal HTTP API contract needed to satisfy them. Respond with ONLY \
a JSON object of shape {\"api\":[{\"method\":\"POST\",\"path\":\"/login\",\
\"request\":{...},\"response\":{...},\"status_codes\":[200,401]}],\"notes\":\"...\"}. \
No prose outside the JSON.";

const TASK_INSTRUCTION: &str = "Produce API contract JSON as specified.";

pub struct DevAgent {
    llm: ClaudeClient,
    model: ClaudeModel,
    budget: ContextBudget,
}

impl DevAgent {
    pub fn new(llm: ClaudeClient, model: ClaudeModel) -> Self {
        Self {
            llm,
            model,
            budget: ContextBudget {
                total_context_tokens: 8000,
                system_prompt_reserve: 500,
                current_task_reserve: 4000,
                history_budget: 3500,
            },
        }
    }

    pub fn with_budget(mut self, budget: ContextBudget) -> Self {
        self.budget = budget;
        self
    }
}

#[async_trait]
impl Agent for DevAgent {
    fn role(&self) -> Role {
        Role::Dev
    }

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Story) {
            return Err(AgentError::Other(format!(
                "Dev: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let (system, prompt) = if let Some(assembler) = &ctx.assembler {
            let (sys, prompt, tokens, entries) = assembler
                .assemble(ctx.session_id, self.role(), &msg, SYSTEM, TASK_INSTRUCTION, &self.budget)
                .await;
            tracing::debug!(tokens, entries, "Dev: assembled context with memory");
            (sys, prompt)
        } else {
            let prompt = PromptBuilder::new()
                .json_section("Stories", &msg.payload)
                .section("Task", TASK_INSTRUCTION)
                .build();
            (SYSTEM.to_string(), prompt)
        };

        let text = self
            .llm
            .complete(self.model, Some(&system), &prompt, 2048)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let api = parse_json(&text).map_err(|e| AgentError::Llm(e.to_string()))?;

        // Forward combined (stories + api) to Test for planning.
        let combined = serde_json::json!({
            "stories": msg.payload,
            "api": api,
        });
        let to_test = msg.reply(Role::Dev, Role::Test, TaskKind::ImplSpec, combined);
        Ok(AgentOutput::Dispatch(vec![to_test]))
    }
}
