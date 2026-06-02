//! MCP server exposing anno-rag's retrieval surface to Cowork over stdio.
//!
//! Tools: `search`, `rehydrate`, `detect`, `vault_stats`, memory_*, `anno_health`.
//! Reuse rmcp 1.6's `#[tool_router]` + `#[tool_handler]` pattern from
//! `vendor/cloakpipe/crates/cloakpipe-mcp/src/lib.rs`.
//!
//! This crate was extracted from `anno-rag::mcp` so that Phase 8 of the
//! tabular-review plan can attach `anno-rag-tabular` review tools here
//! without creating a cycle (anno-rag-tabular already depends on anno-rag).

#![warn(missing_docs)]

pub mod health;
mod indexer;
pub mod knowledge;
pub mod tabular;

use crate::indexer::SyncSummary;
use anno_rag::config::{AnnoRagConfig, MemoryNerMode};
use anno_rag::pipeline::Pipeline;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;

/// State held by the MCP server: either a pre-built Pipeline (eager) or a
/// lazily-initialised one (deferred until the first tool call).
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    knowledge: Arc<OnceCell<Arc<crate::knowledge::KnowledgeService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    tabular_storage: Arc<OnceCell<Arc<anno_rag_tabular::storage::StorageHandle>>>,
    #[allow(dead_code)] // populated + consumed by the rmcp #[tool_router] macro
    tool_router: ToolRouter<Self>,
}

// ---- Pipeline helpers ----

impl AnnoRagServer {
    async fn pipeline(&self) -> anno_rag::error::Result<&Pipeline> {
        self.pipeline
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                let key = self.key;
                async move {
                    // Models are available if either ANNO_MODELS_DIR is explicitly set
                    // OR the two expected model subdirectories exist at the default
                    // cache location. This avoids any set_var from main().
                    let models_available = std::env::var("ANNO_MODELS_DIR").is_ok() || {
                        let default_models = cfg.models_cache();
                        default_models.join("multilingual-e5-small").exists()
                            && default_models.join("gliner2-multi-v1-onnx").exists()
                    };
                    if !models_available {
                        return Err(anno_rag::error::Error::Config(
                            "Models not downloaded. Ask me to 'Set up anno-rag' \
                             or run `anno-rag download-models` in a terminal, \
                             then restart the extension."
                                .into(),
                        ));
                    }
                    Pipeline::new((*cfg).clone(), key).await.map(Arc::new)
                }
            })
            .await
            .map(|arc| arc.as_ref())
    }

    fn pipeline_arc(&self) -> Option<Arc<Pipeline>> {
        self.pipeline.get().cloned()
    }

    async fn knowledge(&self) -> anno_knowledge_store::Result<&crate::knowledge::KnowledgeService> {
        self.knowledge
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                async move { crate::knowledge::KnowledgeService::open(&cfg).map(Arc::new) }
            })
            .await
            .map(|arc| arc.as_ref())
    }

    async fn tabular_storage(
        &self,
    ) -> anno_rag_tabular::error::Result<&anno_rag_tabular::storage::StorageHandle> {
        self.tabular_storage
            .get_or_try_init(|| {
                // Use index_path() (data_dir/index.lance) — the same root that
                // the Pipeline and CLI use.  Using data_dir directly would open
                // a disconnected LanceDB root, making MCP and CLI work with
                // separate databases.
                let index_path = Arc::clone(&self.cfg).index_path();
                async move {
                    let uri = index_path.to_str().ok_or_else(|| {
                        anno_rag_tabular::error::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "index_path is not valid UTF-8",
                        ))
                    })?;
                    let conn = std::sync::Arc::new(
                        lancedb::connect(uri)
                            .execute()
                            .await
                            .map_err(anno_rag_tabular::error::Error::Lance)?,
                    );
                    anno_rag_tabular::storage::StorageHandle::open(conn)
                        .await
                        .map(std::sync::Arc::new)
                }
            })
            .await
            .map(|arc| arc.as_ref())
    }
}

// ---- Constructors ----

impl AnnoRagServer {
    /// Construct with a pre-built pipeline (eager path). Used by serve_stdio and tests.
    #[must_use]
    pub fn new(pipeline: Pipeline, cfg: AnnoRagConfig) -> Self {
        let cell = OnceCell::new();
        // Safety: cell is freshly created; set cannot fail on a new OnceCell.
        let _ = cell.set(Arc::new(pipeline)).ok();
        Self {
            pipeline: Arc::new(cell),
            knowledge: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key: [0u8; 32],
            tabular_storage: Arc::new(OnceCell::new()),
            tool_router: Self::tool_router(),
        }
    }

    /// Construct with deferred pipeline init (lazy path). Used by serve_stdio_lazy.
    #[must_use]
    pub fn new_lazy(cfg: AnnoRagConfig, key: [u8; 32]) -> Self {
        Self {
            pipeline: Arc::new(OnceCell::new()),
            knowledge: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key,
            tabular_storage: Arc::new(OnceCell::new()),
            tool_router: Self::tool_router(),
        }
    }
}

// ---- Tool parameter types ----

/// Parameters for `knowledge_add_local_folder`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeAddFolderParams {
    /// Absolute path to the local folder to register as a knowledge source.
    pub path: String,
}

/// Parameters for `index`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct IndexParams {
    /// Absolute path to index.
    pub path: String,
    /// Indexing profile: general, legal, or all.
    #[serde(default = "default_index_profile")]
    pub profile: String,
}

fn default_index_profile() -> String {
    "general".to_string()
}

fn validate_profile(profile: &str) -> Result<(), String> {
    match profile {
        "general" | "legal" | "all" => Ok(()),
        other => Err(format!(
            "Unsupported index profile '{other}'. Expected one of: general, legal, all."
        )),
    }
}

fn knowledge_sync_issue(summary: &SyncSummary) -> Option<String> {
    (summary.failed > 0 || summary.truncated).then(|| {
        format!(
            "knowledge sync incomplete: failed={}, truncated={}",
            summary.failed, summary.truncated
        )
    })
}

/// Parameters for `knowledge_sync`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeSyncParams {
    /// Optional source id; if omitted, all local-folder sources are synced.
    #[serde(default)]
    pub source_id: Option<String>,
}

/// Parameters for `knowledge_forget`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeForgetParams {
    /// Source id whose content should be removed from SQLite and FTS.
    pub source_id: String,
}

/// Parameters for the `search` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// User query text. May contain PII — will be pseudonymized through the vault.
    pub query: String,
    /// Number of results to return.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// When true, re-score the top candidates with the cross-encoder
    /// reranker. Requires the server built with `--features rerank`;
    /// otherwise this call returns a clear error.
    #[serde(default)]
    pub rerank: bool,
}

fn default_top_k() -> usize {
    10
}

/// Parameters for the `rehydrate` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct RehydrateParams {
    /// Text containing pseudo-tokens (`EMAIL_1`, `PERSON_3`, …) to restore.
    pub text: String,
}

/// Parameters for the `detect` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct DetectParams {
    /// Text to scan for PII without replacement.
    pub text: String,
}

// ---- Tool response types ----

#[derive(Serialize)]
struct SearchHitWire {
    doc_id: String,
    chunk_id: String,
    source_path: String,
    folder_path: String,
    chunk_idx: u32,
    text_pseudo: String,
    page: Option<u32>,
    char_start: u32,
    char_end: u32,
    score: f32,
}

#[derive(Serialize)]
struct SearchResult {
    hits: Vec<SearchHitWire>,
}

#[derive(Serialize)]
struct RehydrateResult {
    text: String,
    tokens_rehydrated: usize,
}

#[derive(Serialize)]
struct DetectResult {
    entities: Vec<EntityInfo>,
}

#[derive(Serialize)]
struct EntityInfo {
    original: String,
    category: String,
    confidence: f64,
    source: String,
    start: usize,
    end: usize,
}

#[derive(Serialize)]
struct VaultStatsResult {
    total_mappings: usize,
    categories: std::collections::HashMap<String, u32>,
}

// ---- Memory tool param + result types (T10) ----

/// Parameters for the `anno_init_vault` tool.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct InitVaultParams {
    /// User-supplied passphrase. Must be at least 12 characters. Argon2id-
    /// derived into a 32-byte key; never stored or logged in cleartext.
    pub passphrase: String,
}

/// Parameters for the `memory_save` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemorySaveParams {
    /// Plaintext memory body. PII is tokenized via the vault before persist.
    pub text: String,
    /// Memory category — one of `fact`, `preference`, `reference`, `context`.
    /// Defaults to `context` when absent.
    #[serde(default)]
    pub kind: Option<String>,
    /// Cowork session id (optional).
    #[serde(default)]
    pub session_id: Option<String>,
}

fn parse_kind(s: &str) -> Option<anno_rag::memory::MemoryKind> {
    match s {
        "fact" => Some(anno_rag::memory::MemoryKind::Fact),
        "preference" => Some(anno_rag::memory::MemoryKind::Preference),
        "reference" => Some(anno_rag::memory::MemoryKind::Reference),
        "context" => Some(anno_rag::memory::MemoryKind::Context),
        _ => None,
    }
}

#[derive(Serialize)]
struct MemorySaveResultWire {
    id: String,
    stored_text: String,
    redacted_text: String,
    token_count: usize,
    ner_mode: MemoryNerMode,
}

/// Parameters for the `memory_recall` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryRecallParams {
    /// Free-text query. Pseudonymized at the boundary before search.
    pub query: String,
    /// Max hits to return.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional per-session filter.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional category filter. Strings matching `fact|preference|reference|context`.
    #[serde(default)]
    pub kinds: Option<Vec<String>>,
    /// Bi-temporal cutoff: only memories valid at this instant are returned.
    /// `None` means "now". v0.2.
    #[serde(default)]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    /// When true, expand the top-k hybrid hits via the entity-refs graph
    /// (one-hop neighbours, bounded by `graph_per_hop_limit`). v0.2.
    #[serde(default)]
    pub graph_expand: bool,
    /// When true, re-score the recalled hits with the cross-encoder
    /// reranker. Requires the server built with `--features rerank`.
    #[serde(default)]
    pub rerank: bool,
}

/// Parameters for the `memory_graph_recall` tool (v0.2).
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryGraphRecallParams {
    /// Seed entity — either a canonical id (`pii:LABEL:TOKEN` /
    /// `ent:TAG:value`) or a free-text name (will be canonicalised as
    /// MISC for the lookup).
    pub entity: String,
    /// BFS depth. Capped by `cfg.graph_max_hops` (default 2).
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
    /// Rows scanned per hop. Capped by `cfg.graph_per_hop_limit` (default 50).
    #[serde(default = "default_per_hop_limit")]
    pub per_hop_limit: usize,
    /// Bi-temporal cutoff. `None` means "now".
    #[serde(default)]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_max_hops() -> u8 {
    2
}
fn default_per_hop_limit() -> usize {
    50
}

