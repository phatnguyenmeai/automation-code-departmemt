//! Telegram Bot API channel plugin.
//!
//! Provides bidirectional Telegram integration:
//! - Receive messages from Telegram users/groups via webhook or long-polling
//! - Send reports and status updates to Telegram chats
//! - Request approvals via inline keyboard buttons
//! - Handle callback queries for approval responses

use crate::channel::{ChannelEvent, ChannelPlugin, ChannelReply};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

// ─── Telegram Bot API Types ───

/// Telegram Update object (subset of fields we care about).
#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub chat: TelegramChat,
    pub text: Option<String>,
    pub date: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub message: Option<TelegramMessage>,
    pub data: Option<String>,
}

/// Inline keyboard markup for approval buttons.
#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub callback_data: Option<String>,
}

/// Response from Telegram's sendMessage API.
#[derive(Debug, Deserialize)]
struct SendMessageResponse {
    ok: bool,
    result: Option<TelegramMessage>,
    description: Option<String>,
}

/// Response from Telegram's getUpdates API.
#[derive(Debug, Deserialize)]
struct GetUpdatesResponse {
    ok: bool,
    result: Option<Vec<TelegramUpdate>>,
    description: Option<String>,
}

/// Response from answerCallbackQuery API.
#[derive(Debug, Deserialize)]
struct AnswerCallbackResponse {
    ok: bool,
    description: Option<String>,
}

// ─── Approval Flow ───

/// An approval request awaiting user response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique ID for this approval.
    pub id: String,
    /// Session that originated this request.
    pub session_id: String,
    /// Chat to send the approval prompt to.
    pub chat_id: i64,
    /// Description of what needs approval.
    pub description: String,
    /// Who requested the approval (agent role).
    pub requested_by: String,
    /// Timestamp of the request.
    pub created_at: i64,
}

/// Result of an approval request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResult {
    pub id: String,
    pub status: ApprovalStatus,
    pub responded_by: Option<String>,
    pub responded_at: Option<i64>,
    pub comment: Option<String>,
}

/// Pending approval with a channel to notify when resolved.
struct PendingApproval {
    #[allow(dead_code)]
    request: ApprovalRequest,
    tx: Option<oneshot::Sender<ApprovalResult>>,
}

// ─── Telegram Plugin ───

/// Configuration for the Telegram channel plugin.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    /// Bot token from @BotFather.
    pub bot_token: String,
    /// Default chat ID to send reports to.
    pub default_chat_id: Option<i64>,
    /// Allowed user IDs that can interact with the bot.
    /// Empty means all users are allowed.
    #[serde(default)]
    pub allowed_users: Vec<i64>,
    /// Whether to use webhook mode (true) or long-polling (false).
    #[serde(default)]
    pub webhook_mode: bool,
    /// Webhook URL (required if webhook_mode is true).
    pub webhook_url: Option<String>,
    /// Parse mode for messages (HTML or MarkdownV2).
    #[serde(default = "default_parse_mode")]
    pub parse_mode: String,
}

fn default_parse_mode() -> String {
    "HTML".into()
}

/// Telegram channel plugin for agent communication.
pub struct TelegramPlugin {
    config: TelegramConfig,
    http: reqwest::Client,
    /// Pending approval requests indexed by approval ID.
    pending_approvals: Arc<Mutex<HashMap<String, PendingApproval>>>,
    /// Completed approvals for query.
    completed_approvals: Arc<Mutex<Vec<ApprovalResult>>>,
    /// Last update_id for long-polling offset.
    last_update_id: Arc<Mutex<Option<i64>>>,
}

impl TelegramPlugin {
    /// Create a new Telegram plugin with the given configuration.
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
            completed_approvals: Arc::new(Mutex::new(Vec::new())),
            last_update_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Base URL for Telegram Bot API.
    fn api_url(&self, method: &str) -> String {
        format!(
            "https://api.telegram.org/bot{}/{}",
            self.config.bot_token, method
        )
    }

    /// Check if a user is allowed to interact with the bot.
    fn is_user_allowed(&self, user_id: i64) -> bool {
        self.config.allowed_users.is_empty() || self.config.allowed_users.contains(&user_id)
    }

    // ─── Bot API Methods ───

