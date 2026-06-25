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
mod detect_label;
mod envelope;
pub mod health;
mod indexer;
pub mod knowledge;
mod legal;
mod legal_maintenance;
pub mod model_inventory;
mod params;
mod review;
mod search;
pub mod tabular;
mod wire;

use crate::allowed_roots::AllowedRoots;
use crate::indexer::SyncSummary;
use anno_rag::config::{AnnoRagConfig, MemoryNerMode};
use anno_rag::pipeline::Pipeline;
use legal::*;
use params::*;
use review::*;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use search::*;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{OnceCell, RwLock};
use wire::*;

/// Time budget for a single `knowledge_sync` / `index` MCP call.
///
/// The MCP transport enforces a ~60 s request timeout.  On CPU-only machines
/// each file costs ~6 s (GLiNER ONNX + e5-small embedder), so processing
/// more than ~10 files in a single call would reliably timeout.
///
/// `max_millis` in `SyncOptions` makes the sync loop check elapsed time after
/// each file and return a truncated summary when the budget is exhausted.
/// The caller sees `{"summary": {"truncated": true, ...}}` and should call
/// again — already-indexed files are skipped in O(1) via the content-hash
/// cache, so each subsequent call makes forward progress.
///
/// 45 s leaves 15 s of headroom for warmup checks, path validation, DB
/// writes, and response serialization within the 60 s MCP window.
const MCP_SYNC_BUDGET_MILLIS: u64 = 45_000;

/// Warmup lifecycle for background model loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WarmupPhase {
    /// Background warmup has not started yet.
    Idle,
    /// Auto-downloading models. `progress_pct` is 0–100 (best-effort).
    Downloading { started_ms: u64, progress_pct: u8 },
    /// Warmup in progress. `started_ms` = Unix epoch milliseconds at spawn time.
    Loading { started_ms: u64 },
    /// Both models loaded successfully.
    Ready { elapsed_ms: u64 },
    /// One or both models failed to load.
    Failed { error: String },
}

/// State held by the MCP server: either a pre-built Pipeline (eager) or a
/// lazily-initialised one (deferred until the first tool call).
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    knowledge: Arc<OnceCell<Arc<crate::knowledge::KnowledgeService>>>,
    corpus: Arc<OnceCell<Arc<crate::corpus::CorpusService>>>,
    legal_maintenance: Arc<OnceCell<Arc<crate::legal_maintenance::LegalMaintenanceService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: Arc<RwLock<[u8; 32]>>,
    allowed_roots: AllowedRoots,
    tabular_storage: Arc<OnceCell<Arc<anno_rag_tabular::storage::StorageHandle>>>,
    extraction_status: Arc<RwLock<HashMap<anno_rag_tabular::ReviewId, ReviewExtractionStatus>>>,
    /// corpus_key → running job_id; guards against duplicate concurrent ingest.
    active_ingest_jobs: Arc<RwLock<HashMap<String, String>>>,
    warmup_phase: Arc<RwLock<WarmupPhase>>,
    #[allow(dead_code)] // populated + consumed by the rmcp #[tool_router] macro
    tool_router: ToolRouter<Self>,
}

// ---- Pipeline helpers ----

