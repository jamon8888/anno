//! Status summaries for the knowledge service.

use serde::{Deserialize, Serialize};

/// User-visible local knowledge status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeStatus {
    /// Number of configured source integrations.
    pub sources: u64,
    /// Number of registered source accounts.
    pub accounts: u64,
    /// Number of selected sync scopes.
    pub scopes: u64,
    /// Number of discovered or indexed objects.
    pub objects: u64,
    /// Number of indexed vector chunks.
    pub chunks: u64,
    /// Number of objects in a failed state.
    pub failures: u64,
    /// Whether the embedding and extraction models are loaded.
    pub models_loaded: bool,
}
