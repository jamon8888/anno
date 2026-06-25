//! Local legal knowledge graph + node/edge batch API.

use crate::error::{Error, Result};
use crate::legal::types::{ArticleRef, CourtRef, PartyKind};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// Human (or automated) validation of an extracted fact.
    Validation {
        /// Validation id.
        validation_id: Uuid,
        /// Chunk UUID that contains the validated fact.
        chunk_id: Uuid,
        /// Field name that was validated (e.g. `"obligation:paiement"`).
        field_name: String,
        /// Action taken: `"confirm"`, `"reject"`, or `"correct"`.
        action: String,
        /// Corrected value when `action == "correct"`.
        corrected_value: Option<String>,
        /// Optional free-text note from the reviewer.
        note: Option<String>,
        /// When the validation was recorded.
        validated_at: chrono::DateTime<chrono::Utc>,
        /// Optional actor identifier (reviewer email or system name).
        actor: Option<String>,
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

/// Hint for a cross-document relationship detected in the text of `doc_id`.
#[derive(Debug, Clone)]
pub struct CrossDocLinkHint {
    /// Edge type to create, e.g. `"APPEALS"`, `"AMENDS"`, `"CITES"`.
    pub edge_type: &'static str,
    /// Target document UUID when already known (from a prior index lookup).
    pub target_doc_id: Option<Uuid>,
    /// Verbatim quote or title fragment that triggered this hint (used for
    /// fuzzy matching when `target_doc_id` is `None`).
    pub matching_quote: Option<String>,
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

    /// Create cross-document edges based on detected references.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn link_cross_documents(&self, doc_id: Uuid, hints: &[CrossDocLinkHint]) -> Result<()>;

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

    /// Return documents and chunks associated with a normalized party.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn party_dossier(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "party_dossier",
            HashMap::from([("party".to_string(), party.to_string())]),
        )
        .await
    }

    /// Return obligations attached to a normalized obligor party.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn obligations_owed_by(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "obligations_owed_by",
            HashMap::from([("party".to_string(), party.to_string())]),
        )
        .await
    }

    /// Return documents and chunks citing a normalized article reference.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn citation_chain(&self, article_ref: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "citation_chain",
            HashMap::from([("article_ref".to_string(), article_ref.to_string())]),
        )
        .await
    }

    /// Return chronological events for a dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn procedural_timeline(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "procedural_timeline",
            HashMap::from([("dossier_id".to_string(), dossier_id.to_string())]),
        )
        .await
    }

    /// Return document rows for a case-file dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn case_file_documents(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "case_file_documents",
            HashMap::from([("dossier_id".to_string(), dossier_id.to_string())]),
        )
        .await
    }

    /// Return distinct party rows for a case-file dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn case_file_parties(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "case_file_parties",
            HashMap::from([("dossier_id".to_string(), dossier_id.to_string())]),
        )
        .await
    }

    /// Return chronological event rows for a case-file dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn case_file_events(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "case_file_events",
            HashMap::from([("dossier_id".to_string(), dossier_id.to_string())]),
        )
        .await
    }

    /// Return an appeal chain rooted at a document.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn appeal_chain(
        &self,
        doc_id: &str,
        max_depth: u32,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "appeal_chain",
            HashMap::from([
                ("doc_id".to_string(), doc_id.to_string()),
                ("max_depth".to_string(), max_depth.to_string()),
            ]),
        )
        .await
    }

    /// Return parties linked to a document.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn contract_parties(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "contract_parties",
            HashMap::from([("doc_id".to_string(), doc_id.to_string())]),
        )
        .await
    }

    /// Return obligations linked to a document via MENTIONS edges.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn contract_obligations(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "contract_obligations",
            HashMap::from([("doc_id".to_string(), doc_id.to_string())]),
        )
        .await
    }

    /// Return risk findings for a document or dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn risk_findings(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        let key = if is_dossier {
            "risk_findings_dossier"
        } else {
            "risk_findings_doc"
        };
        self.cypher(
            key,
            HashMap::from([("scope".to_string(), scope_id.to_string())]),
        )
        .await
    }
}

