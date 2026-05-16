//! Offset / quote round-trip verification.
//!
//! For each citation in a cell, fetch the parent chunk and check:
//!
//! 1. `char_start <= char_end` (well-formed range).
//! 2. `char_end <= chunk.content.len()` (range is in-bounds).
//! 3. `chunk.content[char_start..char_end] == quoted_text` exactly
//!    (the LLM didn't hallucinate the quote).
//!
//! Byte-level comparison is correct because the schema's `char_start`
//! / `char_end` are documented as BYTE offsets, not codepoint offsets.
//! UTF-8 boundary slicing panics on bad indices, so we validate by
//! using `str::get(range)` rather than direct slicing.
//!
//! A single failed citation pulls the whole cell's `confidence` down
//! to `Low` — the audit log gets the per-citation reason via tracing.
//! The cell itself is **not** dropped; surfacing it with `Low`
//! confidence is more useful than silently dropping it.

use crate::error::Result;
use crate::extract::ChunkSource;
use crate::storage::cells::{Cell, Citation, Confidence};
use std::collections::HashMap;
use uuid::Uuid;

/// Verify every citation in `cell` against the source chunks fetched
/// via `chunks`. Mutates `cell.confidence` to `Low` if any citation
/// fails offset/quote round-trip; otherwise leaves it untouched.
///
/// `cell.support_score` is **not** touched here — that's
/// `super::support`'s job (T29).
///
/// # Errors
///
/// Returns the underlying [`Error`](crate::error::Error) from
/// [`ChunkSource::chunks_for_doc`] if chunk fetch fails. Missing
/// chunks (chunk_id in citation has no match in the doc) trigger a
/// confidence downgrade — not an error — since that's data the LLM
/// got wrong, not infra failure.
pub async fn verify_cell_offsets(
    cell: &mut Cell,
    doc_id: Uuid,
    chunks: &dyn ChunkSource,
) -> Result<()> {
    let chunk_list = chunks.chunks_for_doc(doc_id).await?;
    let by_id: HashMap<Uuid, &str> = chunk_list
        .iter()
        .map(|c| (c.id, c.content.as_str()))
        .collect();

    let mut all_pass = true;
    for cite in &cell.citations {
        if !check_citation(cite, &by_id) {
            all_pass = false;
            // Don't break — emit one trace event per failed citation
            // so audit can show every reason.
        }
    }

    if !all_pass {
        cell.confidence = Confidence::Low;
    }
    Ok(())
}

