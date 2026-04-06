#!/bin/bash
# Scaffold a new Rust backend service crate within the workspace.
# Usage: ./scaffold-service.sh <service-name>
#
# Creates:
#   crates/<service-name>/
#   ├── Cargo.toml
#   └── src/
#       ├── lib.rs
#       ├── handlers.rs
#       ├── models.rs
#       ├── errors.rs
#       └── routes.rs

set -euo pipefail

SERVICE_NAME="${1:?Usage: scaffold-service.sh <service-name>}"
CRATE_DIR="crates/${SERVICE_NAME}"

if [ -d "$CRATE_DIR" ]; then
    echo "Error: $CRATE_DIR already exists"
    exit 1
fi

echo "Creating service crate: $SERVICE_NAME"

mkdir -p "$CRATE_DIR/src"

# Cargo.toml
cat > "$CRATE_DIR/Cargo.toml" << EOF
[package]
name = "${SERVICE_NAME}"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
axum.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
async-trait.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
EOF

# lib.rs
cat > "$CRATE_DIR/src/lib.rs" << 'EOF'
pub mod errors;
pub mod handlers;
pub mod models;
pub mod routes;
EOF

# errors.rs
cat > "$CRATE_DIR/src/errors.rs" << 'EOF'
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::Validation(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(serde_json::json!({"error": msg}))).into_response()
    }
}
EOF

# models.rs
cat > "$CRATE_DIR/src/models.rs" << 'EOF'
use serde::{Deserialize, Serialize};

/// Example model — replace with your domain types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateItemRequest {
    pub name: String,
}
EOF

# handlers.rs
cat > "$CRATE_DIR/src/handlers.rs" << 'EOF'
use axum::extract::Path;
use axum::Json;

use crate::errors::AppError;
use crate::models::{CreateItemRequest, Item};

pub async fn list_items() -> Result<Json<Vec<Item>>, AppError> {
    // TODO: implement
    Ok(Json(vec![]))
}

pub async fn get_item(Path(id): Path<String>) -> Result<Json<Item>, AppError> {
    // TODO: implement
    Err(AppError::NotFound(format!("item {id} not found")))
}

pub async fn create_item(
    Json(req): Json<CreateItemRequest>,
) -> Result<Json<Item>, AppError> {
    // TODO: implement
    let _ = req;
    Err(AppError::Internal(anyhow::anyhow!("not implemented")))
}
EOF

# routes.rs
cat > "$CRATE_DIR/src/routes.rs" << 'EOF'
use axum::{routing::get, Router};
use crate::handlers;

pub fn routes() -> Router {
    Router::new()
        .route("/api/v1/items", get(handlers::list_items).post(handlers::create_item))
        .route("/api/v1/items/:id", get(handlers::get_item))
}
EOF

echo "Service '$SERVICE_NAME' scaffolded at $CRATE_DIR"
echo "Don't forget to add it to the workspace members in Cargo.toml"
