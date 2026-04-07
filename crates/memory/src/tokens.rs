//! Token counting utilities for context budget management.
//!
//! Uses a simple heuristic (chars / 4) as a Claude-compatible approximation.
//! This avoids pulling in heavy tokenizer dependencies while providing
//! good-enough estimates for budget management.

use crate::MemoryEntry;

/// Estimate token count for a text string.
///
/// Uses the heuristic of ~4 characters per token, which is a reasonable
/// approximation for English text with Claude's tokenizer.
pub fn estimate_tokens(text: &str) -> usize {
    // ~4 chars per token is a standard approximation for BPE tokenizers.
    // We add a small buffer (10%) to avoid underestimation.
    let raw = text.len() / 4;
    raw + raw / 10
}

/// Estimate token count for a JSON value (serialized form).
pub fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    let text = serde_json::to_string(value).unwrap_or_default();
    estimate_tokens(&text)
}

/// Select entries that fit within a token budget using greedy selection.
///
/// Returns references to entries that fit, preserving their order.
/// Entries are consumed in order until the budget is exhausted.
pub fn fits_budget<'a>(entries: &'a [MemoryEntry], budget: usize) -> Vec<&'a MemoryEntry> {
    let mut remaining = budget;
    let mut selected = Vec::new();
    for entry in entries {
        if entry.token_estimate <= remaining {
            remaining -= entry.token_estimate;
            selected.push(entry);
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        // 100 chars → ~25 tokens, + 10% → ~27
        let text = "a".repeat(100);
        let est = estimate_tokens(&text);
        assert!(est >= 25 && est <= 30, "got {est}");
    }

    #[test]
    fn estimate_json_tokens_basic() {
        let val = serde_json::json!({"key": "value", "number": 42});
        let est = estimate_json_tokens(&val);
        assert!(est > 0);
    }

    #[test]
    fn fits_budget_selects_within_limit() {
        use chrono::Utc;
        let entries = vec![
            MemoryEntry {
                source: crate::MemorySource::ShortTerm,
                content: serde_json::json!("entry1"),
                token_estimate: 100,
                relevance: 1.0,
                created_at: Utc::now(),
            },
            MemoryEntry {
                source: crate::MemorySource::ShortTerm,
                content: serde_json::json!("entry2"),
                token_estimate: 200,
                relevance: 0.9,
                created_at: Utc::now(),
            },
            MemoryEntry {
                source: crate::MemorySource::ShortTerm,
                content: serde_json::json!("entry3"),
                token_estimate: 300,
                relevance: 0.8,
                created_at: Utc::now(),
            },
        ];

        let selected = fits_budget(&entries, 250);
        assert_eq!(selected.len(), 1); // only first fits (100 <= 250, 200 would make 300 > 250)

        let selected = fits_budget(&entries, 350);
        assert_eq!(selected.len(), 2); // first two fit (100 + 200 = 300 <= 350)
    }

    #[test]
    fn empty_text_zero_tokens() {
        assert_eq!(estimate_tokens(""), 0);
    }
}
