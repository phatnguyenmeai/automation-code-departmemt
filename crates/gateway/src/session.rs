use crate::transcript::TranscriptHandle;
use agent_core::TaskMessage;
use std::path::PathBuf;
use uuid::Uuid;

/// Durable session: routes each recorded message to the batched transcript
/// writer. No in-memory `Vec` — the transcript file is the source of truth.
#[derive(Clone)]
pub struct Session {
    pub id: Uuid,
    pub workspace_id: String,
    pub run_dir: PathBuf,
    transcript: TranscriptHandle,
}

impl Session {
    pub fn new(
        id: Uuid,
        workspace_id: impl Into<String>,
        run_dir: PathBuf,
        transcript: TranscriptHandle,
    ) -> Self {
        Self {
            id,
            workspace_id: workspace_id.into(),
            run_dir,
            transcript,
        }
    }

    /// Record a dispatched message to the transcript (non-blocking).
    pub fn record(&self, msg: &TaskMessage) {
        self.transcript.record(msg.clone());
    }
}
