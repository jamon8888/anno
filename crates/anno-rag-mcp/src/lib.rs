//! MCP server exposing anno-rag's retrieval surface to Cowork over stdio.
//!
//! Tools: `search`, `rehydrate`, `detect`, `vault_stats`, memory_*.
//! Reuse rmcp 1.6's `#[tool_router]` + `#[tool_handler]` pattern from
//! `vendor/cloakpipe/crates/cloakpipe-mcp/src/lib.rs`.
//!
//! This crate was extracted from `anno-rag::mcp` so that Phase 8 of the
//! tabular-review plan can attach `anno-rag-tabular` review tools here
//! without creating a cycle (anno-rag-tabular already depends on anno-rag).

#![warn(missing_docs)]

use anno_rag::config::AnnoRagConfig;
use anno_rag::pipeline::Pipeline;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// State held by the MCP server: a fully-initialized Pipeline plus config.
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<Pipeline>,
    cfg: AnnoRagConfig,
    #[allow(dead_code)] // populated + consumed by the rmcp #[tool_router] macro
    tool_router: ToolRouter<Self>,
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
    redacted_text: String,
    token_count: usize,
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

// ---- Tool router ----

#[tool_router]
impl AnnoRagServer {
    /// Search the indexed corpus. Pseudonymizes the query through the local
    /// vault, embeds it, and returns top-K ranked pseudonymized chunks.
    #[tool(
        description = "Search the indexed corpus. Pseudonymizes the query through the local vault, embeds it, returns top-K ranked chunks. Chunks are pseudonymized — call rehydrate(text) to restore originals."
    )]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        match self.pipeline.search(&params.query, params.top_k).await {
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
        match self.pipeline.rehydrate(&params.text).await {
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
        match self.pipeline.detect(&params.text) {
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
        let s = self.pipeline.vault_stats().await;
        let wire = VaultStatsResult {
            total_mappings: s.total_mappings,
            categories: s.categories,
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
    }

    /// Save a memory. PII is tokenized through the local vault before storage.
    #[tool(
        description = "Save a memory. PII is tokenized through the local vault before storage. Returns the new id and the redacted text actually persisted."
    )]
    async fn memory_save(&self, Parameters(p): Parameters<MemorySaveParams>) -> String {
        let start = std::time::Instant::now();
        let kind = p.kind.as_deref().and_then(parse_kind);
        let r = self.pipeline.save_memory(&p.text, kind, p.session_id).await;
        let elapsed = start.elapsed().as_millis() as u64;
        match r {
            Ok(s) => {
                tracing::info!(
                    target: "anno_rag::memory::audit",
                    tool = "memory_save",
                    result = "ok",
                    duration_ms = elapsed,
                    ""
                );
                let wire = MemorySaveResultWire {
                    id: s.id.as_string(),
                    redacted_text: s.redacted_text,
                    token_count: s.token_refs.len(),
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
        let start = std::time::Instant::now();
        let kinds = p
            .kinds
            .as_ref()
            .map(|v| v.iter().filter_map(|k| parse_kind(k)).collect::<Vec<_>>());
        let r = self
            .pipeline
            .recall_memory(
                &p.query,
                p.top_k,
                p.session_id,
                kinds,
                p.as_of,
                p.graph_expand,
            )
            .await;
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
        let id = match &p.id {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(u) => Some(anno_rag::memory::MemoryId(u)),
                Err(e) => return format!("Error: bad id: {e}"),
            },
            None => None,
        };
        let start = std::time::Instant::now();
        let r = self
            .pipeline
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
        let start = std::time::Instant::now();
        let kind = p.kind.as_deref().and_then(parse_kind);
        let r = self
            .pipeline
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
        let start = std::time::Instant::now();
        let r = self
            .pipeline
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
        let id = match uuid::Uuid::parse_str(&p.id) {
            Ok(u) => anno_rag::memory::MemoryId(u),
            Err(e) => return format!("Error: bad id: {e}"),
        };
        let when = p.at.unwrap_or_else(chrono::Utc::now);
        let start = std::time::Instant::now();
        let r = self.pipeline.invalidate_memory(&id, Some(when)).await;
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
}

#[tool_handler]
impl ServerHandler for AnnoRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "anno-rag MCP server. Tools: search (pseudonymized retrieval), \
                 rehydrate (restore originals), detect (dry-run scan), vault_stats, \
                 memory_save / memory_recall / memory_forget / memory_list \
                 (PII-safe session memory; GDPR Art. 17 cascades to vault tokens), \
                 memory_graph_recall / memory_invalidate (v0.2 entity-graph + \
                 bi-temporal). memory_recall accepts as_of + graph_expand.",
            )
            .with_server_info(Implementation::new(
                self.cfg.mcp_server_name.clone(),
                env!("CARGO_PKG_VERSION"),
            ))
    }
}

impl AnnoRagServer {
    /// Construct a new MCP server. Owns the pipeline through `Arc`.
    #[must_use]
    pub fn new(pipeline: Pipeline, cfg: AnnoRagConfig) -> Self {
        Self {
            pipeline: Arc::new(pipeline),
            cfg,
            tool_router: Self::tool_router(),
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
