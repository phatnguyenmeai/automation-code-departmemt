//! WebSocket endpoint for real-time pipeline event streaming.
//!
//! Clients connect to `/ws` and receive JSON-encoded PipelineEvents
//! as agents process tasks. Inspired by OpenClaw's real-time event system.

use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
    routing::get,
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.events_tx.subscribe();
    tracing::info!("websocket client connected");

    // Send a welcome message.
    let welcome = serde_json::json!({
        "type": "connected",
        "message": "Connected to agentdept gateway. You will receive pipeline events.",
    });
    if socket
        .send(Message::Text(serde_json::to_string(&welcome).unwrap_or_default().into()))
        .await
        .is_err()
    {
        return;
    }

    // Forward broadcast events to this client.
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(evt) => {
                        let json = serde_json::to_string(&evt).unwrap_or_default();
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "websocket client lagged, skipping events");
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    _ => {} // ignore other client messages
                }
            }
        }
    }

    tracing::info!("websocket client disconnected");
}