    /// Send a text message to a Telegram chat.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<TelegramMessage, String> {
        self.send_message_with_markup(chat_id, text, None).await
    }

    /// Send a message with optional inline keyboard.
    pub async fn send_message_with_markup(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<TelegramMessage, String> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": self.config.parse_mode,
        });

        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup).map_err(|e| e.to_string())?;
        }

        let resp = self
            .http
            .post(self.api_url("sendMessage"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("telegram send: {e}"))?;

        let result: SendMessageResponse = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result.ok {
            result
                .result
                .ok_or_else(|| "empty result from telegram".into())
        } else {
            Err(format!(
                "telegram error: {}",
                result.description.unwrap_or_default()
            ))
        }
    }

    /// Edit an existing message's text.
    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
    ) -> Result<(), String> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
            "parse_mode": self.config.parse_mode,
        });

        if let Some(markup) = reply_markup {
            body["reply_markup"] = serde_json::to_value(markup).map_err(|e| e.to_string())?;
        }

        let resp = self
            .http
            .post(self.api_url("editMessageText"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("telegram edit: {e}"))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result["ok"].as_bool().unwrap_or(false) {
            Ok(())
        } else {
            Err(format!(
                "telegram edit error: {}",
                result["description"].as_str().unwrap_or("unknown")
            ))
        }
    }

    /// Answer a callback query (dismiss the loading indicator on the button).
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<(), String> {
        let mut body = serde_json::json!({
            "callback_query_id": callback_query_id,
        });

        if let Some(t) = text {
            body["text"] = serde_json::Value::String(t.to_string());
        }

        let resp = self
            .http
            .post(self.api_url("answerCallbackQuery"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("telegram callback: {e}"))?;

        let result: AnswerCallbackResponse = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result.ok {
            Ok(())
        } else {
            Err(format!(
                "telegram callback error: {}",
                result.description.unwrap_or_default()
            ))
        }
    }

    /// Long-poll for new updates from Telegram.
    pub async fn get_updates(&self, timeout: u32) -> Result<Vec<TelegramUpdate>, String> {
        let offset = {
            let guard = self.last_update_id.lock().await;
            guard.map(|id| id + 1)
        };

        let mut body = serde_json::json!({
            "timeout": timeout,
            "allowed_updates": ["message", "callback_query"],
        });

        if let Some(off) = offset {
            body["offset"] = serde_json::json!(off);
        }

        let resp = self
            .http
            .post(self.api_url("getUpdates"))
            .json(&body)
            .timeout(std::time::Duration::from_secs((timeout + 10) as u64))
            .send()
            .await
            .map_err(|e| format!("telegram poll: {e}"))?;

        let result: GetUpdatesResponse = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if !result.ok {
            return Err(format!(
                "telegram poll error: {}",
                result.description.unwrap_or_default()
            ));
        }

        let updates = result.result.unwrap_or_default();

        // Update offset.
        if let Some(last) = updates.last() {
            let mut guard = self.last_update_id.lock().await;
            *guard = Some(last.update_id);
        }

        Ok(updates)
    }

    /// Set webhook URL with Telegram.
    pub async fn set_webhook(&self, url: &str) -> Result<(), String> {
        let body = serde_json::json!({
            "url": url,
            "allowed_updates": ["message", "callback_query"],
        });

        let resp = self
            .http
            .post(self.api_url("setWebhook"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("telegram webhook: {e}"))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result["ok"].as_bool().unwrap_or(false) {
            tracing::info!(%url, "telegram webhook set");
            Ok(())
        } else {
            Err(format!(
                "telegram webhook error: {}",
                result["description"].as_str().unwrap_or("unknown")
            ))
        }
    }

    /// Delete webhook (switch to long-polling mode).
    pub async fn delete_webhook(&self) -> Result<(), String> {
        let resp = self
            .http
            .post(self.api_url("deleteWebhook"))
            .send()
            .await
            .map_err(|e| format!("telegram: {e}"))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result["ok"].as_bool().unwrap_or(false) {
            Ok(())
        } else {
            Err(format!(
                "telegram error: {}",
                result["description"].as_str().unwrap_or("unknown")
            ))
        }
    }

    // ─── High-Level Agent Methods ───

    /// Send a pipeline report to the default chat or a specific chat.
    pub async fn send_report(
        &self,
        chat_id: Option<i64>,
        session_id: &str,
        title: &str,
        body: &str,
        status: &str,
    ) -> Result<(), String> {
        let target = chat_id
            .or(self.config.default_chat_id)
            .ok_or_else(|| "no chat_id and no default_chat_id configured".to_string())?;

        let status_emoji = match status {
            "completed" => "\u{2705}",   // ✅
            "failed" => "\u{274c}",      // ❌
            "running" => "\u{23f3}",     // ⏳
            "blocked" => "\u{26a0}\u{fe0f}", // ⚠️
            _ => "\u{2139}\u{fe0f}",     // ℹ️
        };

        let message = format!(
            "{status_emoji} <b>{title}</b>\n\n\
             <b>Session:</b> <code>{session_id}</code>\n\
             <b>Status:</b> {status}\n\n\
             {body}"
        );

        self.send_message(target, &message).await?;
        Ok(())
    }

    /// Request approval from a Telegram user/chat.
    ///
    /// Returns a receiver that resolves when the user responds.
    pub async fn request_approval(
        &self,
        request: ApprovalRequest,
    ) -> Result<oneshot::Receiver<ApprovalResult>, String> {
        let chat_id = request.chat_id;
        let approval_id = request.id.clone();

        let keyboard = InlineKeyboardMarkup {
            inline_keyboard: vec![vec![
                InlineKeyboardButton {
                    text: "\u{2705} Approve".into(),
                    callback_data: Some(format!("approve:{}", approval_id)),
                },
                InlineKeyboardButton {
                    text: "\u{274c} Reject".into(),
                    callback_data: Some(format!("reject:{}", approval_id)),
                },
            ]],
        };

        let message = format!(
            "\u{1f514} <b>Approval Required</b>\n\n\
             <b>ID:</b> <code>{}</code>\n\
             <b>Session:</b> <code>{}</code>\n\
             <b>Requested by:</b> {}\n\n\
             {}\n\n\
             Please approve or reject:",
            request.id, request.session_id, request.requested_by, request.description,
        );

        self.send_message_with_markup(chat_id, &message, Some(keyboard))
            .await?;

        let (tx, rx) = oneshot::channel();

        {
            let mut approvals = self.pending_approvals.lock().await;
            approvals.insert(
                approval_id,
                PendingApproval {
                    request,
                    tx: Some(tx),
                },
            );
        }

        Ok(rx)
    }

    /// Handle a callback query from an inline keyboard button.
    pub async fn handle_callback(&self, query: &CallbackQuery) -> Result<(), String> {
        let data = query.data.as_deref().unwrap_or("");

        let (action, approval_id) = data
            .split_once(':')
            .ok_or_else(|| format!("invalid callback data: {data}"))?;

        let status = match action {
            "approve" => ApprovalStatus::Approved,
            "reject" => ApprovalStatus::Rejected,
            _ => return Err(format!("unknown action: {action}")),
        };

        let user_name = query
            .from
            .username
            .clone()
            .unwrap_or_else(|| query.from.first_name.clone());

        let now = chrono::Utc::now().timestamp();

        let result = ApprovalResult {
            id: approval_id.to_string(),
            status: status.clone(),
            responded_by: Some(user_name.clone()),
            responded_at: Some(now),
            comment: None,
        };

        // Answer the callback query.
        let answer_text = match &status {
            ApprovalStatus::Approved => "Approved!",
            ApprovalStatus::Rejected => "Rejected!",
            ApprovalStatus::Pending => "Pending",
        };
        self.answer_callback_query(&query.id, Some(answer_text))
            .await?;

        // Update the message to reflect the decision.
        if let Some(ref msg) = query.message {
            let status_text = match &status {
                ApprovalStatus::Approved => {
                    format!("\u{2705} <b>APPROVED</b> by @{user_name}")
                }
                ApprovalStatus::Rejected => {
                    format!("\u{274c} <b>REJECTED</b> by @{user_name}")
                }
                ApprovalStatus::Pending => "Pending".into(),
            };

            let original = msg.text.as_deref().unwrap_or("");
            // Strip HTML for reconstruction — keep the original text and append decision.
            let updated = format!("{original}\n\n{status_text}");

            // Edit message to remove buttons and show result.
            let _ = self
                .edit_message_text(msg.chat.id, msg.message_id, &updated, None)
                .await;
        }

        // Notify the waiting approval handler.
        {
            let mut approvals = self.pending_approvals.lock().await;
            if let Some(mut pending) = approvals.remove(approval_id) {
                if let Some(tx) = pending.tx.take() {
                    let _ = tx.send(result.clone());
                }
            }
        }

        // Store completed approval.
        {
            let mut completed = self.completed_approvals.lock().await;
            completed.push(result);
        }

        Ok(())
    }

    /// Process a single update (message or callback query).
    pub async fn process_update(&self, update: &TelegramUpdate) -> Option<ChannelEvent> {
        // Handle callback queries (approval responses).
        if let Some(ref cq) = update.callback_query {
            if !self.is_user_allowed(cq.from.id) {
                tracing::warn!(user_id = cq.from.id, "unauthorized callback query");
                return None;
            }
            if let Err(e) = self.handle_callback(cq).await {
                tracing::error!(error = %e, "failed to handle callback query");
            }
            return None;
        }

        // Handle regular messages.
        if let Some(ref msg) = update.message {
            let user = msg.from.as_ref()?;
            if !self.is_user_allowed(user.id) {
                tracing::warn!(user_id = user.id, "unauthorized message");
                return None;
            }

            let text = msg.text.as_deref().unwrap_or("");
            if text.is_empty() {
                return None;
            }

            // Handle /start command.
            if text == "/start" {
                let welcome = format!(
                    "\u{1f916} <b>AgentDept Bot</b>\n\n\
                     I'm your virtual engineering department assistant.\n\n\
                     <b>Commands:</b>\n\
                     /start - Show this message\n\
                     /status - Check pipeline status\n\
                     /sessions - List recent sessions\n\
                     /help - Show help\n\n\
                     Send me a requirement to start a new pipeline run!"
                );
                let _ = self.send_message(msg.chat.id, &welcome).await;
                return None;
            }

            // Handle /status command.
            if text == "/status" || text == "/sessions" || text == "/help" {
                // These will be handled by the server layer — return as a command event.
                return Some(ChannelEvent {
                    channel: "telegram".into(),
                    sender: user.id.to_string(),
                    text: text.to_string(),
                    metadata: serde_json::json!({
                        "chat_id": msg.chat.id,
                        "message_id": msg.message_id,
                        "chat_type": msg.chat.chat_type,
                        "username": user.username,
                        "first_name": user.first_name,
                        "is_command": true,
                    }),
                });
            }

            // Regular message — treat as a requirement/interaction.
            return Some(ChannelEvent {
                channel: "telegram".into(),
                sender: user.id.to_string(),
                text: text.to_string(),
                metadata: serde_json::json!({
                    "chat_id": msg.chat.id,
                    "message_id": msg.message_id,
                    "chat_type": msg.chat.chat_type,
                    "username": user.username,
                    "first_name": user.first_name,
                }),
            });
        }

        None
    }

    /// Get pending approval count.
    pub async fn pending_approval_count(&self) -> usize {
        self.pending_approvals.lock().await.len()
    }

    /// Get list of completed approvals.
    pub async fn completed_approvals(&self) -> Vec<ApprovalResult> {
        self.completed_approvals.lock().await.clone()
    }

    /// Get bot info to verify the token.
    pub async fn get_me(&self) -> Result<TelegramUser, String> {
        let resp = self
            .http
            .get(self.api_url("getMe"))
            .send()
            .await
            .map_err(|e| format!("telegram: {e}"))?;

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("telegram parse: {e}"))?;

        if result["ok"].as_bool().unwrap_or(false) {
            serde_json::from_value(result["result"].clone())
                .map_err(|e| format!("telegram parse user: {e}"))
        } else {
            Err(format!(
                "telegram error: {}",
                result["description"].as_str().unwrap_or("unknown")
            ))
        }
    }
}