/// Parameters for the `memory_invalidate` tool (v0.2).
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryInvalidateParams {
    /// Memory id (stringified UUID).
    pub id: String,
    /// Timestamp at which the memory becomes invalid. `None` means "now".
    #[serde(default)]
    pub at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct MemoryInvalidateResultWire {
    id: String,
    invalidated: bool,
    valid_to: String,
}

#[derive(Serialize)]
struct MemoryHitWire {
    id: String,
    text: String,
    kind: String,
    created_at: String,
    entity_refs: Vec<String>,
    score: f32,
}

#[derive(Serialize)]
struct MemoryRecallResultWire {
    hits: Vec<MemoryHitWire>,
}

/// Parameters for the `memory_forget` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryForgetParams {
    /// Forget by exact memory id (stringified UUID).
    #[serde(default)]
    pub id: Option<String>,
    /// Forget the top-`limit` hits of this hybrid-recall query.
    #[serde(default)]
    pub query: Option<String>,
    /// Cap on rows to forget when using `query`.
    #[serde(default = "default_forget_limit")]
    pub limit: usize,
    /// Preview which rows would be forgotten without mutating anything.
    #[serde(default)]
    pub dry_run: bool,
}

fn default_forget_limit() -> usize {
    5
}

#[derive(Serialize)]
struct MemoryForgetResultWire {
    forgotten_ids: Vec<String>,
    vault_tokens_purged: usize,
    note: String,
}

/// Parameters for the `memory_list` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryListParams {
    /// Optional per-session filter.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional category filter — `fact|preference|reference|context`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Page size.
    #[serde(default = "default_list_limit")]
    pub limit: usize,
    /// Pagination cursor — the RFC 3339 `created_at` of the previous
    /// page's last row.
    #[serde(default)]
    pub cursor: Option<String>,
}

fn default_list_limit() -> usize {
    20
}

#[derive(Serialize)]
struct MemoryListResultWire {
    items: Vec<MemoryHitWire>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
struct DownloadModelsResult {
    status: String,
    path: String,
    message: String,
}

// ---- Legal tool param + result types (D1) ----

/// Parameters for the `legal_ingest` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalIngestParams {
    /// Absolute path to the folder containing legal documents to ingest.
    pub folder: String,
    /// When true, recurse into sub-folders. Defaults to false.
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Serialize)]
struct LegalIngestResult {
    ingested: usize,
    folder: String,
}

/// Parameters for the `legal_search` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalSearchParams {
    /// Free-text query. PII is pseudonymized before embedding.
    pub query: String,
    /// Maximum number of results.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional doc_type filter (e.g. `"contract"`, `"judgment"`).
    #[serde(default)]
    pub doc_type: Option<String>,
    /// Optional legal_domain filter (e.g. `"droit_commercial"`, `"droit_travail"`).
    #[serde(default)]
    pub legal_domain: Option<String>,
    /// Optional jurisdiction filter (e.g. `"France"`, `"Paris"`).
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Optional dossier_id filter.
    #[serde(default)]
    pub dossier_id: Option<String>,
    /// Filter to chunks that mention any of these normalized party forms (e.g. `["org:acme"]`).
    #[serde(default)]
    pub parties: Vec<String>,
    /// Filter to chunks with any of these party roles.
    #[serde(default)]
    pub party_roles: Vec<String>,
    /// Filter to chunks that cite any of these normalized article refs (e.g. `["code_civil:1240"]`).
    #[serde(default)]
    pub legal_refs: Vec<String>,
    /// Filter to chunks with any of these clause types.
    #[serde(default)]
    pub clause_types: Vec<String>,
    /// Filter to chunks with any of these obligation kinds.
    #[serde(default)]
    pub obligation_kinds: Vec<String>,
    /// Filter to chunks with any of these risk flags.
    #[serde(default)]
    pub risk_flags: Vec<String>,
    /// Minimum extraction confidence (0–1).
    #[serde(default)]
    pub min_confidence: Option<f32>,
}

#[derive(Serialize)]
struct LegalSearchHitWire {
    chunk_id: String,
    doc_id: String,
    text_pseudo: String,
    score: f32,
}

#[derive(Serialize)]
struct LegalSearchResult {
    hits: Vec<LegalSearchHitWire>,
}

/// Parameters for the `legal_graph_query` tool.
///
/// `intent` discriminator: `"party_dossier"` | `"obligations_owed_by"` |
/// `"citation_chain"` | `"procedural_timeline"` | `"appeal_chain"`.
/// The remaining fields supply the intent's required parameters.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalGraphQueryParams {
    /// Which named traversal to run. One of: party_dossier, obligations_owed_by,
    /// citation_chain, procedural_timeline, appeal_chain.
    pub intent: String,
    /// party_dossier / obligations_owed_by: normalized party identifier.
    pub party: Option<String>,
    /// citation_chain: normalized article reference (e.g. "C.civ.1240").
    pub article_ref: Option<String>,
    /// procedural_timeline: dossier identifier.
    pub dossier_id: Option<String>,
    /// appeal_chain: root document id.
    pub doc_id: Option<String>,
    /// appeal_chain: maximum appeal hops (default 10).
    pub max_depth: Option<u32>,
}

#[derive(Serialize)]
struct LegalGraphQueryResult {
    rows: Vec<std::collections::HashMap<String, String>>,
}

/// Parameters for the `legal_rehydrate_citation` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalRehydrateCitationParams {
    /// Chunk UUID (stringified) to fetch.
    pub chunk_id: String,
    /// UTF-8 byte offset of the citation span start (inclusive).
    pub byte_start: u32,
    /// UTF-8 byte offset of the citation span end (exclusive).
    pub byte_end: u32,
}

#[derive(Serialize)]
struct LegalRehydrateCitationResult {
    text: String,
    tokens_rehydrated: usize,
}

// ---- D2 params/results ----

/// Parameters for `legal_extract_contract`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalExtractContractParams {
    /// Document id to extract a review grid for.
    pub doc_id: String,
}

/// Parameters for `legal_extract_case_file`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalExtractCaseFileParams {
    /// Dossier id to extract a review grid for.
    pub dossier_id: String,
}

/// Parameters for `legal_timeline`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalTimelineParams {
    /// Dossier id to retrieve the procedural timeline for.
    pub dossier_id: String,
}

/// Parameters for `legal_risk_review`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalRiskReviewParams {
    /// Document or dossier id to scope the risk review to.
    pub scope_id: String,
    /// When true, treat `scope_id` as a dossier id; otherwise as a doc id.
    pub is_dossier: bool,
}

// ---- D3 params/results ----

/// Parameters for `legal_mandatory_clause_audit`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalMandatoryClauseAuditParams {
    /// Document UUID (stringified).
    pub doc_id: String,
    /// Document type used to select the checklist
    /// (e.g. `"b2b_contract"`, `"employment"`, `"rgpd"`).
    pub doc_type: String,
}

// ---- D4 params/results ----

/// Parameters for `legal_prescription_check`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalPrescriptionCheckParams {
    /// Prescription category (e.g. `"contractuel"`, `"responsabilite_decennale"`).
    pub category: String,
    /// Anchor event date in ISO-8601 format (e.g. `"2020-01-15T00:00:00Z"`).
    pub event_date: String,
    /// Interrupting events (mise en demeure, assignation, etc.).
    pub interrupting_events: Vec<LegalInterruptingEventWire>,
}

/// Wire representation of an event that interrupts or suspends prescription.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalInterruptingEventWire {
    /// Event kind, e.g. `"mise_en_demeure"`, `"assignation"`.
    pub kind: String,
    /// ISO-8601 date of the interrupting event.
    pub date: String,
}

// ---- D5 params/results ----

/// Parameters for `legal_validate_field`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalValidateFieldParams {
    /// Chunk UUID (stringified) that contains the extracted fact.
    pub chunk_id: String,
    /// Field name being validated (e.g. `"obligation:paiement"`).
    pub field_name: String,
    /// Action: `"confirm"`, `"reject"`, or `"correct"`.
    pub action: String,
    /// Corrected value when action is `"correct"`.
    pub corrected_value: Option<String>,
    /// Optional free-text note from the reviewer.
    pub note: Option<String>,
    /// Optional reviewer identifier (email or system name).
    pub actor: Option<String>,
}

// ---- Tabular review tool param + result types ----

/// Parameters for `review_create`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewCreateParams {
    /// Human-readable review name (e.g. "NDA batch 2026-05").
    pub name: String,
    /// Built-in template id to load columns from. One of:
    /// nda-v1, customer-contract-v1, real-estate-v1, employment-v1, ip-v1.
    /// When absent an empty review is created (add columns separately).
    #[serde(default)]
    pub template_id: Option<String>,
    /// Optional folder path scoped to this review (informational only).
    #[serde(default)]
    pub scope_folder: Option<String>,
}

/// Parameters for `review_add_rows`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewAddRowsParams {
    /// Review UUID (returned by review_create).
    pub review_id: String,
    /// List of document UUIDs to add as rows. Must already be ingested.
    pub doc_ids: Vec<String>,
    /// When true, force re-extraction of all columns even if cells exist.
    #[serde(default)]
    pub force_reextract: bool,
}

/// Parameters for `review_refine_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewRefineCellParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// Extra instruction prepended to the column prompt for this re-extraction.
    pub instruction: String,
}

/// Parameters for `review_set_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewSetCellParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// New cell value (any JSON: string, number, bool, array, object).
    pub value: serde_json::Value,
    /// Lock the cell after writing so it cannot be auto-overwritten.
    #[serde(default)]
    pub lock: bool,
    /// Reviewer identifier (email or name).
    #[serde(default)]
    pub actor: Option<String>,
}

/// Parameters for `review_lock_cell` and `review_unlock_cell`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewCellLockParams {
    /// Review UUID.
    pub review_id: String,
    /// Row UUID.
    pub row_id: String,
    /// Column UUID.
    pub col_id: String,
    /// Reviewer identifier (email or name).
    #[serde(default)]
    pub actor: Option<String>,
}

