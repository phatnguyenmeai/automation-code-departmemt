# Rust Error Handling Reference

## Library vs Application Errors

### Library errors (use `thiserror`)
```rust
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("entity not found: {entity_type} with id {id}")]
    NotFound { entity_type: &'static str, id: String },
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
}
```

### Application errors (use `anyhow`)
```rust
use anyhow::{Context, Result};

async fn process_order(id: &str) -> Result<Order> {
    let order = repo.find(id)
        .await
        .context("failed to fetch order")?
        .ok_or_else(|| anyhow::anyhow!("order {id} not found"))?;
    Ok(order)
}
```

## Error Conversion Chain

```
DomainError → AppError → HTTP Response
   (thiserror)    (thiserror + IntoResponse)
```

## Validation Pattern

```rust
pub fn validate_email(email: &str) -> Result<(), DomainError> {
    if !email.contains('@') || email.len() < 5 {
        return Err(DomainError::Validation(format!("invalid email: {email}")));
    }
    Ok(())
}
```
