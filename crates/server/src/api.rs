//! REST API routes for the gateway server.
//!
//! Endpoints:
//! - POST /api/run          — Submit a new requirement
//! - GET  /api/sessions     — List past sessions
//! - GET  /api/sessions/:id — Get session details + messages
//! - GET  /api/tools        — List available tools
//! - GET  /api/skills       — List registered skills
//! - GET  /api/health       — Health check

use crate::state::{AppState, PipelineEvent};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/run", post(submit_requirement))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/tools", get(list_tools))
        .route("/api/skills", get(list_skills))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "agentdept-gateway" }))
}

#[derive(Deserialize)]
struct RunRequest {
    requirement: String,
    workspace_id: Option<String>,
}

async fn submit_requirement(
    State(state): State<AppState>,
    Json(body): Json<RunRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let session_id = uuid::Uuid::new_v4();
    let workspace_id = body.workspace_id.as_deref().unwrap_or("default");

    state
        .storage
        .create_session(session_id, workspace_id, Some(&body.requirement))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("storage: {e}")))?;

    // Emit event for WebSocket subscribers.
    state.emit(PipelineEvent {
        session_id: session_id.to_string(),
        event_type: "session_created".into(),
        data: serde_json::json!({
            "requirement": body.requirement,
            "workspace_id": workspace_id,
        }),
    });

    Ok(Json(serde_json::json!({
        "session_id": session_id.to_string(),
        "status": "created",
        "message": "Pipeline queued. Connect to /ws for real-time updates.",
    })))
}

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list_sessions(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let sessions = state
        .storage
        .list_sessions(params.limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("storage: {e}")))?;

    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "count": sessions.len(),
    })))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let uuid: uuid::Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid UUID".into()))?;

    let session = state
        .storage
        .load_session(uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("session: {e}")))?;

    let messages = state
        .storage
        .load_messages(uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("messages: {e}")))?;

    Ok(Json(serde_json::json!({
        "session": session,
        "messages": messages.iter().map(|m| serde_json::json!({
            "id": m.id.0.to_string(),
            "from": m.from.as_str(),
            "to": m.to.as_str(),
            "kind": serde_json::to_value(&m.kind).unwrap_or_default(),
            "priority": serde_json::to_value(&m.priority).unwrap_or_default(),
        })).collect::<Vec<_>>(),
        "message_count": messages.len(),
    })))
}

async fn list_tools(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "tools": state.tool_registry.all_schemas(),
        "count": state.tool_registry.list().len(),
    }))
}

async fn list_skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let registry = state.skill_registry.lock().await;
    let skills: Vec<_> = registry
        .list()
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "description": s.description,
                "version": s.version,
                "tools": s.tools,
                "tags": s.tags,
            })
        })
        .collect();
    Json(serde_json::json!({
        "skills": skills,
        "count": skills.len(),
    }))
}
