//! Cross-encoder support scoring.
//!
//! For each citation in a cell, score the `(column.prompt,
//! citation.quoted_text)` pair on a 0..1 scale of "does this quote
//! support the answer to this column?". Bins:
//!
//! - `>= 0.7`  → `Confidence::High`
//! - `0.4..0.7` → `Confidence::Medium`
//! - `< 0.4`   → `Confidence::Low`
//!
//! The cell's `support_score` records the **max** score across its
//! citations — the strongest piece of evidence wins. (Mean would
//! penalize cells that cite both the canonical clause and a
//! peripheral mention; max better reflects "is there any solid
//! evidence?".)
//!
//! Production wire-up uses a cross-encoder model (camembert-L6 or
//! bge-reranker-v2-m3 for FR legal text). v1 ships only the trait
//! and a deterministic mock — `TODO(v1.x): wire a real Candle/ONNX
//! cross-encoder scorer once the model lands in anno-rag.`
//!
//! The trait deliberately mirrors [`ChunkSource`](crate::extract::ChunkSource)
//! in shape: async, `Send + Sync`, takes &self, returns the score
//! the caller asked for. Easy to swap implementations in tests.

use crate::error::Result;
use crate::storage::cells::{Cell, Confidence};
use async_trait::async_trait;

/// Confidence bin threshold for [`Confidence::High`]. Scores at or
/// above this value bin to `High`. Tuned per v1.1 spec §19.
pub const HIGH_THRESHOLD: f32 = 0.7;
/// Confidence bin threshold for [`Confidence::Medium`]. Scores at or
/// above this value (and below [`HIGH_THRESHOLD`]) bin to `Medium`;
/// scores below this value bin to `Low`. Tuned per v1.1 spec §19.
pub const MEDIUM_THRESHOLD: f32 = 0.4;

/// Async source of pairwise relevance scores.
///
/// Implementations score a `(query, passage)` pair on a 0.0..1.0
/// scale. Higher = stronger support. The trait is `Send + Sync` so a
/// single instance can be held by an `Arc` and used across the
/// fanout's tokio tasks.
#[async_trait]
pub trait SupportScorer: Send + Sync {
    /// Score how strongly `passage` supports answering the question
    /// in `query`. Implementations should clamp output to `[0.0, 1.0]`;
    /// [`verify_cell_support`] re-clamps defensively in case an
    /// implementation forgets.
    ///
    /// # Errors
    ///
    /// Returns [`Error`](crate::error::Error) on model-load or
    /// inference failure. Callers in [`verify_cell_support`] treat
    /// such errors as fatal for the verifier pass — the cell keeps
    /// whatever confidence it had before T29 ran.
    async fn score(&self, query: &str, passage: &str) -> Result<f32>;
}

/// Verify a cell against `scorer`. Sets `cell.support_score` to the
/// **max** citation score, and sets `cell.confidence` to the bin
/// derived from that score — but only if the new bin is **stricter**
/// than the existing one (T28 may have already downgraded to Low; we
/// never upgrade past T28's verdict).
///
/// Empty citation list is impossible per the JSON schema's
/// `minItems: 1`, but defensively handled: leaves cell unchanged.
///
/// # Errors
///
/// Propagates `Error` from `scorer.score`. Each scoring call is
/// awaited sequentially per citation — small N per cell, batching
/// optimisations live in a later task.
pub async fn verify_cell_support(
    cell: &mut Cell,
    column_prompt: &str,
    scorer: &dyn SupportScorer,
) -> Result<()> {
    if cell.citations.is_empty() {
        return Ok(());
    }

    let mut best: f32 = 0.0;
    for cite in &cell.citations {
        let s = scorer.score(column_prompt, &cite.quoted_text).await?;
        // Clamp defensively in case an implementation forgets.
        let s = s.clamp(0.0, 1.0);
        if s > best {
            best = s;
        }
    }

    cell.support_score = best;

    // T28's only signal is downgrading to Low on offset/quote
    // failure — it never touches Medium/High. So we honour an
    // existing Low (treat it as a sticky lock) but otherwise let
    // the support bin set the final confidence, including
    // upgrading Medium → High when the cross-encoder is confident.
    cell.confidence = if matches!(cell.confidence, Confidence::Low) {
        Confidence::Low
    } else {
        bin(best)
    };
    Ok(())
}

