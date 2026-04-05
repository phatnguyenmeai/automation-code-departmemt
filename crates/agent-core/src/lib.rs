//! Core types and traits for the dev-department agent system.
//!
//! Defines the `Agent` trait, `TaskMessage`, `Role`, `ArtifactRef`, and the
//! shared context used by every concrete agent implementation and the
//! gateway.

pub mod agent;
pub mod artifact;
pub mod message;
pub mod role;

pub use agent::{Agent, AgentCtx, AgentOutput, Dispatcher};
pub use artifact::{ArtifactRef, ArtifactStore};
pub use message::{Priority, TaskId, TaskKind, TaskMessage};
pub use role::Role;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("llm error: {0}")]
    Llm(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("artifact not found: {0}")]
    NotFound(String),
    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AgentError>;
