//! Plugin system inspired by OpenClaw's plugin-first architecture.
//!
//! Defines extensible traits for tools, channels, and skills, plus a
//! [`ToolRegistry`] that agents can query at runtime. This allows adding
//! capabilities without recompiling the core pipeline.

pub mod builtin;
pub mod channel;
pub mod registry;
pub mod skill;
pub mod tool;

pub use channel::{ChannelEvent, ChannelPlugin};
pub use registry::ToolRegistry;
pub use skill::{SkillManifest, SkillRegistry};
pub use tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