/// SQLite-backed legal knowledge graph handle.
///
/// This is the v0 graph backend: typed local persistence for the named legal
/// graph traversals, stored under `cfg.index_path()/legal_kg/graph.sqlite`.
pub struct SqliteLegalGraphStore {
    root: PathBuf,
    db_path: PathBuf,
}

/// Compatibility name kept for existing pipeline code.
pub type LanceGraphStore = SqliteLegalGraphStore;

#[derive(Debug)]
struct NodeRow {
    label: &'static str,
    id: String,
    normalized_key: String,
    doc_id: Option<String>,
    props_json: String,
}

impl SqliteLegalGraphStore {
    /// Open or create the legal KG directory under `cfg.index_path()/legal_kg`.
    ///
    /// # Errors
    /// Returns [`Error::Graph`] if the directory or database cannot be created.
    pub async fn open(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let root = cfg.index_path().join("legal_kg");
        std::fs::create_dir_all(&root).map_err(|e| Error::Graph(format!("mkdir legal_kg: {e}")))?;
        let db_path = root.join("graph.sqlite");
        let store = Self { root, db_path };
        store.with_conn(init_schema)?;
        Ok(store)
    }

    /// Graph root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| Error::Graph(format!("open sqlite legal graph: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| Error::Graph(format!("enable sqlite foreign keys: {e}")))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| Error::Graph(format!("enable sqlite wal: {e}")))?;
        f(&conn)
    }

    fn party_dossier_rows(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id AS doc_id,
                            c.id AS chunk_id,
                            json_extract(d.props_json, '$.doc_type') AS doc_type,
                            json_extract(d.props_json, '$.legal_domain') AS legal_domain,
                            json_extract(d.props_json, '$.jurisdiction') AS jurisdiction,
                            json_extract(d.props_json, '$.document_date') AS document_date,
                            json_extract(d.props_json, '$.dossier_id') AS dossier_id,
                            json_extract(party_edge.props_json, '$.role') AS role,
                            json_extract(c.props_json, '$.byte_start') AS byte_start,
                            json_extract(c.props_json, '$.byte_end') AS byte_end,
                            json_extract(c.props_json, '$.page') AS page
                     FROM legal_nodes p
                     JOIN legal_edges party_edge
                       ON party_edge.from_label = 'Party'
                      AND party_edge.from_key = p.id
                      AND party_edge.to_label = 'Document'
                      AND party_edge.edge_type = 'PARTY_TO'
                     JOIN legal_nodes d
                       ON d.label = 'Document'
                      AND d.id = party_edge.to_key
                     JOIN legal_edges chunk_edge
                       ON chunk_edge.from_label = 'Document'
                      AND chunk_edge.from_key = d.id
                      AND chunk_edge.to_label = 'Chunk'
                      AND chunk_edge.edge_type = 'HAS_CHUNK'
                     JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = chunk_edge.to_key
                     WHERE p.label = 'Party'
                       AND p.normalized_key = ?1
                     ORDER BY document_date, d.id, c.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![party])
        })
    }

    fn obligations_owed_by_rows(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT o.id AS obligation_id,
                            json_extract(o.props_json, '$.kind') AS obligation_kind,
                            json_extract(o.props_json, '$.text_pseudo') AS text_pseudo,
                            c.id AS chunk_id,
                            c.doc_id AS doc_id,
                            json_extract(mention_edge.props_json, '$.byte_start') AS byte_start,
                            json_extract(mention_edge.props_json, '$.byte_end') AS byte_end
                     FROM legal_nodes p
                     JOIN legal_edges bound_edge
                       ON bound_edge.from_label = 'Party'
                      AND bound_edge.from_key = p.id
                      AND bound_edge.to_label = 'Obligation'
                      AND bound_edge.edge_type = 'BOUND_BY'
                     JOIN legal_nodes o
                       ON o.label = 'Obligation'
                      AND o.id = bound_edge.to_key
                     LEFT JOIN legal_edges mention_edge
                       ON mention_edge.to_label = 'Obligation'
                      AND mention_edge.to_key = o.id
                      AND mention_edge.from_label = 'Chunk'
                      AND mention_edge.edge_type = 'MENTIONS'
                     LEFT JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = mention_edge.from_key
                     WHERE p.label = 'Party'
                       AND p.normalized_key = ?1
                     ORDER BY c.doc_id, c.id, o.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![party])
        })
    }

    fn citation_chain_rows(&self, article_ref: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id AS doc_id,
                            c.id AS chunk_id,
                            a.normalized_key AS article_ref,
                            json_extract(d.props_json, '$.dossier_id') AS dossier_id
                     FROM legal_nodes a
                     JOIN legal_edges ref_edge
                       ON ref_edge.to_label = 'Article'
                      AND ref_edge.to_key = a.id
                      AND ref_edge.from_label = 'Document'
                      AND ref_edge.edge_type = 'REFERENCES'
                     JOIN legal_nodes d
                       ON d.label = 'Document'
                      AND d.id = ref_edge.from_key
                     JOIN legal_edges mention_edge
                       ON mention_edge.from_label = 'Chunk'
                      AND mention_edge.to_label = 'Article'
                      AND mention_edge.to_key = a.id
                      AND mention_edge.edge_type = 'MENTIONS'
                     JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = mention_edge.from_key
                      AND c.doc_id = d.id
                     WHERE a.label = 'Article'
                       AND a.normalized_key = ?1
                     ORDER BY d.id, c.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![article_ref])
        })
    }

    fn procedural_timeline_rows(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT e.id AS event_id,
                            json_extract(e.props_json, '$.kind') AS event_kind,
                            json_extract(e.props_json, '$.event_date') AS event_date,
                            json_extract(e.props_json, '$.deadline_date') AS deadline_date,
                            c.id AS chunk_id,
                            d.id AS doc_id,
                            json_extract(d.props_json, '$.dossier_id') AS dossier_id
                     FROM legal_nodes d
                     JOIN legal_edges chunk_edge
                       ON chunk_edge.from_label = 'Document'
                      AND chunk_edge.from_key = d.id
                      AND chunk_edge.to_label = 'Chunk'
                      AND chunk_edge.edge_type = 'HAS_CHUNK'
                     JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = chunk_edge.to_key
                     JOIN legal_edges mention_edge
                       ON mention_edge.from_label = 'Chunk'
                      AND mention_edge.from_key = c.id
                      AND mention_edge.to_label = 'Event'
                      AND mention_edge.edge_type = 'MENTIONS'
                     JOIN legal_nodes e
                       ON e.label = 'Event'
                      AND e.id = mention_edge.to_key
                     WHERE d.label = 'Document'
                       AND json_extract(d.props_json, '$.dossier_id') = ?1
                     ORDER BY event_date, deadline_date, d.id, c.id, e.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![dossier_id])
        })
    }

    fn case_file_documents_rows(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id AS doc_id,
                            json_extract(d.props_json, '$.doc_type') AS doc_type
                       FROM legal_nodes d
                      WHERE d.label = 'Document'
                        AND json_extract(d.props_json, '$.dossier_id') = ?1
                      ORDER BY d.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![dossier_id])
        })
    }

    fn case_file_parties_rows(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT
                            json_extract(p.props_json, '$.canonical_name') AS value,
                            json_extract(party_edge.props_json, '$.role') AS role
                       FROM legal_nodes d
                       JOIN legal_edges party_edge
                         ON party_edge.from_label = 'Party'
                        AND party_edge.to_label = 'Document'
                        AND party_edge.to_key = d.id
                        AND party_edge.edge_type = 'PARTY_TO'
                       JOIN legal_nodes p
                         ON p.label = 'Party'
                        AND p.id = party_edge.from_key
                      WHERE d.label = 'Document'
                        AND json_extract(d.props_json, '$.dossier_id') = ?1
                      ORDER BY value, role",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![dossier_id])
        })
    }

    fn case_file_events_rows(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT json_extract(e.props_json, '$.kind') AS kind,
                            json_extract(e.props_json, '$.event_date') AS event_date,
                            c.id AS cid
                       FROM legal_nodes d
                       JOIN legal_edges chunk_edge
                         ON chunk_edge.from_label = 'Document'
                        AND chunk_edge.from_key = d.id
                        AND chunk_edge.to_label = 'Chunk'
                        AND chunk_edge.edge_type = 'HAS_CHUNK'
                       JOIN legal_nodes c
                         ON c.label = 'Chunk'
                        AND c.id = chunk_edge.to_key
                       JOIN legal_edges mention_edge
                         ON mention_edge.from_label = 'Chunk'
                        AND mention_edge.from_key = c.id
                        AND mention_edge.to_label = 'Event'
                        AND mention_edge.edge_type = 'MENTIONS'
                       JOIN legal_nodes e
                         ON e.label = 'Event'
                        AND e.id = mention_edge.to_key
                      WHERE d.label = 'Document'
                        AND json_extract(d.props_json, '$.dossier_id') = ?1
                      ORDER BY event_date, d.id, c.id, e.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![dossier_id])
        })
    }

    fn appeal_chain_rows(
        &self,
        doc_id: &str,
        max_depth: u32,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "WITH RECURSIVE chain(depth, from_doc_id, to_doc_id, path) AS (
                        SELECT 1,
                               e.from_key,
                               e.to_key,
                               e.from_key || '>' || e.to_key
                          FROM legal_edges e
                         WHERE e.from_label = 'Document'
                           AND e.to_label = 'Document'
                           AND e.edge_type = 'APPEALS'
                           AND e.from_key = ?1
                        UNION ALL
                        SELECT chain.depth + 1,
                               e.from_key,
                               e.to_key,
                               chain.path || '>' || e.to_key
                          FROM chain
                          JOIN legal_edges e
                            ON e.from_label = 'Document'
                           AND e.to_label = 'Document'
                           AND e.edge_type = 'APPEALS'
                           AND e.from_key = chain.to_doc_id
                         WHERE chain.depth < ?2
                     )
                     SELECT depth, from_doc_id, to_doc_id, path
                       FROM chain
                      ORDER BY depth, from_doc_id, to_doc_id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![doc_id, max_depth])
        })
    }

    fn contract_parties_rows(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT json_extract(p.props_json, '$.canonical_name') AS value,
                            json_extract(party_edge.props_json, '$.role') AS role
                     FROM legal_edges party_edge
                     JOIN legal_nodes p
                       ON p.label = 'Party'
                      AND p.id = party_edge.from_key
                     WHERE party_edge.from_label = 'Party'
                       AND party_edge.to_label = 'Document'
                       AND party_edge.to_key = ?1
                       AND party_edge.edge_type = 'PARTY_TO'
                     ORDER BY value",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![doc_id])
        })
    }

    fn contract_obligations_rows(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT json_extract(o.props_json, '$.kind') AS kind,
                            json_extract(o.props_json, '$.text_pseudo') AS text,
                            c.id AS cid
                     FROM legal_nodes d
                     JOIN legal_edges chunk_edge
                       ON chunk_edge.from_label = 'Document'
                      AND chunk_edge.from_key = d.id
                      AND chunk_edge.to_label = 'Chunk'
                      AND chunk_edge.edge_type = 'HAS_CHUNK'
                     JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = chunk_edge.to_key
                     JOIN legal_edges mention_edge
                       ON mention_edge.from_label = 'Chunk'
                      AND mention_edge.from_key = c.id
                      AND mention_edge.to_label = 'Obligation'
                      AND mention_edge.edge_type = 'MENTIONS'
                     JOIN legal_nodes o
                       ON o.label = 'Obligation'
                      AND o.id = mention_edge.to_key
                     WHERE d.label = 'Document'
                       AND d.id = ?1
                     ORDER BY c.id, o.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![doc_id])
        })
    }

    fn risk_findings_rows(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        let filter_col = if is_dossier {
            "json_extract(d.props_json, '$.dossier_id')"
        } else {
            "d.id"
        };
        let query = format!(
            "SELECT r.id AS rid,
                    json_extract(r.props_json, '$.severity') AS severity,
                    json_extract(r.props_json, '$.category') AS category,
                    json_extract(r.props_json, '$.text_pseudo') AS text
             FROM legal_nodes d
             JOIN legal_edges chunk_edge
               ON chunk_edge.from_label = 'Document'
              AND chunk_edge.from_key = d.id
              AND chunk_edge.to_label = 'Chunk'
              AND chunk_edge.edge_type = 'HAS_CHUNK'
             JOIN legal_nodes c
               ON c.label = 'Chunk'
              AND c.id = chunk_edge.to_key
             JOIN legal_edges mention_edge
               ON mention_edge.from_label = 'Chunk'
              AND mention_edge.from_key = c.id
              AND mention_edge.to_label = 'Risk'
              AND mention_edge.edge_type = 'MENTIONS'
             JOIN legal_nodes r
               ON r.label = 'Risk'
              AND r.id = mention_edge.to_key
             WHERE d.label = 'Document'
               AND {filter_col} = ?1
             ORDER BY
               CASE json_extract(r.props_json, '$.severity')
                 WHEN 'high' THEN 1
                 WHEN 'medium' THEN 2
                 ELSE 3
               END,
               r.id"
        );
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&query).map_err(sql_err)?;
            collect_rows(&mut stmt, params![scope_id])
        })
    }

    /// True if the knowledge graph holds at least one node associated with
    /// `document_id`. Cheap existence probe — `LIMIT 1`, no row materialisation.
    ///
    /// # Errors
    /// Returns [`crate::error::Error::Graph`] on SQLite failure.
    pub fn document_has_kg_nodes(&self, document_id: uuid::Uuid) -> crate::error::Result<bool> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT 1 FROM legal_nodes WHERE doc_id = ?1 LIMIT 1")
                .map_err(sql_err)?;
            let mut rows = stmt
                .query(rusqlite::params![document_id.to_string()])
                .map_err(sql_err)?;
            let exists = rows.next().map_err(sql_err)?.is_some();
            Ok(exists)
        })
    }
}

