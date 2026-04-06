//! Always-on HTTP/WebSocket gateway server, inspired by OpenClaw's gateway.
//!
//! Provides:
//! - REST API for submitting requirements and querying sessions
//! - WebSocket endpoint for real-time pipeline events
//! - Webhook endpoints for channel integrations (Slack, Discord, etc.)
//! - Embedded web UI dashboard
//!
//! The server binds to a configurable port (default 18789, matching
//! OpenClaw's convention) and keeps the gateway running continuously.

pub mod api;
pub mod channels;
pub mod state;
pub mod ui;
pub mod ws;

use axum::Router;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

/// Build the full application router.
pub fn build_router(state: state::AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(api::routes())
        .merge(channels::routes())
        .merge(ws::routes())
        .merge(ui::routes())
        .layer(cors)
        .with_state(state)
}

/// Start the server on the given address.
pub async fn serve(state: state::AppState, addr: SocketAddr) -> std::io::Result<()> {
    let app = build_router(state);
    tracing::info!(%addr, "gateway server listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}
