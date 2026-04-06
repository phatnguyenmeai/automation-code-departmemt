//! Shared application state for the server.

use plugin::{ChannelPlugin, SkillRegistry, ToolRegistry};
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
    /// Broadcast channel for real-time pipeline events.
    pub events_tx: broadcast::Sender<PipelineEvent>,
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
            events_tx,
        }
    }

    /// Broadcast a pipeline event to all WebSocket subscribers.
    pub fn emit(&self, event: PipelineEvent) {
        let _ = self.events_tx.send(event);
    }
}
