use serde::{Deserialize, Serialize};
use std::time::Duration;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Copy)]
pub enum ClaudeModel {
    Opus,
    Sonnet,
    Haiku,
}

impl ClaudeModel {
    pub fn id(&self) -> &'static str {
        match self {
            ClaudeModel::Opus => "claude-opus-4-5",
            ClaudeModel::Sonnet => "claude-sonnet-4-5",
            ClaudeModel::Haiku => "claude-haiku-4-5",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "opus" => Some(ClaudeModel::Opus),
            "sonnet" => Some(ClaudeModel::Sonnet),
            "haiku" => Some(ClaudeModel::Haiku),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("empty response")]
    Empty,
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct ClaudeClient {
    api_key: String,
    http: reqwest::Client,
}

impl ClaudeClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");
        Self {
            api_key: api_key.into(),
            http,
        }
    }

    pub fn from_env() -> Result<Self, ClaudeError> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::Api {
            status: 0,
            body: "ANTHROPIC_API_KEY not set".into(),
        })?;
        Ok(Self::new(key))
    }

    /// Send one turn with an optional system prompt. Returns concatenated text content.
    pub async fn complete(
        &self,
        model: ClaudeModel,
        system: Option<&str>,
        user: &str,
        max_tokens: u32,
    ) -> Result<String, ClaudeError> {
        let body = MessagesRequest {
            model: model.id().to_string(),
            max_tokens,
            system: system.map(|s| s.to_string()),
            messages: vec![Message {
                role: "user".into(),
                content: user.to_string(),
            }],
        };

        tracing::debug!(model = model.id(), "claude request");

        let resp = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClaudeError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: MessagesResponse = resp.json().await?;
        let text = parsed
            .content
            .into_iter()
            .filter_map(|b| if b.r#type == "text" { Some(b.text) } else { None })
            .collect::<Vec<_>>()
            .join("");

        if text.is_empty() {
            return Err(ClaudeError::Empty);
        }
        Ok(text)
    }
}

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    r#type: String,
    #[serde(default)]
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids() {
        assert!(ClaudeModel::Opus.id().starts_with("claude-opus"));
        assert!(ClaudeModel::Sonnet.id().starts_with("claude-sonnet"));
    }

    #[test]
    fn model_from_str() {
        assert!(matches!(ClaudeModel::from_str("opus"), Some(ClaudeModel::Opus)));
        assert!(ClaudeModel::from_str("unknown").is_none());
    }
}
