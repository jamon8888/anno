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

mod allowed_roots;
pub mod corpus;
mod corpus_sync;
pub mod health;
mod indexer;
pub mod knowledge;
mod legal_maintenance;
pub mod model_inventory;
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
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{OnceCell, RwLock};

/// State held by the MCP server: either a pre-built Pipeline (eager) or a
/// lazily-initialised one (deferred until the first tool call).
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    knowledge: Arc<OnceCell<Arc<crate::knowledge::KnowledgeService>>>,
    corpus: Arc<OnceCell<Arc<crate::corpus::CorpusService>>>,
    legal_maintenance: Arc<OnceCell<Arc<crate::legal_maintenance::LegalMaintenanceService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    tabular_storage: Arc<OnceCell<Arc<anno_rag_tabular::storage::StorageHandle>>>,
    extraction_status: Arc<RwLock<HashMap<anno_rag_tabular::ReviewId, ReviewExtractionStatus>>>,
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
                    let inventory =
                        crate::model_inventory::ModelInventoryService::new(&cfg).inspect();
                    if !inventory.ready {
                        return Err(anno_rag::error::Error::Config(format!(
                            "Models not ready at {} (state={}). Ask me to 'Set up anno-rag' \
                             or run `anno-rag download-models` in a terminal, \
                             then restart the extension.",
                            inventory.path,
                            inventory.state.as_str()
                        )));
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

    async fn corpus(&self) -> anno_corpus_store::Result<&crate::corpus::CorpusService> {
        self.corpus
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                async move { crate::corpus::CorpusService::open(&cfg).map(Arc::new) }
            })
            .await
            .map(|arc| arc.as_ref())
    }

    async fn legal_maintenance(
        &self,
    ) -> anno_rag::Result<&crate::legal_maintenance::LegalMaintenanceService> {
        self.legal_maintenance
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                async move {
                    crate::legal_maintenance::LegalMaintenanceService::open(&cfg)
                        .await
                        .map(Arc::new)
                }
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

    async fn filter_ingested_doc_ids(
        &self,
        ids: Vec<uuid::Uuid>,
        failed: &mut Vec<String>,
    ) -> Result<Vec<uuid::Uuid>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let cfg = self.cfg.as_ref().clone();
        let store = anno_rag::store::Store::open(&cfg)
            .await
            .map_err(|e| format!("opening RAG index: {e}"))?;
        let mut ingested = Vec::new();
        for doc_id in ids {
            match store.doc_exists(doc_id).await {
                Ok(true) => ingested.push(doc_id),
                Ok(false) => failed.push(doc_id.to_string()),
                Err(e) => return Err(format!("checking doc_id {doc_id}: {e}")),
            }
        }
        Ok(ingested)
    }

    async fn filter_review_doc_ids_by_corpus(
        &self,
        review_id: anno_rag_tabular::ReviewId,
        ids: Vec<uuid::Uuid>,
        failed: &mut Vec<String>,
    ) -> Result<Vec<uuid::Uuid>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        let review_corpus = service
            .store()
            .corpus_for_binding(
                anno_corpus_core::CorpusBindingKind::TabularReview,
                &review_id.0.to_string(),
            )
            .map_err(|e| e.to_string())?;
        let allowed_docs = service
            .store()
            .document_ids_for_corpus(review_corpus, "legal")
            .map_err(|e| e.to_string())?;
        let allowed = allowed_docs
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        Ok(ids
            .into_iter()
            .filter(|doc_id| {
                let ok = allowed.contains(doc_id);
                if !ok {
                    failed.push(format!("{doc_id}: outside review corpus"));
                }
                ok
            })
            .collect())
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
            corpus: Arc::new(OnceCell::new()),
            legal_maintenance: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key: [0u8; 32],
            tabular_storage: Arc::new(OnceCell::new()),
            extraction_status: Arc::new(RwLock::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    /// Construct with deferred pipeline init (lazy path). Used by serve_stdio_lazy.
    #[must_use]
    pub fn new_lazy(cfg: AnnoRagConfig, key: [u8; 32]) -> Self {
        Self {
            pipeline: Arc::new(OnceCell::new()),
            knowledge: Arc::new(OnceCell::new()),
            corpus: Arc::new(OnceCell::new()),
            legal_maintenance: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key,
            tabular_storage: Arc::new(OnceCell::new()),
            extraction_status: Arc::new(RwLock::new(HashMap::new())),
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

fn default_true() -> bool {
    true
}

/// Parameters for `privacy_prepare_folder`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PrivacyPrepareFolderParams {
    /// Local source root to prepare.
    pub source_root: String,
    /// Recurse into subfolders.
    #[serde(default = "default_true")]
    pub recursive: bool,
}

/// Parameters for `privacy_finalize_folder`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct PrivacyFinalizeFolderParams {
    /// Local `vault` workspace path.
    pub workspace: String,
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

/// Parameters for the unified `forget` tool.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ForgetParams {
    /// Source id (UUID), legal corpus id returned by sources(), or explicit local folder path supplied by the user.
    pub target: String,
}

/// Parameters for corpus lookup tools.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct CorpusGetParams {
    /// Stable corpus id.
    pub corpus_id: String,
}

/// Parameters for the `legacy_search` tool.
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

/// Parameters for the unified `search` tool.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SearchUnifiedParams {
    /// User query text.
    pub query: String,
    /// Maximum number of results. Default: 10.
    #[serde(default = "default_search_unified_top_k")]
    pub top_k: usize,
    /// Search mode: `fast`, `semantic`, or omitted for scope-dependent auto mode.
    #[serde(default)]
    pub mode: Option<String>,
    /// Search scope: `all` (default), `knowledge`, or `legal`.
    #[serde(default)]
    pub scope: Option<String>,
    /// Optional legal-search filters when legal scope is included.
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
    /// Optional corpus id to constrain the query.
    #[serde(default)]
    pub corpus_id: Option<String>,
    /// Explicitly allow cross-corpus search.
    #[serde(default)]
    pub allow_cross_corpus: bool,
}

fn default_search_unified_top_k() -> usize {
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
    corpus_id: Option<String>,
    document_label: Option<String>,
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
    output_root: String,
    output_scope: String,
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
    /// Optional corpus id to constrain the legal query.
    #[serde(default)]
    pub corpus_id: Option<String>,
    /// Explicitly allow cross-corpus legal search.
    #[serde(default)]
    pub allow_cross_corpus: bool,
}

fn build_legal_search_params(p: &SearchUnifiedParams) -> LegalSearchParams {
    let filters = p.filters.as_ref().and_then(serde_json::Value::as_object);

    LegalSearchParams {
        query: p.query.clone(),
        top_k: p.top_k,
        doc_type: filter_string(filters, "doc_type"),
        legal_domain: filter_string(filters, "legal_domain"),
        jurisdiction: filter_string(filters, "jurisdiction"),
        dossier_id: filter_string(filters, "dossier_id"),
        parties: filter_string_vec(filters, "parties"),
        party_roles: filter_string_vec(filters, "party_roles"),
        legal_refs: filter_string_vec(filters, "legal_refs"),
        clause_types: filter_string_vec(filters, "clause_types"),
        obligation_kinds: filter_string_vec(filters, "obligation_kinds"),
        risk_flags: filter_string_vec(filters, "risk_flags"),
        min_confidence: filter_f32(filters, "min_confidence"),
        corpus_id: p.corpus_id.clone(),
        allow_cross_corpus: p.allow_cross_corpus,
    }
}

