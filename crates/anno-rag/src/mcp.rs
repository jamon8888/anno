//! MCP server exposing anno-rag's retrieval surface to Cowork over stdio.
//!
//! Tools: `search`, `rehydrate`, `detect`, `vault_stats`.
//! Reuse rmcp 1.6's `#[tool_router]` + `#[tool_handler]` pattern from
//! `vendor/cloakpipe/crates/cloakpipe-mcp/src/lib.rs`.
//!
//! Task 6 wires the tool ROUTING and SCHEMAS. Tool bodies for rehydrate,
//! detect, vault_stats are PLACEHOLDERS — Task 7 fills them once
//! `Pipeline` exposes the underlying helpers. `search` is wired live
//! because `Pipeline::search` already exists.

use crate::config::AnnoRagConfig;
use crate::pipeline::Pipeline;
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
}

#[tool_handler]
impl ServerHandler for AnnoRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "anno-rag MCP server. Tools: search (pseudonymized retrieval), \
                 rehydrate (restore originals), detect (dry-run scan), vault_stats.",
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
/// Returns [`crate::error::Error::Detect`] if the rmcp transport fails to
/// initialize or the server loop returns an error.
pub async fn serve_stdio(pipeline: Pipeline, cfg: AnnoRagConfig) -> crate::error::Result<()> {
    let server = AnnoRagServer::new(pipeline, cfg);
    tracing::info!("anno-rag MCP server starting on stdio");

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| crate::error::Error::Detect(format!("MCP server failed to start: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| crate::error::Error::Detect(format!("MCP server error: {e}")))?;
    Ok(())
}
