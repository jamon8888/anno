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
        // Build the French-tokenized FTS index for hybrid search.
        match self.store.maybe_build_fts_index().await {
            Ok(true) => tracing::info!("built French FTS index on chunks.text_pseudo"),
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "FTS index build skipped"),
        }
        Ok(count)
    }

    /// Search: pseudonymize query → embed → store.search.
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;
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
        let r = Rehydrator::rehydrate(text, &guard)
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

    /// Remove every vault mapping that matches `subject_ref` (original or
    /// token). Idempotent: returns a receipt with `mappings_removed = 0` if
    /// nothing matched. Persists the vault on success.
    ///
    /// # Errors
    /// Returns [`Error::Vault`] if the vault could not be persisted.
    pub async fn forget(&self, subject_ref: &str) -> Result<ErasureReceipt> {
        let removed = self.vault.forget(subject_ref).await?;
        let now = chrono::Utc::now().to_rfc3339();
        Ok(match removed {
            Some(m) => ErasureReceipt {
                subject_ref: subject_ref.to_string(),
                mappings_removed: 1,
                token: Some(m.token),
                category: Some(format!("{:?}", m.category)),
                executed_at: now,
            },
            None => ErasureReceipt {
                subject_ref: subject_ref.to_string(),
                mappings_removed: 0,
                token: None,
                category: None,
                executed_at: now,
            },
        })
    }

    /// Look up every vault mapping for `subject_ref` (original or token).
    pub async fn find_subject(&self, subject_ref: &str) -> SubjectMatches {
        let matches = self
            .vault
            .find_subject(subject_ref)
            .await
            .into_iter()
            .map(|m| SubjectMatch {
                original: m.original,
                token: m.token,
                category: format!("{:?}", m.category),
            })
            .collect();
        SubjectMatches {
            subject_ref: subject_ref.to_string(),
            matches,
        }
    }

    /// Export the matches for `subject_ref` in the requested format. JSON
    /// returns the [`SubjectMatches`] shape; CSV returns
    /// `original,token,category` rows with a header.
    ///
    /// # Errors
    /// Returns [`Error::Audit`] if serialisation fails.
    pub async fn export_subject(
        &self,
        subject_ref: &str,
        format: ExportFormat,
    ) -> Result<Vec<u8>> {
        let res = self.find_subject(subject_ref).await;
        match format {
            ExportFormat::Json => serde_json::to_vec_pretty(&res)
                .map_err(|e| Error::Audit(format!("json serialize: {e}"))),
            ExportFormat::Csv => {
                let mut buf = Vec::new();
                {
                    let mut w = csv::Writer::from_writer(&mut buf);
                    w.write_record(["original", "token", "category"])
                        .map_err(|e| Error::Audit(format!("csv header: {e}")))?;
                    for m in &res.matches {
                        w.write_record([&m.original, &m.token, &m.category])
                            .map_err(|e| Error::Audit(format!("csv row: {e}")))?;
                    }
                    w.flush()
                        .map_err(|e| Error::Audit(format!("csv flush: {e}")))?;
                }
                Ok(buf)
            }
        }
    }

    /// Detect PII in `text`, pseudonymize with the vault, embed the
    /// tokenized text, and persist as a Memory row. The on-disk text is
    /// **always** the tokenized form; cleartext never reaches the
    /// `memories` LanceDB collection.
    ///
    /// # Errors
    /// Returns [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] depending on which layer fails.
    pub async fn save_memory(
        &self,
        text: &str,
        kind: Option<crate::memory::MemoryKind>,
        session_id: Option<String>,
    ) -> Result<SavedMemory> {
        let entities = self.detector_get_or_init()?.detect(text)?;
        let (tokenized, token_refs) =
            self.vault.pseudonymize_with_refs(text, &entities).await?;

        let mut embedding = self
            .embedder()
            .await?
            .embed_batch(std::slice::from_ref(&tokenized))?;
        let embedding = embedding.pop().ok_or_else(|| {
            Error::Embed("embed_batch returned no vector for memory".into())
        })?;

        let now = chrono::Utc::now();
        let id = crate::memory::MemoryId::new();
        let m = crate::memory::Memory {
            id: id.clone(),
            session_id,
            kind: kind.unwrap_or(crate::memory::MemoryKind::Context),
            text: tokenized.clone(),
            created_at: now,
            accessed_at: now,
            valid_from: now,
            valid_to: None,
            embedding,
            token_refs: token_refs.clone(),
            entity_refs: vec![],
        };

        self.store.memory_insert(&m).await?;

        Ok(SavedMemory {
            id,
            redacted_text: tokenized,
            token_refs,
        })
    }

    /// Hybrid recall: detect + pseudonymize the query, embed (e5 query
    /// prefix), run the dense+FTS RRF-reranked search on the `memories`
    /// table, optionally filter by `session_id` / `kinds`, rehydrate.
    ///
    /// `top_k` is the maximum returned. The search oversamples by 2× to
    /// give the filter some headroom; the final result is truncated.
    ///
    /// # Errors
    /// Returns [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] depending on which layer fails.
    pub async fn recall_memory(
        &self,
        query: &str,
        top_k: usize,
        session_id: Option<String>,
        kinds: Option<Vec<crate::memory::MemoryKind>>,
    ) -> Result<Vec<crate::memory::MemoryHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let (tokenized_query, _) =
            self.vault.pseudonymize_with_refs(query, &entities).await?;
        let query_vec = self.embedder().await?.embed_query(&tokenized_query)?;

        let mut raw = self
            .store
            .memories_hybrid_search(&query_vec, &tokenized_query, top_k.saturating_mul(2))
            .await?;

        if let Some(allowed) = &kinds {
            raw.retain(|h| allowed.iter().any(|k| *k == h.kind));
        }
        if let Some(s) = &session_id {
            // Match the session OR rows with no session (cross-session
            // facts shouldn't be hidden by a per-session recall).
            raw.retain(|h| h.session_id.as_deref() == Some(s.as_str()) || h.session_id.is_none());
        }
        raw.truncate(top_k);

        let mut out: Vec<crate::memory::MemoryHit> = Vec::with_capacity(raw.len());
        for row in raw {
            let rehydrated = self.rehydrate(&row.text_tokenized).await?;
            out.push(crate::memory::MemoryHit {
                id: row.id,
                text: rehydrated.text,
                kind: row.kind,
                created_at: row.created_at,
                score: row.score,
            });
        }
        Ok(out)
    }

    /// GDPR Art. 17 erasure on a Memory row.
    ///
    /// Either `id` OR `query` must be set (not both, not neither). When
    /// `query` is set, runs a hybrid recall and forgets up to `limit`
    /// rows. When `id` is set, `limit` is ignored.
    ///
    /// Cascade semantics: for every `TokenRef` on a deleted row, the
    /// vault entry is purged ONLY if no other memory row references it.
    /// This avoids breaking rehydration of co-occurring memories that
    /// reference the same pseudonym.
    ///
    /// When `dry_run` is true, returns the list of ids that *would* be
    /// forgotten without mutating either the memories table or the vault.
    ///
    /// # Errors
    /// Returns [`Error::Memory`] for bad arguments (neither/both id+query),
    /// [`Error::Store`] on memories-table failures, [`Error::Vault`] on
    /// cascade failures.
    pub async fn forget_memory(
        &self,
        id: Option<crate::memory::MemoryId>,
        query: Option<String>,
        limit: usize,
        dry_run: bool,
    ) -> Result<ForgetResult> {
        let targets: Vec<crate::memory::Memory> = match (id, query) {
            (Some(mid), None) => self
                .store
                .memory_get(&mid)
                .await?
                .into_iter()
                .collect(),
            (None, Some(q)) => {
                let hits = self.recall_memory(&q, limit, None, None).await?;
                let mut out = Vec::with_capacity(hits.len());
                for h in hits.iter().take(limit) {
                    let uid = uuid::Uuid::parse_str(&h.id)
                        .map_err(|e| Error::Memory(format!("bad id: {e}")))?;
                    if let Some(m) =
                        self.store.memory_get(&crate::memory::MemoryId(uid)).await?
                    {
                        out.push(m);
                    }
                }
                out
            }
            (Some(_), Some(_)) => {
                return Err(Error::Memory(
                    "exactly one of id / query must be set, not both".into(),
                ));
            }
            (None, None) => {
                return Err(Error::Memory(
                    "exactly one of id / query must be set".into(),
                ));
            }
        };

        if dry_run {
            return Ok(ForgetResult {
                forgotten_ids: targets.iter().map(|t| t.id.as_string()).collect(),
                vault_tokens_purged: 0,
            });
        }

        // Snapshot candidate tokens BEFORE the deletes, since
        // token_reference_count must run after deletion to count the
        // remaining references (which excludes the target rows).
        let mut candidate_tokens: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for t in &targets {
            for r in &t.token_refs {
                candidate_tokens.insert(r.token.clone());
            }
        }

        let mut forgotten_ids = Vec::with_capacity(targets.len());
        for t in &targets {
            self.store.memory_delete_by_id(&t.id).await?;
            forgotten_ids.push(t.id.as_string());
        }

        let mut purged = 0usize;
        for token in candidate_tokens {
            let count = self.store.token_reference_count(&token).await?;
            if count == 0 {
                // Reuse the v0.4 vault primitive — Vault::forget takes
                // either the original value or the token, and returns
                // Some(RemovedMapping) iff a vault entry actually went away.
                if self.vault.forget(&token).await?.is_some() {
                    purged += 1;
                }
            }
        }

        Ok(ForgetResult {
            forgotten_ids,
            vault_tokens_purged: purged,
        })
    }
}