/// Parameters for `review_export`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewExportParams {
    /// Review UUID.
    pub review_id: String,
    /// Export format: "csv", "markdown", or "xlsx".
    #[serde(default = "default_export_format")]
    pub format: String,
    /// Absolute path where the XLSX file will be written. Required when
    /// format is "xlsx". Ignored for csv/markdown (returned as string).
    #[serde(default)]
    pub output_path: Option<String>,
}

fn default_export_format() -> String {
    "csv".into()
}

/// Parameters for `review_get`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewGetParams {
    /// Review UUID.
    pub review_id: String,
}

#[derive(Serialize)]
struct ReviewCreateResult {
    review_id: String,
    name: String,
    columns_loaded: usize,
}

#[derive(Serialize)]
struct ReviewAddRowsResult {
    rows_added: usize,
    extraction_started: bool,
    failed_doc_ids: Vec<String>,
}

#[derive(Serialize)]
struct ReviewRefineCellResult {
    ok: bool,
    note: String,
}

#[derive(Serialize)]
struct ReviewSetCellResult {
    ok: bool,
    locked: bool,
}

#[derive(Serialize)]
struct ReviewCellLockResult {
    ok: bool,
    locked: bool,
}

#[derive(Serialize)]
struct ReviewGetResult {
    review_id: String,
    name: String,
    columns: Vec<ReviewColumnWire>,
    rows: Vec<ReviewRowWire>,
    cells: Vec<ReviewCellWire>,
}

#[derive(Serialize)]
struct ReviewColumnWire {
    id: String,
    name: String,
    prompt: String,
    order: u32,
}

#[derive(Serialize)]
struct ReviewRowWire {
    id: String,
    doc_id: String,
    doc_label: String,
}

#[derive(Serialize)]
struct ReviewCellWire {
    row_id: String,
    col_id: String,
    value: serde_json::Value,
    confidence: String,
    support_score: f32,
    locked: bool,
    version: u32,
}

// ---- Internal tool implementations ----

impl AnnoRagServer {
    async fn search_impl(&self, params: SearchParams) -> Result<serde_json::Value, String> {
        let p = self.pipeline().await.map_err(|e| e.to_string())?;
        let result = if params.rerank {
            #[cfg(feature = "rerank")]
            {
                tracing::info!(
                    target: "anno_rag::audit",
                    tool = "search",
                    score_source = "cross_encoder",
                    ""
                );
                p.search_reranked(&params.query, params.top_k, self.cfg.rerank_pool_size)
                    .await
            }
            #[cfg(not(feature = "rerank"))]
            {
                return Err("rerank requested but server built without \
                        the `rerank` feature"
                    .to_string());
            }
        } else {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "search",
                score_source = "rrf",
                ""
            );
            p.search(&params.query, params.top_k).await
        };
        let hits = result.map_err(|e| e.to_string())?;
        let wire = SearchResult {
            hits: hits
                .into_iter()
                .map(|h| SearchHitWire {
                    doc_id: h.doc_id.to_string(),
                    chunk_id: h.chunk_id.to_string(),
                    source_path: h.source_path,
                    folder_path: h.folder_path,
                    chunk_idx: h.chunk_idx,
                    text_pseudo: h.text_pseudo,
                    page: h.page,
                    char_start: h.char_start,
                    char_end: h.char_end,
                    score: h.score,
                })
                .collect(),
        };
        serde_json::to_value(wire).map_err(|e| e.to_string())
    }

    async fn vault_stats_impl(&self) -> Result<serde_json::Value, String> {
        let p = self.pipeline().await.map_err(|e| e.to_string())?;
        let s = p.vault_stats().await;
        let wire = VaultStatsResult {
            total_mappings: s.total_mappings,
            categories: s.categories,
        };
        serde_json::to_value(wire).map_err(|e| e.to_string())
    }

    async fn legal_ingest_impl(&self, p: LegalIngestParams) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let folder = std::path::Path::new(&p.folder);
        let out = folder.join("anon");
        let start = std::time::Instant::now();
        match pipeline.ingest_folder(folder, p.recursive, &out).await {
            Ok(n) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_ingest",
                    result = "ok",
                    duration_ms = start.elapsed().as_millis() as u64,
                    ingested = n,
                    ""
                );
                serde_json::to_value(LegalIngestResult {
                    ingested: n,
                    folder: p.folder,
                })
                .map_err(|e| e.to_string())
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_ingest",
                    result = "error",
                    "{e}"
                );
                Err(e.to_string())
            }
        }
    }

    async fn legal_search_impl(&self, p: LegalSearchParams) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let filters = anno_rag::legal::types::LegalSearchFilters {
            doc_type: p.doc_type,
            legal_domain: p.legal_domain,
            jurisdiction: p.jurisdiction,
            dossier_id: p.dossier_id,
            parties: p.parties,
            party_roles: p.party_roles,
            legal_refs: p.legal_refs,
            clause_types: p.clause_types,
            obligation_kinds: p.obligation_kinds,
            risk_flags: p.risk_flags,
            min_confidence: p.min_confidence,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        match pipeline.legal_search(&p.query, p.top_k, filters).await {
            Ok(hits) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_search",
                    result = "ok",
                    duration_ms = start.elapsed().as_millis() as u64,
                    n = hits.len(),
                    ""
                );
                serde_json::to_value(LegalSearchResult {
                    hits: hits
                        .into_iter()
                        .map(|h| LegalSearchHitWire {
                            chunk_id: h.chunk_id.to_string(),
                            doc_id: h.doc_id.to_string(),
                            text_pseudo: h.text_pseudo,
                            score: h.score,
                        })
                        .collect(),
                })
                .map_err(|e| e.to_string())
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_search",
                    result = "error",
                    "{e}"
                );
                Err(e.to_string())
            }
        }
    }

    async fn knowledge_sources_impl(&self) -> Result<Vec<serde_json::Value>, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        Ok(service.sources())
    }

    async fn knowledge_status_impl(&self) -> Result<anno_knowledge_core::KnowledgeStatus, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.status().map_err(|e| e.to_string())
    }

    async fn knowledge_search_impl(
        &self,
        p: crate::knowledge::KnowledgeSearchParams,
    ) -> Result<crate::knowledge::KnowledgeSearchResponse, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.search(p).map_err(|e| e.to_string())
    }

    async fn knowledge_add_local_folder_impl(&self, path: &str) -> Result<String, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.add_local_folder(path).map_err(|e| e.to_string())
    }

    async fn knowledge_sync_impl(&self, p: KnowledgeSyncParams) -> Result<SyncSummary, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        service
            .sync(pipeline, self.cfg.as_ref(), p.source_id.as_deref())
            .await
    }

    pub(crate) async fn index_impl_routing(&self, p: IndexParams) -> String {
        if let Err(error) = validate_profile(&p.profile) {
            return serde_json::json!({
                "ok": false,
                "error": error,
            })
            .to_string();
        }

        let mut knowledge = serde_json::Value::Null;
        let mut legal = serde_json::Value::Null;
        let mut errors = Vec::new();

        if matches!(p.profile.as_str(), "general" | "all") {
            match self.knowledge_add_local_folder_impl(&p.path).await {
                Ok(source_id) => {
                    match self
                        .knowledge_sync_impl(KnowledgeSyncParams {
                            source_id: Some(source_id.clone()),
                        })
                        .await
                    {
                        Ok(summary) => {
                            if let Some(issue) = knowledge_sync_issue(&summary) {
                                errors.push(issue);
                            }
                            knowledge = serde_json::json!({
                                "source_id": source_id,
                                "summary": summary,
                            });
                        }
                        Err(e) => errors.push(format!("knowledge sync: {e}")),
                    }
                }
                Err(e) => errors.push(format!("knowledge add: {e}")),
            }
        }

        if matches!(p.profile.as_str(), "legal" | "all") {
            match self
                .legal_ingest_impl(LegalIngestParams {
                    folder: p.path.clone(),
                    recursive: true,
                })
                .await
            {
                Ok(value) => legal = value,
                Err(e) => errors.push(format!("legal ingest: {e}")),
            }
        }

        let errors = if errors.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!(errors)
        };

        serde_json::json!({
            "ok": errors.is_null(),
            "profile": p.profile,
            "knowledge": knowledge,
            "legal": legal,
            "errors": errors,
        })
        .to_string()
    }

    async fn knowledge_forget_impl(&self, p: KnowledgeForgetParams) -> Result<u64, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service
            .forget_source(&p.source_id)
            .map_err(|e| e.to_string())
    }
}

// ---- Tool router ----