/// Bin a raw 0..1 support score into a [`Confidence`] level.
#[must_use]
pub fn bin(score: f32) -> Confidence {
    if score >= HIGH_THRESHOLD {
        Confidence::High
    } else if score >= MEDIUM_THRESHOLD {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

/// Deterministic mock for tests. Returns a fixed score regardless of
/// query/passage content.
pub struct MockSupportScorer {
    /// Fixed score the mock returns for every call.
    pub score: f32,
}

impl MockSupportScorer {
    /// Construct a mock that returns `score` for every call.
    #[must_use]
    pub fn new(score: f32) -> Self {
        Self { score }
    }
}

#[async_trait]
impl SupportScorer for MockSupportScorer {
    async fn score(&self, _query: &str, _passage: &str) -> Result<f32> {
        Ok(self.score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ColumnId, ReviewId, RowId};
    use crate::storage::cells::{Author, Cell, Citation, Confidence};
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// Build a cell with `n` citations (all pointing at a random
    /// chunk_id; offsets/quotes don't matter for support scoring) at
    /// the given starting `confidence`.
    fn cell_with_citations(n: usize, confidence: Confidence) -> Cell {
        let review = ReviewId::new();
        let citations = (0..n)
            .map(|i| Citation {
                chunk_id: Uuid::now_v7(),
                char_start: 0,
                char_end: 1,
                quoted_text: format!("quote-{i}"),
                page: None,
            })
            .collect();
        Cell {
            review_id: review,
            row_id: RowId::for_doc(review, Uuid::now_v7()),
            col_id: ColumnId::for_name(review, "c"),
            value: json!("v"),
            reasoning: None,
            citations,
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

    /// Scorer that returns a different score per call, popping the
    /// front of an interior `Vec<f32>`. Panics if asked for more
    /// scores than queued — that surfaces drift between test and
    /// fixture immediately.
    struct ScriptedScorer {
        scores: Mutex<Vec<f32>>,
    }

    impl ScriptedScorer {
        fn new(scores: Vec<f32>) -> Self {
            Self {
                scores: Mutex::new(scores),
            }
        }
    }

    #[async_trait]
    impl SupportScorer for ScriptedScorer {
        async fn score(&self, _query: &str, _passage: &str) -> Result<f32> {
            let mut s = self.scores.lock().unwrap();
            assert!(!s.is_empty(), "ScriptedScorer exhausted");
            Ok(s.remove(0))
        }
    }

    #[test]
    fn bin_thresholds() {
        assert!(matches!(bin(0.0), Confidence::Low));
        assert!(matches!(bin(0.39), Confidence::Low));
        assert!(matches!(bin(0.40), Confidence::Medium));
        assert!(matches!(bin(0.69), Confidence::Medium));
        assert!(matches!(bin(0.70), Confidence::High));
        assert!(matches!(bin(1.0), Confidence::High));
    }

    #[tokio::test]
    async fn verify_sets_support_score_to_max_citation() {
        // Start from Medium (extractor default). 0.5 bins to Medium.
        let mut cell = cell_with_citations(3, Confidence::Medium);
        let scorer = MockSupportScorer::new(0.5);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!((cell.support_score - 0.5).abs() < 1e-6);
        assert!(matches!(cell.confidence, Confidence::Medium));
    }

    #[tokio::test]
    async fn verify_uses_max_across_citations() {
        let mut cell = cell_with_citations(3, Confidence::High);
        let scorer = ScriptedScorer::new(vec![0.2, 0.85, 0.4]);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!((cell.support_score - 0.85).abs() < 1e-6);
        assert!(matches!(cell.confidence, Confidence::High));
    }

    #[tokio::test]
    async fn verify_clamps_above_1_to_1() {
        let mut cell = cell_with_citations(1, Confidence::High);
        let scorer = MockSupportScorer::new(1.5);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!((cell.support_score - 1.0).abs() < 1e-6);
        assert!(matches!(cell.confidence, Confidence::High));
    }

    #[tokio::test]
    async fn verify_clamps_below_0_to_0() {
        let mut cell = cell_with_citations(1, Confidence::High);
        let scorer = MockSupportScorer::new(-0.3);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!(cell.support_score.abs() < 1e-6);
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_never_upgrades_past_t28_low() {
        // T28 already downgraded to Low; high support score must not
        // promote it back.
        let mut cell = cell_with_citations(1, Confidence::Low);
        let scorer = MockSupportScorer::new(0.9);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!((cell.support_score - 0.9).abs() < 1e-6);
        assert!(
            matches!(cell.confidence, Confidence::Low),
            "support must never upgrade past T28 Low"
        );
    }

    #[tokio::test]
    async fn verify_can_downgrade_from_high() {
        let mut cell = cell_with_citations(1, Confidence::High);
        let scorer = MockSupportScorer::new(0.1);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!((cell.support_score - 0.1).abs() < 1e-6);
        assert!(matches!(cell.confidence, Confidence::Low));
    }

    #[tokio::test]
    async fn verify_empty_citations_no_op() {
        let mut cell = cell_with_citations(0, Confidence::Medium);
        cell.support_score = 0.42;
        let scorer = MockSupportScorer::new(0.99);
        verify_cell_support(&mut cell, "Q?", &scorer)
            .await
            .expect("scorer ok");
        assert!(
            (cell.support_score - 0.42).abs() < 1e-6,
            "support_score must be unchanged when citations empty"
        );
        assert!(matches!(cell.confidence, Confidence::Medium));
    }
}
