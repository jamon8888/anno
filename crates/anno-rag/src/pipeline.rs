//! Pipeline orchestration: ingest one doc end-to-end, search.

use crate::config::AnnoRagConfig;
use crate::detect::Detector;
use crate::embed::Embedder;
use crate::error::{Error, Result};
use crate::ingest;
use crate::store::{ChunkRecord, SearchHit, Store};
use crate::vault::Vault;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

/// End-to-end pipeline: detect → pseudonymize → embed → store.
pub struct Pipeline {
    detector: OnceCell<Arc<Detector>>,
    vault: Vault,
    embedder: OnceCell<Arc<Embedder>>,
    store: Store,
    cfg: AnnoRagConfig,
}

impl Pipeline {
    /// Build a new pipeline. Creates the data directory if missing,
    /// opens the vault (with the supplied 32-byte key), opens the LanceDB store.
    ///
    /// The embedder weights (~470 MB) and PII detector are NOT loaded here —
    /// they initialize lazily on first call to [`Pipeline::ingest_one`],
    /// [`Pipeline::search`], or [`Pipeline::detect`]. This keeps startup RSS
    /// under ~200 MB for callers that only need vault stats or rehydration.
    pub async fn new(cfg: AnnoRagConfig, vault_key: [u8; 32]) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(Error::from)?;
        let vault = Vault::open(&cfg.vault_path(), vault_key)?;
        let store = Store::open(&cfg).await?;
        Ok(Self {
            detector: OnceCell::new(),
            vault,
            embedder: OnceCell::new(),
            store,
            cfg,
        })
    }

    /// Lazy-init the embedder. Loads ~470 MB of model weights on first call.
    async fn embedder(&self) -> Result<&Arc<Embedder>> {
        self.embedder
            .get_or_try_init(|| async { Embedder::load(&self.cfg).await.map(Arc::new) })
            .await
    }

    /// Lazy-init the detector. Synchronous because `Detector::new` is sync.
    fn detector_get_or_init(&self) -> Result<&Arc<Detector>> {
        if let Some(d) = self.detector.get() {
            return Ok(d);
        }
        let d = Arc::new(Detector::new()?);
        // OnceCell::set returns Err(value) if already set — ignore since we just checked.
        let _ = self.detector.set(d);
        Ok(self.detector.get().expect("just set"))
    }

    /// Returns `true` if the embedder has been initialized (model weights loaded).
    #[must_use]
    pub fn embedder_loaded(&self) -> bool {
        self.embedder.initialized()
    }

    /// Returns `true` if the PII detector has been initialized.
    #[must_use]
    pub fn detector_loaded(&self) -> bool {
        self.detector.initialized()
    }

    /// Ingest a single file end-to-end. Writes `<stem>.anon.md` to `output_dir`.
    pub async fn ingest_one(&self, path: &Path, output_dir: &Path) -> Result<()> {
        let extracted = ingest::extract(path, &self.cfg).await?;
        let doc_id = Uuid::now_v7();
        let folder_path = path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        // Detect + pseudonymize per chunk.
        let mut pseudo_chunks: Vec<String> = Vec::with_capacity(extracted.chunks.len());
        for chunk in &extracted.chunks {
            let entities = self.detector_get_or_init()?.detect(&chunk.text)?;
            let pseudo = self.vault.pseudonymize(&chunk.text, &entities).await?;
            pseudo_chunks.push(pseudo);
        }

        // Batch-embed all pseudonymized chunks at once for throughput.
        let vectors = self.embedder().await?.embed_batch(&pseudo_chunks)?;
        if vectors.len() != pseudo_chunks.len() {
            return Err(Error::Embed(format!(
                "vectors len {} != chunks len {}",
                vectors.len(),
                pseudo_chunks.len()
            )));
        }

        // Build records.
        let mut records = Vec::with_capacity(extracted.chunks.len());
        for (i, chunk) in extracted.chunks.iter().enumerate() {
            records.push(ChunkRecord {
                doc_id,
                source_path: extracted.source_path.clone(),
                folder_path: folder_path.clone(),
                chunk_idx: chunk.idx,
                text_pseudo: pseudo_chunks[i].clone(),
                page: chunk.page,
                char_start: chunk.char_start,
                char_end: chunk.char_end,
                vector: vectors[i].clone(),
            });
        }

        self.store.upsert(records).await?;

        // Write the anonymized markdown copy.
        std::fs::create_dir_all(output_dir).map_err(Error::from)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc");
        let out_path = output_dir.join(format!("{stem}.anon.md"));
        let full_anon = pseudo_chunks.join("\n\n");
        std::fs::write(&out_path, full_anon).map_err(Error::from)?;

        tracing::info!(path = %path.display(), chunks = extracted.chunks.len(), "ingested");
        Ok(())
    }

    /// Walk a folder and ingest every supported file. Returns the count
    /// of successfully-ingested documents.
    pub async fn ingest_folder(
        &self,
        folder: &Path,
        recursive: bool,
        output_dir: &Path,
    ) -> Result<usize> {
        let mut count = 0_usize;
        let walker = if recursive {
            walkdir::WalkDir::new(folder).into_iter()
        } else {
            walkdir::WalkDir::new(folder).max_depth(1).into_iter()
        };
        for entry in walker.filter_map(std::result::Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !matches!(
                ext.as_str(),
                // Documents
                "pdf" | "docx" | "pptx" | "xlsx" | "xls" | "xlsb" | "xlsm"
                | "txt" | "md" | "rst" | "html" | "htm" | "rtf" | "epub" | "odt" | "ods" | "odp"
                // Email
                | "eml" | "msg"
                // Data / markup
                | "xml" | "csv" | "tsv" | "json" | "yaml" | "yml" | "toml"
                // Archives (kreuzberg extracts + recurses)
                | "zip" | "tar" | "gz" | "bz2" | "xz" | "7z"
                // Code source (tree-sitter)
                | "rs" | "py" | "js" | "ts" | "java" | "c" | "cpp" | "h" | "hpp"
                | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "scala" | "sql"
            ) {
                continue;
            }
            match self.ingest_one(path, output_dir).await {
                Ok(()) => count += 1,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "ingest skipped");
                }
            }
        }
        // Build the vector index in a background-equivalent flow once we
        // cross the configured threshold. Idempotent on retry.
        match self
            .store
            .maybe_build_index(self.cfg.vector_index_threshold)
            .await
        {
            Ok(true) => tracing::info!(
                threshold = self.cfg.vector_index_threshold,
                "built IVF_HNSW_SQ index on chunks.vector"
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "index build skipped"),
        }
        Ok(count)
    }

    /// Search: pseudonymize query → embed → store.search.
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self
            .embedder()
            .await?
            .embed_batch(&[pseudo_q.clone()])?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Embed("embedder returned empty result for query".into()))?;
        self.store.search(&pseudo_q, &qv, top_k).await
    }

    /// Rehydrate pseudo-tokens in `text` back to originals using the vault.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Vault`] if the cloakpipe rehydrator fails (typically
    /// only on malformed inputs — unknown tokens are silently left alone).
    pub async fn rehydrate(&self, text: &str) -> Result<RehydratedText> {
        use cloakpipe_core::rehydrator::Rehydrator;
        let guard = self.vault.lock_inner().await;
        let r = Rehydrator::rehydrate(text, &*guard)
            .map_err(|e| Error::Vault(format!("rehydrator: {e}")))?;
        Ok(RehydratedText {
            text: r.text,
            tokens_rehydrated: r.rehydrated_count,
        })
    }

    /// Detect PII in `text` without replacing. Useful for UI previews.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Detect`] if any layer (FR regex / anno NER) fails.
    pub fn detect(&self, text: &str) -> Result<Vec<cloakpipe_core::DetectedEntity>> {
        self.detector_get_or_init()?.detect(text)
    }

    /// Snapshot of the vault: total mappings + per-category counts.
    pub async fn vault_stats(&self) -> VaultStats {
        let guard = self.vault.lock_inner().await;
        let s = guard.stats();
        VaultStats {
            total_mappings: s.total_mappings,
            categories: s.categories,
        }
    }
}

/// Output of [`Pipeline::rehydrate`].
#[derive(Debug, Clone)]
pub struct RehydratedText {
    /// The text with all known tokens replaced by their originals.
    pub text: String,
    /// Count of tokens that were successfully looked up + replaced.
    pub tokens_rehydrated: usize,
}

/// Output of [`Pipeline::vault_stats`].
#[derive(Debug, Clone)]
pub struct VaultStats {
    /// Total number of token mappings in the vault.
    pub total_mappings: usize,
    /// Count per PII category (e.g. `"Email"`, `"PhoneNumber"`, `"Custom(NIR)"`).
    pub categories: std::collections::HashMap<String, u32>,
}
