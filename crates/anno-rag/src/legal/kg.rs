//! Lance-graph-backed legal knowledge graph + node/edge batch API.

use crate::error::{Error, Result};
use crate::legal::types::{ArticleRef, CourtRef, PartyKind};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// One node staged for write to the legal graph.
#[derive(Debug, Clone)]
pub enum NodeWrite {
    /// Document root node.
    Document {
        /// Document id.
        doc_id: Uuid,
        /// Optional document type.
        doc_type: Option<String>,
        /// Optional legal domain.
        legal_domain: Option<String>,
        /// Optional jurisdiction.
        jurisdiction: Option<String>,
        /// Optional document date.
        document_date: Option<DateTime<Utc>>,
        /// Optional dossier id.
        dossier_id: Option<String>,
    },
    /// Chunk node bridging graph and LanceDB chunks.
    Chunk {
        /// Chunk id.
        chunk_id: Uuid,
        /// Parent document id.
        doc_id: Uuid,
        /// Chunk byte start.
        byte_start: u32,
        /// Chunk byte end.
        byte_end: u32,
        /// Optional source page.
        page: Option<u32>,
    },
    /// Party node.
    Party {
        /// Party id.
        party_id: Uuid,
        /// Party kind.
        kind: PartyKind,
        /// Canonical display name.
        canonical_name: String,
        /// Normalized form, for example `org:acme`.
        normalized_form: String,
        /// Optional SIREN.
        siren: Option<String>,
    },
    /// Court node.
    Court {
        /// Court id.
        court_id: String,
        /// Court reference.
        court: CourtRef,
    },
    /// Article node.
    Article {
        /// Article id.
        article_id: Uuid,
        /// Article reference.
        article: ArticleRef,
    },
    /// Obligation node.
    Obligation {
        /// Obligation id.
        obligation_id: Uuid,
        /// Obligation kind.
        kind: String,
        /// Pseudonymized obligation text.
        text_pseudo: String,
    },
    /// Amount node.
    Amount {
        /// Amount id.
        amount_id: Uuid,
        /// Value in cents.
        value_cents: i64,
        /// Currency code.
        currency: String,
        /// Amount scope.
        scope: String,
    },
    /// Event node.
    Event {
        /// Event id.
        event_id: Uuid,
        /// Event kind.
        kind: String,
        /// Optional event date.
        event_date: Option<DateTime<Utc>>,
        /// Optional deadline date.
        deadline_date: Option<DateTime<Utc>>,
    },
    /// Risk node.
    Risk {
        /// Risk id.
        risk_id: Uuid,
        /// Risk severity.
        severity: String,
        /// Risk category.
        category: String,
        /// Pseudonymized risk text.
        text_pseudo: String,
    },
    /// Mandatory clause check node.
    MandatoryClauseCheck {
        /// Check id.
        check_id: Uuid,
        /// Requirement key.
        requirement: String,
        /// Check status.
        status: String,
    },
}

/// One edge staged for write to the legal graph.
#[derive(Debug, Clone)]
pub struct EdgeWrite {
    /// Source node label.
    pub from_label: &'static str,
    /// Source node key.
    pub from_key: String,
    /// Destination node label.
    pub to_label: &'static str,
    /// Destination node key.
    pub to_key: String,
    /// Edge type.
    pub edge_type: &'static str,
    /// String properties attached to the edge.
    pub props: HashMap<String, String>,
}

/// Batch of nodes staged for graph write.
#[derive(Debug, Default)]
pub struct NodeBatch {
    /// Staged nodes.
    pub nodes: Vec<NodeWrite>,
}

impl NodeBatch {
    /// Create an empty node batch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a document node.
    pub fn add_document(
        &mut self,
        doc_id: Uuid,
        doc_type: Option<String>,
        legal_domain: Option<String>,
        jurisdiction: Option<String>,
        document_date: Option<DateTime<Utc>>,
        dossier_id: Option<String>,
    ) {
        self.nodes.push(NodeWrite::Document {
            doc_id,
            doc_type,
            legal_domain,
            jurisdiction,
            document_date,
            dossier_id,
        });
    }

