//! SQLite-backed storage, inspired by OpenClaw's default SQLite persistence.
//!
//! Uses a single database file (default: `data/sessions.db`). All SQL runs on
//! a blocking thread via `tokio::task::spawn_blocking` so the async runtime is
//! never blocked by disk I/O.

use crate::{Result, SessionRecord, SessionStatus, Storage, StorageError};
use agent_core::TaskMessage;
use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// SQLite storage backend.
///
/// Wraps a `rusqlite::Connection` behind `Arc<Mutex<_>>` so it can be shared
/// across async tasks (rusqlite connections are not Send, so we move all access
/// into `spawn_blocking`).
#[derive(Clone)]
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    path: PathBuf,
}

impl SqliteStorage {
    /// Open (or create) the database at `path` and run migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::Other(format!("create dir: {e}")))?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    /// Open an in-memory database (useful for tests).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'running',
                requirement TEXT,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id          TEXT PRIMARY KEY,
                session_id  TEXT NOT NULL REFERENCES sessions(id),
                parent_id   TEXT,
                from_role   TEXT NOT NULL,
                to_role     TEXT NOT NULL,
                kind        TEXT NOT NULL,
                payload     TEXT NOT NULL,
                priority    TEXT NOT NULL DEFAULT 'normal',
                created_at  TEXT NOT NULL,
                seq         INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id, seq);

            CREATE INDEX IF NOT EXISTS idx_sessions_updated
                ON sessions(updated_at DESC);
            ",
        )?;
        Ok(())
    }

    /// Run a closure on the connection inside `spawn_blocking`.
    async fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| StorageError::Other(e.to_string()))?;
            f(&conn)
        })
        .await
        .map_err(|e| StorageError::Other(format!("spawn_blocking: {e}")))?
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn create_session(
        &self,
        id: Uuid,
        workspace_id: &str,
        requirement: Option<&str>,
    ) -> Result<()> {
        let wid = workspace_id.to_string();
        let req = requirement.map(|s| s.to_string());
        self.with_conn(move |conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO sessions (id, workspace_id, status, requirement, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id.to_string(), wid, "running", req, now, now],
            )?;
            Ok(())
        })
        .await
    }

    async fn update_session_status(&self, id: Uuid, status: SessionStatus) -> Result<()> {
        self.with_conn(move |conn| {
            let now = Utc::now().to_rfc3339();
            let rows = conn.execute(
                "UPDATE sessions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.to_string(), now, id.to_string()],
            )?;
            if rows == 0 {
                return Err(StorageError::NotFound(id.to_string()));
            }
            Ok(())
        })
        .await
    }

    async fn load_session(&self, id: Uuid) -> Result<SessionRecord> {
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT id, workspace_id, status, requirement, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok(SessionRecord {
                        id: row
                            .get::<_, String>(0)?
                            .parse()
                            .unwrap_or_default(),
                        workspace_id: row.get(1)?,
                        status: row
                            .get::<_, String>(2)?
                            .parse()
                            .unwrap_or(SessionStatus::Running),
                        requirement: row.get(3)?,
                        created_at: row
                            .get::<_, String>(4)?
                            .parse()
                            .unwrap_or_default(),
                        updated_at: row
                            .get::<_, String>(5)?
                            .parse()
                            .unwrap_or_default(),
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StorageError::NotFound(id.to_string())
                }
                other => StorageError::Sqlite(other),
            })
        })
        .await
    }

    async fn list_sessions(&self, limit: usize) -> Result<Vec<SessionRecord>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, workspace_id, status, requirement, created_at, updated_at
                 FROM sessions ORDER BY updated_at DESC LIMIT ?1",
            )?;
            let rows = stmt
                .query_map(params![limit as i64], |row| {
                    Ok(SessionRecord {
                        id: row
                            .get::<_, String>(0)?
                            .parse()
                            .unwrap_or_default(),
                        workspace_id: row.get(1)?,
                        status: row
                            .get::<_, String>(2)?
                            .parse()
                            .unwrap_or(SessionStatus::Running),
                        requirement: row.get(3)?,
                        created_at: row
                            .get::<_, String>(4)?
                            .parse()
                            .unwrap_or_default(),
                        updated_at: row
                            .get::<_, String>(5)?
                            .parse()
                            .unwrap_or_default(),
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    }

    async fn record_message(&self, session_id: Uuid, msg: &TaskMessage) -> Result<()> {
        let msg_json = serde_json::to_string(msg)?;
        let msg_id = msg.id.0.to_string();
        let parent_id = msg.parent_id.map(|p| p.0.to_string());
        let from_role = msg.from.as_str().to_string();
        let to_role = msg.to.as_str().to_string();
        let kind = serde_json::to_value(&msg.kind)?;
        let kind_str = kind
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let priority = serde_json::to_value(&msg.priority)?
            .as_str()
            .unwrap_or("normal")
            .to_string();
        let payload = serde_json::to_string(&msg.payload)?;

        self.with_conn(move |conn| {
            let now = Utc::now().to_rfc3339();
            // Use a subquery to auto-increment seq within the session.
            conn.execute(
                "INSERT INTO messages (id, session_id, parent_id, from_role, to_role, kind, payload, priority, created_at, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                         COALESCE((SELECT MAX(seq) FROM messages WHERE session_id = ?2), 0) + 1)",
                params![
                    msg_id,
                    session_id.to_string(),
                    parent_id,
                    from_role,
                    to_role,
                    kind_str,
                    payload,
                    priority,
                    now,
                ],
            )?;
            // Touch session updated_at
            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                params![now, session_id.to_string()],
            )?;
            let _ = msg_json; // suppress unused warning; we store fields individually
            Ok(())
        })
        .await
    }

    async fn load_messages(&self, session_id: Uuid) -> Result<Vec<TaskMessage>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT from_role, to_role, kind, payload, priority, id, parent_id
                 FROM messages WHERE session_id = ?1 ORDER BY seq ASC",
            )?;
            let rows = stmt
                .query_map(params![session_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            let mut messages = Vec::with_capacity(rows.len());
            for (from_str, to_str, kind_str, payload_str, priority_str, id_str, parent_str) in rows
            {
                // Reconstruct TaskMessage from stored fields.
                let from: agent_core::Role =
                    serde_json::from_value(serde_json::json!(from_str)).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let to: agent_core::Role =
                    serde_json::from_value(serde_json::json!(to_str)).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let kind: agent_core::TaskKind =
                    serde_json::from_value(serde_json::json!({ "kind": kind_str })).map_err(
                        |e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        },
                    )?;
                let payload: serde_json::Value = serde_json::from_str(&payload_str).map_err(
                    |e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    },
                )?;
                let priority: agent_core::Priority =
                    serde_json::from_value(serde_json::json!(priority_str)).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                let id = agent_core::TaskId(id_str.parse().unwrap_or_default());
                let parent_id = parent_str.map(|s| agent_core::TaskId(s.parse().unwrap_or_default()));

                messages.push(TaskMessage {
                    id,
                    parent_id,
                    from,
                    to,
                    kind,
                    payload,
                    priority,
                });
            }
            Ok(messages)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{Priority, Role, TaskKind, TaskMessage};

    #[tokio::test]
    async fn roundtrip_session_and_messages() {
        let store = SqliteStorage::in_memory().unwrap();
        let sid = Uuid::new_v4();

        store
            .create_session(sid, "test-ws", Some("build login"))
            .await
            .unwrap();

        let session = store.load_session(sid).await.unwrap();
        assert_eq!(session.workspace_id, "test-ws");
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.requirement.as_deref(), Some("build login"));

        let msg = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            serde_json::json!({"text": "build login page"}),
        );
        store.record_message(sid, &msg).await.unwrap();

        let msgs = store.load_messages(sid).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].from, Role::PM);
        assert_eq!(msgs[0].to, Role::BA);

        store
            .update_session_status(sid, SessionStatus::Completed)
            .await
            .unwrap();
        let session = store.load_session(sid).await.unwrap();
        assert_eq!(session.status, SessionStatus::Completed);
    }

    #[tokio::test]
    async fn list_sessions_ordered() {
        let store = SqliteStorage::in_memory().unwrap();

        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        store.create_session(s1, "ws1", None).await.unwrap();
        store.create_session(s2, "ws2", None).await.unwrap();

        // Touch s1 so it becomes more recent.
        store
            .update_session_status(s1, SessionStatus::Completed)
            .await
            .unwrap();

        let list = store.list_sessions(10).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, s1); // most recently updated
    }

    #[tokio::test]
    async fn message_priority_preserved() {
        let store = SqliteStorage::in_memory().unwrap();
        let sid = Uuid::new_v4();
        store.create_session(sid, "ws", None).await.unwrap();

        let mut msg = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            serde_json::json!(null),
        );
        msg.priority = Priority::High;
        store.record_message(sid, &msg).await.unwrap();

        let loaded = store.load_messages(sid).await.unwrap();
        assert_eq!(loaded[0].priority, Priority::High);
    }
}