impl AnnoRagServer {
    async fn pipeline(&self) -> anno_rag::error::Result<&Pipeline> {
        self.pipeline
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                let key_arc = Arc::clone(&self.key);
                async move {
                    let key = *key_arc.read().await;
                    let inventory =
                        crate::model_inventory::ModelInventoryService::new(&cfg).inspect();
                    if !inventory.ready {
                        let mut missing: Vec<&str> = Vec::new();
                        if !inventory.embedder.ready {
                            missing.extend(inventory.embedder.missing_files.iter().map(String::as_str));
                        }
                        if !inventory.gliner.ready {
                            missing
                                .extend(inventory.gliner.missing_files.iter().map(String::as_str));
                        }
                        let missing_summary = if missing.is_empty() {
                            String::new()
                        } else {
                            format!(" Missing files: {}.", missing.join(", "))
                        };
                        let env_hint = if inventory.from_env {
                            format!(" (ANNO_MODELS_DIR={})", inventory.path)
                        } else {
                            String::new()
                        };
                        return Err(anno_rag::error::Error::Config(format!(
                            "Models not ready at {path}{env_hint} (state={state}).{missing_summary} \
                             Run `anno-rag download-models` in a terminal, then restart the MCP server.",
                            path = inventory.path,
                            state = inventory.state.as_str(),
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

    /// Return the initialized `Pipeline`, or a JSON error string ready to return to the caller.
    ///
    /// Returns `Err(json_string)` when warmup is in progress (phase `Loading`),
    /// permanently failed, or models are still missing. Returns `Ok(&Pipeline)` when ready.
    ///
    /// Tool handlers replace `self.pipeline().await.map_err(|e| ...)` with this.
    async fn require_models(&self) -> Result<&Pipeline, String> {
        let phase = self.warmup_phase.read().await.clone();
        match phase {
            WarmupPhase::Downloading {
                started_ms,
                progress_pct,
            } => {
                let elapsed_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(started_ms / 1000);
                return Err(serde_json::json!({
                    "ok": false,
                    "error": "models_downloading",
                    "download_progress_pct": progress_pct,
                    "message": format!(
                        "anno-rag is downloading ML models ({progress_pct}% complete, \
                         {elapsed_s}s elapsed). Please retry in a moment."
                    ),
                    "hint": "Run `anno-rag status` to check download progress."
                })
                .to_string());
            }
            WarmupPhase::Loading { started_ms } => {
                let elapsed_s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(started_ms / 1000);
                return Err(serde_json::json!({
                    "ok": false,
                    "error": "models_loading",
                    "message": format!(
                        "anno-rag is loading ML models in the background \
                         ({elapsed_s}s elapsed). This typically takes 60–120 s on \
                         first startup. Please retry in a moment."
                    ),
                    "hint": "Run `anno-rag status` to check progress."
                })
                .to_string());
            }
            WarmupPhase::Failed { ref error } => {
                return Err(serde_json::json!({
                    "ok": false,
                    "error": "models_failed",
                    "message": format!("Model loading failed: {error}"),
                    "hint": "Run `anno-rag download-models` in a terminal, then restart the MCP server."
                })
                .to_string());
            }
            WarmupPhase::Idle | WarmupPhase::Ready { .. } => {}
        }
        // Phase is Ready or Idle — fall through to normal pipeline check.
        self.pipeline().await.map_err(|e| {
            serde_json::json!({
                "ok": false,
                "error": "pipeline_error",
                "message": e.to_string(),
                "hint": "Run `anno-rag download-models` in a terminal, then restart the MCP server."
            })
            .to_string()
        })
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

    fn validate_existing_mcp_path(
        &self,
        label: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<std::path::PathBuf, String> {
        self.allowed_roots.validate_existing_path(label, path)
    }

    fn validate_existing_or_removed_mcp_path(
        &self,
        label: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<std::path::PathBuf, String> {
        self.allowed_roots
            .validate_existing_or_removed_path(label, path)
    }

    fn validate_output_mcp_path(
        &self,
        label: &str,
        path: impl AsRef<std::path::Path>,
    ) -> Result<std::path::PathBuf, String> {
        self.allowed_roots.validate_output_path(label, path)
    }

    #[cfg(test)]
    fn with_allowed_roots_for_test(mut self, allowed_roots: AllowedRoots) -> Self {
        self.allowed_roots = allowed_roots;
        self
    }
}

// ---- Constructors ----

fn allowed_roots_from_env() -> AllowedRoots {
    AllowedRoots::from_env().unwrap_or_else(|error| {
        tracing::warn!(%error, "invalid ANNO_RAG_ALLOWED_ROOTS; denying MCP path access");
        AllowedRoots::deny_all()
    })
}

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
            key: Arc::new(RwLock::new([0u8; 32])),
            allowed_roots: allowed_roots_from_env(),
            tabular_storage: Arc::new(OnceCell::new()),
            extraction_status: Arc::new(RwLock::new(HashMap::new())),
            active_ingest_jobs: Arc::new(RwLock::new(HashMap::new())),
            warmup_phase: Arc::new(RwLock::new(WarmupPhase::Idle)),
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
            key: Arc::new(RwLock::new(key)),
            allowed_roots: allowed_roots_from_env(),
            tabular_storage: Arc::new(OnceCell::new()),
            extraction_status: Arc::new(RwLock::new(HashMap::new())),
            active_ingest_jobs: Arc::new(RwLock::new(HashMap::new())),
            warmup_phase: Arc::new(RwLock::new(WarmupPhase::Idle)),
            tool_router: Self::tool_router(),
        }
    }
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
        let p = self.require_models().await?;
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
        let p = self.require_models().await?;
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
        let source_root = self.validate_existing_mcp_path("source_root", &p.source_root)?;
        let pipeline = self.require_models().await?;
        let summary = pipeline
            .privacy_prepare_folder(&source_root, p.recursive)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::to_value(summary).map_err(|e| e.to_string())
    }

    async fn privacy_finalize_folder_impl(
        &self,
        p: PrivacyFinalizeFolderParams,
    ) -> Result<serde_json::Value, String> {
        let workspace = self.validate_existing_mcp_path("workspace", &p.workspace)?;
        let pipeline = self.require_models().await?;
        let summary = pipeline
            .privacy_finalize_folder(&workspace)
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
            "returns_document_content": false,
            "allowed_roots": self.allowed_roots.summary()
        })
    }

    async fn legal_ingest_impl(
        &self,
        p: LegalIngestParams,
        corpus_id: Option<anno_corpus_core::CorpusId>,
    ) -> Result<serde_json::Value, String> {
        let folder = self.validate_existing_mcp_path("folder", &p.folder)?;
        let folder_string = folder.display().to_string();

        // Ensure models are loaded before spawning — avoids a warmup race inside the task.
        self.require_models().await?;
        let pipeline_arc = self
            .pipeline_arc()
            .expect("require_models succeeded → pipeline is set");

        // Use a hashed key so raw filesystem paths are never surfaced in status output.
        let corpus_key = corpus_id
            .map(|id| id.as_string())
            .unwrap_or_else(|| legal_folder_id(&folder_string));

        // Atomic same-process dedup: check + insert under write lock so two concurrent
        // MCP calls cannot both pass the guard and enqueue duplicate jobs.
        let job_id = uuid::Uuid::new_v4().to_string();
        {
            let mut guard = self.active_ingest_jobs.write().await;
            if let Some(existing_job_id) = guard.get(&corpus_key) {
                return serde_json::to_value(serde_json::json!({
                    "ok": false,
                    "job_id": existing_job_id,
                    "status": "already_running",
                    "folder": p.folder,
                    "message": "an ingest job is already running for this corpus"
                }))
                .map_err(|e| e.to_string());
            }
            guard.insert(corpus_key.clone(), job_id.clone());
        }

        // Register the job in the knowledge store (with cross-process dedup and rollback).
        let knowledge = self.knowledge().await.map_err(|e| e.to_string())?;
        // Cross-process dedup: check the DB for a running job that may have been
        // started by another server instance (the in-memory map only covers this process).
        if let Some(existing_job_id) = knowledge
            .running_job_for_corpus(&corpus_key)
            .map_err(|e| e.to_string())?
        {
            self.active_ingest_jobs.write().await.remove(&corpus_key);
            return serde_json::to_value(serde_json::json!({
                "ok": false,
                "job_id": existing_job_id,
                "status": "already_running",
                "folder": p.folder,
                "message": "an ingest job is already running for this corpus"
            }))
            .map_err(|e| e.to_string());
        }
        let out = corpus_id
            .map(|id| corpus_legal_output_dir(self.cfg.as_ref(), id))
            .unwrap_or_else(|| folder.join("anon"));

        // Count eligible files so job_status shows a meaningful denominator immediately.
        let files_total =
            anno_rag::pipeline::count_ingest_candidates(&folder, p.recursive, &out) as i64;

        if let Err(e) = knowledge.insert_job(
            &job_id,
            "legal_ingest",
            corpus_id.map(|id| id.as_string()).as_deref(),
            files_total,
        ) {
            self.active_ingest_jobs.write().await.remove(&corpus_key);
            return Err(e.to_string());
        }

        // Clone everything the background task needs.
        let cfg_arc = Arc::clone(&self.cfg);
        let active_jobs = Arc::clone(&self.active_ingest_jobs);
        let job_id_task = job_id.clone();
        let folder_task = folder.clone();
        let folder_str_task = p.folder.clone();
        let out_task = out.clone();
        let recursive = p.recursive;

        tokio::task::spawn(async move {
            let start = std::time::Instant::now();

            let ingest_result = if let Some(corpus_id) = corpus_id {
                pipeline_arc
                    .ingest_folder_scoped_summary(
                        &folder_task,
                        recursive,
                        &out_task,
                        anno_rag::pipeline::LegalIngestScope {
                            corpus_id,
                            root: folder_task.clone(),
                        },
                    )
                    .await
            } else {
                pipeline_arc
                    .ingest_folder(&folder_task, recursive, &out_task)
                    .await
                    .map(|ingested| anno_rag::pipeline::LegalIngestSummary {
                        ingested,
                        documents: Vec::new(),
                    })
            };

            // Re-open knowledge store for progress/status updates.
            let knowledge_task = crate::knowledge::KnowledgeService::open(&cfg_arc).ok();

            match ingest_result {
                Ok(summary) => {
                    // Register bindings/documents in corpus store.
                    if let Some(corpus_id) = corpus_id {
                        if let Ok(corpus_service) = crate::corpus::CorpusService::open(&cfg_arc) {
                            let binding_id = legal_folder_id(&folder_str_task);
                            let display_label = std::path::Path::new(&folder_str_task)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(&folder_str_task)
                                .to_string();
                            let _ = corpus_service.store().add_binding(
                                corpus_id,
                                anno_corpus_core::CorpusBindingKind::LegalFolder,
                                &binding_id,
                                &serde_json::json!({
                                    "label": display_label,
                                    "source_path": folder_str_task.clone()
                                }),
                            );
                            for document in &summary.documents {
                                let _ = corpus_service.store().add_document(
                                    corpus_id,
                                    document.document_id,
                                    "legal",
                                    &document.source_path,
                                    document.relative_path.as_deref(),
                                    &document.content_id,
                                    &serde_json::json!({"folder_id": binding_id.clone()}),
                                );
                            }
                        }
                    }
                    tracing::info!(
                        target: "anno_rag::legal::audit",
                        tool = "legal_ingest",
                        result = "ok",
                        duration_ms = start.elapsed().as_millis() as u64,
                        ingested = summary.ingested,
                        job_id = %job_id_task,
                        ""
                    );
                    if let Some(ref ks) = knowledge_task {
                        let _ = ks.update_job_progress(&job_id_task, summary.ingested as i64);
                        let _ = ks.set_job_status(
                            &job_id_task,
                            anno_knowledge_store::JobStatus::Done,
                            None,
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "anno_rag::legal::audit",
                        tool = "legal_ingest",
                        result = "error",
                        job_id = %job_id_task,
                        "{e}"
                    );
                    if let Some(ref ks) = knowledge_task {
                        let _ = ks.set_job_status(
                            &job_id_task,
                            anno_knowledge_store::JobStatus::Failed,
                            Some(&e.to_string()),
                        );
                    }
                }
            }

            active_jobs.write().await.remove(&corpus_key);
        });

        serde_json::to_value(serde_json::json!({
            "ok": true,
            "job_id": job_id,
            "status": "running",
            "folder": p.folder,
            "output_root": out.display().to_string(),
        }))
        .map_err(|e| e.to_string())
    }

    async fn legal_search_impl(&self, p: LegalSearchParams) -> Result<serde_json::Value, String> {
        let svc = self.corpus().await.map_err(|e| e.to_string())?;
        let effective = match svc.resolve_effective(p.corpus_id.as_deref(), p.allow_cross_corpus) {
            Ok(eff) => eff,
            // Multiple corpora and no explicit choice: return an actionable,
            // structured disambiguation instead of an opaque error.
            Err(anno_corpus_core::CorpusGuardError::CorpusRequired) => {
                let rows = svc.store().list_corpora().map_err(|e| e.to_string())?;
                return Ok(crate::envelope::envelope(
                    crate::envelope::status::CORPUS_REQUIRED,
                    "Plusieurs dossiers indexés. Précisez un dossier ou demandez une recherche transversale.",
                    "Relancez avec corpus_id/alias, ou allow_cross_corpus: true pour un contrôle de conflits.",
                    serde_json::json!({
                        "available": rows
                            .iter()
                            .map(|c| serde_json::json!({
                                "corpus_id": c.corpus_id.as_string(),
                                "alias": c.alias,
                                "label": c.label_pseudo,
                                "health": c.health,
                            }))
                            .collect::<Vec<_>>(),
                    }),
                ));
            }
            Err(e) => return Err(e.to_string()),
        };
        self.legal_search_impl_with_effective(p, &effective).await
    }

    async fn legal_search_impl_with_effective(
        &self,
        p: LegalSearchParams,
        effective: &anno_corpus_core::EffectiveCorpus,
    ) -> Result<serde_json::Value, String> {
        let pipeline = self.require_models().await?;
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
            rerank,
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
        let pool = self.cfg.rerank_pool_size;
        let start = std::time::Instant::now();
        let result = match self.legal_document_ids_for_effective(effective).await? {
            Some(doc_ids) => {
                if rerank {
                    #[cfg(feature = "rerank")]
                    {
                        pipeline
                            .legal_search_scoped_reranked(&query, top_k, filters, &doc_ids, pool)
                            .await
                    }
                    #[cfg(not(feature = "rerank"))]
                    {
                        tracing::warn!("rerank requested but server built without `rerank` feature; falling back to unranked scoped search");
                        pipeline
                            .legal_search_scoped(&query, top_k, filters, &doc_ids)
                            .await
                    }
                } else {
                    pipeline
                        .legal_search_scoped(&query, top_k, filters, &doc_ids)
                        .await
                }
            }
            None => {
                if rerank {
                    #[cfg(feature = "rerank")]
                    {
                        pipeline
                            .legal_search_reranked(&query, top_k, filters, pool)
                            .await
                    }
                    #[cfg(not(feature = "rerank"))]
                    {
                        tracing::warn!("rerank requested but server built without `rerank` feature; falling back to unranked search");
                        pipeline.legal_search(&query, top_k, filters).await
                    }
                } else {
                    pipeline.legal_search(&query, top_k, filters).await
                }
            }
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
        let is_ready = matches!(*self.warmup_phase.read().await, WarmupPhase::Ready { .. });
        let knowledge = {
            let mut val = self
                .knowledge_status_impl()
                .await
                .ok()
                .and_then(|s| serde_json::to_value(s).ok())
                .unwrap_or(serde_json::Value::Null);
            if let serde_json::Value::Object(ref mut map) = val {
                map.insert(
                    "models_loaded".to_string(),
                    serde_json::Value::Bool(is_ready),
                );
            }
            val
        };

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
            None => {
                // Pipeline not yet loaded — check all key sources so vault
                // status is accurate before the first tool call and in Docker
                // where the OS keyring is unavailable.
                let initialized = anno_rag::vault::is_vault_key_usable();
                serde_json::json!({
                    "available": initialized,
                    "reason": if initialized { "pipeline_not_yet_loaded" } else { "vault_not_initialized" },
                    "total_mappings": null,
                    "categories": {},
                })
            }
        };

        let inventory =
            crate::model_inventory::ModelInventoryService::new(self.cfg.as_ref()).inspect();
        let loaded = self.pipeline_arc();
        let warmup_info = {
            let phase = self.warmup_phase.read().await;
            match &*phase {
                WarmupPhase::Idle => serde_json::json!({ "phase": "idle" }),
                WarmupPhase::Downloading {
                    started_ms,
                    progress_pct,
                } => {
                    let elapsed_s = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_sub(started_ms / 1000);
                    serde_json::json!({ "phase": "downloading", "elapsed_s": elapsed_s, "download_progress_pct": progress_pct })
                }
                WarmupPhase::Loading { started_ms } => {
                    let elapsed_s = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_sub(started_ms / 1000);
                    serde_json::json!({ "phase": "loading", "elapsed_s": elapsed_s })
                }
                WarmupPhase::Ready { elapsed_ms } => {
                    serde_json::json!({ "phase": "ready", "elapsed_ms": elapsed_ms })
                }
                WarmupPhase::Failed { error } => {
                    serde_json::json!({ "phase": "failed", "error": error })
                }
            }
        };
        let models = serde_json::json!({
            "inventory": inventory,
            "embedder_loaded": loaded.as_ref().is_some_and(|p| p.embedder_loaded()),
            "detector_loaded": loaded.as_ref().is_some_and(|p| p.detector_loaded()),
            "warmup_phase": warmup_info["phase"],
            "warmup": warmup_info,
        });

        let ingest_jobs = {
            let guard = self.active_ingest_jobs.read().await;
            guard
                .iter()
                .map(|(corpus_key, job_id)| {
                    serde_json::json!({ "corpus_key": corpus_key, "job_id": job_id })
                })
                .collect::<Vec<_>>()
        };

        serde_json::json!({
            "ok": true,
            "knowledge": knowledge,
            "legal": legal,
            "vault": vault,
            "models": models,
            "ingest_jobs": ingest_jobs,
        })
        .to_string()
    }

    async fn knowledge_status_impl(&self) -> Result<anno_knowledge_core::KnowledgeStatus, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.status().map_err(|e| e.to_string())
    }

    /// Mark any jobs left in `running` state from a previous crashed/killed
    /// process as `interrupted`. Called once at server startup.
    async fn sweep_interrupted_jobs(&self) {
        match self.knowledge().await {
            Ok(ks) => match ks.mark_running_jobs_interrupted() {
                Ok(n) if n > 0 => tracing::warn!(
                    swept = n,
                    "marked {n} stale running job(s) as interrupted at startup"
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!("startup job sweep failed: {e}"),
            },
            Err(e) => tracing::warn!("startup job sweep: could not open knowledge store: {e}"),
        }
    }

    async fn job_status_impl(&self, job_id: &str) -> Result<serde_json::Value, String> {
        let knowledge = self.knowledge().await.map_err(|e| e.to_string())?;
        match knowledge.get_job(job_id).map_err(|e| e.to_string())? {
            Some(row) => serde_json::to_value(serde_json::json!({
                "job_id":     row.job_id,
                "job_type":   row.job_type,
                "corpus_id":  row.corpus_id,
                "status":     row.status,
                "files_done": row.files_done,
                "files_total": row.files_total,
                "last_error": row.last_error,
            }))
            .map_err(|e| e.to_string()),
            None => Err(format!("no job found with id={job_id}")),
        }
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

        let svc = match self.corpus().await {
            Ok(svc) => svc,
            Err(e) => {
                return serde_json::json!({"ok": false, "error": e.to_string()}).to_string();
            }
        };
        let effective = match svc.resolve_effective(p.corpus_id.as_deref(), p.allow_cross_corpus) {
            Ok(effective) => effective,
            Err(anno_corpus_core::CorpusGuardError::CorpusRequired) => {
                let rows = match svc.store().list_corpora() {
                    Ok(r) => r,
                    Err(e) => {
                        return serde_json::json!({"ok": false, "error": e.to_string()})
                            .to_string();
                    }
                };
                return crate::envelope::envelope(
                    crate::envelope::status::CORPUS_REQUIRED,
                    "Plusieurs dossiers indexés. Précisez un dossier ou demandez une recherche transversale.",
                    "Relancez avec corpus_id/alias, ou allow_cross_corpus: true pour un contrôle de conflits.",
                    serde_json::json!({
                        "available": rows
                            .iter()
                            .map(|c| serde_json::json!({
                                "corpus_id": c.corpus_id.as_string(),
                                "alias": c.alias,
                                "label": c.label_pseudo,
                                "health": c.health,
                            }))
                            .collect::<Vec<_>>(),
                    }),
                )
                .to_string();
            }
            Err(e) => {
                return serde_json::json!({"ok": false, "error": e.to_string()}).to_string();
            }
        };

        // Read freshness BEFORE the opportunistic sync so search.freshness and
        // corpus_health.freshness always report the same pre-sync index state (issue #75).
        let (index_fresh, freshness) = match self.freshness_for_effective(&effective).await {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("freshness failed: {error}"));
                (false, "unknown".to_string())
            }
        };

        let sync_status = self
            .maybe_sync_knowledge_before_search(&effective, &scope, &mut warnings)
            .await;

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
        let path = self
            .validate_existing_mcp_path("path", path)?
            .display()
            .to_string();
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        service.add_local_folder(&path).map_err(|e| e.to_string())
    }

    async fn knowledge_sync_impl(&self, p: KnowledgeSyncParams) -> Result<SyncSummary, String> {
        let service = self.knowledge().await.map_err(|e| e.to_string())?;
        self.validate_knowledge_sync_paths(service, p.source_id.as_deref())?;
        let pipeline = self.require_models().await?;
        service
            .sync(
                pipeline,
                self.cfg.as_ref(),
                p.source_id.as_deref(),
                crate::indexer::SyncOptions {
                    max_millis: Some(MCP_SYNC_BUDGET_MILLIS),
                    ..crate::indexer::SyncOptions::default()
                },
            )
            .await
    }

    fn validate_knowledge_sync_paths(
        &self,
        service: &crate::knowledge::KnowledgeService,
        source_id: Option<&str>,
    ) -> Result<(), String> {
        service.validate_sync_source_paths(source_id, |path| {
            self.validate_existing_mcp_path("source_path", path)
                .map(|_| ())
        })
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
            let options = crate::indexer::SyncOptions {
                max_files: p
                    .max_files
                    .unwrap_or_else(|| crate::indexer::SyncOptions::default().max_files),
                max_millis: p.max_millis,
            };
            for source_id in &selected_sources {
                if let Err(error) =
                    self.validate_knowledge_sync_paths(service, Some(source_id.as_str()))
                {
                    warnings.push(format!("knowledge source {source_id}: {error}"));
                    continue;
                }
                let pipeline = self.require_models().await?;
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

        let path = match self.validate_existing_mcp_path("path", &p.path) {
            Ok(path) => path.display().to_string(),
            Err(error) => {
                return serde_json::json!({
                    "ok": false,
                    "error": error,
                })
                .to_string();
            }
        };
        let p = IndexParams {
            path,
            profile: p.profile,
        };

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
                            // Only hard failures go into errors (makes ok=false).
                            // truncated=true means the 45 s MCP time budget was
                            // reached — that is expected behaviour, not a failure.
                            // Callers should retry: already-indexed files are
                            // skipped in O(1) via content-hash, so each call
                            // makes forward progress.
                            if summary.failed > 0 {
                                errors.push(format!(
                                    "knowledge sync: {} file(s) failed",
                                    summary.failed
                                ));
                            }
                            knowledge = serde_json::json!({
                                "source_id": source_id,
                                "summary": summary,
                                "hint": if summary.truncated {
                                    Some("Time budget reached — call index() again to continue. Already-indexed files are skipped automatically.")
                                } else {
                                    None
                                },
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
            let target = match self.validate_existing_or_removed_mcp_path("target", &p.target) {
                Ok(path) => path.display().to_string(),
                Err(error) => {
                    return serde_json::json!({
                        "ok": false,
                        "removed": {
                            "knowledge_objects": 0u64,
                            "legal_chunks": 0u64,
                            "tabular_reviews": 0u64
                        },
                        "errors": [error],
                    })
                    .to_string();
                }
            };

            match self.knowledge_forget_by_path(&target).await {
                Ok(removed) => knowledge_removed = removed,
                Err(e) => errors.push(format!("knowledge forget: {e}")),
            }

            match self.legal_maintenance().await {
                Ok(service) => match service.forget_folder_path(&target).await {
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

        if let Err(json) = self.require_models().await {
            let error = json;
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
                let rows = match service.store().list_corpora() {
                    Ok(rows) => rows,
                    Err(e) => return format!("Error: {e}"),
                };
                let corpora: Vec<_> = rows
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "corpus_id":   r.corpus_id.as_string(),
                            "label":       r.label_pseudo,
                            "health":      r.health,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "ok":    true,
                    "count": corpora.len(),
                    "corpora": corpora,
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
        let p = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let p = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
        };
        match p.detect(&params.text) {
            Ok(entities) => {
                let info: Vec<EntityInfo> = entities
                    .into_iter()
                    .map(|e| EntityInfo {
                        original: e.original,
                        category: match &e.category {
                            anno_rag::EntityCategory::Custom(s) => s.clone(),
                            other => format!("{other:?}"),
                        },
                        confidence: e.confidence,
                        source: crate::detect_label::source_label(&e.source).to_string(),
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
        if result.ok {
            // Re-derive so self.key reflects the passphrase-based key from now on.
            // Any subsequent pipeline() call (lazy init) will use the correct key.
            match anno_rag::vault::derive_key() {
                Ok(new_key) => *self.key.write().await = new_key,
                Err(e) => tracing::warn!("anno_init_vault: could not re-derive key: {e}"),
            }
        }
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
            // Pipeline not yet loaded — check all key sources (env passphrase
            // takes priority over keyring so Docker/CI always reports correctly).
            anno_rag::vault::is_vault_key_usable()
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
                );
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
        };
        let doc_id = match self.corpus().await {
            Ok(svc) => match svc.resolve_doc_ref(&p.doc_id) {
                Ok(id) => id,
                Err(_) if uuid::Uuid::parse_str(&p.doc_id).is_ok() => p.doc_id.clone(),
                Err(e) => return format!("Error: {e}"),
            },
            Err(_) => p.doc_id.clone(),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_extract_contract(&doc_id).await {
            Ok(review) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_extract_contract",
                    doc_id = doc_id,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
        };
        let dossier_id = match self.corpus().await {
            Ok(svc) => match svc.resolve_doc_ref(&p.dossier_id) {
                Ok(id) => id,
                Err(_) if uuid::Uuid::parse_str(&p.dossier_id).is_ok() => p.dossier_id.clone(),
                Err(e) => return format!("Error: {e}"),
            },
            Err(_) => p.dossier_id.clone(),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_timeline(&dossier_id).await {
            Ok(tl) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_timeline",
                    dossier_id = dossier_id,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
        };
        let scope_id = match self.corpus().await {
            Ok(svc) => match svc.resolve_doc_ref(&p.scope_id) {
                Ok(id) => id,
                Err(_) if uuid::Uuid::parse_str(&p.scope_id).is_ok() => p.scope_id.clone(),
                Err(e) => return format!("Error: {e}"),
            },
            Err(_) => p.scope_id.clone(),
        };
        let start = std::time::Instant::now();
        match pipeline.legal_risk_review(&scope_id, p.is_dossier).await {
            Ok(review) => {
                tracing::info!(
                    target: "anno_rag::legal::audit",
                    tool = "legal_risk_review",
                    scope_id = scope_id,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
        let pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
                return format!("Error: unknown action '{other}'. Valid: confirm, reject, correct");
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
        let _pipeline = match self.require_models().await {
            Ok(p) => p,
            Err(json) => return json,
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
                );
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
                let path = match self.validate_output_mcp_path("output_path", &path_str) {
                    Ok(path) => path,
                    Err(error) => return format!("Error: {error}"),
                };
                // Preserve the existing absolute-path requirement when
                // ANNO_RAG_ALLOWED_ROOTS is not configured.
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

    /// Check the status of an async ingest job.
    #[tool(
        description = "Return the current status of a background ingest job. Use the job_id returned by legal_ingest. Returns status (running|done|failed|interrupted), files_done, files_total, and last_error if any."
    )]
    async fn job_status(
        &self,
        Parameters(p): Parameters<crate::knowledge::JobStatusParams>,
    ) -> String {
        match self.job_status_impl(&p.job_id).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("Error: {e}")),
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
        if let Err(json) = self.require_models().await {
            return json;
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
    server.sweep_interrupted_jobs().await;
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
    server.sweep_interrupted_jobs().await;
    tracing::info!("anno-rag MCP server starting (lazy) on stdio");

    let transport = rmcp::transport::stdio();
    let warmup_server = server.clone();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server failed to start: {e}")))?;

    // Warm up models in background so first tool call doesn't block for ~78s.
    // FIXED — builds pipeline then loads both ML models in parallel.
    //
    // Set Loading phase BEFORE spawn so that any tool call arriving between
    // tokio::spawn and the task's first await sees Loading (not Idle) and
    // returns the "retry in a moment" JSON immediately rather than falling
    // through to a blocking detector_get_or_init (100 s on ONNX cold start).
    let started_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    *warmup_server.warmup_phase.write().await = WarmupPhase::Downloading {
        started_ms,
        progress_pct: 0,
    };
    tokio::spawn(async move {
        tracing::info!("anno-rag: background model warmup starting");

        // Step 0: auto-download models if any are absent.
        {
            let inv =
                crate::model_inventory::ModelInventoryService::new(warmup_server.cfg.as_ref())
                    .inspect();
            if !inv.ready {
                tracing::info!("anno-rag: models absent — auto-downloading (~545 MB)");
                if let Err(e) =
                    anno_rag::download_models::download(warmup_server.cfg.as_ref()).await
                {
                    tracing::error!("anno-rag: model download failed: {e}");
                    *warmup_server.warmup_phase.write().await = WarmupPhase::Failed {
                        error: format!("model download failed: {e}"),
                    };
                    return;
                }
                tracing::info!("anno-rag: model download complete");
            }
        }
        *warmup_server.warmup_phase.write().await = WarmupPhase::Loading { started_ms };

        // Step 1: init Pipeline struct (opens LanceDB + vault, ~2 s).
        let pipeline_arc = match warmup_server.pipeline().await {
            Ok(_) => warmup_server.pipeline_arc(),
            Err(e) => {
                tracing::warn!("anno-rag: warmup skipped — pipeline init failed: {e}");
                *warmup_server.warmup_phase.write().await = WarmupPhase::Failed {
                    error: e.to_string(),
                };
                return;
            }
        };

        let Some(arc) = pipeline_arc else {
            tracing::warn!("anno-rag: warmup skipped — pipeline_arc returned None after init");
            *warmup_server.warmup_phase.write().await = WarmupPhase::Failed {
                error: "pipeline_arc returned None".into(),
            };
            return;
        };

        // Step 2: load embedder + detector in parallel (detector in spawn_blocking).
        let outcome = arc.warmup().await;

        let elapsed_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64)
            .saturating_sub(started_ms);

        if outcome.all_ok() {
            tracing::info!(elapsed_ms, "anno-rag: background model warmup complete");
            *warmup_server.warmup_phase.write().await = WarmupPhase::Ready { elapsed_ms };
        } else {
            let error = [
                outcome.embedder_error.map(|e| format!("embedder: {e}")),
                outcome.detector_error.map(|e| format!("detector: {e}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ");
            tracing::warn!(
                elapsed_ms,
                "anno-rag: background model warmup failed: {error}"
            );
            *warmup_server.warmup_phase.write().await = WarmupPhase::Failed { error };
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
mod allowed_roots_server_tests {
    use super::*;
    use crate::allowed_roots::AllowedRoots;

    fn server_with_allowed_root(root: &std::path::Path) -> (AnnoRagServer, tempfile::TempDir) {
        let data_dir = tempfile::tempdir().expect("data dir");
        let cfg = AnnoRagConfig {
            data_dir: data_dir.path().to_path_buf(),
            ..Default::default()
        };
        let raw = root.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("allowed roots");
        (
            AnnoRagServer::new_lazy(cfg, [0u8; 32]).with_allowed_roots_for_test(policy),
            data_dir,
        )
    }

    #[tokio::test]
    async fn rejects_mcp_paths_outside_allowed_roots() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);

        let err = server
            .privacy_prepare_folder_impl(PrivacyPrepareFolderParams {
                source_root: outside.to_string_lossy().to_string(),
                recursive: true,
            })
            .await
            .expect_err("outside root rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[tokio::test]
    async fn privacy_status_reports_allowed_roots() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);

        let status = server.privacy_status_impl().await;

        assert_eq!(status["ok"], true);
        assert_eq!(status["allowed_roots"]["enforced"], true);
        assert_eq!(status["allowed_roots"]["root_count"], 1);
        assert!(status["allowed_roots"].get("roots").is_none());
    }

    #[tokio::test]
    async fn index_rejects_outside_path_before_registering_corpus() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);

        let body = server
            .index_impl_routing(IndexParams {
                path: outside.to_string_lossy().to_string(),
                profile: "general".to_string(),
            })
            .await;
        let value: serde_json::Value = serde_json::from_str(&body).expect("json");

        assert_eq!(value["ok"], false);
        assert!(value["error"]
            .as_str()
            .expect("error")
            .contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[tokio::test]
    async fn forget_allows_removed_target_under_allowed_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);

        let body = server
            .forget_impl_routing(ForgetParams {
                target: allowed.join("removed-folder").display().to_string(),
            })
            .await;
        let value: serde_json::Value = serde_json::from_str(&body).expect("json");

        assert_eq!(value["ok"], true);
        assert_eq!(value["removed"]["knowledge_objects"], 0);
        assert_eq!(value["removed"]["legal_chunks"], 0);
    }

    #[tokio::test]
    async fn knowledge_sync_rejects_persisted_source_outside_allowed_roots_before_pipeline() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);
        let source_id = server
            .knowledge()
            .await
            .expect("knowledge")
            .add_local_folder(&outside.to_string_lossy())
            .expect("register outside source");

        let err = server
            .knowledge_sync_impl(KnowledgeSyncParams {
                source_id: Some(source_id),
            })
            .await
            .expect_err("outside source rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test]
    async fn sync_corpus_skips_persisted_source_outside_allowed_roots_before_pipeline() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let allowed = tmp.path().join("allowed");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let (server, _data_dir) = server_with_allowed_root(&allowed);
        let outside_string = outside.to_string_lossy().to_string();
        let source_id = server
            .knowledge()
            .await
            .expect("knowledge")
            .add_local_folder(&outside_string)
            .expect("register outside source");
        let corpus = server
            .corpus()
            .await
            .expect("corpus")
            .register_index_root(&outside_string, "general")
            .expect("register corpus");
        server
            .corpus()
            .await
            .expect("corpus")
            .store()
            .add_binding(
                corpus.corpus_id,
                anno_corpus_core::CorpusBindingKind::KnowledgeSource,
                &source_id,
                &serde_json::json!({"profile": "general"}),
            )
            .expect("bind source");

        let result = server
            .sync_corpus_impl(crate::corpus_sync::SyncCorpusParams {
                corpus_id: corpus.corpus_id.as_string(),
                sources: None,
                outputs: vec!["knowledge_fast".to_string()],
                max_files: None,
                max_millis: None,
            })
            .await
            .expect("sync corpus");

        assert_eq!(result.freshness, "maybe_stale");
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("outside ANNO_RAG_ALLOWED_ROOTS")));
        assert!(server.pipeline_arc().is_none());
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
        let cfg = AnnoRagConfig::default();
        let e = cfg.embedder_dir();
        let n = cfg.ner_onnx_dir();
        let p = cfg.ner_pii_onnx_dir();
        let c = cfg.ner_candle_dir();
        let rels: Vec<String> = vec![
            format!("{e}/config.json"),
            format!("{e}/model.safetensors"),
            format!("{e}/tokenizer.json"),
            format!("{n}/fp32_v2/classifier_fp32.onnx"),
            format!("{n}/fp32_v2/count_lstm_fixed_fp32.onnx"),
            format!("{n}/fp32_v2/count_pred_argmax_fp32.onnx"),
            format!("{n}/fp32_v2/encoder_fp32.onnx"),
            format!("{n}/fp32_v2/schema_gather_fp32.onnx"),
            format!("{n}/fp32_v2/scorer_fp32.onnx"),
            format!("{n}/fp32_v2/span_rep_fp32.onnx"),
            format!("{n}/fp32_v2/token_gather_fp32.onnx"),
            format!("{n}/fp32_v2/tokenizer.json"),
            // PII NER model (fastino/gliner2-privacy-filter-PII-multi)
            format!("{p}/fp32_v2/classifier_fp32.onnx"),
            format!("{p}/fp32_v2/count_lstm_fixed_fp32.onnx"),
            format!("{p}/fp32_v2/count_pred_argmax_fp32.onnx"),
            format!("{p}/fp32_v2/encoder_fp32.onnx"),
            format!("{p}/fp32_v2/schema_gather_fp32.onnx"),
            format!("{p}/fp32_v2/scorer_fp32.onnx"),
            format!("{p}/fp32_v2/span_rep_fp32.onnx"),
            format!("{p}/fp32_v2/token_gather_fp32.onnx"),
            format!("{p}/fp32_v2/tokenizer.json"),
            format!("{c}/tokenizer.json"),
            format!("{c}/config.json"),
            format!("{c}/encoder_config/config.json"),
            format!("{c}/model.safetensors"),
        ];
        for rel in &rels {
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

    #[tokio::test(flavor = "current_thread")]
    async fn pipeline_error_includes_missing_files_and_command() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let _models_env = ScopedAnnoModelsDir::unset();
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let err = server
            .pipeline()
            .await
            .err()
            .expect("pipeline must fail when models are absent");
        let msg = err.to_string();
        assert!(
            msg.contains("anno-rag download-models"),
            "error must mention the fix command: {msg}"
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
        assert!(v["warnings"].as_array().expect("warnings").iter().all(|w| {
            !w.as_str()
                .unwrap_or("")
                .contains("legal scope skipped in fast mode")
        }));
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
        // Should warn about FTS fallback, not silently skip.
        assert!(warnings.iter().any(|w| {
            w.as_str()
                .unwrap_or("")
                .contains("knowledge index uses FTS only")
        }));
        // Knowledge scope must NOT be skipped — it falls back to fast mode.
        assert_eq!(v["scope_modes"]["knowledge"], "fast");
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
        // vault state depends on the local keyring / ANNO_RAG_VAULT_PASSPHRASE env var;
        // assert the paired (available, reason) relationship rather than each field independently.
        let available = v["vault"]["available"]
            .as_bool()
            .expect("vault.available must be a bool");
        let reason = v["vault"]["reason"]
            .as_str()
            .expect("vault.reason must be a string");
        assert!(
            matches!(
                (available, reason),
                (false, "vault_not_initialized") | (true, "pipeline_not_yet_loaded")
            ),
            "unexpected vault state: available={available}, reason={reason}"
        );
        assert!(v["models"].get("inventory").is_some());
        assert_eq!(v["models"]["embedder_loaded"], false);
        assert_eq!(v["models"]["detector_loaded"], false);
        assert!(server.pipeline_arc().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn status_includes_warmup_phase() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let _models_env = ScopedAnnoModelsDir::unset();
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        *server.warmup_phase.write().await = WarmupPhase::Loading { started_ms: 0 };

        let status_json = server.status_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&status_json).unwrap();
        assert_eq!(v["models"]["warmup_phase"], "loading");
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
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        std::fs::create_dir_all(models_dir.join(cfg.embedder_dir())).unwrap();
        std::fs::create_dir_all(models_dir.join(cfg.ner_onnx_dir())).unwrap();
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

#[cfg(test)]
mod warmup_phase_tests {
    use super::*;

    #[test]
    fn all_pipeline_calls_replaced_with_require_models() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
        )
        .unwrap();
        // Count only lines that contain the call but are NOT inside the sentinel test itself
        // (the test body and the doc comment inside require_models add noise).
        let needle = concat!("self", ".pipeline().await");
        let remaining = src
            .lines()
            .filter(|l| l.contains(needle))
            .filter(|l| !l.contains("all_pipeline_calls_replaced_with_require_models"))
            .filter(|l| !l.contains("remaining"))
            .filter(|l| !l.contains("///"))
            .count();
        // Only the definition of pipeline() itself and its use inside require_models() should remain.
        assert!(
            remaining <= 2,
            "{remaining} remaining self.pipeline().await call-sites — expected ≤2"
        );
    }

    #[test]
    fn warmup_phase_debug_display() {
        let phase = WarmupPhase::Loading { started_ms: 0 };
        let s = format!("{phase:?}");
        assert!(s.contains("Loading"));
        let phase2 = WarmupPhase::Ready { elapsed_ms: 5000 };
        assert!(format!("{phase2:?}").contains("5000"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn serve_stdio_lazy_warmup_phase_starts_idle() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let _models_env = crate::model_inventory::test_env::ScopedAnnoModelsDir::unset();
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        // Phase must start Idle before any warmup is triggered.
        let phase = server.warmup_phase.read().await.clone();
        assert_eq!(phase, WarmupPhase::Idle, "phase must start Idle");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn require_models_returns_loading_when_warmup_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let _models_env = crate::model_inventory::test_env::ScopedAnnoModelsDir::unset();
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        // Force phase to Loading.
        *server.warmup_phase.write().await = WarmupPhase::Loading { started_ms: 0 };

        let result = server.require_models().await;
        assert!(result.is_err(), "must return Err during loading");
        let json_str = match result {
            Err(s) => s,
            Ok(_) => panic!("expected Err but got Ok"),
        };
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "models_loading");
    }

    #[test]
    fn corpus_required_envelope_has_convention_fields() {
        use crate::envelope::{envelope, status};
        let v = envelope(
            status::CORPUS_REQUIRED,
            "Plusieurs dossiers indexés.",
            "Relancez avec corpus_id/alias.",
            serde_json::json!({ "available": [] }),
        );
        assert_eq!(v["status"], "corpus_required");
        assert!(v["available"].is_array());
        assert!(v["message"].is_string());
        assert!(v["hint"].is_string());
    }
}
