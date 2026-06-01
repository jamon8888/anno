//! MCP-facing facade for local knowledge tools.

use anno_knowledge_core::{KnowledgeSearchMode, KnowledgeSearchRequest, KnowledgeStatus};
use anno_knowledge_store::KnowledgeControlStore;
use anno_rag::config::AnnoRagConfig;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Parameters for `knowledge_search`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeSearchParams {
    /// User query. Phase 1 uses local FTS only and returns pseudonymized snippets.
    pub query: String,
    /// Maximum result count.
    #[serde(default = "default_knowledge_top_k")]
    pub top_k: usize,
    /// Search mode. Phase 1 accepts only `fast`.
    #[serde(default)]
    pub mode: Option<String>,
}

fn default_knowledge_top_k() -> usize {
    10
}

/// Result for `knowledge_search`.
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeSearchResponse {
    /// Effective search mode.
    pub mode: String,
    /// Pseudonymized hits.
    pub hits: Vec<anno_knowledge_core::KnowledgeSearchHit>,
}

/// Local knowledge service. Opens SQLite only; does not load `Pipeline`.
pub struct KnowledgeService {
    store: KnowledgeControlStore,
}

impl KnowledgeService {
    /// Open the local knowledge service from Anno config.
    ///
    /// # Errors
    /// Returns store errors if the SQLite database cannot be opened.
    pub fn open(cfg: &AnnoRagConfig) -> anno_knowledge_store::Result<Self> {
        let path = knowledge_db_path(cfg);
        Ok(Self {
            store: KnowledgeControlStore::open(path)?,
        })
    }

    /// Return local status without loading ML models.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn status(&self) -> anno_knowledge_store::Result<KnowledgeStatus> {
        self.store.status()
    }

    /// Return configured sources. Phase 1 returns an empty list until source CRUD lands.
    #[must_use]
    pub fn sources(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Search the local knowledge FTS index.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn search(
        &self,
        params: KnowledgeSearchParams,
    ) -> anno_knowledge_store::Result<KnowledgeSearchResponse> {
        let mode = params.mode.as_deref().unwrap_or("fast");
        if mode != "fast" {
            return Ok(KnowledgeSearchResponse {
                mode: "fast".to_string(),
                hits: Vec::new(),
            });
        }

        let request = KnowledgeSearchRequest::new(params.query).with_top_k(params.top_k);
        let hits = match request.mode {
            KnowledgeSearchMode::Fast => self.store.search_fast(&request)?,
            KnowledgeSearchMode::Semantic | KnowledgeSearchMode::Deep => Vec::new(),
        };
        Ok(KnowledgeSearchResponse {
            mode: "fast".to_string(),
            hits,
        })
    }
}

fn knowledge_db_path(cfg: &AnnoRagConfig) -> PathBuf {
    cfg.data_dir.join("knowledge.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    #[test]
    fn service_status_opens_empty_store_without_models() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let status = service.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_empty_store_returns_empty_hits() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let result = service
            .search(KnowledgeSearchParams {
                query: "contrat".to_string(),
                top_k: 5,
                mode: None,
            })
            .expect("search");

        assert_eq!(result.mode, "fast");
        assert!(result.hits.is_empty());
    }
}
