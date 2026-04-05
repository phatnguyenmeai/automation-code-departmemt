//! Filesystem-backed `ArtifactStore` implementation.
//!
//! Layout (all paths relative to `run_dir`):
//! ```text
//! artifacts/
//!   <role>/
//!     <kind>-<parent_short>.json
//!     <kind>-<parent_short>.md
//! ```
//!
//! The `parent_short` is 8 hex chars of the *input* task's uuid — this
//! is the key resume uses to detect "already produced".

use agent_core::{ArtifactRef, ArtifactStore, Result as AgentResult, Role, TaskId};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct FsArtifactStore {
    run_dir: PathBuf,
}

impl FsArtifactStore {
    pub fn new(run_dir: impl Into<PathBuf>) -> Self {
        Self {
            run_dir: run_dir.into(),
        }
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    fn rel_json_path(role: Role, kind: &str, parent_id: TaskId) -> PathBuf {
        PathBuf::from("artifacts")
            .join(role.as_str())
            .join(format!("{}-{}.json", kind, parent_id.short()))
    }

    fn rel_md_path(role: Role, kind: &str, parent_id: TaskId) -> PathBuf {
        PathBuf::from("artifacts")
            .join(role.as_str())
            .join(format!("{}-{}.md", kind, parent_id.short()))
    }
}

#[async_trait]
impl ArtifactStore for FsArtifactStore {
    async fn write(
        &self,
        role: Role,
        kind: &str,
        parent_id: TaskId,
        produced_by: TaskId,
        data: &serde_json::Value,
        markdown: &str,
    ) -> AgentResult<ArtifactRef> {
        let rel_json = Self::rel_json_path(role, kind, parent_id);
        let rel_md = Self::rel_md_path(role, kind, parent_id);
        let abs_json = self.run_dir.join(&rel_json);
        let abs_md = self.run_dir.join(&rel_md);

        if let Some(parent) = abs_json.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json_bytes = serde_json::to_vec_pretty(data)?;
        fs::write(&abs_json, json_bytes).await?;
        fs::write(&abs_md, markdown.as_bytes()).await?;

        Ok(ArtifactRef {
            json_path: rel_json,
            md_path: rel_md,
            kind: kind.to_string(),
            role,
            task_id: produced_by,
        })
    }

    async fn read(&self, r: &ArtifactRef) -> AgentResult<serde_json::Value> {
        let abs = self.run_dir.join(&r.json_path);
        let bytes = fs::read(&abs).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn exists(
        &self,
        role: Role,
        kind: &str,
        parent_id: TaskId,
    ) -> AgentResult<Option<ArtifactRef>> {
        let rel_json = Self::rel_json_path(role, kind, parent_id);
        let rel_md = Self::rel_md_path(role, kind, parent_id);
        let abs_json = self.run_dir.join(&rel_json);
        if fs::try_exists(&abs_json).await? {
            // Read back the JSON to recover the `produced_by` task id stored
            // in a well-known metadata path: since we don't embed metadata
            // in the JSON itself, we reconstruct a fresh TaskId here.
            // Resume logic doesn't actually need this id (it's only for
            // chaining new messages, which won't happen on skip-path).
            Ok(Some(ArtifactRef {
                json_path: rel_json,
                md_path: rel_md,
                kind: kind.to_string(),
                role,
                task_id: parent_id, // placeholder; caller must not rely on this
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::TaskId;
    use tempfile::TempDir;

    #[tokio::test]
    async fn write_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = FsArtifactStore::new(tmp.path());
        let parent = TaskId::new();
        let produced = TaskId::new();
        let data = serde_json::json!({ "hello": "world" });

        let r = store
            .write(Role::BA, "story", parent, produced, &data, "# Story\n\n- one\n")
            .await
            .unwrap();

        assert_eq!(r.kind, "story");
        assert_eq!(r.role, Role::BA);

        let back = store.read(&r).await.unwrap();
        assert_eq!(back, data);

        let md = tokio::fs::read_to_string(tmp.path().join(&r.md_path)).await.unwrap();
        assert!(md.contains("# Story"));
    }

    #[tokio::test]
    async fn exists_detects_written_artifact() {
        let tmp = TempDir::new().unwrap();
        let store = FsArtifactStore::new(tmp.path());
        let parent = TaskId::new();

        assert!(store.exists(Role::Dev, "impl-spec", parent).await.unwrap().is_none());

        store
            .write(
                Role::Dev,
                "impl-spec",
                parent,
                TaskId::new(),
                &serde_json::json!({}),
                "# impl\n",
            )
            .await
            .unwrap();

        assert!(store.exists(Role::Dev, "impl-spec", parent).await.unwrap().is_some());
    }
}