// ─── ChannelPlugin Implementation ───

#[async_trait]
impl ChannelPlugin for TelegramPlugin {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn parse_inbound(
        &self,
        _headers: &[(String, String)],
        body: &[u8],
    ) -> Result<Option<ChannelEvent>, String> {
        let update: TelegramUpdate = serde_json::from_slice(body)
            .map_err(|e| format!("invalid telegram update: {e}"))?;

        Ok(self.process_update(&update).await)
    }

    async fn send_reply(&self, reply: ChannelReply) -> Result<(), String> {
        let chat_id: i64 = reply
            .recipient
            .parse()
            .map_err(|_| format!("invalid chat_id: {}", reply.recipient))?;

        self.send_message(chat_id, &reply.text).await?;
        Ok(())
    }
}

// ─── Long-Polling Runner ───

/// Spawn a long-polling task that continuously fetches updates from Telegram.
///
/// Returns a join handle and a receiver for channel events (messages that
/// should be processed by the pipeline).
pub fn spawn_polling(
    plugin: Arc<TelegramPlugin>,
    poll_timeout: u32,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Receiver<ChannelEvent>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(64);

    let handle = tokio::spawn(async move {
        tracing::info!(timeout = poll_timeout, "telegram long-polling started");

        loop {
            match plugin.get_updates(poll_timeout).await {
                Ok(updates) => {
                    for update in &updates {
                        if let Some(event) = plugin.process_update(update).await {
                            if tx.send(event).await.is_err() {
                                tracing::info!("telegram polling receiver dropped, stopping");
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "telegram polling error");
                    // Back off on error.
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });

    (handle, rx)
}
