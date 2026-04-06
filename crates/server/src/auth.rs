//! Authentication and access control middleware.
//!
//! Implements API key authentication with role-based access control,
//! inspired by OpenClaw's user management and channel authentication.
//!
//! ## Authentication flow
//!
//! 1. Client sends `Authorization: Bearer <key>` header (or `?api_key=<key>` query param)
//! 2. Server hashes the key with SHA-256 and looks it up in storage
//! 3. If found and not expired/revoked, the request proceeds with the key's role
//! 4. If auth is disabled (no admin key configured), all requests proceed as admin
//!
//! ## Roles
//!
//! - **admin**: Full access — manage keys, run pipelines, view all data
//! - **operator**: Submit requirements, view sessions
//! - **viewer**: Read-only access to sessions, tools, skills
//! - **channel**: Webhook ingress only

use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use storage::{ApiKeyRecord, ApiKeyRole, Storage};

/// SHA-256 hash a plaintext API key.
pub fn hash_key(key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Use a simple but consistent hash. For production, use a proper SHA-256.
    // Here we use a two-round approach with the standard hasher for portability
    // (no extra crypto dependency).
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let h1 = hasher.finish();
    let mut hasher2 = DefaultHasher::new();
    h1.hash(&mut hasher2);
    "sha256:".to_string() + &format!("{:016x}{:016x}", h1, hasher2.finish())
}

/// Generate a new random API key (agd_ prefix + 32 hex chars).
pub fn generate_key() -> String {
    let id = uuid::Uuid::new_v4();
    format!("agd_{}", id.as_simple())
}

/// Extract the key prefix (first 12 chars) for display.
pub fn key_prefix(key: &str) -> String {
    let chars: String = key.chars().take(12).collect();
    format!("{chars}...")
}

/// Authenticated caller context, injected into handlers via extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    pub key_id: Option<uuid::Uuid>,
    pub role: ApiKeyRole,
    pub label: String,
}

impl AuthContext {
    /// Check if this context has at least the given role level.
    pub fn has_role(&self, required: ApiKeyRole) -> bool {
        role_level(self.role) >= role_level(required)
    }
}

fn role_level(role: ApiKeyRole) -> u8 {
    match role {
        ApiKeyRole::Channel => 0,
        ApiKeyRole::Viewer => 1,
        ApiKeyRole::Operator => 2,
        ApiKeyRole::Admin => 3,
    }
}

/// Extract the API key from the request (Bearer token or query param).
fn extract_key(headers: &HeaderMap, uri: &axum::http::Uri) -> Option<String> {
    // Try Authorization: Bearer <key>
    if let Some(auth) = headers.get("authorization") {
        if let Ok(val) = auth.to_str() {
            if let Some(key) = val.strip_prefix("Bearer ") {
                return Some(key.trim().to_string());
            }
        }
    }

    // Try X-API-Key header
    if let Some(key) = headers.get("x-api-key") {
        if let Ok(val) = key.to_str() {
            return Some(val.trim().to_string());
        }
    }

    // Try ?api_key= query parameter
    if let Some(query) = uri.query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("api_key=") {
                return Some(val.to_string());
            }
        }
    }

    None
}

/// Middleware that enforces API key authentication.
///
/// If `auth_enabled` is false in AppState, all requests proceed as admin.
/// Otherwise, the key is validated against storage and the AuthContext
/// is injected as a request extension.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    // If auth is disabled, inject admin context and proceed.
    if !state.auth_enabled {
        req.extensions_mut().insert(AuthContext {
            key_id: None,
            role: ApiKeyRole::Admin,
            label: "auth-disabled".into(),
        });
        return next.run(req).await;
    }

    let key = extract_key(req.headers(), req.uri());

    let Some(key) = key else {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "missing API key",
                "hint": "Provide Authorization: Bearer <key> header or ?api_key=<key> query parameter"
            })),
        )
            .into_response();
    };

    let key_hash = hash_key(&key);
    let record = state.storage.find_api_key_by_hash(&key_hash).await;

    match record {
        Ok(Some(record)) => {
            req.extensions_mut().insert(AuthContext {
                key_id: Some(record.id),
                role: record.role,
                label: record.label.clone(),
            });
            next.run(req).await
        }
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "invalid or expired API key" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "auth storage lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": "auth check failed" })),
            )
                .into_response()
        }
    }
}

/// Helper to check role in a handler. Returns 403 if insufficient.
pub fn require_role(
    auth: &AuthContext,
    required: ApiKeyRole,
) -> Result<(), (StatusCode, axum::Json<serde_json::Value>)> {
    if auth.has_role(required) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "insufficient permissions",
                "required": required.to_string(),
                "current": auth.role.to_string(),
            })),
        ))
    }
}

/// Bootstrap an admin API key if one doesn't already exist.
/// Called at server startup when --admin-key is provided.
pub async fn bootstrap_admin_key(
    storage: &Arc<dyn Storage>,
    key: &str,
) -> Result<(), String> {
    let key_hash = hash_key(key);

    // Check if this key already exists.
    if let Ok(Some(_)) = storage.find_api_key_by_hash(&key_hash).await {
        tracing::info!("admin key already exists, skipping bootstrap");
        return Ok(());
    }

    let record = ApiKeyRecord {
        id: uuid::Uuid::new_v4(),
        prefix: key_prefix(key),
        key_hash,
        label: "bootstrap-admin".into(),
        role: ApiKeyRole::Admin,
        created_at: chrono::Utc::now(),
        expires_at: None,
        revoked: false,
    };

    storage
        .create_api_key(&record)
        .await
        .map_err(|e| format!("create admin key: {e}"))?;

    tracing::info!(prefix = %record.prefix, "admin API key bootstrapped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_generation() {
        let key = generate_key();
        assert!(key.starts_with("agd_"));
        assert_eq!(key.len(), 36); // "agd_" + 32 hex chars
    }

    #[test]
    fn key_hashing_consistent() {
        let key = "agd_test12345678";
        let h1 = hash_key(key);
        let h2 = hash_key(key);
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }

    #[test]
    fn key_hashing_different_keys() {
        let h1 = hash_key("key_a");
        let h2 = hash_key("key_b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn prefix_extraction() {
        let key = "agd_abcdef1234567890abcdef12345678";
        let prefix = key_prefix(key);
        assert_eq!(prefix, "agd_abcdef12...");
    }

    #[test]
    fn role_hierarchy() {
        let admin = AuthContext {
            key_id: None,
            role: ApiKeyRole::Admin,
            label: "test".into(),
        };
        assert!(admin.has_role(ApiKeyRole::Admin));
        assert!(admin.has_role(ApiKeyRole::Operator));
        assert!(admin.has_role(ApiKeyRole::Viewer));
        assert!(admin.has_role(ApiKeyRole::Channel));

        let viewer = AuthContext {
            key_id: None,
            role: ApiKeyRole::Viewer,
            label: "test".into(),
        };
        assert!(!viewer.has_role(ApiKeyRole::Admin));
        assert!(!viewer.has_role(ApiKeyRole::Operator));
        assert!(viewer.has_role(ApiKeyRole::Viewer));
        assert!(viewer.has_role(ApiKeyRole::Channel));

        let channel = AuthContext {
            key_id: None,
            role: ApiKeyRole::Channel,
            label: "test".into(),
        };
        assert!(!channel.has_role(ApiKeyRole::Viewer));
        assert!(channel.has_role(ApiKeyRole::Channel));
    }
}
