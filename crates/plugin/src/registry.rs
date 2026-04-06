//! Tool registry — a central catalog of available tools.
//!
//! Modeled after OpenClaw's tool dispatch: agents request tools by name,
//! the registry resolves and executes them.

use crate::tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;

/// Central registry of all available tool plugins.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolPlugin>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool plugin.
    pub fn register(&mut self, tool: Arc<dyn ToolPlugin>) {
        let name = tool.name().to_string();
        tracing::info!(tool = %name, "registered tool plugin");
        self.tools.insert(name, tool);
    }

    /// List all registered tool names.
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get a tool's schema by name.
    pub fn schema(&self, name: &str) -> Option<serde_json::Value> {
        self.tools.get(name).map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "args": t.args_schema(),
            })
        })
    }

    /// List all tools with their schemas (for LLM tool-use prompts).
    pub fn all_schemas(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "args": t.args_schema(),
            })
        }).collect()
    }

    /// Execute a named tool.
    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::Execution(format!("unknown tool: {name}")))?;
        tool.execute(args, ctx).await
    }

    /// Check if a tool exists.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;

    #[test]
    fn register_and_list() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(builtin::ShellTool));
        reg.register(Arc::new(builtin::FileReadTool));
        reg.register(Arc::new(builtin::FileWriteTool));
        reg.register(Arc::new(builtin::HttpRequestTool));

        let names = reg.list();
        assert!(names.contains(&"shell"));
        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"file_write"));
        assert!(names.contains(&"http_request"));
        assert_eq!(names.len(), 4);
    }

    #[tokio::test]
    async fn execute_shell() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(builtin::ShellTool));

        let ctx = ToolContext {
            workspace_id: "test".into(),
            session_id: "test".into(),
            working_dir: "/tmp".into(),
        };

        let result = reg
            .execute(
                "shell",
                serde_json::json!({ "command": "echo hello" }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output["stdout"]
            .as_str()
            .unwrap()
            .contains("hello"));
    }
}