    /// Add a chunk node.
    pub fn add_chunk(
        &mut self,
        chunk_id: Uuid,
        doc_id: Uuid,
        byte_start: u32,
        byte_end: u32,
        page: Option<u32>,
    ) {
        self.nodes.push(NodeWrite::Chunk {
            chunk_id,
            doc_id,
            byte_start,
            byte_end,
            page,
        });
    }

    /// Append nodes from another source.
    pub fn absorb(&mut self, other: Vec<NodeWrite>) {
        self.nodes.extend(other);
    }
}

/// Batch of edges staged for graph write.
#[derive(Debug, Default)]
pub struct EdgeBatch {
    /// Staged edges.
    pub edges: Vec<EdgeWrite>,
}

impl EdgeBatch {
    /// Create an empty edge batch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append edges from another source.
    pub fn absorb(&mut self, other: Vec<EdgeWrite>) {
        self.edges.extend(other);
    }
}

/// Legal knowledge graph abstraction.
#[async_trait]
pub trait LegalKnowledgeGraph: Send + Sync {
    /// Upsert node and edge batches atomically where the backend supports it.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()>;

    /// Delete graph state for one document.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn delete_doc(&self, doc_id: Uuid) -> Result<()>;

    /// Compact graph storage.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn compact(&self) -> Result<()>;

    /// Execute a constrained graph query.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn cypher(
        &self,
        query: &str,
        params: HashMap<String, String>,
    ) -> Result<Vec<HashMap<String, String>>>;
}

/// Lance-graph-backed knowledge graph handle.
///
/// Phase 1: directory is created and all write operations are no-ops.
/// Real lance-graph write integration (LanceDB table-per-label + cypher
/// execution) lands in Stage D when the graph intents are wired up.
pub struct LanceGraphStore {
    root: std::path::PathBuf,
}

impl LanceGraphStore {
    /// Open or create the legal KG directory under `cfg.index_path()/legal_kg`.
    ///
    /// # Errors
    /// Returns [`Error::Graph`] if the directory cannot be created.
    pub async fn open(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let root = cfg.index_path().join("legal_kg");
        std::fs::create_dir_all(&root)
            .map_err(|e| Error::Graph(format!("mkdir legal_kg: {e}")))?;
        Ok(Self { root })
    }

    /// Graph root path.
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}

#[async_trait]
impl LegalKnowledgeGraph for LanceGraphStore {
    async fn upsert_batch(&self, _nodes: &NodeBatch, _edges: &EdgeBatch) -> Result<()> {
        // Phase 1 no-op: real LanceDB table-per-label writes land in Stage D.
        Ok(())
    }

    async fn delete_doc(&self, _doc_id: Uuid) -> Result<()> {
        // Phase 1 no-op.
        Ok(())
    }

    async fn compact(&self) -> Result<()> {
        // Phase 1 no-op.
        Ok(())
    }

    async fn cypher(
        &self,
        _query: &str,
        _params: HashMap<String, String>,
    ) -> Result<Vec<HashMap<String, String>>> {
        // Phase 1 no-op: returns empty result set.
        // Real cypher execution via lance-graph wired in Stage D.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[derive(Default)]
    pub struct InMemoryKG {
        pub nodes: std::sync::Mutex<Vec<NodeWrite>>,
        pub edges: std::sync::Mutex<Vec<EdgeWrite>>,
    }

    #[async_trait]
    impl LegalKnowledgeGraph for InMemoryKG {
        async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> crate::Result<()> {
            self.nodes.lock().unwrap().extend(nodes.nodes.clone());
            self.edges.lock().unwrap().extend(edges.edges.clone());
            Ok(())
        }

        async fn delete_doc(&self, _doc_id: Uuid) -> crate::Result<()> {
            Ok(())
        }

        async fn compact(&self) -> crate::Result<()> {
            Ok(())
        }

        async fn cypher(
            &self,
            _query: &str,
            _params: HashMap<String, String>,
        ) -> crate::Result<Vec<HashMap<String, String>>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn in_memory_kg_round_trip_nodes_and_edges() {
        let kg = InMemoryKG::default();
        let mut nodes = NodeBatch::new();
        nodes.add_document(Uuid::nil(), Some("contract".into()), None, None, None, None);
        let edges = EdgeBatch::default();

        kg.upsert_batch(&nodes, &edges).await.unwrap();

        assert_eq!(kg.nodes.lock().unwrap().len(), 1);
    }
}