/// Result returned by [`Pipeline::forget_memory`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct ForgetResult {
    /// Stringified ids of memory rows that were deleted (or would be, when
    /// `dry_run` is true).
    pub forgotten_ids: Vec<String>,
    /// Number of vault entries actually purged. Always 0 when `dry_run`.
    pub vault_tokens_purged: usize,
}

/// Receipt returned by [`Pipeline::save_memory`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SavedMemory {
    /// Newly minted memory id.
    pub id: crate::memory::MemoryId,
    /// Tokenized form of the input text (what got persisted).
    pub redacted_text: String,
    /// `(category, token)` pairs minted for the GDPR Art. 17 cascade.
    pub token_refs: Vec<crate::memory::TokenRef>,
}

/// Receipt returned by [`Pipeline::forget`]. Suitable for inclusion in an
/// audit event.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErasureReceipt {
    /// Whatever the caller passed (original or token).
    pub subject_ref: String,
    /// Number of vault mappings removed (0 if subject was unknown).
    pub mappings_removed: usize,
    /// Token that was retired, if a mapping was found.
    pub token: Option<String>,
    /// Category of the retired mapping, if any.
    pub category: Option<String>,
    /// UTC timestamp of the operation, RFC 3339.
    pub executed_at: String,
}

/// Result of [`Pipeline::find_subject`]. Multiple matches may be returned
/// (the cloakpipe primitive currently returns 0 or 1 entries; the type is
/// vec-shaped for a future fuzzy match).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubjectMatches {
    /// The subject reference the caller looked up.
    pub subject_ref: String,
    /// Zero or more matches.
    pub matches: Vec<SubjectMatch>,
}

