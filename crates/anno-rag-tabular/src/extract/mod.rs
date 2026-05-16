//! Extraction engine — turn a document's chunks into `Vec<Cell>` via
//! one or more LLM calls.
//!
//! Entry point: [`Extractor::extract_doc`]. It fetches the document's
//! chunks from a [`ChunkSource`] and delegates the LLM round-trip +
//! response parsing to [`batch::extract_batch`].
//!
//! ## Why a [`ChunkSource`] trait?
//!
//! The plan called for a direct dep on `anno-rag-store`; the workspace
//! has a single `anno-rag` crate whose [`anno_rag::store::Store`] does
//! not yet expose a "give me all chunks for this doc_id" method (it
//! exposes hybrid search + raw upsert). Rather than bolting an ad-hoc
//! query onto `Store` for this one caller, this module defines a small
//! trait the engine takes by `Arc`. Tests use an in-memory impl; v1.x
//! will gain an `AnnoRagChunkSource` adapter (see TODO at the bottom of
//! this file).
//!
//! The pre-verifier cells returned by [`Extractor::extract_doc`] carry
//! placeholder `support_score = 0.0` and `confidence = Medium`. The
//! verifier (Phase 6) overwrites both fields once it has cross-encoded
//! each citation.

pub(crate) mod batch;

use crate::error::Result;
use crate::ids::ReviewId;
use crate::llm::LlmClient;
use crate::schema::Column;
use crate::storage::cells::Cell;
use async_trait::async_trait;
use std::sync::Arc;

/// One chunk handed to the extractor. Pared-down view of
/// `anno_rag::store::ChunkRecord` / `SearchHit` — the extraction engine
/// only needs the chunk id, the text body, and (for paginated sources)
/// the page number.
#[derive(Debug, Clone)]
pub struct ChunkRef {
    /// Deterministic chunk UUID (matches `anno_rag::store::SearchHit::chunk_id`).
    pub id: uuid::Uuid,
    /// Owning document id.
    pub doc_id: uuid::Uuid,
    /// Pseudonymized chunk text. The LLM sees this verbatim, wrapped in
    /// `[CHUNK::<id>]…[/CHUNK]` markers.
    pub content: String,
    /// Page number for paginated sources (`None` for free-form text).
    pub page: Option<u32>,
}

/// Source of chunks for the extraction engine. Abstracted as a trait so
/// the engine can be unit-tested against an in-memory fixture and later
/// pointed at the real `anno_rag::store::Store` via an adapter.
#[async_trait]
pub trait ChunkSource: Send + Sync {
    /// Return every chunk belonging to `doc_id`, in document order if
    /// possible (caller is allowed to feed them to the LLM as-is).
    ///
    /// # Errors
    ///
    /// Returns a [`crate::error::Error`] wrapping the underlying store
    /// failure.
    async fn chunks_for_doc(&self, doc_id: uuid::Uuid) -> Result<Vec<ChunkRef>>;
}

/// Top-level extraction engine. Owns the LLM client and a chunk source;
/// cheap to clone (both fields are `Arc`).
#[derive(Clone)]
pub struct Extractor {
    llm: Arc<dyn LlmClient>,
    chunks: Arc<dyn ChunkSource>,
}

impl Extractor {
    /// Build an extractor over an [`LlmClient`] and a [`ChunkSource`].
    #[must_use]
    pub fn new(llm: Arc<dyn LlmClient>, chunks: Arc<dyn ChunkSource>) -> Self {
        Self { llm, chunks }
    }

