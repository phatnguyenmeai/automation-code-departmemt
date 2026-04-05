use crate::{other, parse_json, read_ref, skip_if_exists, write_pair};
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

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Story) {
            return Err(AgentError::Other(format!(
                "Dev: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let art = if let Some(existing) =
            skip_if_exists(ctx, Role::Dev, TaskKind::ImplSpec.slug(), msg.id).await?
        {
            tracing::info!("Dev: reusing existing impl-spec artifact");
            existing
        } else {
            let stories = read_ref(ctx, &msg.artifact).await?;
            let prompt = PromptBuilder::new()
                .json_section("Stories", &stories)
                .section("Task", "Produce API contract JSON as specified.")
                .build();
            let text = self
                .llm
                .complete(self.model, Some(SYSTEM), &prompt, 2048)
                .await
                .map_err(|e| AgentError::Llm(e.to_string()))?;
            let api = parse_json(&text).map_err(other)?;
            let combined = serde_json::json!({
                "stories": stories,
                "api": api,
            });
            let md = render_impl_md(&combined);
            write_pair(
                ctx,
                Role::Dev,
                TaskKind::ImplSpec.slug(),
                msg.id,
                &combined,
                &md,
            )
            .await?
        };

        let content = read_ref(ctx, &art).await.unwrap_or_default();
        let summary = impl_summary(&content);
        let to_test = msg.reply(Role::Dev, Role::Test, TaskKind::ImplSpec, art, summary);
        Ok(AgentOutput::Dispatch(vec![to_test]))
    }
}

fn render_impl_md(data: &serde_json::Value) -> String {
    let mut out = String::from("# Impl Spec\n\n## API\n\n");
    out.push_str("| Method | Path | Status codes |\n|---|---|---|\n");
    let empty = vec![];
    let api = data
        .get("api")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    for ep in api {
        let m = ep.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let p = ep.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let codes: Vec<String> = ep
            .get("status_codes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.to_string()).collect())
            .unwrap_or_default();
        out.push_str(&format!("| {m} | {p} | {} |\n", codes.join(", ")));
    }
    if let Some(notes) = data.get("notes").and_then(|v| v.as_str()) {
        out.push_str(&format!("\n## Notes\n\n{notes}\n"));
    }
    out
}

fn impl_summary(data: &serde_json::Value) -> serde_json::Value {
    let endpoints: Vec<String> = data
        .get("api")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|ep| {
                    let m = ep.get("method").and_then(|v| v.as_str())?;
                    let p = ep.get("path").and_then(|v| v.as_str())?;
                    Some(format!("{m} {p}"))
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({ "endpoints": endpoints })
}
