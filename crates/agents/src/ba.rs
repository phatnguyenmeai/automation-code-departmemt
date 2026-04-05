use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};

const SYSTEM: &str = "You are a senior Business Analyst on a software team. \
Given a raw product requirement, produce a concise set of user stories with \
acceptance criteria in Gherkin-lite form. Respond with ONLY a JSON object of \
shape {\"stories\":[{\"id\":\"S1\",\"title\":\"...\",\"as_a\":\"...\",\"i_want\":\"...\",\
\"so_that\":\"...\",\"acceptance_criteria\":[\"given/when/then\"]}]}. No prose.";

pub struct BaAgent {
    llm: ClaudeClient,
    model: ClaudeModel,
}

impl BaAgent {
    pub fn new(llm: ClaudeClient, model: ClaudeModel) -> Self {
        Self { llm, model }
    }
}

#[async_trait]
impl Agent for BaAgent {
    fn role(&self) -> Role {
        Role::BA
    }

    async fn handle(&mut self, msg: TaskMessage, _ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Requirement) {
            return Err(AgentError::Other(format!(
                "BA: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let prompt = PromptBuilder::new()
            .json_section("Requirement", &msg.payload)
            .section("Task", "Produce user stories JSON as specified.")
            .build();

        let text = self
            .llm
            .complete(self.model, Some(SYSTEM), &prompt, 2048)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let stories = parse_json(&text).map_err(|e| AgentError::Llm(e.to_string()))?;

        // Fan out to Dev (API design) and Frontend (UI spec) in parallel.
        let to_dev = msg.reply(Role::BA, Role::Dev, TaskKind::Story, stories.clone());
        let to_fe = msg.reply(Role::BA, Role::Frontend, TaskKind::Story, stories);
        Ok(AgentOutput::Dispatch(vec![to_dev, to_fe]))
    }
}