#[tool_router]
impl AnnoRagServer {
    /// Search the indexed corpus. Pseudonymizes the query through the local
    /// vault, embeds it, and returns top-K ranked pseudonymized chunks.
    #[tool(
        description = "Search the indexed corpus. Pseudonymizes the query through the local vault, embeds it, returns top-K ranked chunks. Chunks are pseudonymized — call rehydrate(text) to restore originals."
    )]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        match self.search_impl(params).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Replace pseudo-tokens in text back with original PII from the vault.
    #[tool(
        description = "Replace pseudo-tokens (PERSON_1, EMAIL_2, NIR_3, etc.) in text back with original PII from the local vault."
    )]
    async fn rehydrate(&self, Parameters(params): Parameters<RehydrateParams>) -> String {
        let p = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        match p.rehydrate(&params.text).await {
            Ok(r) => {
                let wire = RehydrateResult {
                    text: r.text,
                    tokens_rehydrated: r.tokens_rehydrated,
                };
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Dry-run PII detection without replacement.
    #[tool(
        description = "Dry-run scan: detect PII in text without replacing. Returns category, source, confidence, and char offsets for each entity."
    )]
    async fn detect(&self, Parameters(params): Parameters<DetectParams>) -> String {
        let p = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        match p.detect(&params.text) {
            Ok(entities) => {
                let info: Vec<EntityInfo> = entities
                    .into_iter()
                    .map(|e| EntityInfo {
                        original: e.original,
                        category: format!("{:?}", e.category),
                        confidence: e.confidence,
                        source: format!("{:?}", e.source),
                        start: e.start,
                        end: e.end,
                    })
                    .collect();
                serde_json::to_string_pretty(&DetectResult { entities: info })
                    .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Vault diagnostics: total token mappings and per-category counts.
    #[tool(description = "Vault statistics: total token mappings and per-category counts.")]
    async fn vault_stats(&self) -> String {
        match self.vault_stats_impl().await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Initialize the vault keyring entry from a user-supplied passphrase
    /// (spec §14.3 Path B). Passphrase is Argon2id-derived; never logged.
    #[tool(
        description = "Initialize the vault keyring entry from a user-supplied passphrase (≥12 chars). Use on first setup if you want to provide your own passphrase. The passphrase is never logged."
    )]
    async fn anno_init_vault(&self, Parameters(params): Parameters<InitVaultParams>) -> String {
        let result = crate::health::init_vault_with_passphrase(&params.passphrase);
        serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Engine health — version, build target, available tools, vault status.
    #[tool(
        description = "Engine health: version, build target, available tools, vault initialization status. Side-effect-free. Call once per session before other anno tools to verify compatibility."
    )]
    async fn anno_health(&self) -> String {
        // Check vault independently of whether the pipeline has been lazy-
        // inited.  pipeline_arc() returns None until the first memory/search
        // call (OnceCell), so going through it always yields false on a fresh
        // server — even after anno_init_vault has written to the keyring.
        let vault_initialized = if let Some(arc) = self.pipeline_arc() {
            arc.vault_is_initialized()
        } else {
            // Pipeline not yet loaded — read keyring directly.
            use anno_rag::vault::{KEYRING_ACCOUNT, KEYRING_SERVICE};
            keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .ok()
                .and_then(|e| e.get_password().ok())
                .is_some()
        };
        let h = crate::health::EngineHealth {
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            build_target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            signed: option_env!("ANNO_RAG_SIGNED_BUILD").is_some(),
            extension_install: option_env!("ANNO_RAG_EXTENSION_INSTALL").is_some(),
            vault_initialized,
            available_tools: crate::health::all_tool_names(),
        };
        serde_json::to_string_pretty(&h).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Save a memory. Default mode stores raw text immediately and enriches NER refs asynchronously.
    #[tool(
        description = "Save a memory. Default async mode stores raw text immediately and enriches NER refs in the background. Returns the new id and stored text."
    )]
    async fn memory_save(&self, Parameters(p): Parameters<MemorySaveParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        let kind = p.kind.as_deref().and_then(parse_kind);
        let session_id = p.session_id.clone();
        let r = pipeline
            .save_memory(&p.text, kind, session_id.clone())
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(s) => {
                if self.cfg.memory_ner_mode == MemoryNerMode::Async {
                    if let Some(arc_pipeline) = self.pipeline_arc() {
                        let id = s.id.clone();
                        let text = p.text.clone();
                        tokio::spawn(async move {
                            if let Err(e) = arc_pipeline
                                .save_memory_ner_task(id.clone(), text, kind, session_id)
                                .await
                            {
                                tracing::warn!(
                                    target: "anno_rag::memory::audit",
                                    tool = "memory_save_ner_task",
                                    memory_id = %id.as_string(),
                                    result = "error",
                                    "{e}"
                                );
                            }
                        });
                    } else {
                        tracing::warn!(
                            target: "anno_rag::memory::audit",
                            tool = "memory_save",
                            "pipeline_arc() returned None after successful pipeline init; async NER task skipped"
                        );
                    }
                }
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_save",
                    result = "ok",
                    duration_ms = elapsed,
                    ""
                );
                let wire = MemorySaveResultWire {
                    id: s.id.as_string(),
                    stored_text: s.stored_text,
                    redacted_text: s.redacted_text,
                    token_count: s.token_refs.len(),
                    ner_mode: s.ner_mode,
                };
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_save",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// Hybrid recall over memories. Returns rehydrated plaintext.
    #[tool(
        description = "Recall memories by hybrid (vector + FTS) search. Returns rehydrated plaintext for the caller's tenant."
    )]
    async fn memory_recall(&self, Parameters(p): Parameters<MemoryRecallParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        let kinds = p
            .kinds
            .as_ref()
            .map(|v| v.iter().filter_map(|k| parse_kind(k)).collect::<Vec<_>>());
        let r = if p.rerank {
            #[cfg(feature = "rerank")]
            {
                tracing::info!(
                    target: "anno_rag::audit",
                    tool = "memory_recall",
                    score_source = "cross_encoder",
                    ""
                );
                pipeline
                    .recall_memory_reranked(
                        &p.query,
                        p.top_k,
                        p.session_id,
                        kinds,
                        p.as_of,
                        p.graph_expand,
                        self.cfg.rerank_pool_size,
                    )
                    .await
            }
            #[cfg(not(feature = "rerank"))]
            {
                return "Error: rerank requested but server built without \
                        the `rerank` feature"
                    .to_string();
            }
        } else {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "memory_recall",
                score_source = "rrf",
                ""
            );
            pipeline
                .recall_memory(
                    &p.query,
                    p.top_k,
                    p.session_id,
                    kinds,
                    p.as_of,
                    p.graph_expand,
                )
                .await
        };
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(hits) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_recall",
                    result = "ok",
                    duration_ms = elapsed,
                    n = hits.len(),
                    ""
                );
                let wire = MemoryRecallResultWire {
                    hits: hits
                        .into_iter()
                        .map(|h| MemoryHitWire {
                            id: h.id,
                            text: h.text,
                            kind: format!("{:?}", h.kind).to_lowercase(),
                            created_at: h.created_at,
                            entity_refs: h.entity_refs,
                            score: h.score,
                        })
                        .collect(),
                };
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_recall",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// Forget memories by id or by query. Cascades vault tokens.
    #[tool(
        description = "Forget memories by id or by query. Cascades to vault tokens no longer referenced. Returns the SLO note that physical erasure may take up to 24h."
    )]
    async fn memory_forget(&self, Parameters(p): Parameters<MemoryForgetParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let id = match &p.id {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(u) => Some(anno_rag::memory::MemoryId(u)),
                Err(e) => return format!("Error: bad id: {e}"),
            },
            None => None,
        };
        let start = std::time::Instant::now();
        let r = pipeline
            .forget_memory(id, p.query, p.limit, p.dry_run)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(res) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_forget",
                    result = "ok",
                    duration_ms = elapsed,
                    n = res.forgotten_ids.len(),
                    ""
                );
                let wire = MemoryForgetResultWire {
                    forgotten_ids: res.forgotten_ids,
                    vault_tokens_purged: res.vault_tokens_purged,
                    note: "logically forgotten; physical erasure within 24h".into(),
                };
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_forget",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// List memories with cursor pagination.
    #[tool(description = "List memories with optional session/kind filter and cursor pagination.")]
    async fn memory_list(&self, Parameters(p): Parameters<MemoryListParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        let kind = p.kind.as_deref().and_then(parse_kind);
        let r = pipeline
            .list_memories(p.session_id, kind, p.limit, p.cursor)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(page) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_list",
                    result = "ok",
                    duration_ms = elapsed,
                    n = page.items.len(),
                    ""
                );
                let wire = MemoryListResultWire {
                    items: page
                        .items
                        .into_iter()
                        .map(|h| MemoryHitWire {
                            id: h.id,
                            text: h.text,
                            kind: format!("{:?}", h.kind).to_lowercase(),
                            created_at: h.created_at,
                            entity_refs: h.entity_refs,
                            score: h.score,
                        })
                        .collect(),
                    next_cursor: page.next_cursor,
                };
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_list",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// v0.2 graph-recall: 2-hop BFS over entity_refs from a seed entity.
    #[tool(
        description = "Graph-expand from a seed entity over the entity_refs index. Returns the connected subgraph (entities + memories + edges) up to max_hops (default 2)."
    )]
    async fn memory_graph_recall(
        &self,
        Parameters(p): Parameters<MemoryGraphRecallParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        let r = pipeline
            .graph_recall(&p.entity, p.max_hops, p.per_hop_limit, p.as_of)
            .await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(res) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_graph_recall",
                    result = "ok",
                    duration_ms = elapsed,
                    nodes = res.nodes.len(),
                    edges = res.edges.len(),
                    memories = res.memories.len(),
                    ""
                );
                serde_json::to_string_pretty(&res).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_graph_recall",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// v0.2 memory_invalidate: mark a memory invalid at `at` (default now).
    #[tool(
        description = "Mark a memory as no longer valid as of the given timestamp (default: now). No-op if valid_to is already set."
    )]
    async fn memory_invalidate(&self, Parameters(p): Parameters<MemoryInvalidateParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let id = match uuid::Uuid::parse_str(&p.id) {
            Ok(u) => anno_rag::memory::MemoryId(u),
            Err(e) => return format!("Error: bad id: {e}"),
        };
        let when = p.at.unwrap_or_else(chrono::Utc::now);
        let start = std::time::Instant::now();
        let r = pipeline.invalidate_memory(&id, Some(when)).await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(invalidated) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_invalidate",
                    result = if invalidated { "ok" } else { "noop" },
                    duration_ms = elapsed,
                    ""
                );
                serde_json::to_string_pretty(&MemoryInvalidateResultWire {
                    id: p.id,
                    invalidated,
                    valid_to: when.to_rfc3339(),
                })
                .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_invalidate",
                    result = "error",
                    duration_ms = elapsed,
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// Download anno-rag model weights (~970 MB) to the local machine in the
    /// background. Returns immediately. Call again in a few minutes to check
    /// if ready. No parameters needed.
    #[tool(
        description = "Download anno-rag model weights (~970 MB) to the local machine. \
                       Returns immediately — download runs in the background. \
                       Call again in a few minutes to check if ready. \
                       No parameters needed."
    )]
    async fn download_models(&self) -> String {
        let models_dir = self.cfg.models_cache();
        let e5_dir = models_dir.join("multilingual-e5-small");
        let gliner_dir = models_dir.join("gliner2-multi-v1-onnx");

        // Already present — nothing to do.
        if e5_dir.exists() && gliner_dir.exists() {
            let wire = DownloadModelsResult {
                status: "already_present".into(),
                path: models_dir.display().to_string(),
                message: format!(
                    "Models already present at {}. \
                     Set ANNO_MODELS_DIR={} in extension settings \
                     (or leave blank — the default path is detected automatically).",
                    models_dir.display(),
                    models_dir.display()
                ),
            };
            return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
        }

        // Download already in progress (sentinel file present).
        let lock_file = models_dir.join(".download-lock");
        if lock_file.exists() {
            let wire = DownloadModelsResult {
                status: "in_progress".into(),
                path: models_dir.display().to_string(),
                message: format!(
                    "Download already in progress to {}. \
                     Ask again in a few minutes.",
                    models_dir.display()
                ),
            };
            return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
        }

        // Start a new background download.
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            return format!("Error: could not create models dir: {e}");
        }
        if let Err(e) = std::fs::write(&lock_file, b"downloading") {
            return format!("Error: could not write lock file: {e}");
        }

        let cfg_clone = Arc::clone(&self.cfg);
        tokio::task::spawn(async move {
            let result = anno_rag::download_models::download(&cfg_clone).await;
            // Remove lock on both success and failure so next call can retry.
            let _ = std::fs::remove_file(cfg_clone.models_cache().join(".download-lock"));
            match result {
                Ok(_) => tracing::info!(
                    target: "anno_rag::mcp::download_models",
                    "background model download complete"
                ),
                Err(e) => tracing::warn!(
                    target: "anno_rag::mcp::download_models",
                    "background model download failed: {e}"
                ),
            }
        });

        let wire = DownloadModelsResult {
            status: "downloading".into(),
            path: models_dir.display().to_string(),
            message: format!(
                "Downloading anno-rag models to {} (~970 MB total). \
                 This runs in the background and takes 2–15 minutes. \
                 Ask me again in a few minutes — I will confirm when ready.",
                models_dir.display()
            ),
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Ingest a folder of legal documents into the anno-rag index. Documents are
    /// pseudonymized through the local vault and enriched with French legal entities.
    #[tool(
        description = "Ingest a folder of legal documents (PDF, DOCX, TXT, …). \
                       PII is pseudonymized through the local vault. \
                       Legal entities (parties, clauses, citations, obligations, risks) \
                       are extracted and stored for filtered search and graph traversal."
    )]
    async fn legal_ingest(&self, Parameters(p): Parameters<LegalIngestParams>) -> String {
        match self.legal_ingest_impl(p).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Legal-filtered hybrid search. Pseudonymizes the query and restricts
    /// vector + FTS results to chunks matching the supplied legal filters.
    #[tool(
        description = "Hybrid search over the legal corpus with optional filters: \
                       doc_type, legal_domain, jurisdiction, dossier_id, parties, \
                       party_roles, legal_refs, clause_types, obligation_kinds, \
                       risk_flags, min_confidence. \
                       Returns pseudonymized chunk text — call legal_rehydrate_citation \
                       to restore originals."
    )]
    async fn legal_search(&self, Parameters(p): Parameters<LegalSearchParams>) -> String {
        match self.legal_search_impl(p).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Execute a named graph traversal intent over the legal knowledge graph.
    /// Phase 1: returns an empty row set (real lance-graph execution in Stage D).
    #[tool(
        description = "Run a named graph traversal over the legal knowledge graph. \
                       Intents: party_dossier, obligations_owed_by, citation_chain, \
                       procedural_timeline, appeal_chain. \
                       Returns rows from the Cypher result set."
    )]
    async fn legal_graph_query(&self, Parameters(p): Parameters<LegalGraphQueryParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        use anno_rag::legal::query::GraphIntent;
        let intent = match p.intent.as_str() {
            "party_dossier" => match p.party {
                Some(party) => GraphIntent::PartyDossier { party },
                None => return "Error: party_dossier requires `party`".into(),
            },
            "obligations_owed_by" => match p.party {
                Some(party) => GraphIntent::ObligationsOwedBy { party },
                None => return "Error: obligations_owed_by requires `party`".into(),
            },
            "citation_chain" => match p.article_ref {
                Some(article_ref) => GraphIntent::CitationChain { article_ref },
                None => return "Error: citation_chain requires `article_ref`".into(),
            },
            "procedural_timeline" => match p.dossier_id {
                Some(dossier_id) => GraphIntent::ProceduralTimeline { dossier_id },
                None => return "Error: procedural_timeline requires `dossier_id`".into(),
            },
            "appeal_chain" => match p.doc_id {
                Some(doc_id) => GraphIntent::AppealChain {
                    doc_id,
                    max_depth: p.max_depth.unwrap_or(10),
                },
                None => return "Error: appeal_chain requires `doc_id`".into(),
            },
            other => {
                return format!(
                    "Error: unknown intent `{other}`. \
                 Valid: party_dossier, obligations_owed_by, citation_chain, \
                 procedural_timeline, appeal_chain"
                )
            }
        };
        let start = std::time::Instant::now();
        match pipeline.legal_graph_query(intent).await {
            Ok(result) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_graph_query",
                    result = "ok",
                    duration_ms = start.elapsed().as_millis() as u64,
                    rows = result.rows.len(),
                    ""
                );
                serde_json::to_string_pretty(&LegalGraphQueryResult { rows: result.rows })
                    .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_graph_query",
                    result = "error",
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    /// Rehydrate a citation span from a stored chunk. Fetches the chunk by UUID,
    /// slices the given byte range from its pseudonymized text, and restores
    /// PII tokens through the local vault.
    #[tool(
        description = "Rehydrate a citation span (byte_start..byte_end) from a stored \
                       pseudonymized chunk. Returns the original plaintext for the span. \
                       Use chunk_id + offsets from legal_search results."
    )]
    async fn legal_rehydrate_citation(
        &self,
        Parameters(p): Parameters<LegalRehydrateCitationParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let chunk_id = match uuid::Uuid::parse_str(&p.chunk_id) {
            Ok(u) => u,
            Err(e) => return format!("Error: bad chunk_id: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline
            .legal_rehydrate_citation(chunk_id, p.byte_start, p.byte_end)
            .await
        {
            Ok(r) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_rehydrate_citation",
                    result = "ok",
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&LegalRehydrateCitationResult {
                    text: r.text,
                    tokens_rehydrated: r.tokens_rehydrated,
                })
                .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_rehydrate_citation",
                    result = "error",
                    "{e}"
                );
                format!("Error: {e}")
            }
        }
    }

    // ── D2 handlers ──────────────────────────────────────────────────────────

    /// Extract a contract review grid from the legal KG for a given document.
    #[tool(
        description = "Extract a contract review grid (parties, obligations, clauses) \
                       from the legal knowledge graph for `doc_id`. \
                       Returns one row per extracted field with chunk provenance."
    )]
    async fn legal_extract_contract(
        &self,
        Parameters(p): Parameters<LegalExtractContractParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_extract_contract(&p.doc_id).await {
            Ok(review) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_extract_contract",
                    doc_id = p.doc_id,
                    rows = review.rows.len(),
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&review).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Extract a case-file review grid from the legal KG for a given dossier.
    #[tool(
        description = "Extract a case-file review grid (documents, parties, events) \
                       from the legal knowledge graph for `dossier_id`."
    )]
    async fn legal_extract_case_file(
        &self,
        Parameters(p): Parameters<LegalExtractCaseFileParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_extract_case_file(&p.dossier_id).await {
            Ok(review) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_extract_case_file",
                    dossier_id = p.dossier_id,
                    rows = review.rows.len(),
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&review).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Retrieve the procedural timeline for a dossier, ordered chronologically.
    #[tool(description = "Retrieve the procedural timeline for `dossier_id`. \
                       Returns events (kind, event_date, deadline_date, chunk_id) \
                       in chronological order.")]
    async fn legal_timeline(&self, Parameters(p): Parameters<LegalTimelineParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_timeline(&p.dossier_id).await {
            Ok(tl) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_timeline",
                    dossier_id = p.dossier_id,
                    events = tl.events.len(),
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&tl).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Retrieve risk findings for a document or dossier, with lawyer recommendations.
    #[tool(
        description = "Retrieve risk findings for `scope_id`. Set `is_dossier: true` \
                       to collect risks across all documents in the dossier. \
                       Returns findings sorted severity-descending with French-language \
                       lawyer recommendations."
    )]
    async fn legal_risk_review(&self, Parameters(p): Parameters<LegalRiskReviewParams>) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_risk_review(&p.scope_id, p.is_dossier).await {
            Ok(review) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_risk_review",
                    scope_id = p.scope_id,
                    findings = review.findings.len(),
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&review).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── D3 handler ────────────────────────────────────────────────────────────

    /// Run the mandatory-clause checklist for a document and return per-requirement
    /// status with French law references.
    #[tool(
        description = "Audit mandatory clauses for a document. Supported doc_types: \
                       b2b_contract, b2c_contract, employment, lease_commercial, \
                       lease_residential, rgpd. \
                       Returns per-requirement status (present/missing) and aggregate \
                       status (complete/partial/missing) with French law references."
    )]
    async fn legal_mandatory_clause_audit(
        &self,
        Parameters(p): Parameters<LegalMandatoryClauseAuditParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let doc_id = match uuid::Uuid::parse_str(&p.doc_id) {
            Ok(u) => u,
            Err(e) => return format!("Error: bad doc_id: {e}"),
        };
        let start = std::time::Instant::now();
        match pipeline
            .legal_mandatory_clause_audit(doc_id, &p.doc_type)
            .await
        {
            Ok(result) => {
                anno_rag::legal::audit::audit_mandatory(doc_id, &result.status);
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_mandatory_clause_audit",
                    doc_type = p.doc_type,
                    status = result.status,
                    duration_ms = start.elapsed().as_millis() as u64,
                    ""
                );
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── D4 handler ────────────────────────────────────────────────────────────

    /// Compute the French prescription deadline for a given category and anchor date.
    #[tool(
        description = "Compute the French prescription deadline for `category` \
                       (contractuel, quasi_contrat, delictuel, responsabilite_decennale, \
                       biennale_consommation, garantie_vices, action_prud_homale, \
                       prescription_penale_crime). \
                       Accepts interrupting events (mise_en_demeure, assignation, etc.) \
                       that restart the prescription period. Returns prescribes_on date \
                       and is_prescribed flag."
    )]
    async fn legal_prescription_check(
        &self,
        Parameters(p): Parameters<LegalPrescriptionCheckParams>,
    ) -> String {
        use anno_rag::legal::prescription::InterruptingEvent;

        let event_date: chrono::DateTime<chrono::Utc> = match p.event_date.parse() {
            Ok(d) => d,
            Err(e) => return format!("Error: bad event_date: {e}"),
        };

        let mut interrupting = Vec::new();
        for ev in &p.interrupting_events {
            match ev.date.parse::<chrono::DateTime<chrono::Utc>>() {
                Ok(d) => interrupting.push(InterruptingEvent {
                    kind: ev.kind.clone(),
                    date: d,
                }),
                Err(e) => return format!("Error: bad interrupting event date '{}': {e}", ev.date),
            }
        }

        match anno_rag::pipeline::Pipeline::legal_prescription_check(
            &p.category,
            event_date,
            &interrupting,
        ) {
            Some(result) => {
                anno_rag::legal::audit::audit_prescription(
                    uuid::Uuid::new_v4(),
                    result.prescribes_on,
                );
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
            }
            None => format!(
                "Error: unknown prescription category '{}'. \
                 Valid: contractuel, quasi_contrat, delictuel, responsabilite_decennale, \
                 biennale_consommation, garantie_vices, action_prud_homale, \
                 prescription_penale_crime",
                p.category
            ),
        }
    }

    // ── D5 handler ────────────────────────────────────────────────────────────

    /// Record a human or automated validation of an extracted fact.
    #[tool(description = "Record a validation of an extracted fact. \
                       action: confirm | reject | correct. \
                       When action is 'correct', supply corrected_value. \
                       Writes a Validation node to the KG linked to the chunk. \
                       Returns the validation_id for audit tracing.")]
    async fn legal_validate_field(
        &self,
        Parameters(p): Parameters<LegalValidateFieldParams>,
    ) -> String {
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let chunk_id = match uuid::Uuid::parse_str(&p.chunk_id) {
            Ok(u) => u,
            Err(e) => return format!("Error: bad chunk_id: {e}"),
        };
        let action = match p.action.as_str() {
            "confirm" => anno_rag::pipeline::ValidationAction::Confirm,
            "reject" => anno_rag::pipeline::ValidationAction::Reject,
            "correct" => anno_rag::pipeline::ValidationAction::Correct,
            other => {
                return format!("Error: unknown action '{other}'. Valid: confirm, reject, correct")
            }
        };
        match pipeline
            .legal_validate_field(
                chunk_id,
                p.field_name,
                action,
                p.corrected_value,
                p.note,
                p.actor,
            )
            .await
        {
            Ok(ack) => serde_json::to_string_pretty(&ack).unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Tabular review tools ────────────────────────────────────────────────

    /// Create a tabular review. Optionally load columns from a built-in
    /// template (nda-v1, customer-contract-v1, real-estate-v1,
    /// employment-v1, ip-v1). Returns the new review_id.
    #[tool(
        description = "Create a tabular review. Optionally materialise columns from a \
                       built-in template. Returns review_id. \
                       Built-in templates: nda-v1, customer-contract-v1, \
                       real-estate-v1, employment-v1, ip-v1."
    )]
    async fn review_create(&self, Parameters(p): Parameters<ReviewCreateParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = anno_rag_tabular::ReviewId::new();
        let review = anno_rag_tabular::storage::reviews::Review {
            id: review_id,
            name: p.name.clone(),
            project_id: None,
            template_id: p.template_id.clone(),
            scope_folder: p.scope_folder.clone(),
            created_at: chrono::Utc::now(),
            schema_version: 1,
        };
        if let Err(e) = ts.reviews.create(&review).await {
            return format!("Error: {e}");
        }
        let mut columns_loaded = 0usize;
        if let Some(tid) = &p.template_id {
            match anno_rag_tabular::schema::template::Template::builtin(tid) {
                Ok(tmpl) => {
                    let cols = tmpl.into_columns(review_id);
                    columns_loaded = cols.len();
                    for col in cols {
                        if let Err(e) = ts.columns.add(review_id, &col).await {
                            return format!("Error adding column: {e}");
                        }
                    }
                }
                Err(e) => return format!("Error loading template: {e}"),
            }
        }
        serde_json::to_string_pretty(&ReviewCreateResult {
            review_id: review_id.0.to_string(),
            name: p.name,
            columns_loaded,
        })
        .unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Add document rows to a review and kick off background extraction.
    /// `doc_ids` must be UUIDs of documents already ingested into anno-rag.
    #[tool(
        description = "Add rows (one per document) to a review and start background \
                       LLM extraction. Provide doc_ids as UUID strings from ingested \
                       documents. Extraction runs concurrently (max 8 rows)."
    )]
    async fn review_add_rows(&self, Parameters(p): Parameters<ReviewAddRowsParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        // Verify the review exists before adding rows (mirrors CLI behaviour).
        match ts.reviews.get(review_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return format!("Error: review {} not found", review_id.0),
            Err(e) => return format!("Error: {e}"),
        }
        let mut rows_added = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for id_str in &p.doc_ids {
            let doc_id = match uuid::Uuid::parse_str(id_str) {
                Ok(u) => u,
                Err(_) => {
                    failed.push(id_str.clone());
                    continue;
                }
            };
            let row = anno_rag_tabular::storage::rows::Row {
                id: anno_rag_tabular::RowId::for_doc(review_id, doc_id),
                review_id,
                doc_id,
                folder_path: None,
                created_at: chrono::Utc::now(),
            };
            match ts.rows.add(&row).await {
                Ok(()) => rows_added += 1,
                Err(e) => {
                    tracing::warn!(target: "tabular::mcp", "add_row failed for {doc_id}: {e}");
                    failed.push(id_str.clone());
                }
            }
        }
        // Spawn background extraction if we have rows and a pipeline.
        let extraction_started = if rows_added > 0 {
            match self.pipeline_arc() {
                Some(arc_pipeline) => {
                    // Pre-flight: verify LLM credentials *before* advertising
                    // extraction_started=true.  Without this check the caller
                    // would believe extraction had begun even when the key is
                    // absent, hiding the failure entirely.
                    match anno_rag_tabular::llm::default_from_env() {
                        Err(e) => {
                            tracing::warn!(
                                target: "tabular::mcp",
                                "LLM init failed; background extraction skipped: {e}"
                            );
                            false
                        }
                        Ok(llm_client) => {
                            let ts_clone = ts.clone();
                            let force = p.force_reextract;
                            let llm = std::sync::Arc::from(llm_client);
                            tokio::spawn(async move {
                                let chunk_src = std::sync::Arc::new(
                                    crate::tabular::PipelineChunkSource(arc_pipeline.clone()),
                                );
                                let extractor =
                                    anno_rag_tabular::extract::Extractor::new(llm, chunk_src);
                                let cfg = anno_rag_tabular::fanout::FanoutConfig {
                                    force_reextract: force,
                                    ..Default::default()
                                };
                                match anno_rag_tabular::run_review(
                                    &ts_clone, &extractor, review_id, cfg,
                                )
                                .await
                                {
                                    Ok(outcomes) => {
                                        let ok =
                                            outcomes.iter().filter(|o| o.result.is_ok()).count();
                                        let err = outcomes.len() - ok;
                                        tracing::info!(
                                            target: "tabular::mcp",
                                            review = %review_id.0,
                                            ok,
                                            err,
                                            "background extraction complete"
                                        );
                                    }
                                    Err(e) => tracing::warn!(
                                        target: "tabular::mcp",
                                        "run_review failed: {e}"
                                    ),
                                }
                            });
                            true
                        }
                    }
                }
                None => {
                    tracing::warn!(target: "tabular::mcp", "pipeline not initialised; extraction skipped");
                    false
                }
            }
        } else {
            false
        };
        serde_json::to_string_pretty(&ReviewAddRowsResult {
            rows_added,
            extraction_started,
            failed_doc_ids: failed,
        })
        .unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Re-extract a single cell with an extra instruction prepended to the
    /// column prompt. Bumps the cell version. Locked cells block this.
    #[tool(description = "Re-extract a single cell with an extra instruction. \
                       The instruction is prepended to the column's prompt for \
                       this one call. Bumps cell version. Locked cells are blocked.")]
    async fn review_refine_cell(
        &self,
        Parameters(p): Parameters<ReviewRefineCellParams>,
    ) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let _pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error (pipeline): {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        let col_id = match uuid::Uuid::parse_str(&p.col_id) {
            Ok(u) => anno_rag_tabular::ColumnId(u),
            Err(e) => return format!("Error: bad col_id: {e}"),
        };
        let row_id = match uuid::Uuid::parse_str(&p.row_id) {
            Ok(u) => anno_rag_tabular::RowId(u),
            Err(e) => return format!("Error: bad row_id: {e}"),
        };
        // Fetch the column definition so we can amend its prompt.
        let cols = match ts.columns.list_for_review(review_id).await {
            Ok(c) => c,
            Err(e) => return format!("Error: {e}"),
        };
        let mut col = match cols.into_iter().find(|c| c.id == col_id) {
            Some(c) => c,
            None => return format!("Error: column {col_id:?} not found in review"),
        };
        // Fetch the row.
        let rows = match ts.rows.list_for_review(review_id).await {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let row = match rows.into_iter().find(|r| r.id == row_id) {
            Some(r) => r,
            None => return format!("Error: row {row_id:?} not found"),
        };
        // Reject early if the cell is already locked — avoids wasting LLM credits.
        if let Ok(Some(existing)) = ts.cells.latest(review_id, row_id, col_id).await {
            if existing.locked {
                return "Error: cell is locked; unlock it first with review_unlock_cell".into();
            }
        }
        // Prepend instruction to prompt.
        col.prompt = format!("{}\n\n{}", p.instruction, col.prompt);
        let arc_pipeline = match self.pipeline_arc() {
            Some(a) => a,
            None => return "Error: pipeline not initialised".into(),
        };
        let chunk_src = std::sync::Arc::new(crate::tabular::PipelineChunkSource(arc_pipeline));
        let llm = match anno_rag_tabular::llm::default_from_env() {
            Ok(l) => std::sync::Arc::from(l),
            Err(e) => return format!("Error: LLM init: {e}"),
        };
        let extractor = anno_rag_tabular::extract::Extractor::new(llm, chunk_src);
        let mut extracted = match extractor.extract_doc(review_id, row.doc_id, &[col]).await {
            Ok(cells) => cells,
            Err(e) => return format!("Error: extraction failed: {e}"),
        };
        // Offset verification.
        for cell in &mut extracted {
            if let Err(e) = anno_rag_tabular::verify::offsets::verify_cell_offsets(
                cell,
                row.doc_id,
                extractor.chunks(),
            )
            .await
            {
                tracing::warn!(target: "tabular::mcp", "offset verify: {e}");
            }
        }
        for cell in extracted {
            if let Err(e) = ts.cells.upsert(&cell).await {
                return format!("Error: upsert failed: {e}");
            }
        }
        serde_json::to_string_pretty(&ReviewRefineCellResult {
            ok: true,
            note: "cell re-extracted and versioned".into(),
        })
        .unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Write a human-override value to a cell. Author = Human.
    /// Set `lock: true` to prevent future auto-overwrites.
    #[tool(description = "Write a human override to a cell (any JSON value). \
                       Set lock=true to prevent auto-overwrite by the extractor. \
                       Author is recorded as Human.")]
    async fn review_set_cell(&self, Parameters(p): Parameters<ReviewSetCellParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        let row_id = match uuid::Uuid::parse_str(&p.row_id) {
            Ok(u) => anno_rag_tabular::RowId(u),
            Err(e) => return format!("Error: bad row_id: {e}"),
        };
        let col_id = match uuid::Uuid::parse_str(&p.col_id) {
            Ok(u) => anno_rag_tabular::ColumnId(u),
            Err(e) => return format!("Error: bad col_id: {e}"),
        };
        // Verify the row belongs to this review (prevents cross-review cell writes).
        match ts.rows.list_for_review(review_id).await {
            Err(e) => return format!("Error: {e}"),
            Ok(rows) if !rows.iter().any(|r| r.id == row_id) => {
                return format!(
                    "Error: row {} not found in review {}",
                    row_id.0, review_id.0
                )
            }
            _ => {}
        }
        // NOTE: The read-then-write below is a known TOCTOU: two concurrent
        // callers may both read the same prev_version and write the same next
        // version.  LanceDB is append-only so both rows are stored; `latest()`
        // returns the most recent write.  For the single-process, single-
        // reviewer workload this crate targets this is acceptable.
        // Read latest version to compute next.
        let prev_version = ts
            .cells
            .latest(review_id, row_id, col_id)
            .await
            .ok()
            .flatten()
            .map(|c| c.version)
            .unwrap_or(0);
        let cell = anno_rag_tabular::storage::cells::Cell {
            review_id,
            row_id,
            col_id,
            value: p.value,
            reasoning: None,
            citations: vec![],
            support_score: 1.0,
            confidence: anno_rag_tabular::storage::cells::Confidence::High,
            locked: p.lock,
            version: prev_version + 1,
            author: anno_rag_tabular::storage::cells::Author::Human {
                user_id: p.actor.unwrap_or_else(|| "reviewer".into()),
            },
            updated_at: chrono::Utc::now(),
        };
        match ts.cells.upsert(&cell).await {
            Ok(()) => serde_json::to_string_pretty(&ReviewSetCellResult {
                ok: true,
                locked: p.lock,
            })
            .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Lock a cell so the extraction engine cannot overwrite it.
    #[tool(
        description = "Lock a cell to prevent auto-overwrite by the extraction engine. \
                       Reads the latest value and re-writes it with locked=true. \
                       Author is recorded as Human."
    )]
    async fn review_lock_cell(&self, Parameters(p): Parameters<ReviewCellLockParams>) -> String {
        self.set_cell_lock(p, true).await
    }

    /// Unlock a previously locked cell so the extraction engine may overwrite it.
    #[tool(
        description = "Remove a lock from a cell. The extraction engine may then \
                       overwrite it on the next review_add_rows or review_refine_cell call."
    )]
    async fn review_unlock_cell(&self, Parameters(p): Parameters<ReviewCellLockParams>) -> String {
        self.set_cell_lock(p, false).await
    }

    /// Export a review as CSV, Markdown, or XLSX.
    #[tool(
        description = "Export a review. format: csv (default), markdown, or xlsx. \
                       For xlsx, provide output_path (absolute). \
                       CSV and Markdown are returned as a string in the tool response."
    )]
    async fn review_export(&self, Parameters(p): Parameters<ReviewExportParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        match p.format.as_str() {
            "csv" => match anno_rag_tabular::export::export_csv(ts, review_id).await {
                Ok(s) => s,
                Err(e) => format!("Error: {e}"),
            },
            "markdown" | "md" => {
                match anno_rag_tabular::export::export_markdown(ts, review_id).await {
                    Ok(s) => s,
                    Err(e) => format!("Error: {e}"),
                }
            }
            "xlsx" => {
                let path_str = match &p.output_path {
                    Some(s) => s.clone(),
                    None => return "Error: output_path required for xlsx format".into(),
                };
                let path = std::path::PathBuf::from(&path_str);
                // Require an absolute path to prevent path-traversal writes
                // relative to an unpredictable working directory.
                if !path.is_absolute() {
                    return "Error: output_path must be an absolute path (e.g. /tmp/review.xlsx)"
                        .into();
                }
                let path = path.as_path();
                match anno_rag_tabular::export::export_xlsx(ts, review_id, path).await {
                    Ok(()) => serde_json::json!({ "ok": true, "path": path_str }).to_string(),
                    Err(e) => format!("Error: {e}"),
                }
            }
            other => format!("Error: unknown format '{other}'. Valid: csv, markdown, xlsx"),
        }
    }

    /// Return the full state of a review: columns, rows, and latest cells.
    /// Use this to track extraction progress or read the current grid.
    #[tool(
        description = "Return the full state of a review: columns, rows, and latest \
                       cells with confidence and lock status. Use to poll extraction \
                       progress or read the current grid."
    )]
    async fn review_get(&self, Parameters(p): Parameters<ReviewGetParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        let review = match ts.reviews.get(review_id).await {
            Ok(Some(r)) => r,
            Ok(None) => return format!("Error: review {review_id:?} not found"),
            Err(e) => return format!("Error: {e}"),
        };
        let columns = match ts.columns.list_for_review(review_id).await {
            Ok(c) => c,
            Err(e) => return format!("Error: {e}"),
        };
        let rows = match ts.rows.list_for_review(review_id).await {
            Ok(r) => r,
            Err(e) => return format!("Error: {e}"),
        };
        let cells = match ts.cells.all_for_review_latest(review_id).await {
            Ok(c) => c,
            Err(e) => return format!("Error: {e}"),
        };
        let wire = ReviewGetResult {
            review_id: review_id.0.to_string(),
            name: review.name,
            columns: columns
                .into_iter()
                .map(|c| ReviewColumnWire {
                    id: c.id.0.to_string(),
                    name: c.name,
                    prompt: c.prompt,
                    order: c.order,
                })
                .collect(),
            rows: rows
                .iter()
                .map(|r| ReviewRowWire {
                    id: r.id.0.to_string(),
                    doc_id: r.doc_id.to_string(),
                    doc_label: r
                        .folder_path
                        .as_deref()
                        .and_then(|p| p.rsplit('/').next())
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .unwrap_or_else(|| r.doc_id.to_string()),
                })
                .collect(),
            cells: cells
                .into_iter()
                .map(|c| ReviewCellWire {
                    row_id: c.row_id.0.to_string(),
                    col_id: c.col_id.0.to_string(),
                    value: c.value,
                    confidence: format!("{:?}", c.confidence).to_lowercase(),
                    support_score: c.support_score,
                    locked: c.locked,
                    version: c.version,
                })
                .collect(),
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// List configured Anno knowledge sources. Does not load local ML models.
    #[tool(description = "List configured Anno knowledge sources. Does not load local ML models.")]
    async fn knowledge_sources(&self) -> String {
        match self.knowledge_sources_impl().await {
            Ok(sources) => {
                serde_json::to_string_pretty(&sources).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Return local Anno knowledge status without loading ML models.
    #[tool(
        description = "Return local Anno knowledge status for sources, objects, chunks, and failures. Does not load local ML models."
    )]
    async fn knowledge_status(&self) -> String {
        match self.knowledge_status_impl().await {
            Ok(status) => {
                serde_json::to_string_pretty(&status).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Search Anno's local multi-source knowledge index.
    #[tool(
        description = "Search Anno's local multi-source knowledge index. Phase 1 supports fast SQLite FTS mode and returns pseudonymized snippets only."
    )]
    async fn knowledge_search(
        &self,
        Parameters(p): Parameters<crate::knowledge::KnowledgeSearchParams>,
    ) -> String {
        match self.knowledge_search_impl(p).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Unified folder indexing entry point for general knowledge and legal corpora.
    #[tool(
        description = "Index a local folder using profile general, legal, or all. General registers and syncs knowledge sources; legal ingests legal documents recursively."
    )]
    async fn index(&self, Parameters(p): Parameters<IndexParams>) -> String {
        self.index_impl_routing(p).await
    }

    /// Register a local folder as an Anno knowledge source. Does not load models.
    #[tool(
        description = "Register a local folder as an Anno knowledge source. Does not load local ML models. Run knowledge_sync afterwards to index it."
    )]
    async fn knowledge_add_local_folder(
        &self,
        Parameters(p): Parameters<KnowledgeAddFolderParams>,
    ) -> String {
        match self.knowledge_add_local_folder_impl(&p.path).await {
            Ok(source_id) => serde_json::to_string_pretty(
                &serde_json::json!({"ok": true, "source_id": source_id}),
            )
            .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Sync Anno local-folder knowledge sources end to end. Loads the local NER model.
    #[tool(
        description = "Sync Anno local-folder knowledge sources: walk, extract, pseudonymize locally, and write pseudonymized FTS chunks. Loads the local NER model. Bounded per run; call again to resume large folders."
    )]
    async fn knowledge_sync(&self, Parameters(p): Parameters<KnowledgeSyncParams>) -> String {
        if let Err(e) = self.knowledge().await {
            return format!("Error: {e}");
        }
        if self.pipeline().await.is_err() {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "models_missing",
                    "message": "Models are not available. Fast FTS search works on already-indexed content; indexing is paused.",
                    "next_action": "Run download_models or ask Anno to set up models."
                }
            })
            .to_string();
        }
        match self.knowledge_sync_impl(p).await {
            Ok(summary) => {
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "summary": summary}))
                    .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Remove an Anno knowledge source and all its indexed content.
    #[tool(
        description = "Remove an Anno knowledge source and all its pseudonymized content from SQLite and FTS. Does not load local ML models."
    )]
    async fn knowledge_forget(&self, Parameters(p): Parameters<KnowledgeForgetParams>) -> String {
        match self.knowledge_forget_impl(p).await {
            Ok(removed) => serde_json::to_string_pretty(
                &serde_json::json!({"ok": true, "removed_objects": removed}),
            )
            .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }
}