#[async_trait]
impl LegalKnowledgeGraph for SqliteLegalGraphStore {
    async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction().map_err(sql_err)?;
            for node in &nodes.nodes {
                let row = node_to_row(node)?;
                tx.execute(
                    "INSERT INTO legal_nodes
                        (label, id, normalized_key, doc_id, props_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))
                     ON CONFLICT(label, id) DO UPDATE SET
                        normalized_key = excluded.normalized_key,
                        doc_id = excluded.doc_id,
                        props_json = excluded.props_json,
                        updated_at = datetime('now')",
                    params![
                        row.label,
                        row.id,
                        row.normalized_key,
                        row.doc_id,
                        row.props_json
                    ],
                )
                .map_err(sql_err)?;
            }

            for edge in &edges.edges {
                let props_json = serde_json::to_string(&edge.props)
                    .map_err(|e| Error::Graph(format!("encode edge props: {e}")))?;
                let doc_id = infer_edge_doc_id(&tx, edge)?;
                tx.execute(
                    "INSERT INTO legal_edges
                        (from_label, from_key, to_label, to_key, edge_type, doc_id, props_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), datetime('now'))
                     ON CONFLICT(from_label, from_key, to_label, to_key, edge_type) DO UPDATE SET
                        doc_id = excluded.doc_id,
                        props_json = excluded.props_json,
                        updated_at = datetime('now')",
                    params![
                        edge.from_label,
                        edge.from_key,
                        edge.to_label,
                        edge.to_key,
                        edge.edge_type,
                        doc_id,
                        props_json
                    ],
                )
                .map_err(sql_err)?;
            }
            tx.commit().map_err(sql_err)
        })
    }

    async fn delete_doc(&self, doc_id: Uuid) -> Result<()> {
        let doc_id = doc_id.to_string();
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction().map_err(sql_err)?;
            tx.execute("DELETE FROM legal_edges WHERE doc_id = ?1", params![doc_id])
                .map_err(sql_err)?;
            tx.execute(
                "DELETE FROM legal_nodes
                 WHERE doc_id = ?1 OR (label = 'Document' AND id = ?1)",
                params![doc_id],
            )
            .map_err(sql_err)?;
            tx.commit().map_err(sql_err)
        })
    }

    async fn link_cross_documents(&self, _doc_id: Uuid, _hints: &[CrossDocLinkHint]) -> Result<()> {
        Ok(())
    }

    async fn compact(&self) -> Result<()> {
        self.with_conn(|conn| conn.execute_batch("VACUUM").map_err(sql_err))
    }

    async fn cypher(
        &self,
        query: &str,
        params: HashMap<String, String>,
    ) -> Result<Vec<HashMap<String, String>>> {
        match query {
            "party_dossier" => self.party_dossier_rows(required_param(&params, "party")?),
            "obligations_owed_by" => {
                self.obligations_owed_by_rows(required_param(&params, "party")?)
            }
            "citation_chain" => self.citation_chain_rows(
                params
                    .get("article_ref")
                    .or_else(|| params.get("ref"))
                    .ok_or_else(|| Error::Graph("missing graph query param: article_ref".into()))?,
            ),
            "procedural_timeline" => {
                self.procedural_timeline_rows(required_param(&params, "dossier_id")?)
            }
            "case_file_documents" => {
                self.case_file_documents_rows(required_param(&params, "dossier_id")?)
            }
            "case_file_parties" => {
                self.case_file_parties_rows(required_param(&params, "dossier_id")?)
            }
            "case_file_events" => {
                self.case_file_events_rows(required_param(&params, "dossier_id")?)
            }
            "appeal_chain" => {
                let max_depth = params
                    .get("max_depth")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(3);
                self.appeal_chain_rows(required_param(&params, "doc_id")?, max_depth)
            }
            "contract_parties" => self.contract_parties_rows(required_param(&params, "doc_id")?),
            "contract_obligations" => {
                self.contract_obligations_rows(required_param(&params, "doc_id")?)
            }
            "risk_findings_doc" => {
                self.risk_findings_rows(required_param(&params, "scope")?, false)
            }
            "risk_findings_dossier" => {
                self.risk_findings_rows(required_param(&params, "scope")?, true)
            }
            _ => Err(Error::Graph(
                "raw Cypher execution is not supported by the SQLite graph backend".into(),
            )),
        }
    }

    async fn party_dossier(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.party_dossier_rows(party)
    }

    async fn obligations_owed_by(&self, party: &str) -> Result<Vec<HashMap<String, String>>> {
        self.obligations_owed_by_rows(party)
    }

    async fn citation_chain(&self, article_ref: &str) -> Result<Vec<HashMap<String, String>>> {
        self.citation_chain_rows(article_ref)
    }

    async fn procedural_timeline(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.procedural_timeline_rows(dossier_id)
    }

    async fn case_file_documents(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.case_file_documents_rows(dossier_id)
    }

    async fn case_file_parties(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.case_file_parties_rows(dossier_id)
    }

    async fn case_file_events(&self, dossier_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.case_file_events_rows(dossier_id)
    }

    async fn appeal_chain(
        &self,
        doc_id: &str,
        max_depth: u32,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.appeal_chain_rows(doc_id, max_depth)
    }

    async fn contract_parties(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.contract_parties_rows(doc_id)
    }

    async fn contract_obligations(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.contract_obligations_rows(doc_id)
    }

    async fn risk_findings(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.risk_findings_rows(scope_id, is_dossier)
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS legal_nodes (
            label TEXT NOT NULL,
            id TEXT NOT NULL,
            normalized_key TEXT NOT NULL,
            doc_id TEXT,
            props_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (label, id)
        );
        CREATE INDEX IF NOT EXISTS idx_legal_nodes_label_norm
            ON legal_nodes(label, normalized_key);
        CREATE INDEX IF NOT EXISTS idx_legal_nodes_doc
            ON legal_nodes(doc_id);

        CREATE TABLE IF NOT EXISTS legal_edges (
            from_label TEXT NOT NULL,
            from_key TEXT NOT NULL,
            to_label TEXT NOT NULL,
            to_key TEXT NOT NULL,
            edge_type TEXT NOT NULL,
            doc_id TEXT,
            props_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (from_label, from_key, to_label, to_key, edge_type)
        );
        CREATE INDEX IF NOT EXISTS idx_legal_edges_from
            ON legal_edges(from_label, from_key, edge_type);
        CREATE INDEX IF NOT EXISTS idx_legal_edges_to
            ON legal_edges(to_label, to_key, edge_type);
        CREATE INDEX IF NOT EXISTS idx_legal_edges_doc
            ON legal_edges(doc_id);",
    )
    .map_err(sql_err)
}

