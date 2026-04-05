//! Minimal MCP (Model Context Protocol) stdio client.
//!
//! Spawns an MCP server as a subprocess (e.g. `npx @playwright/mcp@latest`)
//! and exchanges JSON-RPC 2.0 messages over stdio. Only the handshake +
//! `tools/call` are implemented - enough for the Test agent to drive
//! Playwright.

pub mod playwright;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("transport closed")]
    Closed,
    #[error("tool '{0}' not available")]
    ToolNotFound(String),
}

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: i64,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcNotification<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<i64>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcErrorBody>,
    method: Option<String>,
}

#[derive(Deserialize)]
struct JsonRpcErrorBody {
    code: i64,
    message: String,
}

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<serde_json::Value, McpError>>>>>;

/// Stdio-transport MCP client.
pub struct McpClient {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    next_id: AtomicI64,
    _child: Child,
}

impl McpClient {
    /// Spawn an MCP server subprocess with the given program + args.
    pub async fn spawn(program: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

        // Reader task: parse line-delimited JSON-RPC responses.
        let pending_r = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let parsed: Result<JsonRpcResponse, _> = serde_json::from_str(&line);
                match parsed {
                    Ok(resp) => {
                        if let Some(id) = resp.id {
                            let mut p = pending_r.lock().await;
                            if let Some(tx) = p.remove(&id) {
                                if let Some(err) = resp.error {
                                    let _ = tx.send(Err(McpError::Rpc {
                                        code: err.code,
                                        message: err.message,
                                    }));
                                } else {
                                    let _ = tx.send(Ok(resp
                                        .result
                                        .unwrap_or(serde_json::Value::Null)));
                                }
                            }
                        } else if let Some(method) = resp.method {
                            tracing::debug!(method, "mcp notification");
                        }
                    }
                    Err(e) => tracing::warn!(?e, line, "mcp parse error"),
                }
            }
            tracing::debug!("mcp reader exit");
        });

        // Stderr forwarder.
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!(target: "mcp_stderr", "{}", line);
            }
        });

        let client = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicI64::new(1),
            _child: child,
        };

        client.initialize().await?;
        Ok(client)
    }

    async fn initialize(&self) -> Result<(), McpError> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agentdept", "version": "0.1.0" }
        });
        let _ = self.request("initialize", params).await?;
        self.notify(
            "notifications/initialized",
            serde_json::Value::Object(serde_json::Map::new()),
        )
        .await?;
        Ok(())
    }

    async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }
        rx.await.map_err(|_| McpError::Closed)?
    }

    async fn notify(&self, method: &str, params: serde_json::Value) -> Result<(), McpError> {
        let req = JsonRpcNotification {
            jsonrpc: "2.0",
            method,
            params,
        };
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn list_tools(&self) -> Result<serde_json::Value, McpError> {
        self.request("tools/list", serde_json::json!({})).await
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        self.request(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": arguments }),
        )
        .await
    }
}
