//! Engine health collector for the `anno_health` MCP tool (spec §13.1).

use serde::{Deserialize, Serialize};

/// Set at compile time only in the CI signing job.
const SIGNED_BUILD: bool = option_env!("ANNO_RAG_SIGNED_BUILD").is_some();

/// Set at compile time only by the `.mcpb` packaging step.
const EXTENSION_INSTALL: bool = option_env!("ANNO_RAG_EXTENSION_INSTALL").is_some();

/// Wire shape returned by `anno_health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealth {
    pub engine_version: String,
    pub build_target: String,
    pub signed: bool,
    pub extension_install: bool,
    pub vault_initialized: bool,
    pub available_tools: Vec<String>,
}

/// Result of `anno_init_vault`. The passphrase itself is never echoed.
#[derive(Debug, Clone, Serialize)]
pub struct InitVaultResult {
    pub ok: bool,
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
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
