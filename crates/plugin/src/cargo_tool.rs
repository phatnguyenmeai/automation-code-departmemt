//! Cargo tool plugin for Rust workspace operations.
//!
//! Provides `cargo check`, `cargo clippy`, `cargo test`, `cargo fmt`,
//! and `cargo build` commands scoped to the agent's working directory.

use crate::tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
use async_trait::async_trait;

/// Execute cargo commands within a Rust workspace.
pub struct CargoTool;

#[async_trait]
impl ToolPlugin for CargoTool {
    fn name(&self) -> &str {
        "cargo_tool"
    }

    fn description(&self) -> &str {
        "Run cargo commands (check, clippy, test, fmt, build) in a Rust workspace"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["subcommand"],
            "properties": {
                "subcommand": {
                    "type": "string",
                    "description": "Cargo subcommand: check, clippy, test, fmt, build, doc",
                    "enum": ["check", "clippy", "test", "fmt", "build", "doc"]
                },
                "package": {
                    "type": "string",
                    "description": "Specific package/crate to target (-p flag). Omit for whole workspace."
                },
                "args": {
                    "type": "string",
                    "description": "Additional arguments to pass to the cargo subcommand"
                },
                "release": {
                    "type": "boolean",
                    "description": "Build in release mode (--release)",
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
        let subcommand = args["subcommand"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'subcommand'".into()))?;

        // Validate allowed subcommands.
        let allowed = ["check", "clippy", "test", "fmt", "build", "doc"];
        if !allowed.contains(&subcommand) {
            return Err(ToolError::InvalidArgs(format!(
                "subcommand must be one of: {}",
                allowed.join(", ")
            )));
        }

        let mut cmd_parts = vec!["cargo".to_string(), subcommand.to_string()];

        // Add package scope.
        if let Some(pkg) = args["package"].as_str() {
            cmd_parts.push("-p".to_string());
            cmd_parts.push(pkg.to_string());
        }

        // Add release flag.
        if args["release"].as_bool().unwrap_or(false) && subcommand != "fmt" {
            cmd_parts.push("--release".to_string());
        }

        // Clippy-specific: treat warnings as errors.
        if subcommand == "clippy" {
            cmd_parts.push("--".to_string());
            cmd_parts.push("-W".to_string());
            cmd_parts.push("clippy::all".to_string());
        }

        // Fmt-specific: check mode (don't modify files).
        if subcommand == "fmt" {
            cmd_parts.push("--".to_string());
            cmd_parts.push("--check".to_string());
        }

        // Additional args.
        if let Some(extra) = args["args"].as_str() {
            for arg in extra.split_whitespace() {
                cmd_parts.push(arg.to_string());
            }
        }

        let command = cmd_parts.join(" ");
        let timeout_secs = if subcommand == "test" || subcommand == "build" {
            120
        } else {
            60
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
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

                Ok(ToolResult::ok(serde_json::json!({
                    "command": command,
                    "subcommand": subcommand,
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": code,
                    "success": code == 0,
                })))
            }
            Ok(Err(e)) => Ok(ToolResult::err(format!("cargo exec error: {e}"))),
            Err(_) => Err(ToolError::Timeout),
        }
    }
}
