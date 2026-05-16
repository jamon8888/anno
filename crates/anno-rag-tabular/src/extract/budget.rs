//! Column-batch splitter: partitions a Vec<Column> into sub-batches
//! that each fit within a target token budget when combined with the
//! doc body and the JSON-schema envelope.
//!
//! ## Approach
//!
//! We don't have a real tokenizer for Anthropic models in this crate
//! (would pull in `tiktoken`-style heavyweight deps). Instead we use a
//! crude byte→token heuristic: roughly 1 token per 3.5 bytes for
//! English/French legal text. This overestimates token count by ~10%
//! which is the safer direction.
//!
//! The splitter is greedy: it walks columns in order, building a
//! batch until adding the next column would exceed the budget, then
//! starts a fresh batch. This preserves display order across batches
//! (cells from batch[0] and batch[1] are merged by column id at the
//! caller).
//!
//! Conservative defaults:
//! - `DEFAULT_BUDGET_TOKENS = 80_000` — leaves ~120k headroom under
//!   Anthropic's 200k context window for the doc body and the system
//!   prompt.
//! - `BYTES_PER_TOKEN = 4` — slightly more conservative than the 3.5
//!   heuristic; rounds up.

use crate::schema::{json_schema, Column};

/// Default per-call token budget for the column-side of the prompt
/// (schema + per-column instructions). Leaves headroom under
/// Anthropic's 200k window for the doc body and system prompt.
pub const DEFAULT_BUDGET_TOKENS: usize = 80_000;

/// Crude byte→token conversion factor. Slightly more conservative than
/// the empirical 3.5 ratio so we overestimate (safer direction).
pub const BYTES_PER_TOKEN: usize = 4;

/// Fixed-overhead token count per column to account for the
/// `[COLUMN::...]...[/COLUMN]` markers and per-column schema envelope
/// boilerplate not captured by the JSON-schema serialization alone.
const PER_COLUMN_OVERHEAD: usize = 20;

/// Token reserve for the system prompt and JSON-schema envelope
/// scaffolding outside individual column contributions.
const ENVELOPE_OVERHEAD: usize = 500;

/// Estimate the token cost of one column's contribution to the
/// prompt: its name + prompt + JSON-schema fragment. Used by
/// [`split_columns`] internally; exposed for tests + future tuning.
#[must_use]
pub fn estimate_column_tokens(c: &Column) -> usize {
    let schema = json_schema::for_columns(std::slice::from_ref(c));
    let schema_bytes = schema.to_string().len();
    let prompt_bytes = c.prompt.len() + c.name.len();
    schema_bytes / BYTES_PER_TOKEN + prompt_bytes / BYTES_PER_TOKEN + PER_COLUMN_OVERHEAD
}

/// Estimate the doc-body token cost from a chunk slice. Used by the
/// extractor to subtract the doc cost from the LLM budget before
/// splitting columns.
#[must_use]
pub fn estimate_doc_tokens<C: AsRef<str>>(chunk_contents: impl IntoIterator<Item = C>) -> usize {
    let total_bytes: usize = chunk_contents
        .into_iter()
        .map(|c| c.as_ref().len() + 32) // +32 per chunk for [CHUNK::<uuid>]…[/CHUNK] markers
        .sum();
    total_bytes / BYTES_PER_TOKEN
}

