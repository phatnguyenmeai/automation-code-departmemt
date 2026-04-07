//! Plugin system inspired by OpenClaw's plugin-first architecture.
//!
//! Defines extensible traits for tools, channels, and skills, plus a
//! [`ToolRegistry`] that agents can query at runtime. This allows adding
//! capabilities without recompiling the core pipeline.

pub mod builtin;
pub mod cargo_tool;
pub mod channel;
pub mod mongo_tool;
pub mod playwright_tool;
pub mod registry;
pub mod skill;
pub mod telegram;
pub mod tool;

pub use cargo_tool::CargoTool;
pub use channel::{ChannelEvent, ChannelPlugin, ChannelReply};
pub use mongo_tool::MongoTool;
pub use playwright_tool::PlaywrightRunnerTool;
pub use registry::ToolRegistry;
pub use skill::{SkillManifest, SkillRegistry};
pub use telegram::{TelegramConfig, TelegramPlugin};
pub use tool::{ToolContext, ToolError, ToolPlugin, ToolResult};
