//! Resume-state derived from a persisted transcript.
//!
//! Given a transcript, we derive:
//!  - `dispatched`: every `TaskId` ever produced
//!  - `completed_inputs`: for each `(Role, kind)`, the set of `parent_id`s
//!    that already have an output artifact — used for agent-level
//!    idempotency checks
//!  - `in_flight`: messages that were dispatched *to* some role but no
//!    descendant of theirs was subsequently dispatched. These are the
//!    messages we re-inject on resume.

use crate::transcript::{self, TranscriptEntry};
use agent_core::{Role, TaskId, TaskKind, TaskMessage};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Default)]
pub struct ResumeState {
    pub entries: Vec<TranscriptEntry>,
    pub dispatched: HashSet<TaskId>,
    /// Input task ids for which role R has already produced output of
    /// kind K. Keyed by (output_role, output_kind) -> set of parent_id.
    pub completed_inputs: HashMap<(Role, TaskKindKey), HashSet<TaskId>>,
    /// Messages delivered to some role but whose descendants never showed
    /// up in the transcript. These are the earliest in-flight items.
    pub in_flight: Vec<TaskMessage>,
}

/// `TaskKind` has non-hashable structure; use its slug as key.
pub type TaskKindKey = String;

pub async fn load(transcript_path: &Path) -> std::io::Result<ResumeState> {
    let entries = transcript::load(transcript_path).await?;
    Ok(from_entries(entries))
}

pub fn from_entries(entries: Vec<TranscriptEntry>) -> ResumeState {
    let mut dispatched = HashSet::new();
    let mut completed_inputs: HashMap<(Role, TaskKindKey), HashSet<TaskId>> = HashMap::new();
    let mut parents_of_later: HashSet<TaskId> = HashSet::new();

    for e in &entries {
        dispatched.insert(e.msg.id);
        if let Some(parent) = e.msg.parent_id {
            parents_of_later.insert(parent);
            completed_inputs
                .entry((e.msg.from, kind_slug(&e.msg.kind)))
                .or_default()
                .insert(parent);
        }
    }

    // in_flight: msgs dispatched *to* some agent whose id never appears as
    // a parent_id of any later entry = no downstream produced yet.
    let in_flight: Vec<TaskMessage> = entries
        .iter()
        .filter(|e| !parents_of_later.contains(&e.msg.id))
        .map(|e| e.msg.clone())
        .collect();

    ResumeState {
        entries,
        dispatched,
        completed_inputs,
        in_flight,
    }
}

pub fn kind_slug(k: &TaskKind) -> String {
    k.slug().to_string()
}

impl ResumeState {
    /// Has `role` already produced an output of `kind` for input `parent_id`?
    pub fn already_done(&self, role: Role, kind: &str, parent_id: TaskId) -> bool {
        self.completed_inputs
            .get(&(role, kind.to_string()))
            .map(|s| s.contains(&parent_id))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{ArtifactRef, TaskKind, TaskMessage};
    use chrono::Utc;
    use std::path::PathBuf;

    fn entry(msg: TaskMessage) -> TranscriptEntry {
        TranscriptEntry {
            timestamp: Utc::now(),
            msg,
        }
    }

    fn mk_ref(role: Role, kind: &str) -> ArtifactRef {
        ArtifactRef {
            json_path: PathBuf::from("x.json"),
            md_path: PathBuf::from("x.md"),
            kind: kind.into(),
            role,
            task_id: TaskId::new(),
        }
    }

    #[test]
    fn in_flight_is_leaf_messages() {
        // PM dispatches to BA; BA dispatches to Dev; Dev dispatches to Test.
        // in_flight should be just the Test-bound message.
        let a = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            mk_ref(Role::PM, "requirement"),
            serde_json::json!(null),
        );
        let b = a.reply(
            Role::BA,
            Role::Dev,
            TaskKind::Story,
            mk_ref(Role::BA, "story"),
            serde_json::json!(null),
        );
        let c = b.reply(
            Role::Dev,
            Role::Test,
            TaskKind::ImplSpec,
            mk_ref(Role::Dev, "impl-spec"),
            serde_json::json!(null),
        );
        let state = from_entries(vec![entry(a), entry(b), entry(c.clone())]);
        assert_eq!(state.in_flight.len(), 1);
        assert_eq!(state.in_flight[0].id, c.id);
    }

    #[test]
    fn already_done_lookup() {
        let a = TaskMessage::new(
            Role::PM,
            Role::BA,
            TaskKind::Requirement,
            mk_ref(Role::PM, "requirement"),
            serde_json::json!(null),
        );
        let b = a.reply(
            Role::BA,
            Role::Dev,
            TaskKind::Story,
            mk_ref(Role::BA, "story"),
            serde_json::json!(null),
        );
        let state = from_entries(vec![entry(a.clone()), entry(b)]);
        // BA already produced "story" for parent_id = a.id
        assert!(state.already_done(Role::BA, "story", a.id));
        assert!(!state.already_done(Role::BA, "story", TaskId::new()));
    }
}