/// Split a column list into batches whose combined schema + prompt
/// cost stays under `budget_tokens`. The `doc_body_tokens` argument
/// is the rough token cost the doc body itself will consume in the
/// user message — subtracted from the budget so each batch leaves
/// room for the document.
///
/// Returns at least one batch for any non-empty column list; for an
/// empty input returns `vec![]` so callers can short-circuit.
///
/// Degenerate cases:
/// - If `doc_body_tokens` already meets or exceeds `budget_tokens`,
///   returns a single batch containing all columns (don't infinitely
///   split — let the LLM error out).
/// - If a single column's cost exceeds the per-batch budget, that
///   column gets its own batch.
#[must_use]
pub fn split_columns(
    columns: &[Column],
    doc_body_tokens: usize,
    budget_tokens: usize,
) -> Vec<Vec<Column>> {
    if columns.is_empty() {
        return vec![];
    }

    // Degenerate: doc already blows the budget — single batch and bail.
    if doc_body_tokens.saturating_add(ENVELOPE_OVERHEAD) >= budget_tokens {
        return vec![columns.to_vec()];
    }

    let per_batch_budget = budget_tokens - doc_body_tokens - ENVELOPE_OVERHEAD;

    let mut batches: Vec<Vec<Column>> = Vec::new();
    let mut current: Vec<Column> = Vec::new();
    let mut current_cost: usize = 0;

    for col in columns {
        let cost = estimate_column_tokens(col);
        // Single column larger than the whole per-batch budget: emit
        // any pending batch, then give this column its own batch.
        if cost >= per_batch_budget {
            if !current.is_empty() {
                batches.push(std::mem::take(&mut current));
                current_cost = 0;
            }
            batches.push(vec![col.clone()]);
            continue;
        }
        // Adding this column would overflow the current batch — flush.
        if !current.is_empty() && current_cost + cost > per_batch_budget {
            batches.push(std::mem::take(&mut current));
            current_cost = 0;
        }
        current.push(col.clone());
        current_cost += cost;
    }

    if !current.is_empty() {
        batches.push(current);
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReviewId;
    use crate::schema::column::ColumnBuilder;
    use crate::schema::CellType;

    fn col(name: &str, prompt_len: usize) -> Column {
        let r = ReviewId::new();
        ColumnBuilder::new(r, name, &"x".repeat(prompt_len), CellType::Text).build()
    }

    #[test]
    fn split_with_room_returns_single_batch() {
        let cols = vec![col("a", 20), col("b", 20), col("c", 20)];
        let out = split_columns(&cols, 1_000, DEFAULT_BUDGET_TOKENS);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 3);
    }

    #[test]
    fn split_returns_multiple_batches_when_columns_overflow() {
        let cols: Vec<Column> = (0..30).map(|i| col(&format!("c{i}"), 50_000)).collect();
        let out = split_columns(&cols, 1_000, 80_000);
        // Sum of batches must equal the input length.
        let total: usize = out.iter().map(|b| b.len()).sum();
        assert_eq!(total, 30);
        assert!(
            out.len() > 1,
            "expected splitting, got {} batch(es)",
            out.len()
        );
        // Column order is preserved across batches.
        let flat: Vec<&str> = out
            .iter()
            .flat_map(|b| b.iter().map(|c| c.name.as_str()))
            .collect();
        let expected: Vec<String> = (0..30).map(|i| format!("c{i}")).collect();
        let expected_refs: Vec<&str> = expected.iter().map(String::as_str).collect();
        assert_eq!(flat, expected_refs);
    }

    #[test]
    fn single_oversized_column_gets_own_batch() {
        let big = col("huge", 200_000);
        let cols = vec![big];
        let out = split_columns(&cols, 1_000, 80_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 1);
        assert_eq!(out[0][0].name, "huge");
    }

    #[test]
    fn empty_columns_returns_empty_vec() {
        let out = split_columns(&[], 1_000, DEFAULT_BUDGET_TOKENS);
        assert!(out.is_empty());
    }

    #[test]
    fn doc_body_dominates_budget_returns_single_batch_anyway() {
        let cols = vec![col("a", 20), col("b", 20)];
        let out = split_columns(&cols, 100_000, 80_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 2);
    }

    #[test]
    fn estimate_column_tokens_scales_with_prompt_length() {
        let small = col("c", 10);
        let big = col("c", 10_000);
        assert!(
            estimate_column_tokens(&big) > estimate_column_tokens(&small),
            "longer prompt must cost more tokens"
        );
    }
}
