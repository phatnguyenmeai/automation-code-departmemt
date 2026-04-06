//! Built-in tool plugins that ship with the platform.
//!
//! Inspired by OpenClaw's default capabilities: bash, file read/write,
//! and HTTP requests. These cover the most common agent needs without
//! requiring external plugins.

use crate::tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
use async_trait::async_trait;

// ───────────────────────── Shell Tool ─────────────────────────

/// Execute a shell command (bash -c) and return stdout/stderr.
pub struct ShellTool;

#[async_trait]
impl ToolPlugin for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return stdout, stderr, and exit code"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 30)", "default": 30 }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'command'".into()))?;

        let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(30);

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
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
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": code,
                })))
            }
            Ok(Err(e)) => Ok(ToolResult::err(format!("exec error: {e}"))),
            Err(_) => Err(ToolError::Timeout),
        }
    }
}

// ───────────────────────── File Read Tool ─────────────────────────

/// Read a file's contents.
pub struct FileReadTool;

#[async_trait]
impl ToolPlugin for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "File path to read" }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;

        let full_path = if std::path::Path::new(path).is_absolute() {
            std::path::PathBuf::from(path)
        } else {
            std::path::PathBuf::from(&ctx.working_dir).join(path)
        };

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => Ok(ToolResult::ok(serde_json::json!({
                "path": full_path.display().to_string(),
                "content": content,
                "size": content.len(),
            }))),
            Err(e) => Ok(ToolResult::err(format!(
                "read {}: {e}",
                full_path.display()
            ))),
        }
    }
}

// ───────────────────────── File Write Tool ─────────────────────────

/// Write content to a file (creating or overwriting).
pub struct FileWriteTool;

#[async_trait]
impl ToolPlugin for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file, creating directories as needed"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": { "type": "string", "description": "File path to write" },
                "content": { "type": "string", "description": "Content to write" }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'content'".into()))?;

        let full_path = if std::path::Path::new(path).is_absolute() {
            std::path::PathBuf::from(path)
        } else {
            std::path::PathBuf::from(&ctx.working_dir).join(path)
        };

        // Ensure parent directory exists.
        if let Some(parent) = full_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolResult::err(format!("mkdir: {e}")));
            }
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(()) => Ok(ToolResult::ok(serde_json::json!({
                "path": full_path.display().to_string(),
                "bytes_written": content.len(),
            }))),
            Err(e) => Ok(ToolResult::err(format!(
                "write {}: {e}",
                full_path.display()
            ))),
        }
    }
}

// ───────────────────────── HTTP Request Tool ─────────────────────────

/// Make an HTTP request and return the response.
pub struct HttpRequestTool;

#[async_trait]
impl ToolPlugin for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request (GET, POST, PUT, DELETE) and return the response"
    }

    fn args_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string", "description": "Request URL" },
                "method": { "type": "string", "description": "HTTP method (default GET)", "default": "GET" },
                "headers": { "type": "object", "description": "Request headers" },
                "body": { "type": "string", "description": "Request body (for POST/PUT)" }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'url'".into()))?;
        let method = args["method"].as_str().unwrap_or("GET").to_uppercase();

        let client = reqwest::Client::new();
        let mut builder = match method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            other => {
                return Err(ToolError::InvalidArgs(format!(
                    "unsupported method: {other}"
                )))
            }
        };

        // Add headers.
        if let Some(headers) = args["headers"].as_object() {
            for (k, v) in headers {
                if let Some(v) = v.as_str() {
                    builder = builder.header(k.as_str(), v);
                }
            }
        }

        // Add body.
        if let Some(body) = args["body"].as_str() {
            builder = builder.body(body.to_string());
        }

        match builder.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers: serde_json::Map<String, serde_json::Value> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                        )
                    })
                    .collect();
                let body = resp.text().await.unwrap_or_default();

                Ok(ToolResult::ok(serde_json::json!({
                    "status": status,
                    "headers": headers,
                    "body": body,
                })))
            }
            Err(e) => Ok(ToolResult::err(format!("request failed: {e}"))),
        }
    }
}

/// Create a ToolRegistry pre-populated with all built-in tools.
pub fn default_registry() -> crate::registry::ToolRegistry {
    let mut reg = crate::registry::ToolRegistry::new();
    reg.register(std::sync::Arc::new(ShellTool));
    reg.register(std::sync::Arc::new(FileReadTool));
    reg.register(std::sync::Arc::new(FileWriteTool));
    reg.register(std::sync::Arc::new(HttpRequestTool));
    reg
}
