//! Concrete agent implementations: PM, BA, Dev, Frontend, Test.
//!
//! Each agent is a thin adapter that:
//!   1. receives a `TaskMessage`
//!   2. renders a prompt
//!   3. calls Claude
//!   4. parses the response
//!   5. emits follow-up `TaskMessage`s via `AgentOutput::Dispatch`

pub mod ba;
pub mod dev;
pub mod frontend;
pub mod pm;
pub mod test;

pub use ba::BaAgent;
pub use dev::DevAgent;
pub use frontend::FrontendAgent;
pub use pm::PmAgent;
pub use test::TestAgent;

/// Try to extract a JSON block from an LLM response. Accepts:
///   - raw JSON
///   - ```json ... ``` fenced blocks
pub(crate) fn parse_json(text: &str) -> anyhow::Result<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }
    // Look for fenced block.
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        // Strip optional language tag up to newline.
        let body_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after[body_start..];
        if let Some(end) = body.find("```") {
            let json_str = body[..end].trim();
            return Ok(serde_json::from_str(json_str)?);
        }
    }
    anyhow::bail!("no JSON found in response: {}", trimmed)
}
