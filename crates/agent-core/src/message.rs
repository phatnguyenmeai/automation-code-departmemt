use crate::{artifact::ArtifactRef, role::Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Short form for file naming (first 8 hex chars of the uuid).
    pub fn short(&self) -> String {
        let s = self.0.simple().to_string();
        s[..8].to_string()
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Queue lane priority. Higher variants preempt lower ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Normal,
    High,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// Kind of payload carried by a TaskMessage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskKind {
    /// Raw requirement from the end user.
    Requirement,
    /// User story + acceptance criteria emitted by BA.
    Story,
    /// Backend impl spec / API contract emitted by Dev.
    ImplSpec,
    /// Frontend component spec emitted by Frontend.
    FrontendSpec,
    /// Test plan emitted by Test(strategy phase).
    TestPlan,
    /// Test execution report emitted by Test(execution phase).
    TestReport,
    /// PM aggregated final report.
    FinalReport,
    /// Something went wrong, needs PM intervention.
    Blocker,
}

impl TaskKind {
    /// Canonical kind slug used in artifact filenames.
    pub fn slug(&self) -> &'static str {
        match self {
            TaskKind::Requirement => "requirement",
            TaskKind::Story => "story",
            TaskKind::ImplSpec => "impl-spec",
            TaskKind::FrontendSpec => "frontend-spec",
            TaskKind::TestPlan => "test-plan",
            TaskKind::TestReport => "test-report",
            TaskKind::FinalReport => "final-report",
            TaskKind::Blocker => "blocker",
        }
    }
}

/// Message passed between agents via the LaneQueue. The actual payload
/// lives on disk as an artifact pair; this struct carries only the
/// pointer + a small inline `summary` suitable for logging/dispatch
/// decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub id: TaskId,
    /// The upstream message this one was produced in reply to (for tracing
    /// causal chains + resume idempotency lookups).
    pub parent_id: Option<TaskId>,
    pub from: Role,
    pub to: Role,
    pub kind: TaskKind,
    pub artifact: ArtifactRef,
    /// Small inline summary (< ~1KB): counts, titles, key fields. Logged
    /// directly; full content fetched via `ctx.artifacts.read(&artifact)`.
    #[serde(default)]
    pub summary: serde_json::Value,
    #[serde(default)]
    pub priority: Priority,
}

impl TaskMessage {
    pub fn new(
        from: Role,
        to: Role,
        kind: TaskKind,
        artifact: ArtifactRef,
        summary: serde_json::Value,
    ) -> Self {
        Self {
            id: artifact.task_id,
            parent_id: None,
            from,
            to,
            kind,
            artifact,
            summary,
            priority: Priority::Normal,
        }
    }

    pub fn reply(
        &self,
        from: Role,
        to: Role,
        kind: TaskKind,
        artifact: ArtifactRef,
        summary: serde_json::Value,
    ) -> Self {
        Self {
            id: artifact.task_id,
            parent_id: Some(self.id),
            from,
            to,
            kind,
            artifact,
            summary,
            priority: self.priority,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::ArtifactRef;
    use std::path::PathBuf;

    fn dummy_ref(role: Role, kind: &str) -> ArtifactRef {
        let task_id = TaskId::new();
        ArtifactRef {
            json_path: PathBuf::from(format!("artifacts/{}/{}-abc.json", role.as_str(), kind)),
            md_path: PathBuf::from(format!("artifacts/{}/{}-abc.md", role.as_str(), kind)),
            kind: kind.into(),
            role,
            task_id,
        }
    }

    #[test]
    fn serde_roundtrip() {
        let m = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            dummy_ref(Role::PM, "requirement"),
            serde_json::json!({ "text_len": 42 }),
        );
        let s = serde_json::to_string(&m).unwrap();
        let back: TaskMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back.from, Role::PM);
        assert_eq!(back.to, Role::BA);
        assert_eq!(back.artifact.kind, "requirement");
    }

    #[test]
    fn reply_links_parent() {
        let a = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            dummy_ref(Role::PM, "requirement"),
            serde_json::json!(null),
        );
        let b = a.reply(
            Role::BA,
            Role::Dev,
            TaskKind::Story,
            dummy_ref(Role::BA, "story"),
            serde_json::json!({ "count": 3 }),
        );
        assert_eq!(b.parent_id, Some(a.id));
    }

    #[test]
    fn task_id_short_is_8_chars() {
        let id = TaskId::new();
        assert_eq!(id.short().len(), 8);
    }
}