    /// Extract every non-manual column for `doc_id` into a `Vec<Cell>`.
    ///
    /// Cells are pre-verifier: `support_score = 0.0`, `confidence =
    /// Medium`, `version = 1`, `author = Author::System { extractor_version
    /// = llm.model_id() }`. The caller (Phase 6 verifier) reruns each
    /// citation through the cross-encoder and rewrites the score
    /// before the upsert hits LanceDB.
    ///
    /// # Errors
    ///
    /// - [`crate::error::Error::Core`] / underlying store error if
    ///   chunks can't be fetched.
    /// - [`crate::error::Error::Extract`] if the LLM call or response
    ///   parse fails (see [`batch::extract_batch`]).
    pub async fn extract_doc(
        &self,
        review_id: ReviewId,
        doc_id: uuid::Uuid,
        columns: &[Column],
    ) -> Result<Vec<Cell>> {
        let chunks = self.chunks.chunks_for_doc(doc_id).await?;
        // TODO(T24): split `columns` into batches when the assembled
        // prompt exceeds ~80k tokens. Today everything goes in one call.
        batch::extract_batch(self.llm.as_ref(), review_id, doc_id, &chunks, columns).await
    }
}

// TODO(future): provide `AnnoRagChunkSource` that wraps
// `anno_rag::store::Store` once that crate grows a `chunks_for_doc`
// query path. For v1 the in-memory impl in tests is sufficient and the
// public `ChunkSource` trait lets downstream code plug in whatever it
// likes.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ColumnId;
    use crate::llm::mock::MockLlm;
    use crate::schema::column::ColumnBuilder;
    use crate::schema::CellType;
    use crate::storage::cells::{Author, Confidence};
    use serde_json::json;

    /// Tiny in-memory `ChunkSource` for tests. Mapping is doc-id → its
    /// chunks; any other doc-id returns an empty Vec.
    struct InMemoryChunks {
        by_doc: std::collections::HashMap<uuid::Uuid, Vec<ChunkRef>>,
    }

    #[async_trait]
    impl ChunkSource for InMemoryChunks {
        async fn chunks_for_doc(&self, doc_id: uuid::Uuid) -> Result<Vec<ChunkRef>> {
            Ok(self.by_doc.get(&doc_id).cloned().unwrap_or_default())
        }
    }

    /// Stage a `MockLlm` response keyed off the stable `[CHUNK::` prefix
    /// our user-prompt builder always emits.
    fn mk_llm(response: serde_json::Value) -> Arc<MockLlm> {
        let m = MockLlm::new(response.clone());
        m.add_response("[CHUNK::", response);
        Arc::new(m)
    }

    fn mk_chunks(doc_id: uuid::Uuid) -> (uuid::Uuid, Arc<InMemoryChunks>) {
        let chunk_id = uuid::Uuid::now_v7();
        let mut by_doc = std::collections::HashMap::new();
        by_doc.insert(
            doc_id,
            vec![ChunkRef {
                id: chunk_id,
                doc_id,
                content: "Governing law: France. Term: 24 months.".into(),
                page: Some(1),
            }],
        );
        (chunk_id, Arc::new(InMemoryChunks { by_doc }))
    }

    #[tokio::test]
    async fn extract_doc_returns_cells_per_column() {
        let review = ReviewId::new();
        let doc = uuid::Uuid::now_v7();
        let (chunk_id, chunks) = mk_chunks(doc);

        let llm = mk_llm(json!({
            "governing_law": {
                "value": "France",
                "reasoning": "Stated explicitly in the contract preamble.",
                "citations": [{
                    "chunk_id": chunk_id.to_string(),
                    "char_start": 15,
                    "char_end": 21,
                    "quoted_text": "France"
                }]
            },
            "term_months": {
                "value": 24,
                "reasoning": "Numeric term specified after the law clause.",
                "citations": [{
                    "chunk_id": chunk_id.to_string(),
                    "char_start": 29,
                    "char_end": 38,
                    "quoted_text": "24 months"
                }]
            }
        }));

        let cols = vec![
            ColumnBuilder::new(review, "governing_law", "Governing law?", CellType::Text).build(),
            ColumnBuilder::new(review, "term_months", "Term in months?", CellType::Number).build(),
        ];
        let extractor = Extractor::new(llm, chunks);
        let cells = extractor
            .extract_doc(review, doc, &cols)
            .await
            .expect("extract_doc succeeds");

        assert_eq!(cells.len(), 2, "one cell per non-manual column");
        let by_col: std::collections::HashMap<ColumnId, &Cell> =
            cells.iter().map(|c| (c.col_id, c)).collect();
        let law_id = ColumnId::for_name(review, "governing_law");
        let term_id = ColumnId::for_name(review, "term_months");
        assert_eq!(by_col[&law_id].value, json!("France"));
        assert_eq!(by_col[&term_id].value, json!(24));
        assert_eq!(by_col[&law_id].citations.len(), 1);
        assert_eq!(by_col[&law_id].citations[0].quoted_text, "France");
        assert_eq!(by_col[&law_id].citations[0].chunk_id, chunk_id);
    }

    #[tokio::test]
    async fn extract_doc_skips_manual_columns() {
        let review = ReviewId::new();
        let doc = uuid::Uuid::now_v7();
        let (chunk_id, chunks) = mk_chunks(doc);

        // Only the auto column appears in the LLM response — the manual
        // column is never sent to the model in the first place.
        let llm = mk_llm(json!({
            "governing_law": {
                "value": "France",
                "reasoning": "preamble",
                "citations": [{
                    "chunk_id": chunk_id.to_string(),
                    "char_start": 15,
                    "char_end": 21,
                    "quoted_text": "France"
                }]
            }
        }));

        let cols = vec![
            ColumnBuilder::new(review, "governing_law", "Governing law?", CellType::Text).build(),
            ColumnBuilder::new(review, "reviewer_notes", "Notes", CellType::Text)
                .manual()
                .build(),
        ];
        let extractor = Extractor::new(llm, chunks);
        let cells = extractor
            .extract_doc(review, doc, &cols)
            .await
            .expect("extract_doc succeeds");

        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].col_id, ColumnId::for_name(review, "governing_law"));
    }

    #[tokio::test]
    async fn extract_doc_propagates_llm_failure() {
        // Mock returns a non-object value → batch parsing must reject
        // it as Error::Extract for the first auto column.
        let review = ReviewId::new();
        let doc = uuid::Uuid::now_v7();
        let (_chunk_id, chunks) = mk_chunks(doc);
        let llm = mk_llm(json!("this is not an object"));
        let cols =
            vec![ColumnBuilder::new(review, "governing_law", "law?", CellType::Text).build()];

        let extractor = Extractor::new(llm, chunks);
        let err = extractor
            .extract_doc(review, doc, &cols)
            .await
            .expect_err("malformed LLM output must surface");
        match err {
            crate::error::Error::Extract { col, .. } => {
                assert_eq!(col, "governing_law");
            }
            other => panic!("expected Error::Extract, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cell_has_correct_author_and_version() {
        let review = ReviewId::new();
        let doc = uuid::Uuid::now_v7();
        let (chunk_id, chunks) = mk_chunks(doc);
        let llm = mk_llm(json!({
            "governing_law": {
                "value": "France",
                "reasoning": "preamble",
                "citations": [{
                    "chunk_id": chunk_id.to_string(),
                    "char_start": 15,
                    "char_end": 21,
                    "quoted_text": "France"
                }]
            }
        }));
        let cols =
            vec![ColumnBuilder::new(review, "governing_law", "law?", CellType::Text).build()];
        let extractor = Extractor::new(llm, chunks);
        let cells = extractor
            .extract_doc(review, doc, &cols)
            .await
            .expect("extract_doc succeeds");

        let cell = &cells[0];
        assert_eq!(cell.version, 1);
        assert!(!cell.locked);
        assert_eq!(cell.support_score, 0.0);
        assert!(matches!(cell.confidence, Confidence::Medium));
        match &cell.author {
            Author::System { extractor_version } => assert_eq!(extractor_version, "mock"),
            other => panic!("expected Author::System, got {other:?}"),
        }
    }
}