/// One match returned by [`Pipeline::find_subject`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubjectMatch {
    /// Original (sensitive) value.
    pub original: String,
    /// Pseudo-token in the vault.
    pub token: String,
    /// Category key (e.g. `"Person"`, `"Email"`, `"NIR"`).
    pub category: String,
}

/// Output format for [`Pipeline::export_subject`].
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    /// JSON object: `{ "subject_ref": ..., "matches": [...] }`.
    Json,
    /// CSV with header: `original,token,category`.
    Csv,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a Pipeline rooted at `dir` (tempdir-friendly). `Pipeline::new`
    /// opens LanceDB, which takes ~30 s — these tests are gated behind
    /// `--ignored` to keep `cargo test` snappy on every run.
    async fn pipeline_in(dir: &Path) -> Pipeline {
        let mut cfg = AnnoRagConfig::default();
        cfg.data_dir = dir.to_path_buf();
        Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline opens")
    }

    #[tokio::test]
    #[ignore = "Pipeline::new opens LanceDB (~30s) — opt in via --ignored"]
    async fn pipeline_forget_returns_receipt_with_token() {
        use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
        let tmp = tempfile::tempdir().unwrap();
        let p = pipeline_in(tmp.path()).await;
        let _ = p
            .vault
            .pseudonymize(
                "Marie Dupont",
                &[DetectedEntity {
                    original: "Marie Dupont".into(),
                    start: 0,
                    end: 12,
                    category: EntityCategory::Person,
                    confidence: 1.0,
                    source: DetectionSource::Pattern,
                }],
            )
            .await
            .unwrap();

        let receipt = p.forget("Marie Dupont").await.unwrap();
        assert_eq!(receipt.subject_ref, "Marie Dupont");
        assert_eq!(receipt.mappings_removed, 1);
        assert!(receipt.token.is_some());
    }

    #[tokio::test]
    #[ignore = "Pipeline::new opens LanceDB (~30s) — opt in via --ignored"]
    async fn pipeline_forget_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = pipeline_in(tmp.path()).await;
        let r = p.forget("never seen").await.unwrap();
        assert_eq!(r.mappings_removed, 0);
    }

    #[tokio::test]
    #[ignore = "Pipeline::new opens LanceDB (~30s) — opt in via --ignored"]
    async fn pipeline_find_subject_returns_empty_when_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        let p = pipeline_in(tmp.path()).await;
        assert!(p.find_subject("nope").await.matches.is_empty());
    }

    #[test]
    fn erasure_receipt_serialises_to_json() {
        let r = ErasureReceipt {
            subject_ref: "x".into(),
            mappings_removed: 1,
            token: Some("PERSON_1".into()),
            category: Some("Person".into()),
            executed_at: "2026-05-15T00:00:00Z".into(),
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"mappings_removed\":1"));
        assert!(s.contains("\"token\":\"PERSON_1\""));
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
