use crate::{other, parse_json, read_ref, skip_if_exists, write_pair};
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

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        if !matches!(msg.kind, TaskKind::Requirement) {
            return Err(AgentError::Other(format!(
                "BA: unexpected task kind {:?}",
                msg.kind
            )));
        }

        // Resume short-circuit: if story already produced for this input, reuse.
        let art = if let Some(existing) =
            skip_if_exists(ctx, Role::BA, TaskKind::Story.slug(), msg.id).await?
        {
            tracing::info!("BA: reusing existing story artifact");
            existing
        } else {
            let requirement = read_ref(ctx, &msg.artifact).await?;
            let prompt = PromptBuilder::new()
                .json_section("Requirement", &requirement)
                .section("Task", "Produce user stories JSON as specified.")
                .build();

            let text = self
                .llm
                .complete(self.model, Some(SYSTEM), &prompt, 2048)
                .await
                .map_err(|e| AgentError::Llm(e.to_string()))?;
            let stories = parse_json(&text).map_err(other)?;
            let md = render_stories_md(&stories);
            write_pair(ctx, Role::BA, TaskKind::Story.slug(), msg.id, &stories, &md).await?
        };

        let summary = story_summary(&read_ref(ctx, &art).await.unwrap_or_default());
        let to_dev = msg.reply(
            Role::BA,
            Role::Dev,
            TaskKind::Story,
            art.clone(),
            summary.clone(),
        );
        let to_fe = msg.reply(Role::BA, Role::Frontend, TaskKind::Story, art, summary);
        Ok(AgentOutput::Dispatch(vec![to_dev, to_fe]))
    }
}

fn render_stories_md(data: &serde_json::Value) -> String {
    let mut out = String::from("# User Stories\n\n");
    let empty = vec![];
    let stories = data
        .get("stories")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    for (i, s) in stories.iter().enumerate() {
        let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let title = s.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled");
        out.push_str(&format!("## {}. {id}: {title}\n\n", i + 1));
        if let Some(a) = s.get("as_a").and_then(|v| v.as_str()) {
            out.push_str(&format!("- **As a**: {a}\n"));
        }
        if let Some(w) = s.get("i_want").and_then(|v| v.as_str()) {
            out.push_str(&format!("- **I want**: {w}\n"));
        }
        if let Some(t) = s.get("so_that").and_then(|v| v.as_str()) {
            out.push_str(&format!("- **So that**: {t}\n"));
        }
        if let Some(ac) = s.get("acceptance_criteria").and_then(|v| v.as_array()) {
            out.push_str("\n**Acceptance criteria:**\n");
            for c in ac {
                if let Some(s) = c.as_str() {
                    out.push_str(&format!("- {s}\n"));
                }
            }
        }
        out.push('\n');
    }
    out
}

fn story_summary(data: &serde_json::Value) -> serde_json::Value {
    let titles: Vec<_> = data
        .get("stories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("title").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({ "count": titles.len(), "titles": titles })
}
