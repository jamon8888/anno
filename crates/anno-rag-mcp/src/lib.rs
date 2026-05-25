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
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
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
            cfg: Arc::new(cfg),
            key: [0u8; 32],
            tool_router: Self::tool_router(),
        }
    }

    /// Construct with deferred pipeline init (lazy path). Used by serve_stdio_lazy.
    #[must_use]
    pub fn new_lazy(cfg: AnnoRagConfig, key: [u8; 32]) -> Self {
        Self {
            pipeline: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key,
            tool_router: Self::tool_router(),
        }
    }
}

// ---- Tool parameter types ----

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

// ---- Tool router ----

#[tool_router]
impl AnnoRagServer {
    /// Search the indexed corpus. Pseudonymizes the query through the local
    /// vault, embeds it, and returns top-K ranked pseudonymized chunks.
    #[tool(
        description = "Search the indexed corpus. Pseudonymizes the query through the local vault, embeds it, returns top-K ranked chunks. Chunks are pseudonymized — call rehydrate(text) to restore originals."
    )]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let p = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
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
                return "Error: rerank requested but server built without \
                        the `rerank` feature"
                    .to_string();
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
        match result {
            Ok(hits) => {
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
                serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
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
        let p = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
        let s = p.vault_stats().await;
        let wire = VaultStatsResult {
            total_mappings: s.total_mappings,
            categories: s.categories,
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
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
        let vault_initialized = self
            .pipeline_arc()
            .map(|arc| arc.vault_is_initialized())
            .unwrap_or(false);
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
        let pipeline = match self.pipeline().await {
            Ok(p) => p,
            Err(e) => return format!("Error: {e}"),
        };
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
                serde_json::to_string_pretty(&LegalIngestResult {
                    ingested: n,
                    folder: p.folder,
                })
                .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_ingest",
                    result = "error",
                    "{e}"
                );
                format!("Error: {e}")
            }
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
        let pipeline = match self.pipeline().await {
            Ok(pipeline) => pipeline,
            Err(e) => return format!("Error: {e}"),
        };
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
                serde_json::to_string_pretty(&LegalSearchResult {
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
                .unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_search",
                    result = "error",
                    "{e}"
                );
                format!("Error: {e}")
            }
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
    async fn legal_graph_query(
        &self,
        Parameters(p): Parameters<LegalGraphQueryParams>,
    ) -> String {
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
            other => return format!(
                "Error: unknown intent `{other}`. \
                 Valid: party_dossier, obligations_owed_by, citation_chain, \
                 procedural_timeline, appeal_chain"
            ),
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
    #[tool(
        description = "Retrieve the procedural timeline for `dossier_id`. \
                       Returns events (kind, event_date, deadline_date, chunk_id) \
                       in chronological order."
    )]
    async fn legal_timeline(
        &self,
        Parameters(p): Parameters<LegalTimelineParams>,
    ) -> String {
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
    async fn legal_risk_review(
        &self,
        Parameters(p): Parameters<LegalRiskReviewParams>,
    ) -> String {
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
        match pipeline.legal_mandatory_clause_audit(doc_id, &p.doc_type).await {
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
                Ok(d) => interrupting.push(InterruptingEvent { kind: ev.kind.clone(), date: d }),
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
    #[tool(
        description = "Record a validation of an extracted fact. \
                       action: confirm | reject | correct. \
                       When action is 'correct', supply corrected_value. \
                       Writes a Validation node to the KG linked to the chunk. \
                       Returns the validation_id for audit tracing."
    )]
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
                return format!(
                    "Error: unknown action '{other}'. Valid: confirm, reject, correct"
                )
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
            Ok(ack) => {
                serde_json::to_string_pretty(&ack).unwrap_or_else(|e| format!("Error: {e}"))
            }
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
}
