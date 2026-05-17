//! Free-form citation verifier — parse `[chunk_id:byte_start-byte_end]`
//! markers from arbitrary LLM output text and validate each against
//! the chunk store.
//!
//! Used by the upcoming MCP tool `tabular_review.verify_citations_in_output`
//! to support the v1.0 spec §6.7 contract for any LLM response that
//! carries inline citations (not just structured cell extraction).
//!
//! **Marker syntax** (deviation from plan sketch): we use
//! `[chunk_id:byte_start-byte_end]` rather than the plan's
//! `[doc_id:...]` because real citations are chunk-scoped and
//! `chunk_id` is what survives into the cells table. doc_id-scoped
//! offsets would require gluing chunks back together at validation
//! time, which adds no information beyond `chunk_by_id`.

use crate::error::Result;
use crate::extract::ChunkSource;
use crate::storage::cells::Citation;
use crate::verify::offsets::OffsetFailure;
use regex::Regex;
use std::sync::LazyLock;
use uuid::Uuid;

/// Marker shape: `[<chunk_uuid>:<byte_start>-<byte_end>]`. UUID v4/v7
/// patterns both match the standard 36-char hyphenated form.
static MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}):(\d+)-(\d+)\]",
    )
    .expect("hardcoded regex compiles")
});

/// One citation verification outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationReport {
    /// The marker exactly as it appeared in the input text.
    pub marker: String,
    /// Parsed `chunk_id` + offsets. `None` when the marker matched the
    /// regex but the numeric ranges overflowed `u32` (rare; surfaced
    /// as [`Status::ParseError`]).
    pub citation: Option<Citation>,
    /// Verification status.
    pub status: Status,
    /// On [`Status::Ok`]: the matched chunk text. On failure:
    /// human-readable reason. Always non-empty for either branch — the
    /// LLM can surface it directly.
    pub evidence: String,
}

/// Outcome of a single marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Offsets are valid and resolve to a real slice of a real chunk.
    ///
    /// Note: this verifier doesn't have access to the LLM's *claimed*
    /// quoted text — it reports the actual chunk slice as evidence,
    /// so any caller who wants to check "did the LLM say what was
    /// there" must compare against this evidence themselves.
    Ok,
    /// The marker matched the regex but its numeric components
    /// overflowed `u32`. [`CitationReport::citation`] is `None` in
    /// this case.
    ParseError,
    /// The citation parses cleanly but its offsets / quote don't
    /// round-trip against the chunk.
    OffsetFail(OffsetFailure),
}

