//! Shared application state for the server.

use plugin::{ChannelPlugin, SkillRegistry, TelegramPlugin, ToolRegistry};
use std::collections::HashMap;
use std::sync::Arc;
use storage::Storage;
use tokio::sync::{broadcast, Mutex};

/// Event broadcast to WebSocket subscribers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineEvent {
    pub session_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Shared state available to all request handlers.
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub tool_registry: Arc<ToolRegistry>,
    pub skill_registry: Arc<Mutex<SkillRegistry>>,
    pub channels: Arc<HashMap<String, Arc<dyn ChannelPlugin>>>,
    /// Direct reference to the Telegram plugin (if configured).
    pub telegram: Option<Arc<TelegramPlugin>>,
    /// Broadcast channel for real-time pipeline events.
    pub events_tx: broadcast::Sender<PipelineEvent>,
    /// Whether API key authentication is enforced.
    pub auth_enabled: bool,
}

impl AppState {
    pub fn new(
        storage: Arc<dyn Storage>,
        tool_registry: ToolRegistry,
        skill_registry: SkillRegistry,
        channels: HashMap<String, Arc<dyn ChannelPlugin>>,
    ) -> Self {
        let (events_tx, _) = broadcast::channel(256);
        Self {
            storage,
            tool_registry: Arc::new(tool_registry),
            skill_registry: Arc::new(Mutex::new(skill_registry)),
            channels: Arc::new(channels),
            telegram: None,
            events_tx,
            auth_enabled: false,
        }
    }

    /// Set the Telegram plugin reference.
    pub fn with_telegram(mut self, telegram: Arc<TelegramPlugin>) -> Self {
        self.telegram = Some(telegram);
        self
    }

    /// Enable authentication enforcement.
    pub fn with_auth(mut self) -> Self {
        self.auth_enabled = true;
        self
    }

    /// Broadcast a pipeline event to all WebSocket subscribers.
    pub fn emit(&self, event: PipelineEvent) {
        let _ = self.events_tx.send(event);
    }
}
