//! Engine health collector for the `anno_health` MCP tool (spec §13.1).

use serde::{Deserialize, Serialize};

/// Set at compile time only in the CI signing job.
const SIGNED_BUILD: bool = option_env!("ANNO_RAG_SIGNED_BUILD").is_some();

/// Set at compile time only by the `.mcpb` packaging step.
const EXTENSION_INSTALL: bool = option_env!("ANNO_RAG_EXTENSION_INSTALL").is_some();

/// Wire shape returned by `anno_health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealth {
    /// Engine crate version.
    pub engine_version: String,
    /// Current CPU architecture and operating system.
    pub build_target: String,
    /// Whether the binary was built by the signing workflow.
    pub signed: bool,
    /// Whether the binary was packaged for one-click extension install.
    pub extension_install: bool,
    /// Whether the vault can be opened with the configured key.
    pub vault_initialized: bool,
    /// MCP tool names advertised by this server.
    pub available_tools: Vec<String>,
}

/// Result of `anno_init_vault`. The passphrase itself is never echoed.
#[derive(Debug, Clone, Serialize)]
pub struct InitVaultResult {
    /// True when the passphrase was accepted and stored.
    pub ok: bool,
    /// User-facing validation or keyring error when initialization fails.
    pub error: Option<String>,
}

/// Validate the passphrase and store the Argon2id-derived key in the OS
/// keyring. Returns a structured result so callers see `{"ok": false, "error":
/// "..."}` on validation failures rather than a panic or raw error.
pub fn init_vault_with_passphrase(passphrase: &str) -> InitVaultResult {
    if passphrase.trim().is_empty() {
        return InitVaultResult {
            ok: false,
            error: Some("passphrase must not be empty".to_string()),
        };
    }
    if passphrase.chars().count() < 12 {
        return InitVaultResult {
            ok: false,
            error: Some("passphrase must be at least 12 characters".to_string()),
        };
    }
    match anno_rag::vault::store_passphrase_derived_key_in_keyring(passphrase) {
        Ok(()) => InitVaultResult {
            ok: true,
            error: None,
        },
        Err(e) => InitVaultResult {
            ok: false,
            error: Some(format!("keyring write failed: {e}")),
        },
    }
}

/// Build an [`EngineHealth`] snapshot from a live pipeline.
///
/// Extracted for testability — the MCP `anno_health` tool calls the same
/// logic inline but cannot be called directly from integration tests because
/// `#[tool_router]` methods are private.
pub async fn collect_health(
    pipeline: &anno_rag::pipeline::Pipeline,
    _cfg: &anno_rag::config::AnnoRagConfig,
) -> EngineHealth {
    EngineHealth {
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        build_target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        signed: SIGNED_BUILD,
        extension_install: EXTENSION_INSTALL,
        vault_initialized: pipeline.vault_is_initialized(),
        available_tools: all_tool_names(),
    }
}

/// Hardcoded list of tools exposed by the MCP server.
pub fn all_tool_names() -> Vec<String> {
    vec![
        // Core retrieval
        "search",
        "rehydrate",
        "detect",
        "vault_stats",
        // Memory (GDPR Art.17)
        "memory_save",
        "memory_recall",
        "memory_forget",
        "memory_list",
        "memory_graph_recall",
        "memory_invalidate",
        // Engine management
        "anno_health",
        "anno_init_vault",
        "download_models",
        // Legal D1 — ingest + search
        "legal_ingest",
        "legal_search",
        "legal_graph_query",
        "legal_rehydrate_citation",
        // Legal D2 — extraction
        "legal_extract_contract",
        "legal_extract_case_file",
        "legal_timeline",
        "legal_risk_review",
        // Legal D3–D5 — audit + validation
        "legal_mandatory_clause_audit",
        "legal_prescription_check",
        "legal_validate_field",
        // Knowledge (Phase 1 — SQLite FTS, no ML models)
        "knowledge_sources",
        "knowledge_status",
        "knowledge_search",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tool_names_includes_knowledge_tools() {
        let tools = all_tool_names();

        assert!(tools.contains(&"knowledge_sources".to_string()));
        assert!(tools.contains(&"knowledge_status".to_string()));
        assert!(tools.contains(&"knowledge_search".to_string()));
    }
}
