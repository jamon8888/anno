//! MCP-facing facade for local knowledge tools.

use anno_knowledge_core::{KnowledgeSearchMode, KnowledgeSearchRequest, KnowledgeStatus, SourceId};
use anno_knowledge_store::{KnowledgeControlStore, LocalFolderRegistration};
use anno_rag::config::AnnoRagConfig;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::indexer::{sync_local_scope, SyncOptions, SyncSummary};

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
    /// Optional corpus id for scoped search.
    #[serde(default)]
    pub corpus_id: Option<String>,
    /// Explicitly allow cross-corpus search.
    #[serde(default)]
    pub allow_cross_corpus: bool,
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

    /// List configured sources as JSON values. Does not load models.
    #[must_use]
    pub fn sources(&self) -> Vec<serde_json::Value> {
        self.store
            .list_sources()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "source_id": s.source_id.as_string(),
                    "kind": s.kind,
                    "label": s.display_label_pseudo,
                    "enabled": s.enabled,
                })
            })
            .collect()
    }

    /// Register a local folder as a knowledge source. Does not load models.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn add_local_folder(&self, path: &str) -> anno_knowledge_store::Result<String> {
        let label = pseudo_folder_label(path);
        let reg = self.store.register_local_folder(LocalFolderRegistration {
            stable_key: path.to_string(),
            source_label_pseudo: label.clone(),
            scope_label_pseudo: label,
            provider_key: path.to_string(),
        })?;
        Ok(reg.source_id.as_string())
    }

    /// Forget a source: remove all its scopes' objects, chunks, and FTS rows.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn forget_source(&self, source_id: &str) -> anno_knowledge_store::Result<u64> {
        let sources = self.store.list_sources()?;
        for source in &sources {
            if source.source_id.as_string() == source_id {
                return self.store.forget_source(&source.source_id);
            }
        }
        Ok(0)
    }

    /// Forget the local-folder source whose provider path matches `path`.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn forget_source_by_path(&self, path: &str) -> anno_knowledge_store::Result<u64> {
        if let Some(source) = self.store.source_by_provider_key(path)? {
            return self.store.forget_source(&source.source_id);
        }
        Ok(0)
    }

    /// Run a bounded sync over all enabled scopes of a source (or all sources).
    /// Loads the local NER model on demand for pseudonymization.
    ///
    /// # Errors
    /// Returns a string error on setup failure (per-file errors are counted in summary).
    pub async fn sync(
        &self,
        pipeline: &anno_rag::pipeline::Pipeline,
        cfg: &AnnoRagConfig,
        source_id: Option<&str>,
        options: SyncOptions,
    ) -> Result<SyncSummary, String> {
        let sources = self
            .store
            .list_sources()
            .map_err(|e| format!("list_sources: {e}"))?;
        let mut total = SyncSummary::default();
        for source in &sources {
            if let Some(want) = source_id {
                if source.source_id.as_string() != want {
                    continue;
                }
            }
            let scopes = self
                .store
                .enabled_scopes_for_source(&source.source_id)
                .map_err(|e| format!("scopes: {e}"))?;
            for scope in &scopes {
                let s =
                    sync_local_scope(&self.store, pipeline, cfg, source, scope, options).await?;
                total.seen += s.seen;
                total.skipped_unchanged += s.skipped_unchanged;
                total.extracted += s.extracted;
                total.pseudonymized += s.pseudonymized;
                total.fts_ready += s.fts_ready;
                total.forgotten += s.forgotten;
                total.failed += s.failed;
                total.truncated |= s.truncated;
            }
        }
        Ok(total)
    }

    /// Search the local knowledge FTS index.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn search(
        &self,
        params: KnowledgeSearchParams,
    ) -> anno_knowledge_store::Result<KnowledgeSearchResponse> {
        self.search_with_source_ids(params, None)
    }

    /// Search the local knowledge FTS index with an optional source filter.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn search_with_source_ids(
        &self,
        params: KnowledgeSearchParams,
        source_ids: Option<Vec<SourceId>>,
    ) -> anno_knowledge_store::Result<KnowledgeSearchResponse> {
        let mode = params.mode.as_deref().unwrap_or("fast");
        if mode != "fast" {
            return Ok(KnowledgeSearchResponse {
                mode: "fast".to_string(),
                hits: Vec::new(),
            });
        }

        let mut request = KnowledgeSearchRequest::new(params.query).with_top_k(params.top_k);
        if let Some(source_ids) = source_ids {
            if source_ids.is_empty() {
                return Ok(KnowledgeSearchResponse {
                    mode: "fast".to_string(),
                    hits: Vec::new(),
                });
            }
            request = request.with_source_ids(source_ids);
        }
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

/// Coarse pseudonymized label for a local folder (stable, no path component leak).
fn pseudo_folder_label(path: &str) -> String {
    let stable = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, path.as_bytes())
        .simple()
        .to_string();
    format!("local_folder_{}", &stable[..12])
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
                corpus_id: None,
                allow_cross_corpus: false,
            })
            .expect("search");

        assert_eq!(result.mode, "fast");
        assert!(result.hits.is_empty());
    }

    #[test]
    fn add_local_folder_then_sources_lists_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let folder = dir.path().join("corpus");
        std::fs::create_dir_all(&folder).expect("mkdir");

        let service = KnowledgeService::open(&cfg).expect("service");
        let source_id = service
            .add_local_folder(&folder.display().to_string())
            .expect("add");
        assert!(!source_id.is_empty());

        let sources = service.sources();
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn add_local_folder_label_does_not_leak_folder_name() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let folder = dir.path().join("Client Dupont Confidential");
        std::fs::create_dir_all(&folder).expect("mkdir");

        let service = KnowledgeService::open(&cfg).expect("service");
        service
            .add_local_folder(&folder.display().to_string())
            .expect("add");

        let sources = service.sources();
        let label = sources[0]["label"].as_str().expect("label");
        assert!(label.starts_with("local_folder_"));
        assert!(!label.contains("Dupont"));
        assert!(!label.contains("Client"));
        assert!(!label.contains("Confidential"));
    }

    #[test]
    fn forget_source_removes_source_from_sources() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let folder = dir.path().join("corpus");
        std::fs::create_dir_all(&folder).expect("mkdir");

        let service = KnowledgeService::open(&cfg).expect("service");
        let source_id = service
            .add_local_folder(&folder.display().to_string())
            .expect("add");
        assert_eq!(service.sources().len(), 1);

        let removed = service.forget_source(&source_id).expect("forget");
        assert_eq!(removed, 0);
        assert!(service.sources().is_empty());
    }

    #[test]
    fn forget_source_by_path_removes_matching_local_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let folder = dir.path().join("corpus");
        std::fs::create_dir_all(&folder).expect("mkdir");

        let service = KnowledgeService::open(&cfg).expect("service");
        service
            .add_local_folder(&folder.display().to_string())
            .expect("add");
        assert_eq!(service.sources().len(), 1);

        let removed = service
            .forget_source_by_path(&folder.display().to_string())
            .expect("forget by path");
        assert_eq!(removed, 0);
        assert!(service.sources().is_empty());
        assert_eq!(
            service
                .forget_source_by_path(&folder.display().to_string())
                .expect("second forget"),
            0
        );
    }
}