#[tool_handler]
impl ServerHandler for AnnoRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "anno-rag MCP server — French legal RAG with privacy-by-design. \
                 Core: search, rehydrate, detect, vault_stats. \
                 Memory (GDPR Art.17): memory_save, memory_recall, memory_forget, \
                 memory_list, memory_graph_recall, memory_invalidate. \
                 Models: download_models (call on first use). \
                 Legal D1: legal_ingest, legal_search, legal_graph_query, \
                 legal_rehydrate_citation. \
                 Legal D2: legal_extract_contract, legal_extract_case_file, \
                 legal_timeline, legal_risk_review. \
                 Legal D3: legal_mandatory_clause_audit (b2b_contract, b2c_contract, \
                 employment, lease_commercial, lease_residential, rgpd). \
                 Legal D4: legal_prescription_check (8 French prescription categories). \
                 Legal D5: legal_validate_field (confirm/reject/correct extracted facts). \
                 All legal tools pseudonymize through the local vault.",
            )
            .with_server_info(Implementation::new(
                self.cfg.mcp_server_name.clone(),
                env!("CARGO_PKG_VERSION"),
            ))
    }
}

impl AnnoRagServer {
    /// Internal: flip the locked flag on the latest cell version.
    async fn set_cell_lock(&self, p: ReviewCellLockParams, locked: bool) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        let row_id = match uuid::Uuid::parse_str(&p.row_id) {
            Ok(u) => anno_rag_tabular::RowId(u),
            Err(e) => return format!("Error: bad row_id: {e}"),
        };
        let col_id = match uuid::Uuid::parse_str(&p.col_id) {
            Ok(u) => anno_rag_tabular::ColumnId(u),
            Err(e) => return format!("Error: bad col_id: {e}"),
        };
        // NOTE: Read-then-write TOCTOU (same as review_set_cell).  Acceptable
        // for single-process usage; LanceDB append-only semantics ensure no
        // data is lost — latest() will return the winning write.
        let prev = match ts.cells.latest(review_id, row_id, col_id).await {
            Ok(Some(c)) => c,
            Ok(None) => return "Error: no cell found to lock/unlock".into(),
            Err(e) => return format!("Error: {e}"),
        };
        let cell = anno_rag_tabular::storage::cells::Cell {
            locked,
            version: prev.version + 1,
            author: anno_rag_tabular::storage::cells::Author::Human {
                user_id: p.actor.unwrap_or_else(|| "reviewer".into()),
            },
            updated_at: chrono::Utc::now(),
            ..prev
        };
        match ts.cells.upsert(&cell).await {
            Ok(()) => serde_json::to_string_pretty(&ReviewCellLockResult { ok: true, locked })
                .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }
}

