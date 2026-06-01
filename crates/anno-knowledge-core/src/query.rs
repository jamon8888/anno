//! Query types for the knowledge search surface.

use crate::ids::{ChunkId, ObjectId, RevisionId};
use crate::object::ObjectType;
use crate::source::SourceKind;
use serde::{Deserialize, Serialize};

/// Search execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSearchMode {
    /// Fast keyword or approximate-nearest-neighbour search.
    Fast,
    /// Full semantic embedding search.
    Semantic,
    /// Multi-pass reranked deep search.
    Deep,
}

/// Search request used by service code and MCP params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    /// Natural-language query string.
    pub query: String,
    /// Search execution mode.
    pub mode: KnowledgeSearchMode,
    /// Number of results to return (clamped to local-machine budget).
    pub top_k: usize,
}

impl KnowledgeSearchRequest {
    /// Create a request with local-machine defaults.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            mode: KnowledgeSearchMode::Fast,
            top_k: 10,
        }
    }

    /// Set and clamp top-k for local response size.
    #[must_use]
    pub fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k.clamp(1, 50);
        self
    }
}

/// One pseudonymized search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeSearchHit {
    /// Identifier of the matched chunk.
    pub chunk_id: ChunkId,
    /// Parent object identifier.
    pub object_id: ObjectId,
    /// Revision identifier at time of indexing.
    pub revision_id: RevisionId,
    /// Kind of the source this hit came from.
    pub source_kind: SourceKind,
    /// Logical type of the parent object.
    pub object_type: ObjectType,
    /// Pseudonymized title of the parent object, if available.
    pub title_pseudo: Option<String>,
    /// Pseudonymized text snippet for display.
    pub snippet_pseudo: String,
    /// Relevance score in `[0.0, 1.0]`.
    pub score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_search_mode_is_fast() {
        let request = KnowledgeSearchRequest::new("contrat dupont");

        assert_eq!(request.mode, KnowledgeSearchMode::Fast);
        assert_eq!(request.top_k, 10);
    }

    #[test]
    fn top_k_is_clamped_to_local_machine_budget() {
        let request = KnowledgeSearchRequest::new("contrat").with_top_k(500);

        assert_eq!(request.top_k, 50);
    }
}
