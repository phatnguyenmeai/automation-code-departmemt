//! Artifact references and persistence abstraction.
//!
//! Every `TaskMessage` carries an `ArtifactRef` — a pointer to a pair of
//! files (`<name>.json` for data + `<name>.md` for humans) on disk.
//! Agents use the `ArtifactStore` trait to read/write/check-existence;
//! resume logic relies on `exists()` to short-circuit completed steps.

use crate::{message::TaskId, role::Role, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Pointer to a `(json, md)` artifact pair, stored relative to the session
/// run directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Path to the JSON file, relative to the session `run_dir`.
    pub json_path: PathBuf,
    /// Path to the companion Markdown file, relative to the session `run_dir`.
    pub md_path: PathBuf,
    /// Logical artifact kind (e.g. `"story"`, `"impl-spec"`, `"test-report"`).
    pub kind: String,
    /// The role that produced this artifact.
    pub role: Role,
    /// Task id that *produced* this artifact. For resume lookup we index by
    /// the `parent_id` of that task — see `ArtifactStore::exists`.
    pub task_id: TaskId,
}

/// Persistent artifact store. Filesystem impl lives in `gateway::store`.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Write a new artifact pair. Creates subdirectories on demand.
    ///
    /// `parent_id` — the id of the *input* task that produced this artifact.
    /// Files are named `{kind}-{parent_id}.{json,md}` so resume can query by
    /// (role, kind, parent_id).
    async fn write(
        &self,
        role: Role,
        kind: &str,
        parent_id: TaskId,
        produced_by: TaskId,
        data: &serde_json::Value,
        markdown: &str,
    ) -> Result<ArtifactRef>;

    /// Read back JSON content of an artifact.
    async fn read(&self, r: &ArtifactRef) -> Result<serde_json::Value>;

    /// Look up an existing artifact produced by `role` of kind `kind` for
    /// input task `parent_id`. Returns `None` if not yet produced.
    async fn exists(
        &self,
        role: Role,
        kind: &str,
        parent_id: TaskId,
    ) -> Result<Option<ArtifactRef>>;
}
