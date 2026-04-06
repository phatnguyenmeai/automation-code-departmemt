//! Persistence layer inspired by OpenClaw's session storage architecture.
//!
//! Provides a `Storage` trait with a default SQLite backend. Sessions and
//! task messages are persisted so pipelines can be inspected after completion
//! and resumed after interruption.

pub mod sqlite;

use agent_core::TaskMessage;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Status of a stored session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Completed => write!(f, "completed"),
            SessionStatus::Failed => write!(f, "failed"),
            SessionStatus::Interrupted => write!(f, "interrupted"),
        }
    }
}

impl std::str::FromStr for SessionStatus {
    type Err = StorageError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "running" => Ok(SessionStatus::Running),
            "completed" => Ok(SessionStatus::Completed),
            "failed" => Ok(SessionStatus::Failed),
            "interrupted" => Ok(SessionStatus::Interrupted),
            _ => Err(StorageError::Other(format!("unknown status: {s}"))),
        }
    }
}

/// Metadata for a persisted session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Uuid,
    pub workspace_id: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub requirement: Option<String>,
}

/// Core persistence trait modeled after OpenClaw's storage plugin interface.
///
/// Implementations must be Send + Sync for use across async tasks.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Create a new session record.
    async fn create_session(
        &self,
        id: Uuid,
        workspace_id: &str,
        requirement: Option<&str>,
    ) -> Result<()>;

    /// Update session status.
    async fn update_session_status(&self, id: Uuid, status: SessionStatus) -> Result<()>;

    /// Load session metadata by ID.
    async fn load_session(&self, id: Uuid) -> Result<SessionRecord>;

    /// List all sessions, most recent first.
    async fn list_sessions(&self, limit: usize) -> Result<Vec<SessionRecord>>;

    /// Persist a task message within a session.
    async fn record_message(&self, session_id: Uuid, msg: &TaskMessage) -> Result<()>;

    /// Load all messages for a session in insertion order.
    async fn load_messages(&self, session_id: Uuid) -> Result<Vec<TaskMessage>>;
}
