use crate::parse_json;
use agent_core::{
    Agent, AgentCtx, AgentError, AgentOutput, Result, Role, TaskKind, TaskMessage,
};
use async_trait::async_trait;
use llm_claude::{ClaudeClient, ClaudeModel, PromptBuilder};
use mcp_client::playwright::PlaywrightMcp;
use std::sync::Arc;
use tokio::sync::Mutex;

const STRATEGY_SYSTEM: &str = "You are a senior QA lead. Given user stories, \
an API contract, and a frontend spec, produce a prioritized test plan covering \
integration (API) and UI paths plus key edge cases. Respond with ONLY JSON: \
{\"scenarios\":[{\"id\":\"T1\",\"kind\":\"ui|api\",\"title\":\"...\",\"priority\":\
\"p0|p1|p2\",\"steps\":[{\"action\":\"navigate|type|click|assert_text|http_get|http_post\",\
\"args\":{...}}]}]}. No prose.";

const EXEC_SYSTEM: &str = "You are a QA automation engineer analyzing a test \
execution result. Summarize outcomes into JSON: {\"summary\":\"...\",\"passed\":N,\
\"failed\":N,\"details\":[{\"id\":\"T1\",\"status\":\"pass|fail\",\"note\":\"...\"}]}.";

pub struct TestAgent {
    llm: ClaudeClient,
    strategy_model: ClaudeModel,
    exec_model: ClaudeModel,
    /// Buffered inputs from Dev + Frontend.
    buf: Arc<Mutex<Buffer>>,
    /// Base URL for integration tests.
    base_url: String,
    /// Whether to actually spawn Playwright MCP. Set to false to skip UI tests
    /// (useful when the environment has no browser).
    enable_playwright: bool,
}

#[derive(Default)]
struct Buffer {
    stories_and_api: Option<serde_json::Value>,
    frontend: Option<serde_json::Value>,
    origin: Option<TaskMessage>,
}

impl TestAgent {
    pub fn new(
        llm: ClaudeClient,
        strategy_model: ClaudeModel,
        exec_model: ClaudeModel,
        base_url: impl Into<String>,
        enable_playwright: bool,
    ) -> Self {
        Self {
            llm,
            strategy_model,
            exec_model,
            buf: Arc::new(Mutex::new(Buffer::default())),
            base_url: base_url.into(),
            enable_playwright,
        }
    }
}

#[async_trait]
impl Agent for TestAgent {
    fn role(&self) -> Role {
        Role::Test
    }

    async fn handle(&mut self, msg: TaskMessage, _ctx: &AgentCtx) -> Result<AgentOutput> {
        {
            let mut buf = self.buf.lock().await;
            match msg.kind {
                TaskKind::ImplSpec => buf.stories_and_api = Some(msg.payload.clone()),
                TaskKind::FrontendSpec => buf.frontend = Some(msg.payload.clone()),
                other => {
                    return Err(AgentError::Other(format!(
                        "Test: unexpected kind {:?}",
                        other
                    )));
                }
            }
            if buf.origin.is_none() {
                buf.origin = Some(msg.clone());
            }
            if buf.stories_and_api.is_none() || buf.frontend.is_none() {
                tracing::info!("Test: waiting for remaining input");
                return Ok(AgentOutput::Dispatch(vec![]));
            }
        }

        // Both inputs present -> strategy phase.
        let (impl_spec, fe_spec, origin) = {
            let mut buf = self.buf.lock().await;
            (
                buf.stories_and_api.take().expect("impl"),
                buf.frontend.take().expect("fe"),
                buf.origin.take().expect("origin"),
            )
        };

        let plan_prompt = PromptBuilder::new()
            .json_section("ImplSpec (stories+api)", &impl_spec)
            .json_section("FrontendSpec", &fe_spec)
            .section("Base URL", &self.base_url)
            .section("Task", "Produce test plan JSON.")
            .build();

        let plan_text = self
            .llm
            .complete(self.strategy_model, Some(STRATEGY_SYSTEM), &plan_prompt, 3072)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;
        let plan = parse_json(&plan_text).map_err(|e| AgentError::Llm(e.to_string()))?;

        tracing::info!(scenarios = %plan.get("scenarios").map(|v| v.as_array().map(|a| a.len()).unwrap_or(0)).unwrap_or(0), "test plan generated");

        // Execution phase: run each scenario.
        let exec_results = self.execute_plan(&plan).await;

        // Summarize.
        let summary_prompt = PromptBuilder::new()
            .json_section("Plan", &plan)
            .json_section("RawResults", &exec_results)
            .section("Task", "Summarize into the specified JSON.")
            .build();
        let summary_text = self
            .llm
            .complete(self.exec_model, Some(EXEC_SYSTEM), &summary_prompt, 1024)
            .await
            .map_err(|e| AgentError::Llm(e.to_string()))?;
        let summary = parse_json(&summary_text).map_err(|e| AgentError::Llm(e.to_string()))?;

        let report = serde_json::json!({
            "plan": plan,
            "raw_results": exec_results,
            "summary": summary,
        });

        let to_pm = origin.reply(Role::Test, Role::PM, TaskKind::TestReport, report);
        Ok(AgentOutput::Dispatch(vec![to_pm]))
    }
}

