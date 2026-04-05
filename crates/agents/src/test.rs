use crate::{other, parse_json, read_ref, skip_if_exists, write_pair};
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
    buf: Arc<Mutex<Buffer>>,
    base_url: String,
    enable_playwright: bool,
}

#[derive(Default)]
struct Buffer {
    impl_msg: Option<TaskMessage>,
    fe_msg: Option<TaskMessage>,
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

    async fn handle(&mut self, msg: TaskMessage, ctx: &AgentCtx) -> Result<AgentOutput> {
        {
            let mut buf = self.buf.lock().await;
            match msg.kind {
                TaskKind::ImplSpec => buf.impl_msg = Some(msg.clone()),
                TaskKind::FrontendSpec => buf.fe_msg = Some(msg.clone()),
                other => {
                    return Err(AgentError::Other(format!(
                        "Test: unexpected kind {:?}",
                        other
                    )));
                }
            }
            if buf.impl_msg.is_none() || buf.fe_msg.is_none() {
                tracing::info!("Test: waiting for remaining input");
                return Ok(AgentOutput::Dispatch(vec![]));
            }
        }

        let (impl_msg, fe_msg) = {
            let mut buf = self.buf.lock().await;
            (buf.impl_msg.take().unwrap(), buf.fe_msg.take().unwrap())
        };

        // Use impl_msg.id as the canonical "input id" for Test's output
        // artifacts (both impl + fe contribute, but resume key must be stable).
        let input_id = impl_msg.id;

        // --- Strategy phase: reuse plan if already produced ---
        let plan_ref = if let Some(existing) =
            skip_if_exists(ctx, Role::Test, TaskKind::TestPlan.slug(), input_id).await?
        {
            tracing::info!("Test: reusing existing test-plan");
            existing
        } else {
            let impl_spec = read_ref(ctx, &impl_msg.artifact).await?;
            let fe_spec = read_ref(ctx, &fe_msg.artifact).await?;
            let prompt = PromptBuilder::new()
                .json_section("ImplSpec (stories+api)", &impl_spec)
                .json_section("FrontendSpec", &fe_spec)
                .section("Base URL", &self.base_url)
                .section("Task", "Produce test plan JSON.")
                .build();
            let text = self
                .llm
                .complete(self.strategy_model, Some(STRATEGY_SYSTEM), &prompt, 3072)
                .await
                .map_err(|e| AgentError::Llm(e.to_string()))?;
            let plan = parse_json(&text).map_err(other)?;
            let md = render_plan_md(&plan);
            write_pair(ctx, Role::Test, TaskKind::TestPlan.slug(), input_id, &plan, &md).await?
        };

        // --- Execution + report: reuse if already produced ---
        let report_ref = if let Some(existing) =
            skip_if_exists(ctx, Role::Test, TaskKind::TestReport.slug(), input_id).await?
        {
            tracing::info!("Test: reusing existing test-report");
            existing
        } else {
            let plan = read_ref(ctx, &plan_ref).await?;
            let exec_results = self.execute_plan(&plan).await;
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
            let summary = parse_json(&summary_text).map_err(other)?;
            let report = serde_json::json!({
                "plan_ref": plan_ref.json_path,
                "raw_results": exec_results,
                "summary": summary,
            });
            let md = render_report_md(&report);
            write_pair(
                ctx,
                Role::Test,
                TaskKind::TestReport.slug(),
                input_id,
                &report,
                &md,
            )
            .await?
        };

        let content = read_ref(ctx, &report_ref).await.unwrap_or_default();
        let summary = content.get("summary").cloned().unwrap_or_default();
        let to_pm = impl_msg.reply(Role::Test, Role::PM, TaskKind::TestReport, report_ref, summary);
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

        let mut pw: Option<PlaywrightMcp> = None;
        if self.enable_playwright
            && scenarios.iter().any(|s| {
                s.get("kind").and_then(|k| k.as_str()) == Some("ui")
            })
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
            let kind = scenario.get("kind").and_then(|v| v.as_str()).unwrap_or("ui");
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

    /// On resume, restore the buffer by scanning the most recent ImplSpec /
    /// FrontendSpec messages delivered to Test from the transcript.
    pub async fn restore_buffer(&self, msgs: &[TaskMessage]) {
        let mut buf = self.buf.lock().await;
        for m in msgs {
            if m.to != Role::Test {
                continue;
            }
            match m.kind {
                TaskKind::ImplSpec => buf.impl_msg = Some(m.clone()),
                TaskKind::FrontendSpec => buf.fe_msg = Some(m.clone()),
                _ => {}
            }
        }
    }

    /// Expose buffer completeness for resume decisions.
    pub async fn buffer_ready(&self) -> bool {
        let buf = self.buf.lock().await;
        buf.impl_msg.is_some() && buf.fe_msg.is_some()
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

// --- Markdown renderers ---

fn render_plan_md(plan: &serde_json::Value) -> String {
    let mut out = String::from("# Test Plan\n\n");
    let empty = vec![];
    let scenarios = plan
        .get("scenarios")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    for s in scenarios {
        let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        let prio = s.get("priority").and_then(|v| v.as_str()).unwrap_or("?");
        let title = s.get("title").and_then(|v| v.as_str()).unwrap_or("");
        out.push_str(&format!("- [ ] **{id}** ({prio}, {kind}): {title}\n"));
    }
    out
}

fn render_report_md(report: &serde_json::Value) -> String {
    let mut out = String::from("# Test Report\n\n");
    if let Some(summary) = report.get("summary") {
        if let Some(line) = summary.get("summary").and_then(|v| v.as_str()) {
            out.push_str(&format!("{line}\n\n"));
        }
        let passed = summary.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = summary.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
        out.push_str(&format!("**Passed**: {passed}  **Failed**: {failed}\n\n"));
        if let Some(details) = summary.get("details").and_then(|v| v.as_array()) {
            out.push_str("| id | status | note |\n|---|---|---|\n");
            for d in details {
                let id = d.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let st = d.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                let note = d.get("note").and_then(|v| v.as_str()).unwrap_or("");
                out.push_str(&format!("| {id} | {st} | {note} |\n"));
            }
        }
    }
    out
}