/// Start the MCP server on stdio. Blocks until stdin closes.
///
/// # Errors
///
/// Returns [`anno_rag::error::Error::Detect`] if the rmcp transport fails to
/// initialize or the server loop returns an error.
pub async fn serve_stdio(pipeline: Pipeline, cfg: AnnoRagConfig) -> anno_rag::error::Result<()> {
    let server = AnnoRagServer::new(pipeline, cfg);
    tracing::info!("anno-rag MCP server starting on stdio");

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server failed to start: {e}")))?;

    // Graceful shutdown: race the rmcp `waiting()` loop (which exits when the
    // stdio peer closes) against SIGINT / SIGTERM. On signal, cancel the
    // service via the clonable cancellation token — that causes `waiting()`
    // to return `QuitReason::Cancelled`. Either way, the function returns
    // and the Pipeline drops (Store → LanceDB connection closes; vault flushes
    // any pending save). The audit-event tracing emitted by every handler
    // is already on disk because the JsonlAuditSink sync_data's per line.
    let cancel = service.cancellation_token();
    let signal_task = tokio::spawn(async move {
        shutdown_signal_mcp().await;
        cancel.cancel();
    });

    let quit = service
        .waiting()
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server error: {e}")))?;
    signal_task.abort();
    tracing::info!(
        target: "anno_rag::mcp::shutdown",
        event = "stopped",
        reason = ?quit,
        "anno-rag MCP server stopped cleanly"
    );
    Ok(())
}

