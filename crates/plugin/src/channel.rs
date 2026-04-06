//! Channel plugin trait for messaging integrations.
//!
//! Inspired by OpenClaw's 50+ channel connectors. Each channel plugin
//! converts incoming webhook/message payloads into a unified `ChannelEvent`
//! and can send outbound messages back to the platform.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A normalized inbound event from any messaging channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent {
    /// Which channel this came from (e.g. "slack", "discord", "webhook").
    pub channel: String,
    /// Sender identifier (user ID, phone number, etc.).
    pub sender: String,
    /// The message text / requirement content.
    pub text: String,
    /// Optional metadata from the source platform.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Outbound message to send back through a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelReply {
    pub channel: String,
    pub recipient: String,
    pub text: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Trait for messaging channel integrations.
///
/// Channel plugins parse inbound webhooks into `ChannelEvent` and can
/// send replies back to the originating platform.
#[async_trait]
pub trait ChannelPlugin: Send + Sync {
    /// Channel identifier (e.g. "slack", "discord").
    fn name(&self) -> &str;

    /// Parse a raw webhook/HTTP body into a ChannelEvent.
    /// Returns None if the payload is not a message (e.g. a health check).
    async fn parse_inbound(
        &self,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<Option<ChannelEvent>, String>;

    /// Send a reply back through this channel.
    async fn send_reply(&self, reply: ChannelReply) -> Result<(), String>;
}
