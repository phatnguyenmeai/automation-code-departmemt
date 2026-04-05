use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};

const SYSTEM: &str = "You are a senior frontend engineer. Given user stories, \
design the minimal set of pages / components and key interactive elements \
(forms, buttons, inputs) with semantic selectors a test engineer can target. \
Respond with ONLY a JSON object: {\"pages\":[{\"path\":\"/login\",\"components\":\
[{\"name\":\"LoginForm\",\"elements\":[{\"role\":\"textbox\",\"name\":\"Email\"},\
{\"role\":\"button\",\"name\":\"Sign in\"}]}]}]}. No prose.";

pub struct FrontendAgent {
    llm: ClaudeClient,
    model: ClaudeModel,
}

impl FrontendAgent {
    pub fn new(llm: ClaudeClient, model: ClaudeModel) -> Self {
        Self { llm, model }
    }
}

#[async_trait]
impl Agent for FrontendAgent {
    fn role(&self) -> Role {
        Role::Frontend
    }

    async fn handle(&mut self, msg: TaskMessage, _ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Story) {
            return Err(AgentError::Other(format!(
                "Frontend: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let prompt = PromptBuilder::new()
            .json_section("Stories", &msg.payload)
            .section("Task", "Produce frontend spec JSON as specified.")
            .build();

        let text = self
            .llm
            .complete(self.model, Some(SYSTEM), &prompt, 2048)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;

        let spec = parse_json(&text).map_err(|e| AgentError::Llm(e.to_string()))?;

        let to_test = msg.reply(Role::Frontend, Role::Test, TaskKind::FrontendSpec, spec);
        Ok(AgentOutput::Dispatch(vec![to_test]))
    }
}
