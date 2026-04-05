//! Minimal Anthropic Claude API client (non-streaming).
//!
//! Wraps the `POST /v1/messages` endpoint. Only what the agents need:
//! a system prompt, a single user message, and the plain-text response.

pub mod client;
pub mod prompt;

pub use client::{ClaudeClient, ClaudeError, ClaudeModel};
pub use prompt::PromptBuilder;