/// Returns `true` iff this citation's offsets + quote round-trip
/// cleanly. Logs a `tracing::warn` describing the failure mode when
/// the result is `false`.
fn check_citation(c: &Citation, chunks: &HashMap<Uuid, &str>) -> bool {
    let Some(content) = chunks.get(&c.chunk_id) else {
        tracing::warn!(
            target: "tabular::verify::offsets",
            chunk_id = %c.chunk_id,
            reason = "unknown_chunk_id",
            "citation refers to chunk not present in doc"
        );
        return false;
    };

    if c.char_start > c.char_end {
        tracing::warn!(
            target: "tabular::verify::offsets",
            chunk_id = %c.chunk_id,
            char_start = c.char_start,
            char_end = c.char_end,
            reason = "start_after_end",
            "citation char range is inverted"
        );
        return false;
    }

    let start = c.char_start as usize;
    let end = c.char_end as usize;
    if end > content.len() {
        tracing::warn!(
            target: "tabular::verify::offsets",
            chunk_id = %c.chunk_id,
            char_end = end,
            chunk_len = content.len(),
            reason = "out_of_bounds",
            "citation char_end exceeds chunk content length"
        );
        return false;
    }

    // `get(range)` returns None if the range doesn't land on UTF-8
    // boundaries; avoids the panic of direct `&content[start..end]`.
    let Some(slice) = content.get(start..end) else {
        tracing::warn!(
            target: "tabular::verify::offsets",
            chunk_id = %c.chunk_id,
            reason = "utf8_boundary",
            "char_start..char_end does not land on UTF-8 boundaries"
        );
        return false;
    };

    if slice != c.quoted_text {
        tracing::warn!(
            target: "tabular::verify::offsets",
            chunk_id = %c.chunk_id,
            reason = "quote_mismatch",
            "citation quoted_text does not match chunk slice at offsets"
        );
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::ChunkRef;
    use crate::ids::{ColumnId, ReviewId, RowId};
    use crate::storage::cells::{Author, Cell, Citation, Confidence};
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Local in-memory chunk source so the test module is
    /// self-contained — the one in `extract::tests` is private.
    struct InMemoryChunks {
        by_doc: HashMap<Uuid, Vec<ChunkRef>>,
    }

    #[async_trait]
    impl ChunkSource for InMemoryChunks {
        async fn chunks_for_doc(&self, doc_id: Uuid) -> Result<Vec<ChunkRef>> {
            Ok(self.by_doc.get(&doc_id).cloned().unwrap_or_default())
        }
    }

    /// Build a one-doc, one-chunk fixture with the given content.
    /// Returns `(doc_id, chunk_id, ChunkSource)`.
    fn fixture(content: &str) -> (Uuid, Uuid, Arc<InMemoryChunks>) {
        let doc_id = Uuid::now_v7();
        let chunk_id = Uuid::now_v7();
        let mut by_doc = HashMap::new();
        by_doc.insert(
            doc_id,
            vec![ChunkRef {
                id: chunk_id,
                doc_id,
                content: content.to_string(),
                page: Some(1),
            }],
        );
        (doc_id, chunk_id, Arc::new(InMemoryChunks { by_doc }))
    }

    /// Build a cell with a single citation and the given starting
    /// confidence. `chunk_id` is the citation target.
    fn cell_with(
        chunk_id: Uuid,
        char_start: u32,
        char_end: u32,
        quoted_text: &str,
        confidence: Confidence,
    ) -> Cell {
        let review = ReviewId::new();
        Cell {
            review_id: review,
            row_id: RowId::for_doc(review, Uuid::now_v7()),
            col_id: ColumnId::for_name(review, "c"),
            value: json!("v"),
            reasoning: None,
            citations: vec![Citation {
                chunk_id,
                char_start,
                char_end,
                quoted_text: quoted_text.into(),
                page: None,
            }],
            support_score: 0.0,
            confidence,
            locked: false,
            version: 1,
            author: Author::System {
                extractor_version: "test".into(),
            },
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn verify_passes_when_quote_matches_offsets() {
        let (doc_id, chunk_id, chunks) = fixture("Hello world");
        let mut cell = cell_with(chunk_id, 0, 5, "Hello", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Medium));
    }

    #[tokio::test]
    async fn verify_downgrades_on_quote_mismatch() {
        let (doc_id, chunk_id, chunks) = fixture("Hello world");
        let mut cell = cell_with(chunk_id, 0, 5, "Hallo", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_downgrades_on_out_of_bounds() {
        let (doc_id, chunk_id, chunks) = fixture("Hello");
        let mut cell = cell_with(chunk_id, 0, 99, "Hello", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_downgrades_on_inverted_range() {
        let (doc_id, chunk_id, chunks) = fixture("Hello world");
        let mut cell = cell_with(chunk_id, 10, 5, "Hello", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_downgrades_on_unknown_chunk_id() {
        let (doc_id, _chunk_id, chunks) = fixture("Hello world");
        let random = Uuid::now_v7();
        let mut cell = cell_with(random, 0, 5, "Hello", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_downgrades_on_utf8_boundary_split() {
        // "café" is 5 bytes: c(1) a(1) f(1) é(2). Byte index 4 falls
        // in the middle of the é → `get(0..4)` returns None.
        let (doc_id, chunk_id, chunks) = fixture("café");
        let mut cell = cell_with(chunk_id, 0, 4, "caf", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_passes_with_multibyte_chars() {
        let (doc_id, chunk_id, chunks) = fixture("café");
        let mut cell = cell_with(chunk_id, 0, 5, "café", Confidence::Medium);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(matches!(cell.confidence, Confidence::Medium));
    }

    #[tokio::test]
    async fn verify_leaves_high_confidence_alone_when_all_citations_pass() {
        let (doc_id, chunk_id, chunks) = fixture("Hello world");
        let mut cell = cell_with(chunk_id, 0, 5, "Hello", Confidence::High);
        verify_cell_offsets(&mut cell, doc_id, chunks.as_ref())
            .await
            .expect("verify ok");
        assert!(
            matches!(cell.confidence, Confidence::High),
            "verifier must never upgrade confidence"
        );
    }
}
