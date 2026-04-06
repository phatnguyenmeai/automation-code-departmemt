# Axum Patterns Reference

## Middleware Stack

```rust
use axum::middleware;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

let app = Router::new()
    .merge(api_routes())
    .layer(TraceLayer::new_for_http())
    .layer(CorsLayer::permissive())
    .layer(middleware::from_fn(auth_middleware));
```

## Extractors

| Extractor | Use |
|-----------|-----|
| `Path(id)` | URL path parameters |
| `Query(params)` | Query string `?key=val` |
| `Json(body)` | JSON request body |
| `State(state)` | Shared application state |
| `Extension(ext)` | Request-scoped extensions |
| `TypedHeader(h)` | Typed HTTP headers |

## Authentication Middleware

```rust
async fn auth_middleware(
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = verify_jwt(token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}
```

## Pagination

```rust
#[derive(Debug, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}
```

## Graceful Shutdown

```rust
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("install ctrl+c handler");
    tracing::info!("shutdown signal received");
}
```
