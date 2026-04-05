use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};

const SYSTEM: &str = "You are a senior backend engineer. Given user stories, \
design the minimal HTTP API contract needed to satisfy them. Respond with ONLY \
a JSON object of shape {\"api\":[{\"method\":\"POST\",\"path\":\"/login\",\
\"request\":{...},\"response\":{...},\"status_codes\":[200,401]}],\"notes\":\"...\"}. \
No prose outside the JSON.";

pub struct DevAgent {
    llm: ClaudeClient,
    model: ClaudeModel,
}

impl DevAgent {
    pub fn new(llm: ClaudeClient, model: ClaudeModel) -> Self {
        Self { llm, model }
    }
}

#[async_trait]
impl Agent for DevAgent {
    fn role(&self) -> Role {
        Role::Dev
    }

    async fn handle(&mut self, msg: TaskMessage, _ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Story) {
            return Err(AgentError::Other(format!(
                "Dev: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let prompt = PromptBuilder::new()
            .json_section("Stories", &msg.payload)
            .section("Task", "Produce API contract JSON as specified.")
            .build();

        let text = self
            .llm
            .complete(self.model, Some(SYSTEM), &prompt, 2048)
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
