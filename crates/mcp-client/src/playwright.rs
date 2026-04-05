//! Thin convenience wrapper over `McpClient` for Playwright MCP.

use crate::{McpClient, McpError};

pub struct PlaywrightMcp {
    client: McpClient,
}

impl PlaywrightMcp {
    /// Launch `npx @playwright/mcp@latest` as a subprocess.
    pub async fn launch() -> Result<Self, McpError> {
        let client = McpClient::spawn("npx", &["-y", "@playwright/mcp@latest"]).await?;
        Ok(Self { client })
    }

    pub fn inner(&self) -> &McpClient {
        &self.client
    }

    pub async fn navigate(&self, url: &str) -> Result<serde_json::Value, McpError> {
        self.client
            .call_tool("browser_navigate", serde_json::json!({ "url": url }))
            .await
    }

    pub async fn snapshot(&self) -> Result<serde_json::Value, McpError> {
        self.client
            .call_tool("browser_snapshot", serde_json::json!({}))
            .await
    }

    pub async fn click(&self, element: &str, r#ref: &str) -> Result<serde_json::Value, McpError> {
        self.client
            .call_tool(
                "browser_click",
                serde_json::json!({ "element": element, "ref": r#ref }),
            )
            .await
    }

    pub async fn type_text(
        &self,
        element: &str,
        r#ref: &str,
        text: &str,
    ) -> Result<serde_json::Value, McpError> {
        self.client
            .call_tool(
                "browser_type",
                serde_json::json!({
                    "element": element,
                    "ref": r#ref,
                    "text": text,
                    "submit": false
                }),
            )
            .await
    }

    pub async fn close(&self) -> Result<serde_json::Value, McpError> {
        self.client
            .call_tool("browser_close", serde_json::json!({}))
            .await
    }
}
