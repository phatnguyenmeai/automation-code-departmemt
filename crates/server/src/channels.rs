//! Webhook endpoints for channel integrations.
//!
//! Each registered channel plugin gets a webhook route at
//! `/channels/{name}/webhook`. Inbound payloads are parsed by the
//! channel plugin and converted to pipeline requirements.

use crate::state::{AppState, PipelineEvent};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::post,
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/channels/{name}/webhook", post(channel_webhook))
}

async fn channel_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let channel = state
        .channels
        .get(&name)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown channel: {name}")))?;

    // Convert headers to Vec<(String, String)>.
    let header_pairs: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let event = channel
        .parse_inbound(&header_pairs, &body)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("parse: {e}")))?;

    let Some(event) = event else {
        return Ok(Json(serde_json::json!({ "status": "ignored" })));
    };

    // Create a session for this inbound message.
    let session_id = uuid::Uuid::new_v4();
    state
        .storage
        .create_session(session_id, "channel-default", Some(&event.text))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("storage: {e}")))?;

    state.emit(PipelineEvent {
        session_id: session_id.to_string(),
        event_type: "channel_message".into(),
        data: serde_json::json!({
            "channel": event.channel,
            "sender": event.sender,
            "text": event.text,
        }),
    });

    tracing::info!(
        channel = %name,
        sender = %event.sender,
        session_id = %session_id,
        "inbound channel message"
    );

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "session_id": session_id.to_string(),
    })))
}