impl TestAgent {
    async fn execute_plan(&self, plan: &serde_json::Value) -> serde_json::Value {
        let scenarios = plan
            .get("scenarios")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Lazily launch Playwright MCP only if we need UI scenarios.
        let mut pw: Option<PlaywrightMcp> = None;
        if self.enable_playwright && scenarios.iter().any(|s| s.get("kind").and_then(|k| k.as_str()) == Some("ui"))
        {
            match PlaywrightMcp::launch().await {
                Ok(p) => pw = Some(p),
                Err(e) => tracing::warn!(?e, "playwright unavailable, skipping UI"),
            }
        }

        let http = reqwest::Client::new();
        let mut results = Vec::new();

        for scenario in scenarios {
            let id = scenario
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let kind = scenario
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("ui");
            let steps = scenario
                .get("steps")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut step_results = Vec::new();
            let mut ok = true;
            for step in steps {
                let action = step
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = step
                    .get("args")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let res = self.execute_step(&action, &args, kind, &pw, &http).await;
                if let Err(e) = &res {
                    ok = false;
                    step_results.push(serde_json::json!({
                        "action": action, "status": "fail", "error": e
                    }));
                    break;
                } else {
                    step_results.push(serde_json::json!({
                        "action": action, "status": "pass"
                    }));
                }
            }
            results.push(serde_json::json!({
                "id": id,
                "kind": kind,
                "status": if ok { "pass" } else { "fail" },
                "steps": step_results,
            }));
        }

        if let Some(p) = pw {
            let _ = p.close().await;
        }
        serde_json::json!({ "results": results })
    }

    async fn execute_step(
        &self,
        action: &str,
        args: &serde_json::Value,
        kind: &str,
        pw: &Option<PlaywrightMcp>,
        http: &reqwest::Client,
    ) -> std::result::Result<serde_json::Value, String> {
        let base = &self.base_url;
        match action {
            "navigate" => {
                let url = resolve_url(base, args.get("url").and_then(|v| v.as_str()).unwrap_or(""));
                let pw = pw.as_ref().ok_or("playwright unavailable")?;
                pw.navigate(&url).await.map_err(|e| e.to_string())
            }
            "type" => {
                let pw = pw.as_ref().ok_or("playwright unavailable")?;
                let element = args.get("element").and_then(|v| v.as_str()).unwrap_or("");
                let r = args.get("ref").and_then(|v| v.as_str()).unwrap_or("");
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                pw.type_text(element, r, text).await.map_err(|e| e.to_string())
            }
            "click" => {
                let pw = pw.as_ref().ok_or("playwright unavailable")?;
                let element = args.get("element").and_then(|v| v.as_str()).unwrap_or("");
                let r = args.get("ref").and_then(|v| v.as_str()).unwrap_or("");
                pw.click(element, r).await.map_err(|e| e.to_string())
            }
            "assert_text" => {
                let pw = pw.as_ref().ok_or("playwright unavailable")?;
                let needle = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let snap = pw.snapshot().await.map_err(|e| e.to_string())?;
                let text = snap.to_string();
                if text.contains(needle) {
                    Ok(serde_json::json!({"found": needle}))
                } else {
                    Err(format!("text not found: {needle}"))
                }
            }
            "http_get" => {
                let url = resolve_url(base, args.get("url").and_then(|v| v.as_str()).unwrap_or(""));
                let resp = http.get(&url).send().await.map_err(|e| e.to_string())?;
                let status = resp.status().as_u16();
                let expected = args.get("expect_status").and_then(|v| v.as_u64()).unwrap_or(200);
                if status as u64 == expected {
                    Ok(serde_json::json!({"status": status}))
                } else {
                    Err(format!("expected {expected}, got {status}"))
                }
            }
            "http_post" => {
                let url = resolve_url(base, args.get("url").and_then(|v| v.as_str()).unwrap_or(""));
                let body = args.get("body").cloned().unwrap_or(serde_json::json!({}));
                let resp = http.post(&url).json(&body).send().await.map_err(|e| e.to_string())?;
                let status = resp.status().as_u16();
                let expected = args.get("expect_status").and_then(|v| v.as_u64()).unwrap_or(200);
                if status as u64 == expected {
                    Ok(serde_json::json!({"status": status}))
                } else {
                    Err(format!("expected {expected}, got {status}"))
                }
            }
            _ => Err(format!("unknown action '{action}' (kind={kind})")),
        }
    }
}

fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with('/') {
        format!("{}{}", base.trim_end_matches('/'), url)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), url)
    }
}
