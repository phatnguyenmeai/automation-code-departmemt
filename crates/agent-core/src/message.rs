use crate::role::Role;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub id: TaskId,
    /// For cross-referencing in PM aggregation.
    pub parent_id: Option<TaskId>,
    pub from: Role,
    pub to: Role,
    pub kind: TaskKind,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub priority: Priority,
}

impl TaskMessage {
    pub fn new(from: Role, to: Role, kind: TaskKind, payload: serde_json::Value) -> Self {
        Self {
            id: TaskId::new(),
            parent_id: None,
            from,
            to,
            kind,
            payload,
            priority: Priority::Normal,
        }
    }

    pub fn reply(&self, from: Role, to: Role, kind: TaskKind, payload: serde_json::Value) -> Self {
        Self {
            id: TaskId::new(),
            parent_id: Some(self.id),
            from,
            to,
            kind,
            payload,
            priority: self.priority,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let m = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            serde_json::json!({"text": "build login page"}),
        );
        let s = serde_json::to_string(&m).unwrap();
        let back: TaskMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back.from, Role::PM);
        assert_eq!(back.to, Role::BA);
    }

    #[test]
    fn reply_links_parent() {
        let a = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            serde_json::json!(null),
        );
        let b = a.reply(Role::BA, Role::Dev, TaskKind::Story, serde_json::json!([]));
        assert_eq!(b.parent_id, Some(a.id));
    }
}
