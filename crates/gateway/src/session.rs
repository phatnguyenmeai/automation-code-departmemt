use agent_core::TaskMessage;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// In-memory transcript of one orchestration run.
#[derive(Clone)]
pub struct Session {
    pub id: Uuid,
    pub workspace_id: String,
    history: Arc<Mutex<Vec<TaskMessage>>>,
}

impl Session {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record(&self, msg: &TaskMessage) {
        if let Ok(mut h) = self.history.lock() {
            h.push(msg.clone());
        }
    }

    pub fn snapshot(&self) -> Vec<TaskMessage> {
        self.history.lock().map(|h| h.clone()).unwrap_or_default()
    }
}
