//! Tool parameter types for the anno-rag MCP server.

use rmcp::schemars;
use serde::Deserialize;

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

pub(crate) fn default_index_profile() -> String {
    "general".to_string()
}

pub(crate) fn default_true() -> bool {
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

pub(crate) fn validate_profile(profile: &str) -> Result<(), String> {
    match profile {
        "general" | "legal" | "all" => Ok(()),
        other => Err(format!(
            "Unsupported index profile '{other}'. Expected one of: general, legal, all."
        )),
    }
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

pub(crate) fn default_top_k() -> usize {
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

pub(crate) fn default_search_unified_top_k() -> usize {
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

pub(crate) fn parse_kind(s: &str) -> Option<anno_rag::memory::MemoryKind> {
    match s {
        "fact" => Some(anno_rag::memory::MemoryKind::Fact),
        "preference" => Some(anno_rag::memory::MemoryKind::Preference),
        "reference" => Some(anno_rag::memory::MemoryKind::Reference),
        "context" => Some(anno_rag::memory::MemoryKind::Context),
        _ => None,
    }
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

pub(crate) fn default_max_hops() -> u8 {
    2
}
pub(crate) fn default_per_hop_limit() -> usize {
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

pub(crate) fn default_forget_limit() -> usize {
    5
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

pub(crate) fn default_list_limit() -> usize {
    20
}