fn filter_string(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn filter_string_vec(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Vec<String> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn filter_f32(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<f32> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
}

fn normalize_search_scope(scope: Option<String>, warnings: &mut Vec<String>) -> String {
    match scope.as_deref().unwrap_or("all") {
        "all" => "all".to_string(),
        "knowledge" => "knowledge".to_string(),
        "legal" => "legal".to_string(),
        other => {
            warnings.push(format!(
                "unsupported search scope '{other}'; using scope='all'"
            ));
            "all".to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchBackendMode {
    Fast,
    Semantic,
    Skipped,
}

impl SearchBackendMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Semantic => "semantic",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
struct SearchExecutionPlan {
    mode_used: &'static str,
    knowledge: SearchBackendMode,
    legal: SearchBackendMode,
    explicit_fast_legal_error: bool,
}

fn search_execution_plan(
    mode: Option<String>,
    scope: &str,
    warnings: &mut Vec<String>,
) -> SearchExecutionPlan {
    match (mode.as_deref(), scope) {
        (None, "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "all") => SearchExecutionPlan {
            mode_used: "auto",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (None, other) => {
            warnings.push(format!(
                "unsupported normalized search scope '{other}'; using scope='all'"
            ));
            SearchExecutionPlan {
                mode_used: "auto",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Semantic,
                explicit_fast_legal_error: false,
            }
        }
        (Some("fast"), "legal") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: true,
        },
        (Some("fast"), "all") => {
            warnings.push(
                "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results."
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "fast",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("fast"), "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "knowledge") => {
            warnings.push(
                "knowledge scope skipped in semantic mode (knowledge index currently supports fast mode only)"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Skipped,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("semantic"), "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "all") => {
            warnings.push(
                "knowledge scope skipped in semantic mode (knowledge index currently supports fast mode only)"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Skipped,
                legal: SearchBackendMode::Semantic,
                explicit_fast_legal_error: false,
            }
        }
        (Some(other), _) => {
            warnings.push(format!(
                "unsupported search mode '{other}'; using implicit mode for scope='{scope}'"
            ));
            search_execution_plan(None, scope, warnings)
        }
    }
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
    /// Optional corpus id that owns this review.
    #[serde(default)]
    pub corpus_id: Option<String>,
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

/// Parameters for `review_extract`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReviewExtractParams {
    /// Review UUID.
    pub review_id: String,
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
    extraction_error: Option<String>,
}

struct ParsedReviewDocIds {
    valid: Vec<uuid::Uuid>,
    failed: Vec<String>,
}

fn parse_review_doc_ids(doc_ids: &[String]) -> ParsedReviewDocIds {
    let mut valid = Vec::new();
    let mut failed = Vec::new();

    for doc_id in doc_ids {
        match uuid::Uuid::parse_str(doc_id) {
            Ok(id) => valid.push(id),
            Err(_) => failed.push(doc_id.clone()),
        }
    }

    ParsedReviewDocIds { valid, failed }
}

fn combine_review_add_rows_extraction_error(
    mut row_errors: Vec<String>,
    extraction_error: Option<String>,
) -> Option<String> {
    if let Some(error) = extraction_error {
        row_errors.push(error);
    }
    if row_errors.is_empty() {
        None
    } else {
        Some(row_errors.join("; "))
    }
}

#[derive(Serialize)]
struct ReviewExtractResult {
    review_id: String,
    rows: usize,
    columns: usize,
    extraction_started: bool,
    extraction_error: Option<String>,
}

#[derive(Clone, Serialize)]
struct ReviewRowErrorWire {
    row_id: String,
    doc_id: String,
    error: String,
}

#[derive(Clone)]
struct ReviewExtractionStatus {
    review_id: anno_rag_tabular::ReviewId,
    state: String,
    rows: usize,
    columns: usize,
    ok_rows: usize,
    failed_rows: usize,
    row_errors: Vec<ReviewRowErrorWire>,
    last_error: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Serialize)]
struct ReviewExtractionStatusWire {
    review_id: String,
    state: String,
    rows: usize,
    columns: usize,
    ok_rows: usize,
    failed_rows: usize,
    row_errors: Vec<ReviewRowErrorWire>,
    last_error: Option<String>,
    updated_at: String,
}

impl ReviewExtractionStatus {
    fn running(review_id: anno_rag_tabular::ReviewId, rows: usize, columns: usize) -> Self {
        Self {
            review_id,
            state: "running".into(),
            rows,
            columns,
            ok_rows: 0,
            failed_rows: 0,
            row_errors: Vec::new(),
            last_error: None,
            updated_at: chrono::Utc::now(),
        }
    }

    fn completed(
        review_id: anno_rag_tabular::ReviewId,
        rows: usize,
        columns: usize,
        ok_rows: usize,
        row_errors: Vec<ReviewRowErrorWire>,
    ) -> Self {
        let failed_rows = row_errors.len();
        let state = if failed_rows == 0 {
            "completed"
        } else {
            "completed_with_errors"
        };
        Self {
            review_id,
            state: state.into(),
            rows,
            columns,
            ok_rows,
            failed_rows,
            row_errors,
            last_error: None,
            updated_at: chrono::Utc::now(),
        }
    }

    fn blocked(
        review_id: anno_rag_tabular::ReviewId,
        rows: usize,
        columns: usize,
        error: String,
    ) -> Self {
        Self {
            review_id,
            state: "blocked".into(),
            rows,
            columns,
            ok_rows: 0,
            failed_rows: rows,
            row_errors: Vec::new(),
            last_error: Some(error),
            updated_at: chrono::Utc::now(),
        }
    }

    fn to_wire(&self) -> ReviewExtractionStatusWire {
        ReviewExtractionStatusWire {
            review_id: self.review_id.0.to_string(),
            state: self.state.clone(),
            rows: self.rows,
            columns: self.columns,
            ok_rows: self.ok_rows,
            failed_rows: self.failed_rows,
            row_errors: self.row_errors.clone(),
            last_error: self.last_error.clone(),
            updated_at: self.updated_at.to_rfc3339(),
        }
    }
}

fn try_mark_review_extraction_running(
    statuses: &mut HashMap<anno_rag_tabular::ReviewId, ReviewExtractionStatus>,
    review_id: anno_rag_tabular::ReviewId,
    rows: usize,
    columns: usize,
) -> Result<(), ReviewExtractResult> {
    if let Some(existing) = statuses.get(&review_id) {
        if existing.state == "running" {
            return Err(ReviewExtractResult {
                review_id: review_id.0.to_string(),
                rows: existing.rows,
                columns: existing.columns,
                extraction_started: false,
                extraction_error: Some(format!(
                    "extraction already running for review {}",
                    review_id.0
                )),
            });
        }
    }

    statuses.insert(
        review_id,
        ReviewExtractionStatus::running(review_id, rows, columns),
    );
    Ok(())
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
    extraction_status: Option<ReviewExtractionStatusWire>,
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
    async fn create_review_from_params(
        &self,
        p: ReviewCreateParams,
    ) -> Result<ReviewCreateResult, String> {
        let review_corpus = {
            let service = self.corpus().await.map_err(|e| e.to_string())?;
            let count = service.store().corpus_count().map_err(|e| e.to_string())?;
            if p.corpus_id.is_some() || count > 0 {
                match service
                    .resolve_effective(p.corpus_id.as_deref(), false)
                    .map_err(|e| e.to_string())?
                {
                    anno_corpus_core::EffectiveCorpus::Single(corpus_id) => Some(corpus_id),
                    anno_corpus_core::EffectiveCorpus::CrossCorpus => {
                        return Err("tabular reviews must be bound to one corpus".to_string());
                    }
                }
            } else {
                None
            }
        };
        let ts = self.tabular_storage().await.map_err(|e| e.to_string())?;
        let review_id = anno_rag_tabular::ReviewId::new();
        let columns = if let Some(tid) = &p.template_id {
            anno_rag_tabular::schema::template::Template::builtin(tid)
                .map_err(|e| format!("loading template: {e}"))?
                .into_columns(review_id)
        } else {
            Vec::new()
        };
        let columns_loaded = columns.len();
        let review = anno_rag_tabular::storage::reviews::Review {
            id: review_id,
            name: p.name.clone(),
            project_id: None,
            template_id: p.template_id.clone(),
            scope_folder: p.scope_folder.clone(),
            created_at: chrono::Utc::now(),
            schema_version: 1,
        };
        ts.reviews
            .create(&review)
            .await
            .map_err(|e| e.to_string())?;
        for col in columns {
            if let Err(e) = ts.columns.add(review_id, &col).await {
                let add_error = format!("adding column: {e}");
                let mut cleanup_errors = Vec::new();
                if let Err(cleanup_error) = ts.columns.delete_for_review(review_id).await {
                    cleanup_errors.push(format!("deleting review columns: {cleanup_error}"));
                }
                if let Err(cleanup_error) = ts.reviews.delete(review_id).await {
                    cleanup_errors.push(format!("deleting review: {cleanup_error}"));
                }
                if cleanup_errors.is_empty() {
                    return Err(format!("{add_error}; rolled back review {}", review_id.0));
                }
                return Err(format!(
                    "{add_error}; cleanup failed after partial review creation: {}",
                    cleanup_errors.join("; ")
                ));
            }
        }
        if let Some(corpus_id) = review_corpus {
            let bind_result = self
                .corpus()
                .await
                .map_err(|e| e.to_string())?
                .store()
                .add_binding(
                    corpus_id,
                    anno_corpus_core::CorpusBindingKind::TabularReview,
                    &review_id.0.to_string(),
                    &serde_json::json!({
                        "name": p.name.clone(),
                        "template_id": p.template_id.clone(),
                    }),
                );
            if let Err(e) = bind_result {
                let add_error = format!("binding review to corpus: {e}");
                let mut cleanup_errors = Vec::new();
                if let Err(cleanup_error) = ts.columns.delete_for_review(review_id).await {
                    cleanup_errors.push(format!("deleting review columns: {cleanup_error}"));
                }
                if let Err(cleanup_error) = ts.reviews.delete(review_id).await {
                    cleanup_errors.push(format!("deleting review: {cleanup_error}"));
                }
                if cleanup_errors.is_empty() {
                    return Err(format!("{add_error}; rolled back review {}", review_id.0));
                }
                return Err(format!(
                    "{add_error}; cleanup failed after partial review creation: {}",
                    cleanup_errors.join("; ")
                ));
            }
        }
        Ok(ReviewCreateResult {
            review_id: review_id.0.to_string(),
            name: p.name,
            columns_loaded,
        })
    }

    async fn legacy_search_impl(&self, params: SearchParams) -> Result<serde_json::Value, String> {
        let p = self.pipeline().await.map_err(|e| e.to_string())?;
        let result = if params.rerank {
            #[cfg(feature = "rerank")]
            {
                tracing::info!(
                    target: "anno_rag::audit",
                    tool = "legacy_search",
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
                tool = "legacy_search",
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
                    corpus_id: None,
                    document_label: Some(h.doc_id.to_string()),
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

    async fn privacy_prepare_folder_impl(
        &self,
        p: PrivacyPrepareFolderParams,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let summary = pipeline
            .privacy_prepare_folder(std::path::Path::new(&p.source_root), p.recursive)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    }

    async fn privacy_finalize_folder_impl(
        &self,
        p: PrivacyFinalizeFolderParams,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let summary = pipeline
            .privacy_finalize_folder(std::path::Path::new(&p.workspace))
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    }

    async fn privacy_status_impl(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "tools": [
                "privacy_prepare_folder",
                "privacy_finalize_folder",
                "privacy_status"
            ],
            "privacy_boundary": "local",
            "returns_document_content": false
        })
    }

    async fn legal_ingest_impl(
        &self,
        p: LegalIngestParams,
        corpus_id: Option<anno_corpus_core::CorpusId>,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let folder = std::path::Path::new(&p.folder);
        let out = corpus_id
            .map(|corpus_id| corpus_legal_output_dir(self.cfg.as_ref(), corpus_id))
            .unwrap_or_else(|| folder.join("anon"));
        let start = std::time::Instant::now();
        let ingest_result = if let Some(corpus_id) = corpus_id {
            pipeline
                .ingest_folder_scoped_summary(
                    folder,
                    p.recursive,
                    &out,
                    anno_rag::pipeline::LegalIngestScope {
                        corpus_id,
                        root: folder.to_path_buf(),
                    },
                )
                .await
        } else {
            pipeline
                .ingest_folder(folder, p.recursive, &out)
                .await
                .map(|ingested| anno_rag::pipeline::LegalIngestSummary {
                    ingested,
                    documents: Vec::new(),
                })
        };

        match ingest_result {
            Ok(summary) => {
                if let Some(corpus_id) = corpus_id {
                    let service = self.corpus().await.map_err(|e| e.to_string())?;
                    let label = legal_folder_id(&p.folder);
                    service
                        .store()
                        .add_binding(
                            corpus_id,
                            anno_corpus_core::CorpusBindingKind::LegalFolder,
                            &label,
                            &serde_json::json!({
                                "label": label.clone(),
                                "source_path": p.folder.clone()
                            }),
                        )
                        .map_err(|e| e.to_string())?;
                    for document in &summary.documents {
                        service
                            .store()
                            .add_document(
                                corpus_id,
                                document.document_id,
                                "legal",
                                &document.source_path,
                                document.relative_path.as_deref(),
                                &document.content_id,
                                &serde_json::json!({"folder_id": label.clone()}),
                            )
                            .map_err(|e| e.to_string())?;
                    }
                }
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_ingest",
                    result = "ok",
                    duration_ms = start.elapsed().as_millis() as u64,
                    ingested = summary.ingested,
                    ""
                );
                serde_json::to_value(LegalIngestResult {
                    ingested: summary.ingested,
                    folder: p.folder,
                    output_root: out.display().to_string(),
                    output_scope: if corpus_id.is_some() {
                        "corpus_internal".to_string()
                    } else {
                        "legacy_source_anon".to_string()
                    },
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
        let effective = self
            .resolve_effective_corpus(p.corpus_id.as_deref(), p.allow_cross_corpus)
            .await?;
        self.legal_search_impl_with_effective(p, &effective).await
    }

    async fn legal_search_impl_with_effective(
        &self,
        p: LegalSearchParams,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        let LegalSearchParams {
            query,
            top_k,
            doc_type,
            legal_domain,
            jurisdiction,
            dossier_id,
            parties,
            party_roles,
            legal_refs,
            clause_types,
            obligation_kinds,
            risk_flags,
            min_confidence,
            corpus_id: _,
            allow_cross_corpus: _,
        } = p;
        let filters = anno_rag::legal::types::LegalSearchFilters {
            doc_type,
            legal_domain,
            jurisdiction,
            dossier_id,
            parties,
            party_roles,
            legal_refs,
            clause_types,
            obligation_kinds,
            risk_flags,
            min_confidence,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let result = match self.legal_document_ids_for_effective(effective).await? {
            Some(doc_ids) => {
                pipeline
                    .legal_search_scoped(&query, top_k, filters, &doc_ids)
                    .await
            }
            None => pipeline.legal_search(&query, top_k, filters).await,
        };
        match result {
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

    pub(crate) async fn sources_impl_routing(&self) -> String {
        let mut sources = Vec::<serde_json::Value>::new();

        if let Ok(knowledge_sources) = self.knowledge_sources_impl().await {
            for mut source in knowledge_sources {
                if let serde_json::Value::Object(ref mut object) = source {
                    if let Some(source_id) = object
                        .get("source_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                    {
                        object.insert("id".to_string(), serde_json::Value::String(source_id));
                    }
                    object.insert(
                        "kind".to_string(),
                        serde_json::Value::String("knowledge_folder".to_string()),
                    );
                }
                sources.push(source);
            }
        }

        if let Ok(legal_maintenance) = self.legal_maintenance().await {
            if let Ok(paths) = legal_maintenance.list_indexed_folder_paths().await {
                for path in paths {
                    let id = legal_folder_id(&path);
                    sources.push(serde_json::json!({
                        "id": id,
                        "kind": "legal_corpus",
                        "label": id,
                    }));
                }
            }
        }

        serde_json::json!({
            "ok": true,
            "sources": sources,
        })
        .to_string()
    }

    pub(crate) async fn status_impl_routing(&self) -> String {
        let knowledge = self
            .knowledge_status_impl()
            .await
            .ok()
            .and_then(|s| serde_json::to_value(s).ok())
            .unwrap_or(serde_json::Value::Null);

        let legal = match self.legal_maintenance().await {
            Ok(service) => match service.count_chunks().await {
                Ok(n) => serde_json::json!({ "chunks": n }),
                Err(e) => serde_json::json!({ "chunks": null, "error": e.to_string() }),
            },
            Err(e) => serde_json::json!({ "chunks": null, "error": e.to_string() }),
        };

        let vault = match self.pipeline_arc() {
            Some(p) => {
                let stats = p.vault_stats().await;
                serde_json::json!({
                    "available": true,
                    "total_mappings": stats.total_mappings,
                    "categories": stats.categories,
                })
            }
            None => serde_json::json!({
                "available": false,
                "reason": "pipeline_not_initialized",
                "total_mappings": null,
                "categories": {},
            }),
        };

        let inventory =
            crate::model_inventory::ModelInventoryService::new(self.cfg.as_ref()).inspect();
        let loaded = self.pipeline_arc();
        let models = serde_json::json!({
            "inventory": inventory,
            "embedder_loaded": loaded.as_ref().is_some_and(|p| p.embedder_loaded()),
            "detector_loaded": loaded.as_ref().is_some_and(|p| p.detector_loaded()),
        });

        serde_json::json!({
            "ok": true,
            "knowledge": knowledge,
            "legal": legal,
            "vault": vault,
            "models": models,
        })
        .to_string()
    }

    async fn knowledge_status_impl(&self) -> Result<anno_knowledge_core::KnowledgeStatus, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.status().map_err(|e| e.to_string())
    }

    async fn resolve_effective_corpus(
        &self,
        corpus_id: Option<&str>,
        allow_cross_corpus: bool,
    ) -> Result<anno_corpus_core::EffectiveCorpus, String> {
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        service
            .resolve_effective(corpus_id, allow_cross_corpus)
            .map_err(|e| e.to_string())
    }

    async fn freshness_for_effective(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<(bool, String), String> {
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return Ok((false, "cross_corpus".to_string()));
        };
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        let state = service
            .store()
            .sync_state(*corpus_id)
            .map_err(|e| e.to_string())?;
        let freshness = state
            .map(|state| state.freshness)
            .unwrap_or_else(|| "unknown".to_string());
        Ok((freshness == "fresh", freshness))
    }

    async fn maybe_sync_knowledge_before_search(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
        scope: &str,
        warnings: &mut Vec<String>,
    ) -> serde_json::Value {
        if !(scope == "all" || scope == "knowledge") {
            return serde_json::json!({"attempted": false, "reason": "scope_not_knowledge"});
        }
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return serde_json::json!({"attempted": false, "reason": "cross_corpus"});
        };
        if self.pipeline_arc().is_none() {
            return serde_json::json!({
                "attempted": false,
                "reason": "models_not_loaded"
            });
        }
        let p = crate::corpus_sync::SyncCorpusParams {
            corpus_id: corpus_id.as_string(),
            sources: None,
            outputs: vec!["knowledge_fast".to_string()],
            max_files: Some(25),
            max_millis: Some(750),
        };
        match self.sync_corpus_impl(p).await {
            Ok(result) => serde_json::json!({
                "attempted": true,
                "freshness": result.freshness,
                "warnings": result.warnings,
            }),
            Err(error) => {
                warnings.push(format!("opportunistic sync failed: {error}"));
                serde_json::json!({
                    "attempted": true,
                    "error": error
                })
            }
        }
    }

    async fn knowledge_source_ids_for_effective(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<Option<Vec<anno_knowledge_core::SourceId>>, String> {
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return Ok(None);
        };
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        let bindings = service
            .store()
            .binding_ids_for_corpus_kind(
                *corpus_id,
                anno_corpus_core::CorpusBindingKind::KnowledgeSource,
            )
            .map_err(|e| e.to_string())?;
        let mut ids = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let parsed =
                uuid::Uuid::parse_str(&binding).map_err(|e| format!("bad source binding: {e}"))?;
            ids.push(anno_knowledge_core::SourceId::new(parsed));
        }
        Ok(Some(ids))
    }

    async fn legal_document_ids_for_effective(
        &self,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<Option<Vec<uuid::Uuid>>, String> {
        let anno_corpus_core::EffectiveCorpus::Single(corpus_id) = effective else {
            return Ok(None);
        };
        let service = self.corpus().await.map_err(|e| e.to_string())?;
        service
            .store()
            .document_ids_for_corpus(*corpus_id, "legal")
            .map(Some)
            .map_err(|e| e.to_string())
    }

    async fn knowledge_search_impl(
        &self,
        p: crate::knowledge::KnowledgeSearchParams,
    ) -> Result<crate::knowledge::KnowledgeSearchResponse, String> {
        let effective = self
            .resolve_effective_corpus(p.corpus_id.as_deref(), p.allow_cross_corpus)
            .await?;
        self.knowledge_search_impl_with_effective(p, &effective)
            .await
    }

    async fn knowledge_search_impl_with_effective(
        &self,
        p: crate::knowledge::KnowledgeSearchParams,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<crate::knowledge::KnowledgeSearchResponse, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        let source_ids = self.knowledge_source_ids_for_effective(effective).await?;
        service
            .search_with_source_ids(p, source_ids)
            .map_err(|e| e.to_string())
    }

    pub(crate) async fn search_impl_routing(&self, p: SearchUnifiedParams) -> String {
        let mut hits = Vec::<serde_json::Value>::new();
        let mut warnings = Vec::<String>::new();
        let scope = normalize_search_scope(p.scope.clone(), &mut warnings);
        let plan = search_execution_plan(p.mode.clone(), &scope, &mut warnings);
        if plan.explicit_fast_legal_error {
            return serde_json::json!({
                "ok": false,
                "error": "legal scope requires semantic mode",
                "mode_used": plan.mode_used,
                "scope_used": scope,
                "scope_modes": {
                    "knowledge": plan.knowledge.as_str(),
                    "legal": plan.legal.as_str(),
                },
                "warnings": warnings,
            })
            .to_string();
        }

        let effective = match self
            .resolve_effective_corpus(p.corpus_id.as_deref(), p.allow_cross_corpus)
            .await
        {
            Ok(effective) => effective,
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": e,
                })
                .to_string();
            }
        };

        let sync_status = self
            .maybe_sync_knowledge_before_search(&effective, &scope, &mut warnings)
            .await;

        let (index_fresh, freshness) = match self.freshness_for_effective(&effective).await {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("freshness failed: {error}"));
                (false, "unknown".to_string())
            }
        };

        if scope == "all" || scope == "knowledge" {
            if plan.knowledge == SearchBackendMode::Fast {
                match self
                    .knowledge_search_impl_with_effective(
                        crate::knowledge::KnowledgeSearchParams {
                            query: p.query.clone(),
                            top_k: p.top_k,
                            mode: Some("fast".to_string()),
                            corpus_id: p.corpus_id.clone(),
                            allow_cross_corpus: p.allow_cross_corpus,
                        },
                        &effective,
                    )
                    .await
                {
                    Ok(result) => {
                        for hit in result.hits {
                            match serde_json::to_value(hit) {
                                Ok(mut value) => {
                                    if let Some(object) = value.as_object_mut() {
                                        object.insert(
                                            "source".to_string(),
                                            serde_json::Value::String("knowledge".to_string()),
                                        );
                                    }
                                    hits.push(value);
                                }
                                Err(e) => warnings.push(format!("knowledge scope failed: {e}")),
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("knowledge scope failed: {e}")),
                }
            }
        }

        if scope == "all" || scope == "legal" {
            if plan.legal == SearchBackendMode::Semantic {
                match self
                    .legal_search_impl_with_effective(build_legal_search_params(&p), &effective)
                    .await
                {
                    Ok(value) => {
                        if let Some(legal_hits) =
                            value.get("hits").and_then(serde_json::Value::as_array)
                        {
                            for hit in legal_hits {
                                let mut value = hit.clone();
                                if let Some(object) = value.as_object_mut() {
                                    object.insert(
                                        "source".to_string(),
                                        serde_json::Value::String("legal".to_string()),
                                    );
                                }
                                hits.push(value);
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("legal scope failed: {e}")),
                }
            }
        }

        serde_json::json!({
            "ok": true,
            "mode_used": plan.mode_used,
            "scope_used": scope,
            "scope_modes": {
                "knowledge": plan.knowledge.as_str(),
                "legal": plan.legal.as_str(),
            },
            "index_fresh": index_fresh,
            "freshness": freshness,
            "sync": sync_status,
            "hits": hits,
            "warnings": warnings,
        })
        .to_string()
    }

    async fn knowledge_add_local_folder_impl(&self, path: &str) -> Result<String, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.add_local_folder(path).map_err(|e| e.to_string())
    }

    async fn knowledge_sync_impl(&self, p: KnowledgeSyncParams) -> Result<SyncSummary, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
        service
            .sync(
                pipeline,
                self.cfg.as_ref(),
                p.source_id.as_deref(),
                crate::indexer::SyncOptions::default(),
            )
            .await
    }

    async fn sync_corpus_impl(
        &self,
        p: crate::corpus_sync::SyncCorpusParams,
    ) -> Result<crate::corpus_sync::SyncCorpusResult, String> {
        let corpus_id = crate::corpus::parse_corpus_id(&p.corpus_id)?;
        let requested = crate::corpus_sync::parse_requested_outputs(&p.outputs)?;
        let corpus = self.corpus().await.map_err(|e| e.to_string())?;
        if !corpus.corpus_exists(corpus_id).map_err(|e| e.to_string())? {
            return Err(format!("unknown corpus_id: {}", p.corpus_id));
        }

        let bound_sources = corpus
            .store()
            .binding_ids_for_corpus_kind(
                corpus_id,
                anno_corpus_core::CorpusBindingKind::KnowledgeSource,
            )
            .map_err(|e| e.to_string())?;
        let selected_sources: Vec<String> = match p.sources {
            Some(sources) => bound_sources
                .iter()
                .filter(|source_id| sources.iter().any(|wanted| wanted == *source_id))
                .cloned()
                .collect(),
            None => bound_sources.clone(),
        };

        let started_at = chrono::Utc::now().to_rfc3339();
        let mut warnings = Vec::new();
        let mut total = crate::indexer::SyncSummary::default();

        if requested.knowledge_fast {
            let service = self.knowledge().await.map_err(|e| e.to_string())?;
            let pipeline = self.pipeline().await.map_err(|e| e.to_string())?;
            let options = crate::indexer::SyncOptions {
                max_files: p
                    .max_files
                    .unwrap_or_else(|| crate::indexer::SyncOptions::default().max_files),
                max_millis: p.max_millis,
            };
            for source_id in &selected_sources {
                match service
                    .sync(
                        pipeline,
                        self.cfg.as_ref(),
                        Some(source_id.as_str()),
                        options,
                    )
                    .await
                {
                    Ok(summary) => {
                        total.seen += summary.seen;
                        total.skipped_unchanged += summary.skipped_unchanged;
                        total.extracted += summary.extracted;
                        total.pseudonymized += summary.pseudonymized;
                        total.fts_ready += summary.fts_ready;
                        total.forgotten += summary.forgotten;
                        total.failed += summary.failed;
                        total.truncated |= summary.truncated;
                    }
                    Err(error) => warnings.push(format!("knowledge source {source_id}: {error}")),
                }
            }
        }

        let legal = if requested.legal_semantic {
            let legal_folders = corpus
                .store()
                .binding_ids_for_corpus_kind(
                    corpus_id,
                    anno_corpus_core::CorpusBindingKind::LegalFolder,
                )
                .map_err(|e| e.to_string())?;
            let mut ingested = 0usize;
            let mut legal_warnings = Vec::new();
            for folder_id in legal_folders {
                let Some(folder) =
                    source_folder_for_legal_binding(corpus.store(), corpus_id, &folder_id)
                        .map_err(|e| e.to_string())?
                else {
                    legal_warnings.push(format!(
                        "legal folder binding {folder_id} has no source path"
                    ));
                    continue;
                };
                match self
                    .legal_ingest_impl(
                        LegalIngestParams {
                            folder,
                            recursive: true,
                        },
                        Some(corpus_id),
                    )
                    .await
                {
                    Ok(value) => {
                        ingested += value
                            .get("ingested")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0) as usize;
                    }
                    Err(error) => legal_warnings.push(error),
                }
            }
            warnings.extend(
                legal_warnings
                    .iter()
                    .map(|warning| format!("legal: {warning}")),
            );
            serde_json::json!({
                "ran": true,
                "ingested": ingested,
                "warnings": legal_warnings,
            })
        } else {
            serde_json::json!({"ran": false, "reason": "output not requested"})
        };
        let freshness = if total.failed == 0 && !total.truncated && warnings.is_empty() {
            "fresh"
        } else {
            "maybe_stale"
        };
        let finished_at = chrono::Utc::now().to_rfc3339();
        let summary = serde_json::json!({
            "knowledge_fast": total,
            "legal": legal.clone(),
            "warnings": warnings.clone(),
        });
        corpus
            .store()
            .upsert_sync_state(
                corpus_id,
                freshness,
                Some(&started_at),
                Some(&finished_at),
                Some(total.seen),
                None,
                &summary,
            )
            .map_err(|e| e.to_string())?;

        Ok(crate::corpus_sync::SyncCorpusResult {
            ok: true,
            corpus_id: corpus_id.as_string(),
            freshness: freshness.to_string(),
            sources: crate::corpus_sync::SyncSourceSummary {
                bound_sources: bound_sources.len(),
                synced_sources: selected_sources.len(),
                skipped_sources: bound_sources.len().saturating_sub(selected_sources.len()),
            },
            knowledge: serde_json::to_value(total).map_err(|e| e.to_string())?,
            legal,
            warnings,
        })
    }

    pub(crate) async fn index_impl_routing(&self, p: IndexParams) -> String {
        if let Err(error) = validate_profile(&p.profile) {
            return serde_json::json!({
                "ok": false,
                "error": error,
            })
            .to_string();
        }

        let corpus = match self.corpus().await {
            Ok(service) => match service.register_index_root(&p.path, &p.profile) {
                Ok(corpus) => corpus,
                Err(e) => {
                    return serde_json::json!({
                        "ok": false,
                        "error": format!("corpus register: {e}"),
                    })
                    .to_string();
                }
            },
            Err(e) => {
                return serde_json::json!({
                    "ok": false,
                    "error": format!("corpus service: {e}"),
                })
                .to_string();
            }
        };

        let mut knowledge = serde_json::Value::Null;
        let mut legal = serde_json::Value::Null;
        let mut errors = Vec::new();

        if matches!(p.profile.as_str(), "general" | "all") {
            match self.knowledge_add_local_folder_impl(&p.path).await {
                Ok(source_id) => {
                    if let Ok(service) = self.corpus().await {
                        if let Err(e) = service.store().add_binding(
                            corpus.corpus_id,
                            anno_corpus_core::CorpusBindingKind::KnowledgeSource,
                            &source_id,
                            &serde_json::json!({"profile": p.profile.clone()}),
                        ) {
                            errors.push(format!("corpus knowledge binding: {e}"));
                        }
                    }
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
                .legal_ingest_impl(
                    LegalIngestParams {
                        folder: p.path.clone(),
                        recursive: true,
                    },
                    Some(corpus.corpus_id),
                )
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
            "corpus_id": corpus.corpus_id.as_string(),
            "corpus_label": corpus.label_pseudo,
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

    pub(crate) async fn forget_impl_routing(&self, p: ForgetParams) -> String {
        if let Ok(uuid) = uuid::Uuid::parse_str(&p.target) {
            let corpus_id = anno_corpus_core::CorpusId::new(uuid);
            if let Ok(service) = self.corpus().await {
                if service.store().corpus_exists(corpus_id).unwrap_or(false) {
                    return self.forget_corpus(corpus_id).await;
                }
            }
        }

        let mut knowledge_removed = 0;
        let mut legal_removed = 0;
        let mut errors = Vec::<String>::new();

        if p.target.starts_with("legal_folder_") {
            match self.resolve_legal_folder_id(&p.target).await {
                Ok(Some(path)) => match self.legal_maintenance().await {
                    Ok(service) => match service.forget_folder_path(&path).await {
                        Ok(removed) => legal_removed = removed,
                        Err(e) => errors.push(format!("legal forget: {e}")),
                    },
                    Err(e) => errors.push(format!("legal maintenance: {e}")),
                },
                Ok(None) => {}
                Err(e) => errors.push(format!("legal resolve: {e}")),
            }
        } else if uuid::Uuid::parse_str(&p.target).is_ok() {
            match self
                .knowledge_forget_impl(KnowledgeForgetParams {
                    source_id: p.target.clone(),
                })
                .await
            {
                Ok(removed) => knowledge_removed = removed,
                Err(e) => errors.push(format!("knowledge forget: {e}")),
            }
        } else {
            match self.knowledge_forget_by_path(&p.target).await {
                Ok(removed) => knowledge_removed = removed,
                Err(e) => errors.push(format!("knowledge forget: {e}")),
            }

            match self.legal_maintenance().await {
                Ok(service) => match service.forget_folder_path(&p.target).await {
                    Ok(removed) => legal_removed = removed,
                    Err(e) => errors.push(format!("legal forget: {e}")),
                },
                Err(e) => errors.push(format!("legal maintenance: {e}")),
            }
        }

        serde_json::json!({
            "ok": errors.is_empty(),
            "removed": {
                "knowledge_objects": knowledge_removed,
                "legal_chunks": legal_removed,
                "tabular_reviews": 0u64,
            },
            "errors": if errors.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!(errors)
            },
        })
        .to_string()
    }

    async fn forget_corpus(&self, corpus_id: anno_corpus_core::CorpusId) -> String {
        let mut knowledge_removed = 0u64;
        let mut legal_removed = 0u64;
        let mut tabular_reviews = 0u64;
        let mut errors = Vec::<String>::new();

        let service = match self.corpus().await {
            Ok(service) => service,
            Err(e) => return format!("Error: {e}"),
        };
        let bindings = match service.store().bindings_for_corpus(corpus_id) {
            Ok(bindings) => bindings,
            Err(e) => return format!("Error: {e}"),
        };
        let legal_doc_ids = match service.store().document_ids_for_corpus(corpus_id, "legal") {
            Ok(ids) => ids,
            Err(e) => {
                errors.push(format!("corpus legal documents: {e}"));
                Vec::new()
            }
        };
        if !legal_doc_ids.is_empty() {
            match self.legal_maintenance().await {
                Ok(service) => match service.forget_doc_ids(&legal_doc_ids).await {
                    Ok(removed) => {
                        legal_removed += removed;
                    }
                    Err(e) => errors.push(format!("legal forget docs: {e}")),
                },
                Err(e) => errors.push(format!("legal maintenance: {e}")),
            }
        }

        for binding in bindings {
            match binding.binding_kind {
                anno_corpus_core::CorpusBindingKind::KnowledgeSource => {
                    match self
                        .knowledge_forget_impl(KnowledgeForgetParams {
                            source_id: binding.binding_id,
                        })
                        .await
                    {
                        Ok(removed) => knowledge_removed += removed,
                        Err(e) => errors.push(format!("knowledge forget: {e}")),
                    }
                }
                anno_corpus_core::CorpusBindingKind::LegalFolder => {
                    match self.resolve_legal_folder_id(&binding.binding_id).await {
                        Ok(Some(path)) => match self.legal_maintenance().await {
                            Ok(service) => match service.forget_folder_path(&path).await {
                                Ok(removed) => legal_removed += removed,
                                Err(e) => errors.push(format!("legal forget: {e}")),
                            },
                            Err(e) => errors.push(format!("legal maintenance: {e}")),
                        },
                        Ok(None) => {
                            match self.legal_maintenance().await {
                                Ok(service) => {
                                    match service.forget_folder_path(&binding.binding_id).await {
                                        Ok(removed) => legal_removed += removed,
                                        Err(e) => errors.push(format!("legal forget: {e}")),
                                    }
                                }
                                Err(e) => errors.push(format!("legal maintenance: {e}")),
                            };
                        }
                        Err(e) => errors.push(format!("legal resolve: {e}")),
                    }
                }
                anno_corpus_core::CorpusBindingKind::TabularReview => {
                    match self.forget_tabular_review(&binding.binding_id).await {
                        Ok(()) => tabular_reviews += 1,
                        Err(e) => errors.push(format!("tabular forget: {e}")),
                    }
                }
                anno_corpus_core::CorpusBindingKind::LegalDocument => {}
            }
        }

        if errors.is_empty() {
            if let Err(e) = service.store().delete_corpus_registry_rows(corpus_id) {
                errors.push(format!("corpus registry delete: {e}"));
            }
        }

        serde_json::json!({
            "ok": errors.is_empty(),
            "removed": {
                "knowledge_objects": knowledge_removed,
                "legal_chunks": legal_removed,
                "tabular_reviews": tabular_reviews,
            },
            "errors": if errors.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!(errors)
            },
        })
        .to_string()
    }

    async fn forget_tabular_review(&self, review_id: &str) -> Result<(), String> {
        let review_uuid = uuid::Uuid::parse_str(review_id).map_err(|e| e.to_string())?;
        let review_id = anno_rag_tabular::ReviewId(review_uuid);
        let ts = self.tabular_storage().await.map_err(|e| e.to_string())?;
        ts.cells
            .delete_for_review(review_id)
            .await
            .map_err(|e| e.to_string())?;
        ts.rows
            .delete_for_review(review_id)
            .await
            .map_err(|e| e.to_string())?;
        ts.columns
            .delete_for_review(review_id)
            .await
            .map_err(|e| e.to_string())?;
        ts.reviews
            .delete(review_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn knowledge_forget_by_path(&self, path: &str) -> Result<u64, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service
            .forget_source_by_path(path)
            .map_err(|e| e.to_string())
    }

    async fn resolve_legal_folder_id(&self, id: &str) -> Result<Option<String>, String> {
        let service = self.legal_maintenance().await.map_err(|e| e.to_string())?;
        service
            .resolve_folder_id(id, legal_folder_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn start_review_extraction(
        &self,
        ts: anno_rag_tabular::storage::StorageHandle,
        review_id: anno_rag_tabular::ReviewId,
        force_reextract: bool,
    ) -> ReviewExtractResult {
        match ts.reviews.get(review_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return ReviewExtractResult {
                    review_id: review_id.0.to_string(),
                    rows: 0,
                    columns: 0,
                    extraction_started: false,
                    extraction_error: Some(format!("review {} not found", review_id.0)),
                };
            }
            Err(e) => {
                return ReviewExtractResult {
                    review_id: review_id.0.to_string(),
                    rows: 0,
                    columns: 0,
                    extraction_started: false,
                    extraction_error: Some(e.to_string()),
                };
            }
        }

        let columns = match ts.columns.list_for_review(review_id).await {
            Ok(columns) => columns,
            Err(e) => {
                return ReviewExtractResult {
                    review_id: review_id.0.to_string(),
                    rows: 0,
                    columns: 0,
                    extraction_started: false,
                    extraction_error: Some(e.to_string()),
                };
            }
        };
        let rows = match ts.rows.list_for_review(review_id).await {
            Ok(rows) => rows,
            Err(e) => {
                return ReviewExtractResult {
                    review_id: review_id.0.to_string(),
                    rows: 0,
                    columns: columns.len(),
                    extraction_started: false,
                    extraction_error: Some(e.to_string()),
                };
            }
        };
        let row_count = rows.len();
        let column_count = columns.len();

        if row_count == 0 || column_count == 0 {
            let error = if row_count == 0 && column_count == 0 {
                "review has no rows or columns to extract".to_string()
            } else if row_count == 0 {
                "review has no rows to extract".to_string()
            } else {
                "review has no columns to extract".to_string()
            };
            self.extraction_status.write().await.insert(
                review_id,
                ReviewExtractionStatus::blocked(review_id, row_count, column_count, error.clone()),
            );
            return ReviewExtractResult {
                review_id: review_id.0.to_string(),
                rows: row_count,
                columns: column_count,
                extraction_started: false,
                extraction_error: Some(error),
            };
        }

        if let Err(e) = self.pipeline().await {
            let error = e.to_string();
            self.extraction_status.write().await.insert(
                review_id,
                ReviewExtractionStatus::blocked(review_id, row_count, column_count, error.clone()),
            );
            return ReviewExtractResult {
                review_id: review_id.0.to_string(),
                rows: row_count,
                columns: column_count,
                extraction_started: false,
                extraction_error: Some(error),
            };
        }

        let Some(arc_pipeline) = self.pipeline_arc() else {
            let error = "pipeline not initialised".to_string();
            self.extraction_status.write().await.insert(
                review_id,
                ReviewExtractionStatus::blocked(review_id, row_count, column_count, error.clone()),
            );
            return ReviewExtractResult {
                review_id: review_id.0.to_string(),
                rows: row_count,
                columns: column_count,
                extraction_started: false,
                extraction_error: Some(error),
            };
        };

        let llm_client = match anno_rag_tabular::llm::routing_client_from_env(false) {
            Ok(llm_client) => llm_client,
            Err(e) => {
                let error = e.to_string();
                self.extraction_status.write().await.insert(
                    review_id,
                    ReviewExtractionStatus::blocked(
                        review_id,
                        row_count,
                        column_count,
                        error.clone(),
                    ),
                );
                return ReviewExtractResult {
                    review_id: review_id.0.to_string(),
                    rows: row_count,
                    columns: column_count,
                    extraction_started: false,
                    extraction_error: Some(error),
                };
            }
        };

        {
            let mut statuses = self.extraction_status.write().await;
            if let Err(result) = try_mark_review_extraction_running(
                &mut statuses,
                review_id,
                row_count,
                column_count,
            ) {
                return result;
            }
        }

        let status = Arc::clone(&self.extraction_status);
        let llm = Arc::from(llm_client);
        tokio::spawn(async move {
            let chunk_src = Arc::new(crate::tabular::PipelineChunkSource(arc_pipeline));
            let extractor = anno_rag_tabular::extract::Extractor::new(llm, chunk_src);
            let cfg = anno_rag_tabular::fanout::FanoutConfig {
                force_reextract,
                ..Default::default()
            };
            match anno_rag_tabular::run_review(&ts, &extractor, review_id, cfg).await {
                Ok(outcomes) => {
                    let ok_rows = outcomes.iter().filter(|o| o.result.is_ok()).count();
                    let row_errors = outcomes
                        .iter()
                        .filter_map(|o| {
                            o.result.as_ref().err().map(|e| ReviewRowErrorWire {
                                row_id: o.row_id.0.to_string(),
                                doc_id: o.doc_id.to_string(),
                                error: e.to_string(),
                            })
                        })
                        .collect::<Vec<_>>();
                    status.write().await.insert(
                        review_id,
                        ReviewExtractionStatus::completed(
                            review_id,
                            row_count,
                            column_count,
                            ok_rows,
                            row_errors,
                        ),
                    );
                }
                Err(e) => {
                    let error = e.to_string();
                    status.write().await.insert(
                        review_id,
                        ReviewExtractionStatus::blocked(review_id, row_count, column_count, error),
                    );
                }
            }
        });

        ReviewExtractResult {
            review_id: review_id.0.to_string(),
            rows: row_count,
            columns: column_count,
            extraction_started: true,
            extraction_error: None,
        }
    }
}

fn legal_folder_id(path: &str) -> String {
    let stable = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, path.as_bytes())
        .simple()
        .to_string();
    format!("legal_folder_{}", &stable[..12])
}

fn corpus_legal_output_dir(
    cfg: &AnnoRagConfig,
    corpus_id: anno_corpus_core::CorpusId,
) -> std::path::PathBuf {
    cfg.data_dir
        .join("corpora")
        .join(corpus_id.as_string())
        .join("outputs")
        .join("legal-anon")
}

fn source_folder_for_legal_binding(
    store: &anno_corpus_store::CorpusStore,
    corpus_id: anno_corpus_core::CorpusId,
    folder_id: &str,
) -> anno_corpus_store::Result<Option<String>> {
    Ok(store
        .binding_metadata(
            corpus_id,
            anno_corpus_core::CorpusBindingKind::LegalFolder,
            folder_id,
        )?
        .and_then(|metadata| {
            metadata
                .get("source_path")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        }))
}

// ---- Tool router ----

#[tool_router]
impl AnnoRagServer {
    /// Deprecated legacy search over the indexed corpus.
    #[tool(
        description = "Deprecated - use 'search(scope=\"legal\", mode=\"semantic\")' for equivalent behavior. Continues to work."
    )]
    async fn legacy_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        match self.legacy_search_impl(params).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Unified search tool across local indexes.
    #[tool(
        description = "Search Anno's local indexes. Omit mode for scope-dependent auto mode: scope='knowledge' uses fast search, scope='legal' uses semantic legal search, and scope='all' reports per-backend scope_modes. Explicit mode='fast' avoids model loading; with scope='all' it skips legal scope, and with scope='legal' it returns an error. mode='semantic' loads models for legal search. scope='all' (default), 'knowledge', or 'legal'. filters forwarded to legal scope."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchUnifiedParams>) -> String {
        self.search_impl_routing(p).await
    }

    #[tool(
        description = "Synchronize a selected corpus. Defaults to knowledge_fast; legal_semantic must be requested explicitly."
    )]
    async fn sync_corpus(
        &self,
        Parameters(p): Parameters<crate::corpus_sync::SyncCorpusParams>,
    ) -> String {
        match self.sync_corpus_impl(p).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
        }
    }

    #[tool(
        description = "List all indexed sources. Labels and ids are pseudonymous; raw local paths are not returned. Does not load models."
    )]
    async fn sources(&self) -> String {
        self.sources_impl_routing().await
    }

    /// List indexed client corpora without exposing raw filesystem paths.
    #[tool(description = "List indexed client corpora without exposing raw filesystem paths.")]
    async fn corpus_list(&self) -> String {
        match self.corpus().await {
            Ok(service) => {
                let count = match service.store().corpus_count() {
                    Ok(count) => count,
                    Err(e) => return format!("Error: {e}"),
                };
                serde_json::json!({
                    "ok": true,
                    "count": count
                })
                .to_string()
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Return one corpus summary by corpus_id.
    #[tool(description = "Return one corpus summary by corpus_id.")]
    async fn corpus_get(&self, Parameters(p): Parameters<CorpusGetParams>) -> String {
        let parsed = match crate::corpus::parse_corpus_id(&p.corpus_id) {
            Ok(id) => id,
            Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
        };
        let service = match self.corpus().await {
            Ok(service) => service,
            Err(e) => {
                return serde_json::json!({ "ok": false, "error": e.to_string() }).to_string();
            }
        };
        match service.get(parsed) {
            Ok(Some(corpus)) => serde_json::json!({ "ok": true, "corpus": corpus }).to_string(),
            Ok(None) => serde_json::json!({
                "ok": false,
                "error": format!("unknown corpus {}", p.corpus_id),
            })
            .to_string(),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        }
    }

    /// Return one corpus health summary by corpus_id.
    #[tool(description = "Return one corpus health summary by corpus_id.")]
    async fn corpus_health(&self, Parameters(p): Parameters<CorpusGetParams>) -> String {
        let parsed = match crate::corpus::parse_corpus_id(&p.corpus_id) {
            Ok(id) => id,
            Err(e) => return serde_json::json!({ "ok": false, "error": e }).to_string(),
        };
        let service = match self.corpus().await {
            Ok(service) => service,
            Err(e) => {
                return serde_json::json!({ "ok": false, "error": e.to_string() }).to_string();
            }
        };
        match service.health(parsed) {
            Ok(health) => serde_json::json!({ "ok": true, "health": health }).to_string(),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        }
    }

    #[tool(
        description = "Remove an indexed source. Accepts a source_id (UUID), a legal corpus id from sources(), or an explicit folder path. Does not load models."
    )]
    async fn forget(&self, Parameters(p): Parameters<ForgetParams>) -> String {
        self.forget_impl_routing(p).await
    }

    #[tool(
        description = "Anno-wide index health: source counts, chunks, vault stats, model load state. Does not load models."
    )]
    async fn status(&self) -> String {
        self.status_impl_routing().await
    }

    /// Prepare a local folder for privacy review in a generated `vault` workspace.
    #[tool(
        description = "Prepare a local folder for privacy review. Creates a local vault workspace with working Word docs, anonymized docs, reports, and a manifest. Returns paths and counts only."
    )]
    async fn privacy_prepare_folder(
        &self,
        Parameters(p): Parameters<PrivacyPrepareFolderParams>,
    ) -> String {
        match self.privacy_prepare_folder_impl(p).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Finalize a local privacy workspace after user Word edits.
    #[tool(
        description = "Finalize a local vault workspace after Word edits. Reads 'à masquer' and 'à garder' comments locally, regenerates anonymized docs, and returns paths and counts only."
    )]
    async fn privacy_finalize_folder(
        &self,
        Parameters(p): Parameters<PrivacyFinalizeFolderParams>,
    ) -> String {
        match self.privacy_finalize_folder_impl(p).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Report privacy workflow capabilities without loading models.
    #[tool(
        description = "Privacy workflow status and capabilities. Does not return document content."
    )]
    async fn privacy_status(&self) -> String {
        serde_json::to_string_pretty(&self.privacy_status_impl().await)
            .unwrap_or_else(|e| format!("Error: {e}"))
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
        let inventory =
            crate::model_inventory::ModelInventoryService::new(self.cfg.as_ref()).inspect();

        if inventory.ready {
            let wire = DownloadModelsResult {
                status: "already_present".into(),
                path: inventory.path.clone(),
                message: format!(
                    "Models ready at {}. The effective model path is selected by {}.",
                    inventory.path,
                    if inventory.from_env {
                        "ANNO_MODELS_DIR"
                    } else {
                        "the default cache"
                    }
                ),
            };
            return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
        }

        if inventory.from_env {
            let wire = DownloadModelsResult {
                status: inventory.state.as_str().into(),
                path: inventory.path.clone(),
                message: format!(
                    "ANNO_MODELS_DIR points to {}, but required model files are missing. \
                     Fix that directory or unset ANNO_MODELS_DIR before using download_models.",
                    inventory.path
                ),
            };
            return serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"));
        }

        let models_dir = self.cfg.models_cache();
        let lock_file = models_dir.join(".download-lock");
        if inventory.downloading || lock_file.exists() {
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
        description = "Deprecated - use 'index(path, profile=\"legal\")' instead. Continues to work."
    )]
    async fn legal_ingest(&self, Parameters(p): Parameters<LegalIngestParams>) -> String {
        match self.legal_ingest_impl(p, None).await {
            Ok(value) => {
                serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Legal-filtered hybrid search. Pseudonymizes the query and restricts
    /// vector + FTS results to chunks matching the supplied legal filters.
    #[tool(
        description = "Deprecated - use 'search(query, scope=\"legal\", filters={...})' instead. Continues to work."
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
        match self.create_review_from_params(p).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
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
        let parsed = parse_review_doc_ids(&p.doc_ids);
        let mut failed = parsed.failed;
        let valid_in_corpus = match self
            .filter_review_doc_ids_by_corpus(review_id, parsed.valid, &mut failed)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                return serde_json::to_string_pretty(&ReviewAddRowsResult {
                    rows_added: 0,
                    extraction_started: false,
                    failed_doc_ids: failed,
                    extraction_error: Some(e),
                })
                .unwrap_or_else(|e| format!("Error: {e}"));
            }
        };
        let ingested_doc_ids = match self
            .filter_ingested_doc_ids(valid_in_corpus, &mut failed)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                return serde_json::to_string_pretty(&ReviewAddRowsResult {
                    rows_added: 0,
                    extraction_started: false,
                    failed_doc_ids: failed,
                    extraction_error: Some(e),
                })
                .unwrap_or_else(|e| format!("Error: {e}"));
            }
        };
        let mut rows_added = 0usize;
        let mut row_errors = Vec::new();
        for doc_id in ingested_doc_ids {
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
                    row_errors.push(format!("adding row for doc_id {doc_id}: {e}"));
                    failed.push(doc_id.to_string());
                }
            }
        }
        let extraction = if rows_added > 0 {
            Some(
                self.start_review_extraction(ts.clone(), review_id, p.force_reextract)
                    .await,
            )
        } else {
            None
        };
        let extraction_started = extraction
            .as_ref()
            .map(|result| result.extraction_started)
            .unwrap_or(false);
        let extraction_error = combine_review_add_rows_extraction_error(
            row_errors,
            extraction.and_then(|result| result.extraction_error),
        );
        serde_json::to_string_pretty(&ReviewAddRowsResult {
            rows_added,
            extraction_started,
            failed_doc_ids: failed,
            extraction_error,
        })
        .unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Start background extraction for an existing review.
    #[tool(
        description = "Start background LLM extraction for an existing tabular review. \
                       Use force_reextract=true to overwrite existing unlocked cells."
    )]
    async fn review_extract(&self, Parameters(p): Parameters<ReviewExtractParams>) -> String {
        let ts = match self.tabular_storage().await {
            Ok(ts) => ts,
            Err(e) => return format!("Error: {e}"),
        };
        let review_id = match uuid::Uuid::parse_str(&p.review_id) {
            Ok(u) => anno_rag_tabular::ReviewId(u),
            Err(e) => return format!("Error: bad review_id: {e}"),
        };
        let result = self
            .start_review_extraction(ts.clone(), review_id, p.force_reextract)
            .await;
        serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}"))
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
        let llm = match anno_rag_tabular::llm::routing_client_from_env(false) {
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
        let extraction_status = self
            .extraction_status
            .read()
            .await
            .get(&review_id)
            .map(ReviewExtractionStatus::to_wire);
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
            extraction_status,
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// List configured Anno knowledge sources. Does not load local ML models.
    #[tool(description = "Deprecated - use 'sources()' instead. Continues to work.")]
    async fn knowledge_sources(&self) -> String {
        match self.knowledge_sources_impl().await {
            Ok(sources) => {
                serde_json::to_string_pretty(&sources).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Return local Anno knowledge status without loading ML models.
    #[tool(description = "Deprecated - use 'status()' instead. Continues to work.")]
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
        description = "Deprecated - use 'search(query, scope=\"knowledge\")' instead. Continues to work."
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
    #[tool(description = "Deprecated - use 'index(path)' instead. Continues to work.")]
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
        description = "Deprecated - use 'index(path)' (re-indexes idempotently). Continues to work."
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
    #[tool(description = "Deprecated - use 'forget(target)' instead. Continues to work.")]
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

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server failed to start: {e}")))?;

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
mod tabular_status_tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    async fn temp_server() -> (AnnoRagServer, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };
        (AnnoRagServer::new_lazy(cfg, [0u8; 32]), tmp)
    }

    #[test]
    fn parse_review_doc_ids_separates_invalid_uuid_strings() {
        let good = uuid::Uuid::now_v7();
        let parsed = parse_review_doc_ids(&[good.to_string(), "not-a-uuid".into()]);

        assert_eq!(parsed.valid, vec![good]);
        assert_eq!(parsed.failed, vec!["not-a-uuid".to_string()]);
    }

    #[test]
    fn review_add_rows_extraction_error_includes_row_storage_errors() {
        let combined = combine_review_add_rows_extraction_error(
            vec!["adding row for doc_id 018f1a90-f1a0-7000-8000-000000000001: write failed".into()],
            Some("review has no columns to extract".into()),
        )
        .expect("combined error");

        assert!(combined.contains("write failed"));
        assert!(combined.contains("review has no columns to extract"));
    }

    #[test]
    fn completed_extraction_status_converts_to_wire() {
        let review_id = anno_rag_tabular::ReviewId::new();
        let status = ReviewExtractionStatus::completed(
            review_id,
            2,
            3,
            1,
            vec![ReviewRowErrorWire {
                row_id: "row-1".into(),
                doc_id: "doc-1".into(),
                error: "LLM failed".into(),
            }],
        );

        let wire = status.to_wire();

        assert_eq!(wire.review_id, review_id.0.to_string());
        assert_eq!(wire.state, "completed_with_errors");
        assert_eq!(wire.rows, 2);
        assert_eq!(wire.columns, 3);
        assert_eq!(wire.ok_rows, 1);
        assert_eq!(wire.failed_rows, 1);
        assert_eq!(wire.row_errors[0].error, "LLM failed");
    }

    #[test]
    fn blocked_extraction_status_carries_human_error() {
        let review_id = anno_rag_tabular::ReviewId::new();
        let status =
            ReviewExtractionStatus::blocked(review_id, 2, 14, "Models not downloaded".into());
        let wire = status.to_wire();

        assert_eq!(wire.state, "blocked");
        assert_eq!(wire.failed_rows, 2);
        assert_eq!(wire.last_error.as_deref(), Some("Models not downloaded"));
    }

    #[test]
    fn running_extraction_status_blocks_duplicate_start() {
        let review_id = anno_rag_tabular::ReviewId::new();
        let mut statuses = HashMap::new();

        let first = try_mark_review_extraction_running(&mut statuses, review_id, 3, 4);
        assert!(first.is_ok(), "first start should mark running");
        let duplicate = try_mark_review_extraction_running(&mut statuses, review_id, 9, 10)
            .expect_err("second start should be rejected");

        assert!(!duplicate.extraction_started);
        assert_eq!(duplicate.rows, 3);
        assert_eq!(duplicate.columns, 4);
        assert!(duplicate
            .extraction_error
            .as_deref()
            .expect("duplicate error")
            .contains("already running"));
    }

    #[test]
    fn review_get_result_serializes_extraction_status() {
        let review_id = anno_rag_tabular::ReviewId::new();
        let result = ReviewGetResult {
            review_id: review_id.0.to_string(),
            name: "Review".into(),
            columns: Vec::new(),
            rows: Vec::new(),
            cells: Vec::new(),
            extraction_status: Some(ReviewExtractionStatus::running(review_id, 3, 4).to_wire()),
        };

        let json = serde_json::to_value(result).expect("serialize review get result");

        assert_eq!(json["extraction_status"]["state"], "running");
        assert_eq!(json["extraction_status"]["rows"], 3);
        assert_eq!(json["extraction_status"]["columns"], 4);
    }

    #[tokio::test]
    async fn review_create_rejects_bad_template_without_orphan_review() {
        let (server, _tmp) = temp_server().await;
        let ts = server.tabular_storage().await.expect("tabular storage");

        let result = server
            .create_review_from_params(ReviewCreateParams {
                name: "Bad template".into(),
                template_id: Some("missing-template".into()),
                scope_folder: None,
                corpus_id: None,
            })
            .await;

        assert!(result.is_err());
        let reviews = ts.reviews.list().await.expect("list reviews");
        assert!(
            reviews.is_empty(),
            "invalid template must not create a review"
        );
    }

    #[tokio::test]
    async fn review_add_rows_rejects_doc_outside_review_corpus() {
        let (server, _tmp) = temp_server().await;
        let corpus_service = server.corpus().await.expect("corpus");
        let a = corpus_service
            .register_index_root("c:/clients/a", "all")
            .expect("a");
        let b = corpus_service
            .register_index_root("c:/clients/b", "all")
            .expect("b");
        let doc_b = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b-doc");
        corpus_service
            .store()
            .add_document(
                b.corpus_id,
                anno_corpus_core::DocumentInstanceId::new(doc_b),
                "legal",
                "c:/clients/b/doc.pdf",
                Some("doc.pdf"),
                &anno_corpus_core::ContentId::from_bytes(b"doc"),
                &serde_json::json!({}),
            )
            .expect("doc b");

        let created = server
            .create_review_from_params(ReviewCreateParams {
                name: "A review".into(),
                template_id: None,
                scope_folder: None,
                corpus_id: Some(a.corpus_id.as_string()),
            })
            .await
            .expect("review");

        let out = server
            .review_add_rows(Parameters(ReviewAddRowsParams {
                review_id: created.review_id,
                doc_ids: vec![doc_b.to_string()],
                force_reextract: false,
            }))
            .await;

        assert!(out.contains("outside review corpus"), "{out}");
        assert!(out.contains(&doc_b.to_string()), "{out}");
    }
}

#[cfg(test)]
mod lazy_tests {
    use super::*;
    use crate::model_inventory::test_env::ScopedAnnoModelsDir;
    use anno_rag::config::AnnoRagConfig;
    use std::path::Path;

    fn create_required_model_files(models_dir: &Path) {
        for rel in [
            "multilingual-e5-small/config.json",
            "multilingual-e5-small/model.safetensors",
            "multilingual-e5-small/tokenizer.json",
            "gliner2-multi-v1-onnx/fp32_v2/classifier_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/count_lstm_fixed_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/count_pred_argmax_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/encoder_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/schema_gather_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/scorer_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/span_rep_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/token_gather_fp32.onnx",
            "gliner2-multi-v1-onnx/fp32_v2/tokenizer.json",
        ] {
            let path = models_dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("required file parent")).unwrap();
            std::fs::write(path, b"test model file").unwrap();
        }
    }

    #[test]
    fn deprecated_tools_have_deprecation_banner_in_description() {
        let src = include_str!("lib.rs");
        let deprecated = [
            "legal_search",
            "legal_ingest",
            "knowledge_search",
            "knowledge_add_local_folder",
            "knowledge_sync",
            "knowledge_sources",
            "knowledge_status",
            "knowledge_forget",
            "legacy_search",
        ];
        for name in deprecated {
            let needle = format!("async fn {name}(");
            let pos = src
                .find(&needle)
                .unwrap_or_else(|| panic!("tool {name} not found"));
            let before = &src[..pos];
            let tool_block_start = before
                .rfind("#[tool(")
                .unwrap_or_else(|| panic!("no #[tool( before {name}"));
            let tool_block = &src[tool_block_start..pos];
            assert!(
                tool_block.contains("Deprecated"),
                "tool {name} description missing 'Deprecated' marker: {tool_block}",
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lazy_server_returns_error_when_models_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let key = [0u8; 32];

        let _models_env = ScopedAnnoModelsDir::unset();

        let server = AnnoRagServer::new_lazy(cfg, key);
        let result = server
            .legacy_search(Parameters(SearchParams {
                query: "test".into(),
                top_k: 1,
                rerank: false,
            }))
            .await;

        assert!(
            result.contains("Models not ready") || result.contains("Models not downloaded"),
            "expected model readiness error in: {result}"
        );
    }

    #[tokio::test]
    async fn search_fast_all_returns_legal_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("all".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert_eq!(v["mode_used"], "fast");
        assert_eq!(v["scope_used"], "all");
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(warnings.iter().any(|w| w.as_str().unwrap_or("")
            == "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results."));
    }

    #[tokio::test]
    async fn search_legal_without_mode_uses_semantic() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: None,
                scope: Some("legal".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["mode_used"], "semantic");
        assert_eq!(v["scope_modes"]["legal"], "semantic");
        assert!(v["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .all(|w| !w
                .as_str()
                .unwrap_or("")
                .contains("legal scope skipped in fast mode")));
    }

    #[tokio::test]
    async fn search_all_without_mode_reports_auto_scope_modes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: None,
                scope: Some("all".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["mode_used"], "auto");
        assert_eq!(v["scope_modes"]["knowledge"], "fast");
        assert_eq!(v["scope_modes"]["legal"], "semantic");
    }

    #[tokio::test]
    async fn search_reports_unknown_freshness_for_unsynced_single_corpus() {
        let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
        let corpus = server.corpus().await.expect("corpus");
        let registered = corpus
            .register_index_root("c:/clients/a", "general")
            .expect("register");

        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".to_string(),
                top_k: 5,
                mode: Some("fast".to_string()),
                scope: Some("knowledge".to_string()),
                filters: None,
                corpus_id: Some(registered.corpus_id.as_string()),
                allow_cross_corpus: false,
            })
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["index_fresh"], false);
        assert_eq!(parsed["freshness"], "unknown");
    }

    #[tokio::test]
    async fn search_does_not_opportunistically_load_pipeline_when_models_are_cold() {
        let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
        let corpus = server.corpus().await.expect("corpus");
        let registered = corpus
            .register_index_root("c:/clients/a", "general")
            .expect("register");

        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".to_string(),
                top_k: 5,
                mode: Some("fast".to_string()),
                scope: Some("knowledge".to_string()),
                filters: None,
                corpus_id: Some(registered.corpus_id.as_string()),
                allow_cross_corpus: false,
            })
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["sync"]["attempted"], false);
        assert_eq!(parsed["sync"]["reason"], "models_not_loaded");
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn search_marks_index_not_fresh_before_sync_corpus() {
        let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
        let corpus = server.corpus().await.expect("corpus");
        let registered = corpus
            .register_index_root("c:/clients/living-folder", "general")
            .expect("register");

        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "new document".to_string(),
                top_k: 5,
                mode: Some("fast".to_string()),
                scope: Some("knowledge".to_string()),
                filters: None,
                corpus_id: Some(registered.corpus_id.as_string()),
                allow_cross_corpus: false,
            })
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["index_fresh"], false);
        assert!(parsed["freshness"].as_str().is_some());
    }

    #[tokio::test]
    async fn search_still_requires_corpus_when_multiple_corpora_exist_after_freshness_changes() {
        let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
        let corpus = server.corpus().await.expect("corpus");
        corpus
            .register_index_root("c:/clients/a", "general")
            .expect("register a");
        corpus
            .register_index_root("c:/clients/b", "general")
            .expect("register b");

        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".to_string(),
                top_k: 5,
                mode: Some("fast".to_string()),
                scope: Some("knowledge".to_string()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: false,
            })
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["ok"], false);
        assert!(parsed["error"].as_str().unwrap().contains("corpus"));
    }

    #[tokio::test]
    async fn sync_corpus_unknown_corpus_returns_structured_error() {
        let server = AnnoRagServer::new_lazy(AnnoRagConfig::default(), [0u8; 32]);
        let out = server
            .sync_corpus(Parameters(crate::corpus_sync::SyncCorpusParams {
                corpus_id: uuid::Uuid::new_v4().to_string(),
                sources: None,
                outputs: vec!["knowledge_fast".to_string()],
                max_files: None,
                max_millis: None,
            }))
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["ok"], false);
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("unknown corpus_id"));
    }

    #[tokio::test]
    async fn search_fast_legal_returns_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("legal".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], false);
        assert!(v["error"]
            .as_str()
            .unwrap_or("")
            .contains("legal scope requires semantic mode"));
    }

    #[tokio::test]
    async fn search_requires_corpus_when_multiple_exist() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let corpus = server.corpus().await.expect("corpus");
        corpus
            .register_index_root("c:/clients/a", "all")
            .expect("a");
        corpus
            .register_index_root("c:/clients/b", "all")
            .expect("b");

        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("knowledge".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: false,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], false);
        assert!(v["error"]
            .as_str()
            .unwrap_or("")
            .contains("corpus_id is required"));
        assert!(!out.contains("c:/clients/a"));
        assert!(!out.contains("c:/clients/b"));
    }

    #[tokio::test]
    async fn search_fast_knowledge_returns_no_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("knowledge".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["scope_used"], "knowledge");
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn search_semantic_knowledge_returns_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("semantic".into()),
                scope: Some("knowledge".into()),
                filters: None,
                corpus_id: None,
                allow_cross_corpus: true,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["mode_used"], "semantic");
        assert_eq!(v["scope_used"], "knowledge");
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(warnings.iter().any(|w| w
            .as_str()
            .unwrap_or("")
            .contains("knowledge scope skipped in semantic mode")));
    }

    #[tokio::test]
    async fn sources_aggregates_knowledge_and_legal_corpora() {
        let dir = tempfile::tempdir().expect("temp dir");
        let corpus_dir = dir.path().join("corpus");
        std::fs::create_dir_all(&corpus_dir).expect("corpus dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let source_id = server
            .knowledge_add_local_folder_impl(corpus_dir.to_str().expect("utf8 path"))
            .await
            .expect("source id");
        let out = server.sources_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        let sources = v["sources"].as_array().expect("sources array");
        assert!(
            sources.is_empty()
                || sources
                    .iter()
                    .all(|s| { s["kind"] == "knowledge_folder" || s["kind"] == "legal_corpus" })
        );
        let knowledge = sources
            .iter()
            .find(|s| s["kind"] == "knowledge_folder")
            .expect("knowledge source");
        assert_eq!(knowledge["id"], source_id);
        assert_eq!(knowledge["source_id"], source_id);
        assert_ne!(
            knowledge["label"].as_str().unwrap_or(""),
            corpus_dir.to_str().unwrap_or("")
        );
    }

    #[test]
    fn legal_folder_id_is_stable_and_pseudonymous() {
        let path = "C:/legal/client-a";
        let id = legal_folder_id(path);
        assert_eq!(id, legal_folder_id(path));
        assert!(id.starts_with("legal_folder_"));
        assert_eq!(id.len(), "legal_folder_".len() + 12);
        assert!(!id.contains("client-a"));
        assert!(!id.contains(path));
    }

    #[test]
    fn corpus_legal_output_dir_is_internal_and_corpus_scoped() {
        let cfg = AnnoRagConfig {
            data_dir: std::path::PathBuf::from("C:/anno-data"),
            ..AnnoRagConfig::default()
        };
        let corpus_id = anno_corpus_core::CorpusId::from_normalized_root("C:/clients/matter-a");

        let out = corpus_legal_output_dir(&cfg, corpus_id);

        assert_eq!(
            out,
            std::path::PathBuf::from("C:/anno-data")
                .join("corpora")
                .join(corpus_id.as_string())
                .join("outputs")
                .join("legal-anon")
        );
        assert!(!out.starts_with("C:/clients/matter-a"));
    }

    #[tokio::test]
    async fn sources_does_not_initialize_pipeline() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        assert!(server.pipeline_arc().is_none());

        let out = server.sources_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn corpus_service_opens_under_data_dir() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let out = server.corpus_list().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert_eq!(v["count"], 0);
        assert!(dir.path().join("corpus.sqlite3").exists());
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn corpus_get_unknown_returns_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let id = uuid::Uuid::new_v4().to_string();

        let out = server
            .corpus_get(Parameters(CorpusGetParams { corpus_id: id }))
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap_or("").contains("unknown corpus"));
    }

    #[tokio::test]
    async fn corpus_health_unknown_returns_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let id = uuid::Uuid::new_v4().to_string();

        let out = server
            .corpus_health(Parameters(CorpusGetParams { corpus_id: id }))
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap_or("").contains("unknown corpus"));
    }

    #[tokio::test]
    async fn forget_uuid_routes_to_knowledge_forget() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let target = uuid::Uuid::nil().to_string();

        let out = server
            .forget_impl_routing(ForgetParams {
                target: target.clone(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert_eq!(v["removed"]["knowledge_objects"], 0);
        assert_eq!(v["removed"]["legal_chunks"], 0);
        assert!(v["errors"].is_null());
    }

    #[tokio::test]
    async fn forget_corpus_reports_all_backend_buckets() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let corpus = server
            .corpus()
            .await
            .expect("corpus")
            .register_index_root("c:/clients/a", "all")
            .expect("register");

        let out = server
            .forget_impl_routing(ForgetParams {
                target: corpus.corpus_id.as_string(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert!(v["removed"].get("knowledge_objects").is_some());
        assert!(v["removed"].get("legal_chunks").is_some());
        assert!(v["removed"].get("tabular_reviews").is_some());
    }

    #[tokio::test]
    async fn forget_legal_id_is_noop_when_unknown() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        assert!(server.pipeline_arc().is_none());

        let out = server
            .forget_impl_routing(ForgetParams {
                target: "legal_folder_000000000000".to_string(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert_eq!(v["removed"]["knowledge_objects"], 0);
        assert_eq!(v["removed"]["legal_chunks"], 0);
        assert!(v["errors"].is_null());
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn forget_path_attempts_legal_maintenance_without_pipeline() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let out = server
            .forget_impl_routing(ForgetParams {
                target: dir.path().join("client").display().to_string(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["ok"], true);
        assert_eq!(v["removed"]["legal_chunks"], 0);
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn status_returns_unified_health() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        assert!(server.pipeline_arc().is_none());
        let out = server.status_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert!(v.get("knowledge").is_some());
        assert!(v.get("vault").is_some());
        assert!(v.get("models").is_some());
        assert_eq!(v["knowledge"]["objects"], 0);
        assert_eq!(v["vault"]["available"], false);
        assert_eq!(v["vault"]["reason"], "pipeline_not_initialized");
        assert!(v["models"].get("inventory").is_some());
        assert_eq!(v["models"]["embedder_loaded"], false);
        assert_eq!(v["models"]["detector_loaded"], false);
        assert!(server.pipeline_arc().is_none());
    }

    /// When all required model files exist, download_models reports already_present.
    #[tokio::test(flavor = "current_thread")]
    async fn download_models_tool_reports_already_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("models");
        create_required_model_files(&models_dir);
        let _models_env = ScopedAnnoModelsDir::unset();

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

    #[tokio::test(flavor = "current_thread")]
    async fn download_models_tool_rejects_empty_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("models");
        std::fs::create_dir_all(models_dir.join("multilingual-e5-small")).unwrap();
        std::fs::create_dir_all(models_dir.join("gliner2-multi-v1-onnx")).unwrap();

        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let _models_env = ScopedAnnoModelsDir::set(&models_dir);

        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let result = server.download_models().await;

        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("result must be valid JSON");
        assert_eq!(parsed["status"], "partial");
        assert_eq!(
            parsed["path"].as_str().unwrap_or(""),
            models_dir.to_str().unwrap()
        );
        assert!(
            parsed["message"]
                .as_str()
                .unwrap_or("")
                .contains("ANNO_MODELS_DIR"),
            "message should tell user to fix/unset env var: {result}"
        );
    }

    /// When a default-cache download lock exists, the tool reports in_progress
    /// without starting another background download.
    #[tokio::test(flavor = "current_thread")]
    async fn download_models_tool_reports_in_progress_when_lock_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("models");
        std::fs::create_dir_all(&models_dir).unwrap();
        std::fs::write(models_dir.join(".download-lock"), b"downloading").unwrap();
        let _models_env = ScopedAnnoModelsDir::unset();
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let result = server.download_models().await;
        assert!(
            result.contains("in_progress") || result.contains("downloading"),
            "expected in-progress download status in: {result}"
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

        assert!(
            parsed["corpus_id"].as_str().is_some(),
            "index should return corpus_id: {result}"
        );
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