/// Start the MCP server on stdio with deferred pipeline init.
///
/// The `Pipeline` is not built until the first tool call, allowing the server
/// to start within the MCP timeout window even when models are not yet cached.
///
/// # Errors
///
/// Returns [`anno_rag::error::Error::Detect`] if the rmcp transport fails to
/// initialize or the server loop returns an error.
pub async fn serve_stdio_lazy(cfg: AnnoRagConfig, key: [u8; 32]) -> anno_rag::error::Result<()> {
    let server = AnnoRagServer::new_lazy(cfg, key);
    tracing::info!("anno-rag MCP server starting (lazy) on stdio");

    // Clone before serve() so we can pre-warm in the background.
    // Both the serving instance and the warmup clone share the same
    // Arc<OnceCell<Pipeline>>, so the first to initialise wins and
    // the result is reused by all subsequent calls.
    let warmup_server = server.clone();

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server failed to start: {e}")))?;

    // Pre-warm: trigger Pipeline::new in the background immediately after the
    // transport handshake, so models are hot before the first real tool call.
    // Non-fatal: a failure here is logged; the first tool call will retry.
    tokio::spawn(async move {
        tracing::info!("anno-rag pre-warming pipeline (models loading in background)");
        match warmup_server.pipeline().await {
            Ok(_) => tracing::info!("anno-rag pipeline warm — tools ready"),
            Err(e) => tracing::warn!(error = %e, "anno-rag pre-warm failed (non-fatal)"),
        }
    });

    let cancel = service.cancellation_token();
    let signal_task = tokio::spawn(async move {
        shutdown_signal_mcp().await;
        cancel.cancel();
    });

    let quit = service
        .waiting()
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server error: {e}")))?;
    signal_task.abort();
    tracing::info!(
        target: "anno_rag::mcp::shutdown",
        event = "stopped",
        reason = ?quit,
        "anno-rag MCP server stopped cleanly"
    );
    Ok(())
}

