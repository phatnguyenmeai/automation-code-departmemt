//! Playwright tool plugin for running E2E test suites.
//!
//! Wraps `npx playwright test` commands, providing structured test
//! execution results back to the agent pipeline.

use crate::tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
use async_trait::async_trait;

/// Run Playwright test suites and return structured results.
pub struct PlaywrightRunnerTool;

#[async_trait]
impl ToolPlugin for PlaywrightRunnerTool {
    fn name(&self) -> &str {
        "playwright_runner"
    }

    fn description(&self) -> &str {
        "Run Playwright E2E tests and return structured results (pass/fail/skip counts)"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "spec": {
                    "type": "string",
                    "description": "Specific test file or directory to run (relative to project root)"
                },
                "project": {
                    "type": "string",
                    "description": "Browser project to use (chromium, firefox, webkit, mobile)",
                    "default": "chromium"
                },
                "grep": {
                    "type": "string",
                    "description": "Filter tests by title pattern (regex)"
                },
                "workers": {
                    "type": "integer",
                    "description": "Number of parallel workers",
                    "default": 4
                },
                "retries": {
                    "type": "integer",
                    "description": "Number of retries on failure",
                    "default": 0
                },
                "headed": {
                    "type": "boolean",
                    "description": "Run in headed mode (visible browser)",
                    "default": false
                }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let project = args["project"].as_str().unwrap_or("chromium");
        let workers = args["workers"].as_u64().unwrap_or(4);
        let retries = args["retries"].as_u64().unwrap_or(0);
        let headed = args["headed"].as_bool().unwrap_or(false);

        let mut cmd_parts = vec![
            "npx".to_string(),
            "playwright".to_string(),
            "test".to_string(),
            format!("--project={project}"),
            format!("--workers={workers}"),
            format!("--retries={retries}"),
            "--reporter=json".to_string(),
        ];

        if headed {
            cmd_parts.push("--headed".to_string());
        }

        if let Some(grep) = args["grep"].as_str() {
            cmd_parts.push(format!("--grep={grep}"));
        }

        if let Some(spec) = args["spec"].as_str() {
            cmd_parts.push(spec.to_string());
        }

        let command = cmd_parts.join(" ");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 min timeout for E2E
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(&command)
                .current_dir(&ctx.working_dir)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let code = output.status.code().unwrap_or(-1);

                // Try to parse JSON reporter output.
                let parsed = serde_json::from_str::<serde_json::Value>(stdout.trim())
                    .ok();

                // Extract summary if JSON was parsed.
                let summary = parsed.as_ref().map(|json| {
                    let suites = json.get("suites")
                        .and_then(|s| s.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let mut total = 0u64;
                    let mut passed = 0u64;
                    let mut failed = 0u64;
                    let mut skipped = 0u64;

                    for suite in &suites {
                        if let Some(specs) = suite.get("specs").and_then(|s| s.as_array()) {
                            for spec in specs {
                                if let Some(tests) = spec.get("tests").and_then(|t| t.as_array()) {
                                    for test in tests {
                                        total += 1;
                                        match test.get("status").and_then(|s| s.as_str()) {
                                            Some("expected") => passed += 1,
                                            Some("unexpected") => failed += 1,
                                            Some("skipped") => skipped += 1,
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }

                    serde_json::json!({
                        "total": total,
                        "passed": passed,
                        "failed": failed,
                        "skipped": skipped,
                        "success": failed == 0,
                    })
                });

                Ok(ToolResult::ok(serde_json::json!({
                    "command": command,
                    "exit_code": code,
                    "success": code == 0,
                    "summary": summary,
                    "raw_output": if parsed.is_some() { serde_json::Value::Null } else { serde_json::Value::String(stdout) },
                    "stderr": stderr,
                })))
            }
            Ok(Err(e)) => Ok(ToolResult::err(format!("playwright exec error: {e}"))),
            Err(_) => Err(ToolError::Timeout),
        }
    }
}
