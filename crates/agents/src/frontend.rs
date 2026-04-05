use crate::{other, parse_json, read_ref, skip_if_exists, write_pair};
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

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Story) {
            return Err(AgentError::Other(format!(
                "Frontend: unexpected task kind {:?}",
                msg.kind
            )));
        }

        let art = if let Some(existing) =
            skip_if_exists(ctx, Role::Frontend, TaskKind::FrontendSpec.slug(), msg.id).await?
        {
            tracing::info!("Frontend: reusing existing frontend-spec artifact");
            existing
        } else {
            let stories = read_ref(ctx, &msg.artifact).await?;
            let prompt = PromptBuilder::new()
                .json_section("Stories", &stories)
                .section("Task", "Produce frontend spec JSON as specified.")
                .build();
            let text = self
                .llm
                .complete(self.model, Some(SYSTEM), &prompt, 2048)
                .await
                .map_err(|e| AgentError::Llm(e.to_string()))?;
            let spec = parse_json(&text).map_err(other)?;
            let md = render_fe_md(&spec);
            write_pair(
                ctx,
                Role::Frontend,
                TaskKind::FrontendSpec.slug(),
                msg.id,
                &spec,
                &md,
            )
            .await?
        };

        let content = read_ref(ctx, &art).await.unwrap_or_default();
        let summary = fe_summary(&content);
        let to_test = msg.reply(Role::Frontend, Role::Test, TaskKind::FrontendSpec, art, summary);
        Ok(AgentOutput::Dispatch(vec![to_test]))
    }
}

fn render_fe_md(data: &serde_json::Value) -> String {
    let mut out = String::from("# Frontend Spec\n\n");
    let empty = vec![];
    let pages = data.get("pages").and_then(|v| v.as_array()).unwrap_or(&empty);
    for page in pages {
        let path = page.get("path").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!("## Page `{path}`\n\n"));
        let components = page
            .get("components")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        for c in components {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("Unnamed");
            out.push_str(&format!("- **{name}**\n"));
            if let Some(els) = c.get("elements").and_then(|v| v.as_array()) {
                for el in els {
                    let r = el.get("role").and_then(|v| v.as_str()).unwrap_or("?");
                    let n = el.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    out.push_str(&format!("  - {r}: {n}\n"));
                }
            }
        }
        out.push('\n');
    }
    out
}

fn fe_summary(data: &serde_json::Value) -> serde_json::Value {
    let pages: Vec<String> = data
        .get("pages")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("path").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({ "pages": pages })
}