fn node_to_row(node: &NodeWrite) -> Result<NodeRow> {
    match node {
        NodeWrite::Document {
            doc_id,
            doc_type,
            legal_domain,
            jurisdiction,
            document_date,
            dossier_id,
        } => json_row(
            "Document",
            doc_id.to_string(),
            doc_id.to_string(),
            Some(doc_id.to_string()),
            serde_json::json!({
                "doc_type": doc_type,
                "legal_domain": legal_domain,
                "jurisdiction": jurisdiction,
                "document_date": document_date.map(|d| d.to_rfc3339()),
                "dossier_id": dossier_id,
            }),
        ),
        NodeWrite::Chunk {
            chunk_id,
            doc_id,
            byte_start,
            byte_end,
            page,
        } => json_row(
            "Chunk",
            chunk_id.to_string(),
            chunk_id.to_string(),
            Some(doc_id.to_string()),
            serde_json::json!({
                "byte_start": byte_start,
                "byte_end": byte_end,
                "page": page,
            }),
        ),
        NodeWrite::Party {
            party_id,
            kind,
            canonical_name,
            normalized_form,
            siren,
        } => json_row(
            "Party",
            party_id.to_string(),
            normalized_form.clone(),
            None,
            serde_json::json!({
                "kind": format!("{kind:?}"),
                "canonical_name": canonical_name,
                "normalized_form": normalized_form,
                "siren": siren,
            }),
        ),
        NodeWrite::Court { court_id, court } => json_row(
            "Court",
            court_id.clone(),
            court.id.clone(),
            None,
            serde_json::to_value(court).map_err(|e| Error::Graph(format!("encode court: {e}")))?,
        ),
        NodeWrite::Article {
            article_id,
            article,
        } => json_row(
            "Article",
            article_id.to_string(),
            article.normalized_ref(),
            None,
            serde_json::json!({
                "code": article.code,
                "article_num": article.article_num,
                "normalized_ref": article.normalized_ref(),
            }),
        ),
        NodeWrite::Obligation {
            obligation_id,
            kind,
            text_pseudo,
        } => json_row(
            "Obligation",
            obligation_id.to_string(),
            obligation_id.to_string(),
            None,
            serde_json::json!({
                "kind": kind,
                "text_pseudo": text_pseudo,
            }),
        ),
        NodeWrite::Amount {
            amount_id,
            value_cents,
            currency,
            scope,
        } => json_row(
            "Amount",
            amount_id.to_string(),
            amount_id.to_string(),
            None,
            serde_json::json!({
                "value_cents": value_cents,
                "currency": currency,
                "scope": scope,
            }),
        ),
        NodeWrite::Event {
            event_id,
            kind,
            event_date,
            deadline_date,
        } => json_row(
            "Event",
            event_id.to_string(),
            event_id.to_string(),
            None,
            serde_json::json!({
                "kind": kind,
                "event_date": event_date.map(|d| d.to_rfc3339()),
                "deadline_date": deadline_date.map(|d| d.to_rfc3339()),
            }),
        ),
        NodeWrite::Risk {
            risk_id,
            severity,
            category,
            text_pseudo,
        } => json_row(
            "Risk",
            risk_id.to_string(),
            risk_id.to_string(),
            None,
            serde_json::json!({
                "severity": severity,
                "category": category,
                "text_pseudo": text_pseudo,
            }),
        ),
        NodeWrite::MandatoryClauseCheck {
            check_id,
            requirement,
            status,
        } => json_row(
            "MandatoryClauseCheck",
            check_id.to_string(),
            check_id.to_string(),
            None,
            serde_json::json!({
                "requirement": requirement,
                "status": status,
            }),
        ),
        NodeWrite::Validation {
            validation_id,
            chunk_id,
            field_name,
            action,
            corrected_value,
            note,
            validated_at,
            actor,
        } => json_row(
            "Validation",
            validation_id.to_string(),
            validation_id.to_string(),
            None,
            serde_json::json!({
                "chunk_id": chunk_id,
                "field_name": field_name,
                "action": action,
                "corrected_value": corrected_value,
                "note": note,
                "validated_at": validated_at.to_rfc3339(),
                "actor": actor,
            }),
        ),
    }
}