/// Scan `text` for citation markers and verify each against `chunks`.
///
/// Returns one [`CitationReport`] per marker found in document order.
/// Duplicate markers (same `chunk_id` + range) get duplicate reports —
/// callers can dedup if they want.
///
/// # Errors
///
/// Propagates [`Error`](crate::error::Error) from
/// [`ChunkSource::chunk_by_id`] when the underlying store fails. A
/// missing chunk is **not** an error — it surfaces as
/// `Status::OffsetFail(OffsetFailure::UnknownChunkId)`.
pub async fn verify_citations_in_output(
    text: &str,
    chunks: &dyn ChunkSource,
) -> Result<Vec<CitationReport>> {
    let mut out = Vec::new();

    for cap in MARKER_RE.captures_iter(text) {
        let marker = cap
            .get(0)
            .expect("capture group 0 is always present")
            .as_str()
            .to_string();
        let chunk_id_str = &cap[1];
        let start_str = &cap[2];
        let end_str = &cap[3];

        // The regex enforces UUID-shaped hex, so the parse only fails
        // on values like all-zeroes that aren't valid v4/v7 — keep the
        // ParseError branch defensively rather than `.expect`.
        let chunk_id: Uuid = match chunk_id_str.parse() {
            Ok(u) => u,
            Err(_) => {
                out.push(CitationReport {
                    marker,
                    citation: None,
                    status: Status::ParseError,
                    evidence: format!("invalid uuid in marker: {chunk_id_str}"),
                });
                continue;
            }
        };
        let Ok(byte_start) = start_str.parse::<u32>() else {
            out.push(CitationReport {
                marker,
                citation: None,
                status: Status::ParseError,
                evidence: format!("byte_start out of u32 range: {start_str}"),
            });
            continue;
        };
        let Ok(byte_end) = end_str.parse::<u32>() else {
            out.push(CitationReport {
                marker,
                citation: None,
                status: Status::ParseError,
                evidence: format!("byte_end out of u32 range: {end_str}"),
            });
            continue;
        };

        // Fetch chunk + verify.
        let chunk = chunks.chunk_by_id(chunk_id).await?;
        let citation = Citation {
            chunk_id,
            byte_start,
            byte_end,
            // We don't know the LLM's claimed quote in this code path
            // — the marker syntax doesn't carry one. The verifier
            // reports the actual chunk slice in `evidence` instead.
            quoted_text: String::new(),
            page: None,
        };

        let report = match chunk {
            None => CitationReport {
                marker: marker.clone(),
                citation: Some(citation),
                status: Status::OffsetFail(OffsetFailure::UnknownChunkId),
                evidence: format!("chunk {chunk_id} not found"),
            },
            Some(chunk_ref) => {
                let start = byte_start as usize;
                let end = byte_end as usize;
                if byte_start > byte_end {
                    CitationReport {
                        marker: marker.clone(),
                        citation: Some(citation),
                        status: Status::OffsetFail(OffsetFailure::StartAfterEnd),
                        evidence: format!("byte_start ({byte_start}) > byte_end ({byte_end})"),
                    }
                } else if end > chunk_ref.content.len() {
                    CitationReport {
                        marker: marker.clone(),
                        citation: Some(citation),
                        status: Status::OffsetFail(OffsetFailure::OutOfBounds {
                            chunk_len: u32::try_from(chunk_ref.content.len()).unwrap_or(u32::MAX),
                        }),
                        evidence: format!(
                            "byte_end ({byte_end}) > chunk length ({})",
                            chunk_ref.content.len()
                        ),
                    }
                } else {
                    match chunk_ref.content.get(start..end) {
                        Some(slice) => CitationReport {
                            marker: marker.clone(),
                            citation: Some(citation),
                            status: Status::Ok,
                            evidence: slice.to_string(),
                        },
                        None => CitationReport {
                            marker: marker.clone(),
                            citation: Some(citation),
                            status: Status::OffsetFail(OffsetFailure::Utf8Boundary),
                            evidence: format!(
                                "offsets {start}..{end} do not land on UTF-8 boundaries"
                            ),
                        },
                    }
                }
            }
        };
        out.push(report);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::ChunkRef;
    use async_trait::async_trait;
    use std::collections::HashMap;

    /// Local in-memory chunk source for the output verifier tests.
    /// `chunk_by_id` does the real work; `chunks_for_doc` is a stub.
    struct InMemoryChunks {
        by_id: HashMap<Uuid, ChunkRef>,
    }

    impl InMemoryChunks {
        fn with(chunks: Vec<ChunkRef>) -> Self {
            Self {
                by_id: chunks.into_iter().map(|c| (c.id, c)).collect(),
            }
        }
    }

    #[async_trait]
    impl ChunkSource for InMemoryChunks {
        async fn chunks_for_doc(&self, doc_id: Uuid) -> Result<Vec<ChunkRef>> {
            Ok(self
                .by_id
                .values()
                .filter(|c| c.doc_id == doc_id)
                .cloned()
                .collect())
        }

        async fn chunk_by_id(&self, chunk_id: Uuid) -> Result<Option<ChunkRef>> {
            Ok(self.by_id.get(&chunk_id).cloned())
        }
    }

    fn mk_chunk(content: &str) -> ChunkRef {
        ChunkRef {
            id: Uuid::now_v7(),
            doc_id: Uuid::now_v7(),
            content: content.to_string(),
            page: Some(1),
        }
    }

    #[tokio::test]
    async fn parses_and_verifies_single_marker() {
        let chunk = mk_chunk("Hello world");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!("Quote: [{chunk_id}:0-5] end.");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert!(matches!(reports[0].status, Status::Ok));
        assert_eq!(reports[0].evidence, "Hello");
    }

    #[tokio::test]
    async fn parses_multiple_markers_in_order() {
        let chunk = mk_chunk("Hello world");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!(
            "First [{chunk_id}:0-5] then prose [{chunk_id}:6-11] then [{chunk_id}:0-1] end."
        );
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].evidence, "Hello");
        assert_eq!(reports[1].evidence, "world");
        assert_eq!(reports[2].evidence, "H");
    }

    #[tokio::test]
    async fn text_with_no_markers_returns_empty() {
        let src = InMemoryChunks::with(vec![]);
        let reports = verify_citations_in_output("just prose, no citations here.", &src)
            .await
            .expect("verify ok");
        assert!(reports.is_empty());
    }

    #[tokio::test]
    async fn unknown_chunk_id_reports_unknown_chunk_failure() {
        let src = InMemoryChunks::with(vec![]);
        let random = Uuid::now_v7();
        let text = format!("citation [{random}:0-5] here.");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            reports[0].status,
            Status::OffsetFail(OffsetFailure::UnknownChunkId)
        ));
    }

    #[tokio::test]
    async fn out_of_bounds_reported() {
        let chunk = mk_chunk("Hello");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!("[{chunk_id}:0-99]");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        match &reports[0].status {
            Status::OffsetFail(OffsetFailure::OutOfBounds { chunk_len }) => {
                assert_eq!(*chunk_len, 5);
            }
            other => panic!("expected OutOfBounds, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn inverted_range_reported() {
        let chunk = mk_chunk("Hello world");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!("[{chunk_id}:10-5]");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            reports[0].status,
            Status::OffsetFail(OffsetFailure::StartAfterEnd)
        ));
    }

    #[tokio::test]
    async fn utf8_boundary_split_reported() {
        // "café" is 5 bytes: c(1) a(1) f(1) é(2). Byte index 4 falls
        // in the middle of the é → slice returns None.
        let chunk = mk_chunk("café");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!("[{chunk_id}:0-4]");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert!(matches!(
            reports[0].status,
            Status::OffsetFail(OffsetFailure::Utf8Boundary)
        ));
    }

    #[tokio::test]
    async fn markers_inside_prose_are_extracted() {
        let chunk = mk_chunk("Hello world");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        let text = format!("Here is a citation [{chunk_id}:0-5] in prose.");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].evidence, "Hello");
        assert!(reports[0].marker.starts_with('['));
        assert!(reports[0].marker.ends_with(']'));
    }

    #[tokio::test]
    async fn malformed_marker_with_overflow_offsets() {
        let chunk = mk_chunk("Hello world");
        let chunk_id = chunk.id;
        let src = InMemoryChunks::with(vec![chunk]);
        // 99_999_999_999_999 overflows u32 (max ~4.3 × 10^9).
        let text = format!("[{chunk_id}:99999999999999-100]");
        let reports = verify_citations_in_output(&text, &src)
            .await
            .expect("verify ok");
        assert_eq!(reports.len(), 1);
        assert!(matches!(reports[0].status, Status::ParseError));
        assert!(reports[0].citation.is_none());
    }
}
