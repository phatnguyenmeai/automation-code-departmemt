use agent_core::TaskMessage;
use std::sync::{Arc, Mutex};
use storage::{SessionStatus, Storage};
use uuid::Uuid;

/// In-memory transcript of one orchestration run, with optional persistent
/// storage inspired by OpenClaw's session architecture.
///
/// When a `Storage` backend is provided, every recorded message is also
/// persisted to disk so sessions survive restarts and can be inspected later.
#[derive(Clone)]
pub struct Session {
    pub id: Uuid,
    pub workspace_id: String,
    history: Arc<Mutex<Vec<TaskMessage>>>,
    storage: Option<Arc<dyn Storage>>,
}

impl Session {
    /// Create a new ephemeral session (no persistence).
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            history: Arc::new(Mutex::new(Vec::new())),
            storage: None,
        }
    }

    /// Create a new session backed by persistent storage.
    pub fn with_storage(
        workspace_id: impl Into<String>,
        storage: Arc<dyn Storage>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            history: Arc::new(Mutex::new(Vec::new())),
            storage: Some(storage),
        }
    }

    /// Restore a session from storage, loading its message history.
    pub async fn resume(
        id: Uuid,
        storage: Arc<dyn Storage>,
    ) -> Result<Self, storage::StorageError> {
        let record = storage.load_session(id).await?;
        let messages = storage.load_messages(id).await?;
        tracing::info!(
            session_id = %id,
            messages = messages.len(),
            "resumed session from storage"
        );
        Ok(Self {
            id,
            workspace_id: record.workspace_id,
            history: Arc::new(Mutex::new(messages)),
            storage: Some(storage),
        })
    }

    /// Persist the session record to storage (call once after creating).
    pub async fn persist_create(
        &self,
        requirement: Option<&str>,
    ) -> Result<(), storage::StorageError> {
        if let Some(ref store) = self.storage {
            store
                .create_session(self.id, &self.workspace_id, requirement)
                .await?;
        }
        Ok(())
    }

    /// Update session status in storage.
    pub async fn persist_status(
        &self,
        status: SessionStatus,
    ) -> Result<(), storage::StorageError> {
        if let Some(ref store) = self.storage {
            store.update_session_status(self.id, status).await?;
        }
        Ok(())
    }

    /// Record a message to the in-memory transcript and optionally to storage.
    pub fn record(&self, msg: &TaskMessage) {
        if let Ok(mut h) = self.history.lock() {
            h.push(msg.clone());
        }
        // Fire-and-forget persistence on a background task.
        if let Some(ref store) = self.storage {
            let store = store.clone();
            let session_id = self.id;
            let msg = msg.clone();
            tokio::spawn(async move {
                if let Err(e) = store.record_message(session_id, &msg).await {
                    tracing::warn!(error = %e, "failed to persist message");
                }
            });
        }
    }

    pub fn snapshot(&self) -> Vec<TaskMessage> {
        self.history.lock().map(|h| h.clone()).unwrap_or_default()
    }

    /// Returns the storage backend, if any.
    pub fn storage(&self) -> Option<&Arc<dyn Storage>> {
        self.storage.as_ref()
    }
}
