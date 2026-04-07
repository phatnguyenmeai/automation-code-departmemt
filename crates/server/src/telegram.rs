//! Telegram-specific API routes and long-polling integration.
//!
//! Provides:
//! - POST /api/telegram/send       — Send a message to a Telegram chat
//! - POST /api/telegram/report     — Send a pipeline report
//! - POST /api/telegram/approve    — Request approval from Telegram user
//! - GET  /api/telegram/approvals  — List pending/completed approvals
//! - GET  /api/telegram/status     — Check Telegram bot status

use crate::auth::{self, AuthContext};
use crate::state::{AppState, PipelineEvent};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use plugin::telegram::{ApprovalRequest, ApprovalStatus, TelegramPlugin};
use serde::Deserialize;
use std::sync::Arc;
use storage::ApiKeyRole;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/telegram/send", post(send_message))
        .route("/api/telegram/report", post(send_report))
        .route("/api/telegram/approve", post(request_approval))
        .route("/api/telegram/approvals", get(list_approvals))
        .route("/api/telegram/status", get(bot_status))
}

// ─── Send Message ───

#[derive(Deserialize)]
struct SendMessageRequest {
    /// Target chat ID.
    chat_id: i64,
    /// Message text (HTML supported).
    text: String,
}

async fn send_message(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<SendMessageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Operator)?;

    let telegram = get_telegram(&state)?;

    telegram
        .send_message(body.chat_id, &body.text)
        .await
        .map_err(|e| api_error(StatusCode::BAD_GATEWAY, &e))?;

    Ok(Json(serde_json::json!({
        "status": "sent",
        "chat_id": body.chat_id,
    })))
}

// ─── Send Report ───

#[derive(Deserialize)]
struct SendReportRequest {
    chat_id: Option<i64>,
    session_id: String,
    title: String,
    body: String,
    /// Status: completed, failed, running, blocked.
    status: String,
}

async fn send_report(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<SendReportRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Operator)?;

    let telegram = get_telegram(&state)?;

    telegram
        .send_report(
            body.chat_id,
            &body.session_id,
            &body.title,
            &body.body,
            &body.status,
        )
        .await
        .map_err(|e| api_error(StatusCode::BAD_GATEWAY, &e))?;

    state.emit(PipelineEvent {
        session_id: body.session_id.clone(),
        event_type: "telegram_report_sent".into(),
        data: serde_json::json!({
            "title": body.title,
            "status": body.status,
        }),
    });

    Ok(Json(serde_json::json!({
        "status": "sent",
        "session_id": body.session_id,
    })))
}

// ─── Request Approval ───

#[derive(Deserialize)]
struct RequestApprovalBody {
    chat_id: i64,
    session_id: String,
    description: String,
    requested_by: String,
}

async fn request_approval(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<RequestApprovalBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Operator)?;

    let telegram = get_telegram(&state)?;

    let approval_id = uuid::Uuid::new_v4().to_string();

    let request = ApprovalRequest {
        id: approval_id.clone(),
        session_id: body.session_id.clone(),
        chat_id: body.chat_id,
        description: body.description.clone(),
        requested_by: body.requested_by.clone(),
        created_at: chrono::Utc::now().timestamp(),
    };

    let rx = telegram
        .request_approval(request)
        .await
        .map_err(|e| api_error(StatusCode::BAD_GATEWAY, &e))?;

    state.emit(PipelineEvent {
        session_id: body.session_id.clone(),
        event_type: "approval_requested".into(),
        data: serde_json::json!({
            "approval_id": approval_id,
            "description": body.description,
            "requested_by": body.requested_by,
            "chat_id": body.chat_id,
        }),
    });

    // Spawn a task to wait for the approval response and emit an event.
    let events_tx = state.events_tx.clone();
    let session_id = body.session_id.clone();
    let aid = approval_id.clone();
    tokio::spawn(async move {
        match rx.await {
            Ok(result) => {
                let status_str = match result.status {
                    ApprovalStatus::Approved => "approved",
                    ApprovalStatus::Rejected => "rejected",
                    ApprovalStatus::Pending => "pending",
                };
                let _ = events_tx.send(PipelineEvent {
                    session_id,
                    event_type: "approval_resolved".into(),
                    data: serde_json::json!({
                        "approval_id": aid,
                        "status": status_str,
                        "responded_by": result.responded_by,
                    }),
                });
                tracing::info!(
                    approval_id = %aid,
                    status = status_str,
                    "approval resolved via telegram"
                );
            }
            Err(_) => {
                tracing::warn!(approval_id = %aid, "approval channel dropped");
            }
        }
    });

    Ok(Json(serde_json::json!({
        "status": "pending",
        "approval_id": approval_id,
        "chat_id": body.chat_id,
        "message": "Approval request sent to Telegram. Waiting for response.",
    })))
}

// ─── List Approvals ───

async fn list_approvals(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

    let telegram = get_telegram(&state)?;

    let pending_count = telegram.pending_approval_count().await;
    let completed = telegram.completed_approvals().await;

    Ok(Json(serde_json::json!({
        "pending_count": pending_count,
        "completed": completed,
        "completed_count": completed.len(),
    })))
}

// ─── Bot Status ───

async fn bot_status(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

    let telegram = get_telegram(&state)?;

    match telegram.get_me().await {
        Ok(bot) => Ok(Json(serde_json::json!({
            "status": "connected",
            "bot": {
                "id": bot.id,
                "username": bot.username,
                "first_name": bot.first_name,
            },
            "pending_approvals": telegram.pending_approval_count().await,
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "status": "error",
            "error": e,
        }))),
    }
}

// ─── Helpers ───

fn get_telegram(
    state: &AppState,
) -> Result<&Arc<TelegramPlugin>, (StatusCode, Json<serde_json::Value>)> {
    state.telegram.as_ref().ok_or_else(|| {
        api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "telegram channel not configured — set [telegram] in workspace.toml",
        )
    })
}

fn api_error(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}