/// Future that resolves on Ctrl-C (or SIGTERM on Unix). Used to interrupt
/// the rmcp `service.waiting()` loop in `serve_stdio`.
async fn shutdown_signal_mcp() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(
                target: "anno_rag::mcp::shutdown",
                "ctrl_c listener failed: {e}"
            );
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(
                    target: "anno_rag::mcp::shutdown",
                    "SIGTERM listener failed: {e}"
                );
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            tracing::info!(
                target: "anno_rag::mcp::shutdown",
                event = "cancelling",
                signal = "SIGINT",
                "shutdown signal received, cancelling MCP service"
            );
        }
        () = terminate => {
            tracing::info!(
                target: "anno_rag::mcp::shutdown",
                event = "cancelling",
                signal = "SIGTERM",
                "shutdown signal received, cancelling MCP service"
            );
        }
    }
}

#[cfg(test)]
mod lazy_tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    #[tokio::test(flavor = "current_thread")]
    async fn lazy_server_returns_error_when_models_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let key = [0u8; 32];

        let saved = std::env::var("ANNO_MODELS_DIR").ok();
        // Safety: flavor = "current_thread" ensures this test runs on a single OS thread.
        // ANNO_MODELS_DIR is saved and restored so other tests in the same suite are not affected.
        // Note: run with RUST_TEST_THREADS=1 if other tests also mutate ANNO_MODELS_DIR.
        unsafe { std::env::remove_var("ANNO_MODELS_DIR") };

        let server = AnnoRagServer::new_lazy(cfg, key);
        let result = server
            .search(Parameters(SearchParams {
                query: "test".into(),
                top_k: 1,
                rerank: false,
            }))
            .await;

        if let Some(v) = saved {
            unsafe { std::env::set_var("ANNO_MODELS_DIR", v) };
        }
        assert!(
            result.contains("Models not downloaded"),
            "expected 'Models not downloaded' in: {result}"
        );
    }

    /// When both model subdirs exist, download_models reports already_present.
    #[tokio::test(flavor = "current_thread")]
    async fn download_models_tool_reports_already_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("models");
        std::fs::create_dir_all(models_dir.join("multilingual-e5-small")).unwrap();
        std::fs::create_dir_all(models_dir.join("gliner2-multi-v1-onnx")).unwrap();

        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let result = server.download_models().await;
        assert!(
            result.contains("already_present") || result.contains("already present"),
            "expected 'already_present' in: {result}"
        );
        // Parse JSON to compare path field — avoids Windows backslash escaping issues
        // (raw path has `\` but JSON encodes it as `\\`).
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("result must be valid JSON");
        assert_eq!(
            parsed["path"].as_str().unwrap_or(""),
            models_dir.to_str().unwrap(),
            "expected path field to match models_dir"
        );
    }

    /// When models are absent and no lock file exists, the tool starts a
    /// background download and returns the "downloading" status immediately.
    #[tokio::test(flavor = "current_thread")]
    async fn download_models_tool_starts_download_when_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        assert!(!dir
            .path()
            .join("models")
            .join("multilingual-e5-small")
            .exists());

        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let result = server.download_models().await;
        assert!(
            result.contains("downloading")
                || result.contains("in_progress")
                || result.contains("Downloading"),
            "expected download status in: {result}"
        );
    }

    #[test]
    fn index_unknown_profile_returns_error() {
        let err = validate_profile("weird").expect_err("unknown profile should fail validation");
        assert!(
            err.contains("weird"),
            "validation error should name rejected profile: {err}"
        );
    }

    #[test]
    fn index_sync_summary_issue_flags_failed_or_truncated_runs() {
        let clean = SyncSummary::default();
        assert!(knowledge_sync_issue(&clean).is_none());

        let failed = SyncSummary {
            failed: 2,
            ..Default::default()
        };
        let failed_issue = knowledge_sync_issue(&failed).expect("failed issue");
        assert!(failed_issue.contains("failed=2"));

        let truncated = SyncSummary {
            truncated: true,
            ..Default::default()
        };
        let truncated_issue = knowledge_sync_issue(&truncated).expect("truncated issue");
        assert!(truncated_issue.contains("truncated=true"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn index_general_routes_to_knowledge_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let corpus_dir = dir.path().join("corpus");
        std::fs::create_dir_all(&corpus_dir).expect("corpus dir");
        std::fs::write(corpus_dir.join("note.txt"), "Bonjour index").expect("write corpus file");

        let cfg = AnnoRagConfig {
            data_dir: dir.path().join("data"),
            ..Default::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let result = server
            .index_impl_routing(IndexParams {
                path: corpus_dir.to_string_lossy().into_owned(),
                profile: "general".into(),
            })
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("index result must be JSON");

        assert_eq!(parsed["profile"], "general");
        assert!(
            parsed.get("knowledge").is_some(),
            "general profile should attempt knowledge routing: {result}"
        );
        assert!(
            parsed.get("errors").is_some(),
            "index result should include errors field: {result}"
        );
        assert!(
            parsed["legal"].is_null(),
            "general profile should not route to legal ingest: {result}"
        );

        if parsed["ok"] == false {
            let errors = parsed["errors"]
                .as_array()
                .expect("failed index result should include errors array");
            assert!(
                errors
                    .iter()
                    .any(|e| e.as_str().is_some_and(|s| s.contains("knowledge sync"))),
                "failed general index should report knowledge sync error: {result}"
            );
        } else {
            assert!(
                parsed["knowledge"].is_object(),
                "successful general index should include knowledge object: {result}"
            );
        }
    }
}
