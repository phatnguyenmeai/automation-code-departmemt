//! REST API routes for the gateway server.
//!
//! Endpoints:
//! - GET  /api/health         — Health check (public)
//! - POST /api/run            — Submit a new requirement (operator+)
//! - GET  /api/sessions       — List past sessions (viewer+)
//! - GET  /api/sessions/:id   — Get session details + messages (viewer+)
//! - GET  /api/tools          — List available tools (viewer+)
//! - GET  /api/skills         — List registered skills (viewer+)
//! - POST /api/keys           — Create a new API key (admin)
//! - GET  /api/keys           — List API keys (admin)
//! - DELETE /api/keys/:id     — Revoke an API key (admin)
//! - GET  /api/auth/me        — Show current auth context (any authenticated)

use crate::auth::{self, AuthContext};
use crate::state::{AppState, PipelineEvent};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use storage::{ApiKeyRecord, ApiKeyRole};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Public
        .route("/api/health", get(health))
        // Authenticated
        .route("/api/run", post(submit_requirement))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/tools", get(list_tools))
        .route("/api/skills", get(list_skills))
        .route("/api/auth/me", get(auth_me))
        // Admin only
        .route("/api/keys", post(create_key))
        .route("/api/keys", get(list_keys))
        .route("/api/keys/{id}", delete(revoke_key))
}

// ─── Public ───

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "agentdept-gateway" }))
}

// ─── Auth info ───

async fn auth_me(
    Extension(auth): Extension<AuthContext>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "key_id": auth.key_id.map(|id| id.to_string()),
        "role": auth.role,
        "label": auth.label,
    }))
}

// ─── Pipeline ───

#[derive(Deserialize)]
struct RunRequest {
    requirement: String,
    workspace_id: Option<String>,
}

async fn submit_requirement(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<RunRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Operator)?;

    let session_id = uuid::Uuid::new_v4();
    let workspace_id = body.workspace_id.as_deref().unwrap_or("default");

    state
        .storage
        .create_session(session_id, workspace_id, Some(&body.requirement))
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("storage: {e}") })),
        ))?;

    state.emit(PipelineEvent {
        session_id: session_id.to_string(),
        event_type: "session_created".into(),
        data: serde_json::json!({
            "requirement": body.requirement,
            "workspace_id": workspace_id,
            "created_by": auth.label,
        }),
    });

    Ok(Json(serde_json::json!({
        "session_id": session_id.to_string(),
        "status": "created",
        "message": "Pipeline queued. Connect to /ws for real-time updates.",
    })))
}

// ─── Sessions ───

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
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

    let sessions = state
        .storage
        .list_sessions(params.limit)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("storage: {e}") })),
        ))?;

    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "count": sessions.len(),
    })))
}

async fn get_session(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

    let uuid: uuid::Uuid = id.parse().map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "invalid UUID" })),
    ))?;

    let session = state
        .storage
        .load_session(uuid)
        .await
        .map_err(|e| (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("session: {e}") })),
        ))?;

    let messages = state
        .storage
        .load_messages(uuid)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("messages: {e}") })),
        ))?;

    Ok(Json(serde_json::json!({
        "session": session,
        "messages": messages.iter().map(|m| serde_json::json!({
            "id": m.id.0.to_string(),
            "from": m.from.as_str(),
            "to": m.to.as_str(),
            "kind": serde_json::to_value(&m.kind).unwrap_or_default(),
            "payload": &m.payload,
            "priority": serde_json::to_value(&m.priority).unwrap_or_default(),
        })).collect::<Vec<_>>(),
        "message_count": messages.len(),
    })))
}

// ─── Tools & Skills ───

async fn list_tools(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

    Ok(Json(serde_json::json!({
        "tools": state.tool_registry.all_schemas(),
        "count": state.tool_registry.list().len(),
    })))
}

async fn list_skills(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth, ApiKeyRole::Viewer)?;

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
    Ok(Json(serde_json::json!({
        "skills": skills,
        "count": skills.len(),
    })))
}

// ─── API Key Management (admin only) ───

#[derive(Deserialize)]
struct CreateKeyRequest {
    label: String,
    role: String,
    /// Optional expiry in hours from now.
    expires_in_hours: Option<u64>,
}

async fn create_key(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
    Json(body): Json<CreateKeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth_ctx, ApiKeyRole::Admin)?;

    let role: ApiKeyRole = body.role.parse().map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "invalid role",
            "valid_roles": ["admin", "operator", "viewer", "channel"],
        })),
    ))?;

    let plaintext_key = auth::generate_key();
    let key_hash = auth::hash_key(&plaintext_key);
    let prefix = auth::key_prefix(&plaintext_key);

    let expires_at = body
        .expires_in_hours
        .map(|h| chrono::Utc::now() + chrono::Duration::hours(h as i64));

    let record = ApiKeyRecord {
        id: uuid::Uuid::new_v4(),
        prefix: prefix.clone(),
        key_hash,
        label: body.label.clone(),
        role,
        created_at: chrono::Utc::now(),
        expires_at,
        revoked: false,
    };

    state
        .storage
        .create_api_key(&record)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("storage: {e}") })),
        ))?;

    tracing::info!(
        key_prefix = %prefix,
        role = %role,
        label = %body.label,
        "API key created"
    );

    // Return the plaintext key ONCE. It cannot be retrieved again.
    Ok(Json(serde_json::json!({
        "id": record.id.to_string(),
        "key": plaintext_key,
        "prefix": prefix,
        "label": body.label,
        "role": role.to_string(),
        "expires_at": expires_at,
        "warning": "Store this key securely. It cannot be retrieved again.",
    })))
}

async fn list_keys(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth_ctx, ApiKeyRole::Admin)?;

    let keys = state
        .storage
        .list_api_keys()
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("storage: {e}") })),
        ))?;

    Ok(Json(serde_json::json!({
        "keys": keys,
        "count": keys.len(),
    })))
}

async fn revoke_key(
    State(state): State<AppState>,
    Extension(auth_ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    auth::require_role(&auth_ctx, ApiKeyRole::Admin)?;

    let uuid: uuid::Uuid = id.parse().map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "invalid UUID" })),
    ))?;

    state
        .storage
        .revoke_api_key(uuid)
        .await
        .map_err(|e| (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("revoke: {e}") })),
        ))?;

    tracing::info!(key_id = %uuid, "API key revoked");

    Ok(Json(serde_json::json!({
        "status": "revoked",
        "id": uuid.to_string(),
    })))
}
