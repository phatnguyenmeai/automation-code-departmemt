//! Gateway: owns the lane queue, persists transcripts, drives agents.

pub mod lane_queue;
pub mod resume;
pub mod session;
pub mod store;
pub mod transcript;
pub mod workspace;

pub use lane_queue::{Lane, LaneQueue, LaneSender};
pub use resume::ResumeState;
pub use session::Session;
pub use store::FsArtifactStore;
pub use transcript::{TranscriptEntry, TranscriptHandle};
pub use workspace::{Gateway, SessionMeta, Workspace};
