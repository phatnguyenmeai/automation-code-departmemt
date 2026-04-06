//! Core tool plugin trait, modeled after OpenClaw's tool plugin interface.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("timeout")]
    Timeout,
}

/// Result of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: serde_json::Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: serde_json::Value::Null,
            error: Some(message.into()),
        }
    }
}

/// Runtime context passed to tool executions.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_id: String,
    pub session_id: String,
    /// Working directory for file/shell operations.
    pub working_dir: String,
}

/// Trait that all tool plugins must implement.
///
/// Inspired by OpenClaw's tool plugin system — each tool declares its name,
/// a JSON schema for arguments, and an async execute method.
#[async_trait]
pub trait ToolPlugin: Send + Sync {
    /// Unique name of this tool (e.g. "shell", "file_read").
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema describing the expected arguments.
    fn args_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError>;
}