fn json_row(
    label: &'static str,
    id: String,
    normalized_key: String,
    doc_id: Option<String>,
    props: serde_json::Value,
) -> Result<NodeRow> {
    let props_json = serde_json::to_string(&props)
        .map_err(|e| Error::Graph(format!("encode node props: {e}")))?;
    Ok(NodeRow {
        label,
        id,
        normalized_key,
        doc_id,
        props_json,
    })
}

fn infer_edge_doc_id(conn: &Connection, edge: &EdgeWrite) -> Result<Option<String>> {
    if edge.from_label == "Document" {
        return Ok(Some(edge.from_key.clone()));
    }
    if edge.to_label == "Document" {
        return Ok(Some(edge.to_key.clone()));
    }
    if edge.from_label == "Chunk" {
        return node_doc_id(conn, "Chunk", &edge.from_key);
    }
    if edge.to_label == "Chunk" {
        return node_doc_id(conn, "Chunk", &edge.to_key);
    }
    Ok(None)
}

fn node_doc_id(conn: &Connection, label: &str, id: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT doc_id FROM legal_nodes WHERE label = ?1 AND id = ?2",
        params![label, id],
        |row| row.get(0),
    )
    .optional()
    .map_err(sql_err)
    .map(|doc_id| doc_id.flatten())
}

