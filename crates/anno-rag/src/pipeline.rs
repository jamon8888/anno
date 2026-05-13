//! Pipeline orchestration: ingest one doc end-to-end, search.

use crate::config::AnnoRagConfig;
use crate::detect::Detector;
use crate::embed::Embedder;
use crate::error::{Error, Result};
use crate::ingest;
use crate::store::{ChunkRecord, SearchHit, Store};
use crate::vault::Vault;
use std::path::Path;
use uuid::Uuid;

/// End-to-end pipeline: detect → pseudonymize → embed → store.
pub struct Pipeline {
    detector: Detector,
    vault: Vault,
    embedder: Embedder,
    store: Store,
    cfg: AnnoRagConfig,
}

impl Pipeline {
    /// Build a new pipeline. Creates the data directory if missing,
    /// opens the vault (with the supplied 32-byte key), loads the
    /// embedder weights (~470 MB on first call), opens the LanceDB store.
    pub async fn new(cfg: AnnoRagConfig, vault_key: [u8; 32]) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(Error::from)?;
        let detector = Detector::new()?;
        let vault = Vault::open(&cfg.vault_path(), vault_key)?;
        let embedder = Embedder::load(&cfg).await?;
        let store = Store::open(&cfg).await?;
        Ok(Self {
            detector,
            vault,
            embedder,
            store,
            cfg,
        })
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
            let entities = self.detector.detect(&chunk.text)?;
            let pseudo = self.vault.pseudonymize(&chunk.text, &entities).await?;
            pseudo_chunks.push(pseudo);
        }

        // Batch-embed all pseudonymized chunks at once for throughput.
        let vectors = self.embedder.embed_batch(&pseudo_chunks)?;
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
                "pdf" | "docx" | "pptx" | "xlsx" | "txt" | "md" | "html" | "eml" | "msg"
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
        match self.store.maybe_build_index(self.cfg.vector_index_threshold).await {
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
        let entities = self.detector.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self
            .embedder
            .embed_batch(&[pseudo_q.clone()])?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Embed("embedder returned empty result for query".into()))?;
        self.store.search(&pseudo_q, &qv, top_k).await
    }
}