fn collect_rows(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
) -> Result<Vec<HashMap<String, String>>> {
    let column_names = stmt
        .column_names()
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let mut rows = stmt.query(params).map_err(sql_err)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(sql_err)? {
        let mut map = HashMap::new();
        for (idx, column) in column_names.iter().enumerate() {
            let value = match row.get_ref(idx).map_err(sql_err)? {
                rusqlite::types::ValueRef::Null => None,
                rusqlite::types::ValueRef::Integer(v) => Some(v.to_string()),
                rusqlite::types::ValueRef::Real(v) => Some(v.to_string()),
                rusqlite::types::ValueRef::Text(v) => Some(String::from_utf8_lossy(v).into_owned()),
                rusqlite::types::ValueRef::Blob(v) => Some(hex::encode(v)),
            };
            if let Some(value) = value {
                map.insert(column.clone(), value);
            }
        }
        out.push(map);
    }
    Ok(out)
}

fn required_param<'a>(params: &'a HashMap<String, String>, name: &str) -> Result<&'a str> {
    params
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| Error::Graph(format!("missing graph query param: {name}")))
}

fn sql_err(err: rusqlite::Error) -> Error {
    Error::Graph(format!("sqlite legal graph: {err}"))
}

#[cfg(test)]
pub(crate) mod tests {
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

        async fn link_cross_documents(
            &self,
            _doc_id: Uuid,
            _hints: &[CrossDocLinkHint],
        ) -> crate::Result<()> {
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

    #[tokio::test]
    async fn document_has_kg_nodes_reports_presence() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = crate::config::AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let store = SqliteLegalGraphStore::open(&cfg).await.expect("open store");
        let doc = Uuid::new_v4();

        assert!(
            !store.document_has_kg_nodes(doc).unwrap(),
            "empty graph should return false"
        );

        let mut nodes = NodeBatch::new();
        nodes.add_document(doc, Some("contract".into()), None, None, None, None);
        store
            .upsert_batch(&nodes, &EdgeBatch::default())
            .await
            .expect("upsert");

        assert!(
            store.document_has_kg_nodes(doc).unwrap(),
            "after insert should return true"
        );
    }
}
