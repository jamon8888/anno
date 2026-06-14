//! Pipeline orchestration: ingest one doc end-to-end, search.

use crate::config::{AnnoRagConfig, MemoryNerMode};
use crate::detect::Detector;
use crate::embed::Embedder;
use crate::error::{Error, Result};
use crate::ingest;
use crate::legal::enricher::{LegalEnricher, LegalEntityExtractor};
use crate::legal::types::{LegalEntity, LegalLabel};
use crate::store::{ChunkRecord, SearchHit, Store};
use crate::vault::Vault;
use anno_corpus_core::{ContentId, CorpusId, DocumentInstanceId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;
use uuid::Uuid;

/// Metadata carrier for legal entities already detected during ingest.
///
/// Ingest supplies pre-detected legal entities from `Detector::detect_for_ingest`
/// directly; `extract` is not called on the normal ingest path. The model id
/// still reports the GLiNER2 detector because enrichment rows were produced from
/// those spans.
struct PreDetectedLegalExtractor;

#[allow(deprecated)] // NER_MODEL_ID is deprecated; LegalEntityExtractor::model_id must return &'static str
impl LegalEntityExtractor for PreDetectedLegalExtractor {
    fn extract(
        &self,
        _text: &str,
        _labels: &[LegalLabel],
        _thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>> {
        Ok(vec![])
    }

    fn model_id(&self) -> &'static str {
        crate::detect::NER_MODEL_ID
    }

    fn extractor_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

/// Deterministic document id: UUID v5 (OID namespace) of the raw file
/// bytes. Same file content ⇒ same `doc_id` ⇒ the existing
/// `merge_insert(&["doc_id","chunk_idx"])` overwrites its own rows
/// instead of duplicating across `ingest_folder` runs.
#[must_use]
pub(crate) fn doc_uuid(file_bytes: &[u8]) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, file_bytes)
}

/// Corpus-qualified legal ingest context.
#[derive(Debug, Clone)]
pub struct LegalIngestScope {
    /// Corpus that owns the indexed legal document.
    pub corpus_id: CorpusId,
    /// Root folder used to derive stable relative document paths.
    pub root: std::path::PathBuf,
}

/// Document observed during a corpus-scoped legal ingest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalIngestedDocument {
    /// Corpus-qualified legal document id.
    pub document_id: DocumentInstanceId,
    /// Source path used by the legal index.
    pub source_path: String,
    /// Normalized path relative to the corpus root, when scoped.
    pub relative_path: Option<String>,
    /// Content hash of the source bytes.
    pub content_id: ContentId,
}

/// Summary returned by a corpus-scoped legal ingest.
#[derive(Debug, Clone, Default)]
pub struct LegalIngestSummary {
    /// Number of newly indexed documents.
    pub ingested: usize,
    /// Documents that are available in the corpus, including already-indexed ones.
    pub documents: Vec<LegalIngestedDocument>,
}

#[must_use]
pub(crate) fn scoped_doc_uuid(
    corpus_id: CorpusId,
    normalized_relative_path: &str,
    content_id: &ContentId,
) -> Uuid {
    DocumentInstanceId::from_parts(corpus_id, normalized_relative_path, content_id).as_uuid()
}

fn normalized_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

fn intersect_uuids(a: &[uuid::Uuid], b: &[uuid::Uuid]) -> Vec<uuid::Uuid> {
    let b: std::collections::BTreeSet<_> = b.iter().copied().collect();
    a.iter().copied().filter(|id| b.contains(id)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IngestOutcome {
    Ingested {
        used_embedded_ocr: bool,
        document: Option<LegalIngestedDocument>,
    },
    AlreadyIndexed {
        document: Option<LegalIngestedDocument>,
    },
    Skipped,
}

/// End-to-end pipeline: detect → pseudonymize → embed → store.
pub struct Pipeline {
    detector: OnceCell<Arc<Detector>>,
    pub(crate) vault: Vault,
    embedder: OnceCell<Arc<Embedder>>,
    #[cfg(feature = "rerank")]
    reranker: tokio::sync::OnceCell<std::sync::Arc<crate::rerank::Reranker>>,
    store: Store,
    cfg: AnnoRagConfig,
    /// Memories-table row count as of the last `optimize_memories`
    /// fold-in on the recall path. When the live count exceeds this,
    /// recall runs `optimize()` to index the new rows, then advances
    /// the watermark. `Relaxed` is sufficient: a missed/duplicated
    /// optimize is self-correcting on the next recall (idempotent),
    /// never incorrect.
    memory_fts_watermark: std::sync::atomic::AtomicU64,
    /// LanceDB legal chunk enrichment table.
    pub(crate) legal_store: crate::legal::store::LegalStore,
    /// Enrichment retry status sidecar.
    pub(crate) enrichment_status: crate::legal::status::EnrichmentStatusStore,
    /// Legal entity enricher (combined GLiNER2 + rules + normalization).
    pub(crate) legal_enricher: LegalEnricher,
    /// Legal knowledge graph sidecar.
    pub(crate) legal_kg: std::sync::Arc<dyn crate::legal::kg::LegalKnowledgeGraph>,
}

impl Pipeline {
    /// Build a new pipeline. Creates the data directory if missing,
    /// opens the vault (with the supplied 32-byte key), opens the LanceDB store.
    ///
    /// The embedder weights (~470 MB) and PII detector are NOT loaded here —
    /// they initialize lazily on first call to [`Pipeline::ingest_one`],
    /// [`Pipeline::search`], or [`Pipeline::detect`]. This keeps startup RSS
    /// under ~200 MB for callers that only need vault stats or rehydration.
    ///
    /// If `vault.enc` exists but the key no longer matches (reinstall,
    /// passphrase reset, keyring cleared), startup fails with an explicit
    /// vault error. The file is not renamed or deleted automatically because
    /// losing pseudonymisation mappings breaks rehydration.
    pub async fn new(cfg: AnnoRagConfig, vault_key: [u8; 32]) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(Error::from)?;
        let vault = match Vault::open(&cfg.vault_path(), vault_key) {
            Ok(v) => v,
            Err(Error::Vault(ref msg)) if msg.contains("Decryption failed") => {
                return Err(Error::Vault(
                    "vault key mismatch; refusing to replace vault.enc automatically. \
                     Configure the original ANNO_RAG_VAULT_PASSPHRASE/keyring key, \
                     or move the vault/index aside and re-index deliberately."
                        .into(),
                ));
            }
            Err(e) => return Err(e),
        };
        let store = Store::open(&cfg).await?;
        let legal_store = crate::legal::store::LegalStore::open(&cfg).await?;
        // Index setup may fail on a brand-new empty table; treat as non-fatal.
        if let Err(e) = legal_store.setup_indexes().await {
            tracing::warn!(error = %e, "legal index setup skipped (table may be empty)");
        }
        let enrichment_status = crate::legal::status::EnrichmentStatusStore::open(&cfg).await?;
        let legal_enricher = LegalEnricher::new(Arc::new(PreDetectedLegalExtractor));
        let legal_kg: std::sync::Arc<dyn crate::legal::kg::LegalKnowledgeGraph> =
            std::sync::Arc::new(crate::legal::kg::LanceGraphStore::open(&cfg).await?);
        Ok(Self {
            detector: OnceCell::new(),
            vault,
            embedder: OnceCell::new(),
            #[cfg(feature = "rerank")]
            reranker: tokio::sync::OnceCell::new(),
            store,
            cfg,
            memory_fts_watermark: std::sync::atomic::AtomicU64::new(0),
            legal_store,
            enrichment_status,
            legal_enricher,
            legal_kg,
        })
    }

    /// Lazy-init the embedder. Loads ~470 MB of model weights on first call.
    async fn embedder(&self) -> Result<&Arc<Embedder>> {
        self.embedder
            .get_or_try_init(|| async { Embedder::load(&self.cfg).await.map(Arc::new) })
            .await
    }

    /// Lazy-init the detector. Synchronous because `Detector::new` is sync.
    pub(crate) fn detector_get_or_init(&self) -> Result<&Arc<Detector>> {
        if let Some(d) = self.detector.get() {
            return Ok(d);
        }
        let d = Arc::new(Detector::new(&self.cfg)?);
        // OnceCell::set returns Err(value) if already set — ignore since we just checked.
        let _ = self.detector.set(d);
        Ok(self.detector.get().expect("just set"))
    }

    /// Borrow config for privacy workspace orchestration.
    #[must_use]
    pub fn config(&self) -> &AnnoRagConfig {
        &self.cfg
    }

    /// Load detector for privacy workspace orchestration.
    ///
    /// # Errors
    /// Returns detector initialization errors.
    pub fn detector_for_privacy(&self) -> Result<&crate::detect::Detector> {
        self.detector_get_or_init().map(|arc| arc.as_ref())
    }

    /// Borrow vault for privacy workspace orchestration.
    #[must_use]
    pub fn vault_for_privacy(&self) -> &crate::vault::Vault {
        &self.vault
    }

    /// Returns `true` if the embedder has been initialized (model weights loaded).
    #[must_use]
    pub fn embedder_loaded(&self) -> bool {
        self.embedder.initialized()
    }

    /// Returns true if a usable vault key source is configured — either the
    /// `ANNO_RAG_VAULT_PASSPHRASE` env var (Docker / CI) or an OS keyring
    /// entry (interactive installs). The vault file on disk is written lazily
    /// on the first ingest, so file existence gives false negatives.
    #[must_use]
    pub fn vault_is_initialized(&self) -> bool {
        crate::vault::is_vault_key_usable()
    }

    /// Lazy-init the cross-encoder reranker. Downloads ~571 MB (INT8
    /// ONNX) on first call; cached thereafter. Only compiled when the
    /// `rerank` feature is on.
    ///
    /// # Errors
    /// [`Error::Rerank`] if the model fetch or session build fails.
    #[cfg(feature = "rerank")]
    async fn reranker(&self) -> Result<&Arc<crate::rerank::Reranker>> {
        self.reranker
            .get_or_try_init(|| async {
                crate::rerank::Reranker::load(&self.cfg).await.map(Arc::new)
            })
            .await
    }

    /// Returns `true` if the reranker has been initialized.
    #[cfg(feature = "rerank")]
    #[must_use]
    pub fn reranker_loaded(&self) -> bool {
        self.reranker.initialized()
    }

    /// Returns `true` if the PII detector has been initialized.
    #[must_use]
    pub fn detector_loaded(&self) -> bool {
        self.detector.initialized()
    }

    /// List distinct raw legal corpus folder paths from the chunks table.
    /// Intended for local/admin bookkeeping; do not expose these paths in MCP
    /// or UI responses without a privacy gate.
    ///
    /// # Errors
    /// Returns store errors on LanceDB query/decode failure.
    pub async fn store_list_indexed_folder_paths(&self) -> Result<Vec<String>> {
        self.store.list_indexed_folder_paths().await
    }

    /// Count rows in the legal chunks table.
    ///
    /// # Errors
    /// Returns store errors on LanceDB count failure.
    pub async fn store_count_chunks(&self) -> Result<u64> {
        self.store.count_chunks().await
    }

    /// Remove legal corpus state whose `source_path` is inside `path`.
    ///
    /// # Errors
    /// Returns store/legal/graph errors on cascade or chunk deletion failure.
    pub async fn forget_legal_folder_path(&self, path: &str) -> Result<u64> {
        let doc_ids = self.store.doc_ids_for_source_subtree(path).await?;
        for doc_id in &doc_ids {
            self.legal_store.delete_doc(*doc_id).await?;
            self.legal_kg.delete_doc(*doc_id).await?;
            self.enrichment_status.delete_doc(*doc_id).await?;
        }
        let report = self.store.delete_folder_rows(path).await?;
        Ok(report.removed_chunks)
    }

    /// Remove legal corpus state for exact document ids.
    ///
    /// # Errors
    /// Returns store/legal/graph errors on cascade or chunk deletion failure.
    pub async fn forget_legal_doc_ids(&self, doc_ids: &[uuid::Uuid]) -> Result<u64> {
        for doc_id in doc_ids {
            self.legal_store.delete_doc(*doc_id).await?;
            self.legal_kg.delete_doc(*doc_id).await?;
            self.enrichment_status.delete_doc(*doc_id).await?;
        }
        self.store.delete_doc_id_rows(doc_ids).await
    }

    /// Ingest a single file end-to-end. Writes `<stem>.anon.md` to `output_dir`.
    pub async fn ingest_one(&self, path: &Path, output_dir: &Path) -> Result<()> {
        self.ingest_one_counted(path, output_dir, &self.cfg, None)
            .await?;
        Ok(())
    }

    async fn ingest_one_counted(
        &self,
        path: &Path,
        output_dir: &Path,
        cfg: &AnnoRagConfig,
        scope: Option<&LegalIngestScope>,
    ) -> Result<IngestOutcome> {
        let file_bytes = std::fs::read(path).map_err(Error::from)?;
        let content_id = ContentId::from_bytes(&file_bytes);
        let scoped_relative = scope.map(|scope| normalized_relative_path(&scope.root, path));
        let doc_id = match (scope, scoped_relative.as_deref()) {
            (Some(scope), Some(relative)) => {
                scoped_doc_uuid(scope.corpus_id, relative, &content_id)
            }
            _ => doc_uuid(&file_bytes),
        };
        let scoped_document = || {
            scope.map(|_| LegalIngestedDocument {
                document_id: DocumentInstanceId::new(doc_id),
                source_path: path.display().to_string(),
                relative_path: scoped_relative.clone(),
                content_id: content_id.clone(),
            })
        };
        if self.store.doc_exists(doc_id).await? {
            tracing::info!(path = %path.display(), "skip: already ingested (same content)");
            return Ok(IngestOutcome::AlreadyIndexed {
                document: scoped_document(),
            });
        }
        let extracted = ingest::extract(path, cfg).await?;
        let used_embedded_ocr = extracted.ocr_status == ingest::OcrStatus::CompletedEmbedded;
        if !should_index_extracted_doc(&extracted) {
            tracing::warn!(
                path = %path.display(),
                class = ?extracted.class,
                status = ?extracted.ocr_status,
                chunks = extracted.chunks.len(),
                "ingest skipped before indexing"
            );
            return Ok(IngestOutcome::Skipped);
        }
        let folder_path = path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        // ── 1. Layer-aware detect: one NER call per chunk, PII and legal split ──
        let legal_labels = crate::legal::default_legal_labels();
        let legal_thresholds = crate::legal::default_thresholds();
        let detector = self.detector_get_or_init()?;

        let mut detection_bundles: Vec<crate::detect::IngestDetectionBundle> =
            Vec::with_capacity(extracted.chunks.len());
        for chunk in &extracted.chunks {
            let bundle =
                detector.detect_for_ingest(&chunk.text, &legal_labels, &legal_thresholds)?;
            detection_bundles.push(bundle);
        }

        // ── 2. Pseudonymize using PII subset; capture offset map ──
        let mut pseudo_chunks: Vec<String> = Vec::with_capacity(extracted.chunks.len());
        let mut offset_maps: Vec<crate::legal::offsets::PseudoOffsetMap> =
            Vec::with_capacity(extracted.chunks.len());
        for (chunk, bundle) in extracted.chunks.iter().zip(&detection_bundles) {
            let (pseudo, map) = self
                .vault
                .pseudonymize_with_map(&chunk.text, &bundle.pii)
                .await?;
            pseudo_chunks.push(pseudo);
            offset_maps.push(map);
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

        // Content changed (or first ingest): drop any prior rows for
        // this source_path so a superseded doc_id doesn't orphan.
        self.store.delete_doc_rows(&extracted.source_path).await?;
        self.store.upsert(records).await?;

        // ── 4. Legal chunk enrichment dual-write (table only; graph in C2) ──
        let mut legal_rows = Vec::with_capacity(extracted.chunks.len());
        let mut node_batch = crate::legal::kg::NodeBatch::new();
        let mut edge_batch = crate::legal::kg::EdgeBatch::new();
        // Document root node for the graph.
        node_batch.add_document(doc_id, None, None, None, None, None);
        for (i, chunk) in extracted.chunks.iter().enumerate() {
            let chunk_id = crate::store::chunk_uuid(doc_id, chunk.idx);
            // Chunk node bridges graph ↔ LanceDB.
            node_batch.add_chunk(
                chunk_id,
                doc_id,
                chunk.char_start,
                chunk.char_end,
                chunk.page,
            );
            let raw_legal_ents = detection_bundles[i].legal.clone();
            let legal_ents = self.legal_enricher.translate_to_pseudo(
                raw_legal_ents,
                &offset_maps[i],
                &pseudo_chunks[i],
            );
            let fwd = crate::legal::rules::VaultForwardMap {
                alias_to_canonical: Default::default(),
            };
            let (row, _facts, nodes, edges) = self.legal_enricher.enrich_one(
                chunk_id,
                doc_id,
                &pseudo_chunks[i],
                &legal_ents,
                &fwd,
            );
            legal_rows.push(row);
            node_batch.absorb(nodes);
            edge_batch.absorb(edges);
        }
        // ── D3: Mandatory-clause evaluation ──────────────────────────────────
        // Concatenate all pseudonymized chunks into a single text, detect
        // which doc-type checklist applies, and stamp every row with the
        // aggregate status so filtered search can filter by it.
        {
            let full_pseudo_text = pseudo_chunks.join("\n");
            let doc_type_key = legal_rows
                .first()
                .and_then(|r| r.doc_type.as_deref())
                .unwrap_or("unknown");
            if doc_type_key != "unknown" {
                let checks = crate::legal::mandatory::evaluate_doc(doc_type_key, &full_pseudo_text);
                let agg = crate::legal::mandatory::aggregate_status(&checks);
                for row in &mut legal_rows {
                    row.mandatory_clause_status = Some(agg.clone());
                }
                node_batch.absorb(checks.into_iter().map(|c| c.into_node()).collect());
                crate::legal::audit::audit_mandatory(doc_id, &agg);
            }
        }

        // LanceDB flat-table write.
        let lance_ok = match self.legal_store.upsert(&legal_rows).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    doc_id = %doc_id,
                    error = %e,
                    "legal_chunk_enrichment write failed; chunks indexed, marking pending"
                );
                if let Err(e2) = self
                    .enrichment_status
                    .mark_pending(doc_id, extracted.chunks.len() as i32, &e.to_string())
                    .await
                {
                    tracing::warn!(doc_id = %doc_id, error = %e2, "enrichment_status mark_pending also failed");
                }
                false
            }
        };
        // Graph dual-write.
        if lance_ok {
            match self.legal_kg.upsert_batch(&node_batch, &edge_batch).await {
                Ok(()) => {
                    if let Err(e) = self.enrichment_status.mark_ok(doc_id).await {
                        tracing::warn!(doc_id = %doc_id, error = %e, "enrichment_status mark_ok failed");
                    }
                    // Cross-document linking pass (Phase 1: always empty hints).
                    let hints = self.legal_enricher.cross_doc_hints(
                        doc_id,
                        &legal_rows,
                        &node_batch,
                        &edge_batch,
                    );
                    if let Err(e) = self.legal_kg.link_cross_documents(doc_id, &hints).await {
                        tracing::warn!(doc_id = %doc_id, error = %e, "link_cross_documents failed (non-fatal)");
                    }
                }
                Err(e) => {
                    tracing::warn!(doc_id = %doc_id, error = %e, "graph write failed; marking pending");
                    if let Err(e2) = self
                        .enrichment_status
                        .mark_pending(doc_id, extracted.chunks.len() as i32, &e.to_string())
                        .await
                    {
                        tracing::warn!(doc_id = %doc_id, error = %e2, "enrichment_status mark_pending also failed");
                    }
                }
            }
        }

        // Write the anonymized markdown copy.
        std::fs::create_dir_all(output_dir).map_err(Error::from)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc");
        let out_path = output_dir.join(format!("{stem}.anon.md"));
        let full_anon = pseudo_chunks.join("\n\n");
        std::fs::write(&out_path, full_anon).map_err(Error::from)?;

        tracing::info!(path = %path.display(), chunks = extracted.chunks.len(), "ingested");
        Ok(IngestOutcome::Ingested {
            used_embedded_ocr,
            document: scoped_document().map(|mut document| {
                document.source_path.clone_from(&extracted.source_path);
                document
            }),
        })
    }

    /// Walk a folder and ingest every supported file. Returns the count
    /// of successfully-ingested documents.
    pub async fn ingest_folder(
        &self,
        folder: &Path,
        recursive: bool,
        output_dir: &Path,
    ) -> Result<usize> {
        Ok(self
            .ingest_folder_with_scope(folder, recursive, output_dir, None)
            .await?
            .ingested)
    }

    /// Walk a folder and ingest every supported file with corpus-qualified document ids.
    pub async fn ingest_folder_scoped(
        &self,
        folder: &Path,
        recursive: bool,
        output_dir: &Path,
        scope: LegalIngestScope,
    ) -> Result<usize> {
        Ok(self
            .ingest_folder_scoped_summary(folder, recursive, output_dir, scope)
            .await?
            .ingested)
    }

    /// Walk a folder with corpus-qualified document ids and return indexed document metadata.
    pub async fn ingest_folder_scoped_summary(
        &self,
        folder: &Path,
        recursive: bool,
        output_dir: &Path,
        scope: LegalIngestScope,
    ) -> Result<LegalIngestSummary> {
        self.ingest_folder_with_scope(folder, recursive, output_dir, Some(scope))
            .await
    }

    /// Prepare a local privacy workspace under `<source_root>/vault`.
    ///
    /// # Errors
    /// Returns privacy, extraction, detection, vault, or IO errors.
    pub async fn privacy_prepare_folder(
        &self,
        source_root: &Path,
        recursive: bool,
    ) -> Result<crate::privacy_workspace::PrivacyPrepareSummary> {
        crate::privacy_workspace::prepare_folder(self, source_root, recursive).await
    }

    /// Finalize a local privacy workspace after user Word edits.
    ///
    /// # Errors
    /// Returns privacy, extraction, detection, vault, or IO errors.
    pub async fn privacy_finalize_folder(
        &self,
        workspace: &Path,
    ) -> Result<crate::privacy_workspace::PrivacyFinalizeSummary> {
        crate::privacy_workspace::finalize_folder(self, workspace).await
    }

    async fn ingest_folder_with_scope(
        &self,
        folder: &Path,
        recursive: bool,
        output_dir: &Path,
        scope: Option<LegalIngestScope>,
    ) -> Result<LegalIngestSummary> {
        let mut summary = LegalIngestSummary::default();
        let mut ocr_spent = Duration::ZERO;
        let ocr_budget = self.cfg.ocr_batch_budget_secs.map(Duration::from_secs);
        let paths = legal_ingest_candidate_paths(folder, recursive, output_dir);
        for p in paths {
            let doc_cfg = cfg_for_ocr_budget(&self.cfg, ocr_budget, ocr_spent);
            let started = Instant::now();
            match self
                .ingest_one_counted(&p, output_dir, &doc_cfg, scope.as_ref())
                .await
            {
                Ok(IngestOutcome::Ingested {
                    used_embedded_ocr,
                    document,
                }) => {
                    if used_embedded_ocr {
                        ocr_spent += started.elapsed();
                    }
                    summary.ingested += 1;
                    if let Some(document) = document {
                        summary.documents.push(document);
                    }
                }
                Ok(IngestOutcome::AlreadyIndexed { document }) => {
                    if let Some(document) = document {
                        summary.documents.push(document);
                    }
                }
                Ok(IngestOutcome::Skipped) => {}
                Err(e) => {
                    tracing::warn!(path = %p.display(), error = %e, "ingest skipped");
                }
            }
        }
        // Retry pending legal enrichments first so newly-re-enriched docs
        // are covered by the subsequent index optimization pass.
        if let Err(e) = self.drain_enrichment_backlog(64).await {
            tracing::warn!(error = %e, "drain_enrichment_backlog failed (non-fatal)");
        }
        // Consolidate all post-ingest index builds and KG compaction.
        if let Err(e) = self.optimize_after_ingest().await {
            tracing::warn!(error = %e, "optimize_after_ingest failed (non-fatal)");
        }
        Ok(summary)
    }

    /// Search: pseudonymize query → embed → store.search.
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;
        self.store.search(&pseudo_q, &qv, top_k).await
    }

    /// Search + cross-encoder rerank.
    ///
    /// 1. `search` with `pool_size` (over-fetch).
    /// 2. Rehydrate each hit's `text_pseudo` to plaintext via the vault
    ///    — the cross-encoder must see real entities, not `<PERSON_42>`.
    /// 3. Score `(plaintext_query, rehydrated_text)` pairs.
    /// 4. Reorder by score desc; replace `SearchHit::score` with the
    ///    cross-encoder score.
    /// 5. Truncate to `top_k`.
    ///
    /// The plaintext query is used **only** for the rerank stage; the
    /// upstream embed + FTS lookup still runs on the pseudonymized query,
    /// preserving the privacy invariant.
    ///
    /// # Errors
    /// [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Rerank`] per failing layer.
    #[cfg(feature = "rerank")]
    pub async fn search_reranked(
        &self,
        query: &str,
        top_k: usize,
        pool_size: usize,
    ) -> Result<Vec<SearchHit>> {
        let pool = pool_size.max(top_k).max(1);
        let mut hits = self.search(query, pool).await?;
        if hits.is_empty() {
            return Ok(hits);
        }

        let mut passages: Vec<String> = Vec::with_capacity(hits.len());
        for h in &hits {
            let r = self.rehydrate(&h.text_pseudo).await?;
            passages.push(r.text);
        }
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();

        let reranker = self.reranker().await?;
        let scores = reranker.score_pairs_batched(query, &refs, self.cfg.rerank_batch_size)?;

        for (h, s) in hits.iter_mut().zip(&scores) {
            h.score = *s;
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k);
        Ok(hits)
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

    /// Build the `entity_refs` payload for a memory row: merge the vault
    /// token refs (already collected during `pseudonymize_with_refs`) with
    /// non-PII NER entities extracted from the **pre-vault plaintext**.
    ///
    /// Output strings are canonical (see `canonicalize_entity` +
    /// `canonicalize_pii_token`) — they're the keys the LabelList scalar
    /// index on `memories.entity_refs` will filter on for v0.2's 2-hop
    /// graph traversal.
    ///
    /// Person entities returned by the NER are **deliberately skipped** —
    /// names are the vault's job; if a Person slips past the vault into
    /// the entity_refs path, that's a leak we do NOT want a graph traversal
    /// to amplify.
    ///
    /// NER errors are logged at `target = "anno_rag::memory::audit"` and
    /// returned as "vault tokens only" — never panic the save path. Sub-
    /// 0.6 confidence NER hits are filtered out to bound false-positive
    /// graph edges.
    pub fn extract_entities(
        &self,
        plaintext: &str,
        token_refs: &[crate::memory::TokenRef],
    ) -> Vec<String> {
        use crate::canonicalize::{canonicalize_entity, canonicalize_pii_token};
        use anno::{EntityType, Model, StackedNER};
        use std::collections::HashSet;

        let mut out: HashSet<String> = HashSet::new();

        // 1. Vault token refs — already collected upstream; canonical form
        //    embeds the token id so the graph traversal scopes per-tenant.
        for tr in token_refs {
            out.insert(canonicalize_pii_token(&tr.label, &tr.token));
        }

        // 2. Non-PII NER. StackedNER::new() is zero-dependency (Pattern +
        //    Statistical layers); no ONNX cache hit at v0.1 corpus scale.
        let ner = StackedNER::new();
        match ner.extract_entities(plaintext, None) {
            Ok(ents) => {
                for e in ents {
                    if e.confidence.value() < 0.6 {
                        continue;
                    }
                    let tag = match &e.entity_type {
                        EntityType::Organization => "ORG",
                        EntityType::Location => "LOC",
                        EntityType::Person => continue, // vault path only
                        _ => continue,                  // skip Date/Money/etc. — not graph-useful
                    };
                    out.insert(canonicalize_entity(&e.text, tag, &self.cfg.entity_aliases));
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "anno_rag::memory::audit",
                    event = "extract_entities_failed",
                    "{e}"
                );
            }
        }

        let mut v: Vec<String> = out.into_iter().collect();
        v.sort(); // deterministic order on disk
        v
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
    pub async fn export_subject(&self, subject_ref: &str, format: ExportFormat) -> Result<Vec<u8>> {
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

    /// Save a memory according to [`AnnoRagConfig::memory_ner_mode`].
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
        match self.cfg.memory_ner_mode {
            MemoryNerMode::Sync => self.save_memory_sync(text, kind, session_id).await,
            MemoryNerMode::Async | MemoryNerMode::Disabled => {
                self.save_memory_fast(
                    crate::memory::MemoryId::new(),
                    text,
                    kind,
                    session_id,
                    self.cfg.memory_ner_mode,
                )
                .await
            }
        }
    }

    /// Legacy memory-save path: detect PII, pseudonymize with the vault,
    /// embed the tokenized text, and persist as a Memory row.
    ///
    /// # Errors
    /// Returns [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] depending on which layer fails.
    pub async fn save_memory_sync(
        &self,
        text: &str,
        kind: Option<crate::memory::MemoryKind>,
        session_id: Option<String>,
    ) -> Result<SavedMemory> {
        let entities = self.detector_get_or_init()?.detect(text)?;
        let (tokenized, token_refs) = self.vault.pseudonymize_with_refs(text, &entities).await?;

        let mut embedding = self
            .embedder()
            .await?
            .embed_batch(std::slice::from_ref(&tokenized))?;
        let embedding = embedding
            .pop()
            .ok_or_else(|| Error::Embed("embed_batch returned no vector for memory".into()))?;

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
            entity_refs: self.extract_entities(text, &token_refs),
        };

        // v0.2 T4: conflict resolver — only Preference + Reference can
        // auto-invalidate prior rows. Facts + Context are append-only.
        let mut invalidated_ids: Vec<String> = Vec::new();
        if matches!(
            m.kind,
            crate::memory::MemoryKind::Preference | crate::memory::MemoryKind::Reference
        ) {
            let candidates = self
                .store
                .memory_candidates_for_conflict(&m.entity_refs, m.session_id.as_deref())
                .await?;
            for prior in &candidates {
                if crate::conflict::resolves_conflict(
                    &m,
                    prior,
                    self.cfg.conflict_cosine_threshold as f32,
                ) {
                    self.store
                        .memory_update_valid_to(&prior.id, m.created_at)
                        .await?;
                    invalidated_ids.push(prior.id.as_string());
                }
            }
        }

        self.store.memory_insert(&m).await?;

        Ok(SavedMemory {
            id,
            stored_text: tokenized.clone(),
            redacted_text: tokenized,
            token_refs,
            entity_refs: m.entity_refs,
            invalidated_ids,
            ner_mode: MemoryNerMode::Sync,
        })
    }

    /// Fast `memory_save` path for disabled/async NER modes. Stores raw text
    /// with an embedding and leaves NER fields empty for a later enrichment.
    ///
    /// # Errors
    /// Returns [`Error::Embed`] / [`Error::Store`] depending on which layer fails.
    pub async fn save_memory_fast(
        &self,
        id: crate::memory::MemoryId,
        text: &str,
        kind: Option<crate::memory::MemoryKind>,
        session_id: Option<String>,
        ner_mode: MemoryNerMode,
    ) -> Result<SavedMemory> {
        let mut embedding = self.embedder().await?.embed_batch(&[text.to_string()])?;
        let embedding = embedding
            .pop()
            .ok_or_else(|| Error::Embed("embed_batch returned no vector for memory".into()))?;

        let now = chrono::Utc::now();
        let m = crate::memory::Memory {
            id: id.clone(),
            session_id,
            kind: kind.unwrap_or(crate::memory::MemoryKind::Context),
            text: text.to_string(),
            created_at: now,
            accessed_at: now,
            valid_from: now,
            valid_to: None,
            embedding,
            token_refs: Vec::new(),
            entity_refs: Vec::new(),
        };

        self.store.memory_insert(&m).await?;

        Ok(SavedMemory {
            id,
            stored_text: text.to_string(),
            redacted_text: text.to_string(),
            token_refs: Vec::new(),
            entity_refs: Vec::new(),
            invalidated_ids: Vec::new(),
            ner_mode,
        })
    }

    /// Best-effort NER enrichment for a row inserted by [`Self::save_memory_fast`].
    ///
    /// # Errors
    /// Returns detector, vault, or store errors to the caller so the spawned
    /// task can log them without disturbing the already-saved row.
    pub async fn save_memory_ner_task(
        &self,
        id: crate::memory::MemoryId,
        text: String,
        kind: Option<crate::memory::MemoryKind>,
        session_id: Option<String>,
    ) -> Result<()> {
        let entities = self.detector_get_or_init()?.detect(&text)?;
        let (_tokenized, token_refs) = self.vault.pseudonymize_with_refs(&text, &entities).await?;
        let entity_refs = self.extract_entities(&text, &token_refs);

        if matches!(
            kind.unwrap_or(crate::memory::MemoryKind::Context),
            crate::memory::MemoryKind::Preference | crate::memory::MemoryKind::Reference
        ) {
            if let Some(current) = self.store.memory_get(&id).await? {
                let enriched = crate::memory::Memory {
                    token_refs: token_refs.clone(),
                    entity_refs: entity_refs.clone(),
                    ..current
                };
                let candidates = self
                    .store
                    .memory_candidates_for_conflict(&entity_refs, session_id.as_deref())
                    .await?;
                for prior in &candidates {
                    if prior.id == id {
                        continue;
                    }
                    if crate::conflict::resolves_conflict(
                        &enriched,
                        prior,
                        self.cfg.conflict_cosine_threshold as f32,
                    ) {
                        self.store
                            .memory_update_valid_to(&prior.id, enriched.created_at)
                            .await?;
                    }
                }
            }
        }

        self.store
            .memory_update_ner_fields(&id, token_refs, entity_refs)
            .await?;
        Ok(())
    }

    /// Guarantee the memories table is FTS-queryable before a hybrid
    /// recall:
    /// 1. Create the FTS index if absent (idempotent, cheap when built).
    /// 2. If memories were added since the last fold-in, `optimize()` so
    ///    the new rows are covered, then advance the watermark.
    ///
    /// This is the *only* path that keeps memory FTS current —
    /// `spawn_compaction_task` is not wired into any entrypoint.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if index build, count, or optimize fails.
    async fn ensure_memory_searchable(&self) -> Result<()> {
        use std::sync::atomic::Ordering;

        // (1) Idempotent: builds once when the table first has rows,
        // no-ops (count_rows + list_indices) thereafter.
        self.store.build_memories_fts_index().await?;

        // (2) Gate optimize on "rows added since last fold-in" so
        // steady-state recall (no new memories) pays only a count_rows.
        let live = self.store.memory_row_count().await?;
        let mark = self.memory_fts_watermark.load(Ordering::Relaxed);
        if live > mark {
            let min_age = std::time::Duration::from_secs(self.cfg.compaction_min_age_secs);
            self.store.optimize_memories(min_age).await?;
            self.memory_fts_watermark.store(live, Ordering::Relaxed);
        }
        Ok(())
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
        as_of: Option<chrono::DateTime<chrono::Utc>>,
        graph_expand: bool,
    ) -> Result<Vec<crate::memory::MemoryHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let (tokenized_query, _) = self.vault.pseudonymize_with_refs(query, &entities).await?;
        let query_vec = self.embedder().await?.embed_query(&tokenized_query)?;

        self.ensure_memory_searchable().await?;

        let mut raw = self
            .store
            .memories_hybrid_search(&query_vec, &tokenized_query, top_k.saturating_mul(2))
            .await?;

        if let Some(allowed) = &kinds {
            raw.retain(|h| allowed.contains(&h.kind));
        }
        if let Some(s) = &session_id {
            // Match the session OR rows with no session (cross-session
            // facts shouldn't be hidden by a per-session recall).
            raw.retain(|h| h.session_id.as_deref() == Some(s.as_str()) || h.session_id.is_none());
        }

        // Bi-temporal filter. as_of = Some(t) → point-in-time semantics
        // (valid_from <= t AND (valid_to IS NULL OR valid_to > t)).
        // as_of = None → "now": include only currently-valid rows.
        let t_us = as_of.unwrap_or_else(chrono::Utc::now).timestamp_micros();
        raw.retain(|r| r.valid_from_us <= t_us && r.valid_to_us.is_none_or(|v| v > t_us));

        raw.truncate(top_k);

        // Track which ids came from the hybrid arm so the graph-expand pass
        // can tag freshly added rows with HitProvenance::GraphExpand.
        let hybrid_ids: std::collections::HashSet<String> =
            raw.iter().map(|r| r.id.clone()).collect();

        // v0.2 T6: optional graph-expand post-pass. Pulls in memories that
        // share at least one entity with any top-k hit, bounded by
        // graph_per_hop_limit. Bi-temporal predicate already applied.
        if graph_expand {
            let frontier: std::collections::HashSet<String> =
                raw.iter().flat_map(|r| r.entity_refs.clone()).collect();
            if !frontier.is_empty() {
                let frontier_vec: Vec<String> = frontier.into_iter().collect();
                let extras = self
                    .store
                    .memory_filter_by_entities(&frontier_vec, as_of, self.cfg.graph_per_hop_limit)
                    .await?;
                let known: std::collections::HashSet<String> =
                    raw.iter().map(|r| r.id.clone()).collect();
                for m in extras {
                    let id = m.id.as_string();
                    if known.contains(&id) {
                        continue;
                    }
                    raw.push(crate::memory::MemoryHitRow {
                        id,
                        session_id: m.session_id.clone(),
                        text_tokenized: m.text.clone(),
                        kind: m.kind,
                        created_at: m.created_at.to_rfc3339(),
                        valid_from_us: m.valid_from.timestamp_micros(),
                        valid_to_us: m.valid_to.map(|t| t.timestamp_micros()),
                        entity_refs: m.entity_refs.clone(),
                        score: 0.0, // graph-expanded rows carry no RRF score
                    });
                }
            }
        }

        let mut out: Vec<crate::memory::MemoryHit> = Vec::with_capacity(raw.len());
        for row in raw {
            let from_hybrid = hybrid_ids.contains(&row.id);
            let rehydrated = self.rehydrate(&row.text_tokenized).await?;
            out.push(crate::memory::MemoryHit {
                id: row.id,
                text: rehydrated.text,
                kind: row.kind,
                created_at: row.created_at,
                valid_from: ts_us_to_rfc3339(row.valid_from_us),
                valid_to: row.valid_to_us.map(ts_us_to_rfc3339),
                entity_refs: row.entity_refs,
                score: row.score,
                via: if from_hybrid {
                    crate::memory::HitProvenance::Hybrid
                } else {
                    crate::memory::HitProvenance::GraphExpand
                },
            });
        }
        Ok(out)
    }

    /// Memory recall + cross-encoder rerank. Same contract as
    /// [`Pipeline::recall_memory`] plus a `pool_size` over-fetch and a
    /// rerank stage. `recall_memory` already returns rehydrated
    /// plaintext, so the cross-encoder scores `(query, hit.text)`
    /// directly — no extra vault round-trip.
    ///
    /// # Errors
    /// [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Rerank`] per failing layer.
    #[cfg(feature = "rerank")]
    #[allow(clippy::too_many_arguments)]
    pub async fn recall_memory_reranked(
        &self,
        query: &str,
        top_k: usize,
        session_id: Option<String>,
        kinds: Option<Vec<crate::memory::MemoryKind>>,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
        graph_expand: bool,
        pool_size: usize,
    ) -> Result<Vec<crate::memory::MemoryHit>> {
        let pool = pool_size.max(top_k).max(1);
        let mut hits = self
            .recall_memory(query, pool, session_id, kinds, as_of, graph_expand)
            .await?;
        if hits.is_empty() {
            return Ok(hits);
        }

        let passages: Vec<&str> = hits.iter().map(|h| h.text.as_str()).collect();
        let reranker = self.reranker().await?;
        let scores = reranker.score_pairs_batched(query, &passages, self.cfg.rerank_batch_size)?;

        let mut scored: Vec<(crate::memory::MemoryHit, f32)> = hits.drain(..).zip(scores).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(top_k)
            .map(|(mut h, s)| {
                h.score = s;
                h
            })
            .collect())
    }

    /// Two-hop graph recall over `entity_refs`.
    ///
    /// Canonicalises `seed_entity` (if not already a `pii:`/`ent:` form,
    /// treats it as a MISC named entity for canonicalisation). BFS from
    /// the seed for at most `max_hops` (capped by `cfg.graph_max_hops`,
    /// default 2), pulling at most `per_hop_limit` rows per hop (capped
    /// by `cfg.graph_per_hop_limit`, default 50).
    ///
    /// Returns the connected subgraph: entity nodes (with mention counts),
    /// memory-mediated edges, and rehydrated memories tagged
    /// `HitProvenance::GraphExpand`.
    ///
    /// Bi-temporal: `as_of` (defaults to "now") filters out rows whose
    /// `valid_to` ≤ the cutoff, so invalidated branches do not pollute
    /// the graph.
    ///
    /// # Errors
    /// Returns [`Error::Store`] / [`Error::Vault`] on backend failure.
    pub async fn graph_recall(
        &self,
        seed_entity: &str,
        max_hops: u8,
        per_hop_limit: usize,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<crate::memory::GraphRecallResult> {
        use crate::memory::{EntityNode, GraphRecallResult, HitProvenance, MemoryEdge, MemoryHit};
        use std::collections::{HashMap, HashSet};

        let max_hops = max_hops.min(self.cfg.graph_max_hops);
        let per_hop_limit = per_hop_limit.min(self.cfg.graph_per_hop_limit);

        // 1. Canonicalise seed. Pass-through if already a `pii:` / `ent:`
        //    form; otherwise treat as MISC named entity.
        let canonical_seed = if seed_entity.starts_with("ent:") || seed_entity.starts_with("pii:") {
            seed_entity.to_string()
        } else {
            crate::canonicalize::canonicalize_entity(seed_entity, "MISC", &self.cfg.entity_aliases)
        };

        let mut visited_entities: HashSet<String> = HashSet::new();
        visited_entities.insert(canonical_seed.clone());

        let mut memories_by_id: HashMap<String, crate::memory::Memory> = HashMap::new();
        let mut edges: Vec<MemoryEdge> = Vec::new();
        let mut frontier: Vec<String> = vec![canonical_seed.clone()];

        for _hop in 0..max_hops {
            if frontier.is_empty() {
                break;
            }
            let rows = self
                .store
                .memory_filter_by_entities(&frontier, as_of, per_hop_limit)
                .await?;
            let mut next_frontier: HashSet<String> = HashSet::new();
            for m in rows {
                let mid = m.id.as_string();
                if memories_by_id.contains_key(&mid) {
                    continue;
                }
                // Build edges from any frontier entity in this row to any other entity.
                for from in &m.entity_refs {
                    if !frontier.contains(from) {
                        continue;
                    }
                    for to in &m.entity_refs {
                        if from == to {
                            continue;
                        }
                        edges.push(MemoryEdge {
                            from: from.clone(),
                            via: mid.clone(),
                            to: to.clone(),
                        });
                        if !visited_entities.contains(to) {
                            next_frontier.insert(to.clone());
                        }
                    }
                }
                memories_by_id.insert(mid, m);
            }
            visited_entities.extend(next_frontier.iter().cloned());
            frontier = next_frontier.into_iter().collect();
        }

        // Mention counts per entity, then build node list.
        let mut mention_counts: HashMap<String, u32> = HashMap::new();
        for m in memories_by_id.values() {
            for e in &m.entity_refs {
                *mention_counts.entry(e.clone()).or_insert(0) += 1;
            }
        }
        let mut nodes: Vec<EntityNode> = mention_counts
            .into_iter()
            .map(|(id, c)| {
                let (kind, display) = entity_id_display(&id, &self.vault);
                EntityNode {
                    id,
                    display,
                    kind,
                    mention_count: c,
                }
            })
            .collect();
        nodes.sort_by(|a, b| {
            b.mention_count
                .cmp(&a.mention_count)
                .then_with(|| a.id.cmp(&b.id))
        });

        // Rehydrate memories, tagged GraphExpand.
        let mut memories: Vec<MemoryHit> = Vec::with_capacity(memories_by_id.len());
        for m in memories_by_id.into_values() {
            let r = self.rehydrate(&m.text).await?;
            memories.push(MemoryHit {
                id: m.id.as_string(),
                text: r.text,
                kind: m.kind,
                created_at: m.created_at.to_rfc3339(),
                valid_from: m.valid_from.to_rfc3339(),
                valid_to: m.valid_to.map(|t| t.to_rfc3339()),
                entity_refs: m.entity_refs.clone(),
                score: 0.0,
                via: HitProvenance::GraphExpand,
            });
        }

        // PII seeds: try to resolve to plaintext.
        let seed_resolved = canonical_seed
            .strip_prefix("pii:")
            .and_then(|rest| rest.split_once(':').map(|x| x.1.to_string()))
            .and_then(|tok| self.vault.lookup_blocking(&tok));

        Ok(GraphRecallResult {
            seed: canonical_seed,
            seed_resolved,
            nodes,
            edges,
            memories,
        })
    }

    /// Mark a memory row as invalidated at `at` (defaults to "now"). The
    /// row stays on disk (history-preserving), but `recall_memory` with
    /// `as_of >= at` will exclude it. Guarded by `valid_to IS NULL` so a
    /// double-invalidate is a no-op (returns `Ok(false)`).
    ///
    /// # Errors
    /// Returns [`Error::Store`] on update failure.
    pub async fn invalidate_memory(
        &self,
        id: &crate::memory::MemoryId,
        at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<bool> {
        let when = at.unwrap_or_else(chrono::Utc::now);
        self.store.memory_update_valid_to(id, when).await
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
            (Some(mid), None) => self.store.memory_get(&mid).await?.into_iter().collect(),
            (None, Some(q)) => {
                // Forget by query: scan currently-valid rows (as_of = now);
                // graph_expand off — forget is identity-anchored, not graph-anchored.
                let hits = self
                    .recall_memory(&q, limit, None, None, None, false)
                    .await?;
                let mut out = Vec::with_capacity(hits.len());
                for h in hits.iter().take(limit) {
                    let uid = uuid::Uuid::parse_str(&h.id)
                        .map_err(|e| Error::Memory(format!("bad id: {e}")))?;
                    if let Some(m) = self.store.memory_get(&crate::memory::MemoryId(uid)).await? {
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

fn legal_ingest_candidate_paths(
    folder: &Path,
    recursive: bool,
    output_dir: &Path,
) -> Vec<std::path::PathBuf> {
    let walker = if recursive {
        walkdir::WalkDir::new(folder).into_iter()
    } else {
        walkdir::WalkDir::new(folder).max_depth(1).into_iter()
    };
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in walker
        .filter_entry(|entry| !is_anno_generated_output(folder, entry.path(), output_dir))
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_supported_ingest_path(path) || is_anno_generated_output(folder, path, output_dir) {
            continue;
        }
        paths.push(path.to_path_buf());
    }
    paths.sort();
    paths
}

/// Privacy workspace candidate paths. Excludes generated output folders.
#[must_use]
pub fn privacy_candidate_paths(
    folder: &Path,
    recursive: bool,
    workspace_dir: &Path,
) -> Vec<std::path::PathBuf> {
    legal_ingest_candidate_paths(folder, recursive, workspace_dir)
}

fn is_supported_ingest_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
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
    )
}

fn is_anno_generated_output(source_root: &Path, path: &Path, output_dir: &Path) -> bool {
    if path.starts_with(output_dir) {
        return true;
    }
    if path != source_root
        && path.strip_prefix(source_root).ok().is_some_and(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .map(is_anno_generated_dir_name)
                    .unwrap_or(false)
            })
        })
    {
        return true;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains(".anon."))
        .unwrap_or(false)
}

fn is_anno_generated_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | ".anno" | ".anno-rag" | "vault"
    )
}

fn should_index_extracted_doc(extracted: &ingest::ExtractedDoc) -> bool {
    !extracted.ocr_status.is_deferred() && !extracted.chunks.is_empty()
}

fn cfg_for_ocr_budget(
    base: &AnnoRagConfig,
    budget: Option<Duration>,
    spent: Duration,
) -> AnnoRagConfig {
    let mut cfg = base.clone();
    if budget.is_some_and(|limit| spent >= limit) {
        cfg.ocr_mode = crate::config::OcrMode::Off;
        cfg.enable_ocr = false;
    }
    cfg
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

/// One page returned by [`Pipeline::list_memories`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListPage {
    /// Rehydrated hits, ordered by `created_at` DESC.
    pub items: Vec<crate::memory::MemoryHit>,
    /// Cursor to feed to the next call to get the following page. RFC 3339
    /// timestamp of the row immediately after the last item on this page.
    /// `None` when the result set is exhausted.
    pub next_cursor: Option<String>,
}

impl Pipeline {
    /// Cursor-paginated list of memories. Filters by optional session +
    /// kind; orders by `created_at` DESC; rehydrates each row's text.
    ///
    /// # Errors
    /// Returns [`Error::Store`] / [`Error::Vault`] on backend failures.
    pub async fn list_memories(
        &self,
        session_id: Option<String>,
        kind: Option<crate::memory::MemoryKind>,
        limit: usize,
        cursor: Option<String>,
    ) -> Result<ListPage> {
        let kind_str = kind.map(|k| match k {
            crate::memory::MemoryKind::Fact => "fact",
            crate::memory::MemoryKind::Preference => "preference",
            crate::memory::MemoryKind::Reference => "reference",
            crate::memory::MemoryKind::Context => "context",
        });
        let (rows, next_cursor) = self
            .store
            .memory_list(session_id.as_deref(), kind_str, limit, cursor.as_deref())
            .await?;

        let mut items: Vec<crate::memory::MemoryHit> = Vec::with_capacity(rows.len());
        for m in rows {
            let rehydrated = self.rehydrate(&m.text).await?;
            items.push(crate::memory::MemoryHit {
                id: m.id.as_string(),
                text: rehydrated.text,
                kind: m.kind,
                created_at: m.created_at.to_rfc3339(),
                valid_from: m.valid_from.to_rfc3339(),
                valid_to: m.valid_to.map(|v| v.to_rfc3339()),
                entity_refs: m.entity_refs,
                score: 0.0,
                via: crate::memory::HitProvenance::Hybrid,
            });
        }
        Ok(ListPage { items, next_cursor })
    }

    /// Run a single compaction pass on the memories table. Reclaims
    /// tombstoned bytes from prior `forget_memory` calls. Suitable for
    /// admin-triggered compaction; the background ticker (see
    /// [`Self::spawn_compaction_task`]) runs this on a 24h interval by
    /// default.
    ///
    /// # Errors
    /// Returns [`Error::Store`] on optimize failure.
    pub async fn compact_now(&self) -> Result<()> {
        let min_age = std::time::Duration::from_secs(self.cfg.compaction_min_age_secs);
        self.store.optimize_memories(min_age).await
    }

    /// Spawn a tokio task that calls [`Self::compact_now`] on a fixed
    /// interval (configurable via `compaction_interval_secs`, default 24h).
    /// The first tick is skipped so startup is cheap.
    ///
    /// Failures are logged at `target = "anno_rag::memory::audit"` with
    /// `event = "compaction_failed"` — the ticker continues. The returned
    /// `JoinHandle` is detached unless the caller stores it; the tokio
    /// runtime cancels detached tasks at shutdown, which is acceptable
    /// for v0.1.
    pub fn spawn_compaction_task(self: std::sync::Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = std::time::Duration::from_secs(self.cfg.compaction_interval_secs);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip the immediate fire
            loop {
                ticker.tick().await;
                if let Err(e) = self.compact_now().await {
                    tracing::warn!(
                        target: "anno_rag::memory::audit",
                        event = "compaction_failed",
                        "{e}"
                    );
                }
            }
        })
    }
}

/// Receipt returned by [`Pipeline::save_memory`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SavedMemory {
    /// Newly minted memory id.
    pub id: crate::memory::MemoryId,
    /// Text actually persisted. Raw for async/disabled, tokenized for sync.
    pub stored_text: String,
    /// Deprecated alias kept for MCP clients for one release cycle.
    pub redacted_text: String,
    /// `(category, token)` pairs minted for the GDPR Art. 17 cascade.
    pub token_refs: Vec<crate::memory::TokenRef>,
    /// Canonicalised entity references attached to the new row (v0.2 T2).
    /// Surfaces what got LabelList-indexed for the future graph traversal.
    pub entity_refs: Vec<String>,
    /// Ids of prior `Preference` / `Reference` memories the conflict
    /// resolver auto-invalidated on this save (v0.2 T4). Empty for
    /// `Fact` / `Context` saves.
    pub invalidated_ids: Vec<String>,
    /// NER mode used for the synchronous portion of this save.
    pub ner_mode: MemoryNerMode,
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

/// Summary returned by [`Pipeline::drain_enrichment_backlog`].
#[derive(Debug, Clone, Default)]
pub struct EnrichmentDrainReport {
    /// Documents successfully re-enriched on this drain pass.
    pub ok: usize,
    /// Documents that failed again and remain pending.
    pub still_pending: usize,
    /// Documents that exhausted the retry budget (≥ 5 attempts) and are
    /// permanently marked `failed_max_retries`.
    pub failed_max_retries: usize,
}

impl Pipeline {
    /// Re-run legal enrichment for every document whose last attempt was
    /// recorded as `pending`. Caps at `max_docs` per call; call repeatedly
    /// to drain a large backlog.
    ///
    /// Docs that have accumulated ≥ 5 attempts are permanently marked
    /// `failed_max_retries` and skipped on future drains. All errors during
    /// individual re-enrichment are non-fatal and recorded in the status
    /// table; the method only propagates errors from the status store itself.
    ///
    /// # Errors
    /// Returns [`Error::Legal`] when the status table cannot be read or written.
    pub async fn drain_enrichment_backlog(
        &self,
        max_docs: usize,
    ) -> crate::error::Result<EnrichmentDrainReport> {
        let pending = self.enrichment_status.list_pending(max_docs).await?;
        let mut report = EnrichmentDrainReport::default();
        for d in pending {
            if d.attempts >= 5 {
                self.enrichment_status
                    .mark_failed_max_retries(
                        d.doc_id,
                        d.last_error.as_deref().unwrap_or("exhausted"),
                    )
                    .await?;
                report.failed_max_retries += 1;
                continue;
            }
            let chunks = self.store.chunks_by_doc(d.doc_id).await?;
            match self.reenrich_doc(d.doc_id, &chunks).await {
                Ok(()) => {
                    self.enrichment_status.mark_ok(d.doc_id).await?;
                    report.ok += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        doc_id = %d.doc_id,
                        attempt = d.attempts + 1,
                        error = %e,
                        "legal re-enrichment failed"
                    );
                    self.enrichment_status
                        .mark_pending(d.doc_id, d.chunk_count, &e.to_string())
                        .await?;
                    report.still_pending += 1;
                }
            }
        }
        Ok(report)
    }

    /// Re-run rules-only legal enrichment for a document's already-stored
    /// pseudonymized chunks. The vault forward map is empty because canonical
    /// references are already embedded in the pseudonymized text (Phase 1
    /// limitation; a per-doc vault snapshot is introduced in Stage C3).
    async fn reenrich_doc(
        &self,
        doc_id: uuid::Uuid,
        chunks: &[crate::store::SearchHit],
    ) -> crate::error::Result<()> {
        let fwd = crate::legal::rules::VaultForwardMap {
            alias_to_canonical: Default::default(),
        };
        let mut rows = Vec::with_capacity(chunks.len());
        for c in chunks {
            let legal_ents: Vec<crate::legal::types::LegalEntity> = Vec::new();
            let (row, _facts, _nodes, _edges) = self.legal_enricher.enrich_one(
                c.chunk_id,
                doc_id,
                &c.text_pseudo,
                &legal_ents,
                &fwd,
            );
            rows.push(row);
        }
        self.legal_store.upsert(&rows).await?;
        Ok(())
    }

    /// Consolidate all post-ingest index builds and graph compaction into one
    /// call. Idempotent — safe to call multiple times.
    ///
    /// Steps (all individually non-fatal):
    /// 1. Build the IVF_HNSW_SQ vector index on `chunks.vector` once the row
    ///    count crosses `vector_index_threshold`.
    /// 2. Build the French-tokenized FTS index on `chunks.text_pseudo`.
    /// 3. Run `setup_indexes` on the legal enrichment store (scalar + FTS).
    /// 4. Compact the legal knowledge graph.
    ///
    /// # Errors
    /// Only propagates errors from step 1 (vector index build); all other
    /// steps log warnings and continue.
    pub async fn optimize_after_ingest(&self) -> Result<()> {
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
            Err(e) => tracing::warn!(error = %e, "vector index build skipped"),
        }
        match self.store.maybe_build_fts_index().await {
            Ok(true) => tracing::info!("built French FTS index on chunks.text_pseudo"),
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "FTS index build skipped"),
        }
        if let Err(e) = self.legal_store.setup_indexes().await {
            tracing::warn!(error = %e, "legal index setup skipped");
        }
        if let Err(e) = self.legal_kg.compact().await {
            tracing::warn!(error = %e, "KG compact skipped");
        }
        Ok(())
    }

    /// Execute a named graph traversal intent against the legal knowledge graph.
    ///
    /// Dispatches a typed graph intent against the configured graph backend.
    ///
    /// # Errors
    /// Returns [`Error::Graph`] propagated from the KG backend.
    pub async fn legal_graph_query(
        &self,
        intent: crate::legal::query::GraphIntent,
    ) -> Result<crate::legal::query::GraphQueryResult> {
        crate::legal::query::run_intent(self.legal_kg.as_ref(), intent).await
    }

    /// Legal-filtered hybrid search. Pseudonymizes the query through the vault,
    /// embeds it, and — when `filters` specify at least one predicate — restricts
    /// the vector search to the chunk UUIDs that pass the legal store's SQL
    /// filters. Returns up to `top_k` hits with their pseudonymized text.
    ///
    /// # Errors
    /// Returns [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Legal`] per failing layer.
    pub async fn legal_search(
        &self,
        query: &str,
        top_k: usize,
        filters: crate::legal::types::LegalSearchFilters,
    ) -> Result<Vec<crate::legal::types::LegalSearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;

        let chunk_hits = if filters.has_any_filter() {
            let allowed = self
                .legal_store
                .filter_chunk_ids(&filters, top_k.saturating_mul(20).max(100))
                .await?;
            self.store
                .search_filtered_to_chunks(&pseudo_q, &qv, top_k, &allowed)
                .await?
        } else {
            self.store.search(&pseudo_q, &qv, top_k).await?
        };

        Ok(chunk_hits
            .into_iter()
            .map(|h| crate::legal::types::LegalSearchHit {
                chunk_id: h.chunk_id,
                doc_id: h.doc_id,
                text_pseudo: h.text_pseudo,
                score: h.score,
                enrichment: None,
            })
            .collect())
    }

    /// Legal hybrid search restricted to an explicit document set.
    pub async fn legal_search_scoped(
        &self,
        query: &str,
        top_k: usize,
        filters: crate::legal::types::LegalSearchFilters,
        allowed_doc_ids: &[uuid::Uuid],
    ) -> Result<Vec<crate::legal::types::LegalSearchHit>> {
        if allowed_doc_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;
        let corpus_chunk_ids = self.store.chunk_ids_for_docs(allowed_doc_ids).await?;
        let allowed = if filters.has_any_filter() {
            let business = self
                .legal_store
                .filter_chunk_ids(&filters, top_k.saturating_mul(20).max(100))
                .await?;
            intersect_uuids(&corpus_chunk_ids, &business)
        } else {
            corpus_chunk_ids
        };
        let chunk_hits = self
            .store
            .search_filtered_to_chunks(&pseudo_q, &qv, top_k, &allowed)
            .await?;

        Ok(chunk_hits
            .into_iter()
            .map(|h| crate::legal::types::LegalSearchHit {
                chunk_id: h.chunk_id,
                doc_id: h.doc_id,
                text_pseudo: h.text_pseudo,
                score: h.score,
                enrichment: None,
            })
            .collect())
    }

    /// Rehydrate a citation span from a stored pseudonymized chunk.
    ///
    /// Fetches the chunk by `chunk_id`, slices `byte_start..byte_end` from its
    /// pseudonymized text, then rehydrates the resulting span through the vault.
    ///
    /// # Errors
    /// Returns [`Error::Store`] when the chunk is unknown or the byte range is
    /// out of bounds; [`Error::Vault`] on rehydration failure.
    pub async fn legal_rehydrate_citation(
        &self,
        chunk_id: uuid::Uuid,
        byte_start: u32,
        byte_end: u32,
    ) -> Result<RehydratedText> {
        if byte_start >= byte_end {
            return Err(Error::Store("byte_start must be < byte_end".into()));
        }
        let hit = self
            .store
            .chunk_by_id(chunk_id)
            .await?
            .ok_or_else(|| Error::Store(format!("unknown chunk_id: {chunk_id}")))?;
        let span = hit
            .text_pseudo
            .get(byte_start as usize..byte_end as usize)
            .ok_or_else(|| Error::Store("offsets not valid UTF-8 boundary".into()))?;
        self.rehydrate(span).await
    }

    // ── D2: extract workflows ─────────────────────────────────────────────

    /// Extract a contract review grid from the KG for `doc_id`.
    ///
    /// # Errors
    /// Propagates graph backend errors.
    pub async fn legal_extract_contract(
        &self,
        doc_id: &str,
    ) -> Result<crate::legal::extract::ContractReview> {
        crate::legal::extract::extract_contract(self.legal_kg.as_ref(), doc_id).await
    }

    /// Extract a case-file review grid from the KG for `dossier_id`.
    ///
    /// # Errors
    /// Propagates graph backend errors.
    pub async fn legal_extract_case_file(
        &self,
        dossier_id: &str,
    ) -> Result<crate::legal::extract::CaseFileReview> {
        crate::legal::extract::extract_case_file(self.legal_kg.as_ref(), dossier_id).await
    }

    /// Retrieve the procedural timeline for `dossier_id`.
    ///
    /// # Errors
    /// Propagates graph backend errors.
    pub async fn legal_timeline(
        &self,
        dossier_id: &str,
    ) -> Result<crate::legal::extract::ProceduralTimeline> {
        crate::legal::extract::timeline(self.legal_kg.as_ref(), dossier_id).await
    }

    /// Retrieve risk findings for a document or dossier.
    ///
    /// # Errors
    /// Propagates graph backend errors.
    pub async fn legal_risk_review(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<crate::legal::extract::RiskReview> {
        crate::legal::extract::risk_review(self.legal_kg.as_ref(), scope_id, is_dossier).await
    }

    // ── D3: mandatory-clause audit ────────────────────────────────────────

    /// Run the mandatory-clause checklist for all chunks of `doc_id`.
    ///
    /// Fetches all chunks of the document from the store, concatenates their
    /// pseudonymized text, dispatches to the per-doctype evaluator, and
    /// returns the check results with an aggregate status.
    ///
    /// # Errors
    /// Propagates store errors on chunk fetch.
    pub async fn legal_mandatory_clause_audit(
        &self,
        doc_id: Uuid,
        doc_type: &str,
    ) -> Result<MandatoryAuditResult> {
        let chunks = self.store.chunks_by_doc(doc_id).await?;
        let full_text: String = chunks
            .iter()
            .map(|c| c.text_pseudo.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let checks = crate::legal::mandatory::evaluate_doc(doc_type, &full_text);
        let status = crate::legal::mandatory::aggregate_status(&checks);
        Ok(MandatoryAuditResult {
            doc_id,
            doc_type: doc_type.to_string(),
            checks,
            status,
        })
    }

    // ── D4: prescription check ────────────────────────────────────────────

    /// Compute the French prescription deadline for the given category and
    /// anchor date, taking `interrupting_events` into account.
    ///
    /// Returns `None` when the category is not found in the rule table.
    #[must_use]
    pub fn legal_prescription_check(
        category: &str,
        event_date: chrono::DateTime<chrono::Utc>,
        interrupting_events: &[crate::legal::prescription::InterruptingEvent],
    ) -> Option<crate::legal::prescription::PrescriptionResult> {
        crate::legal::prescription::compute_prescription(category, event_date, interrupting_events)
    }

    // ── Chunk accessors for anno-rag-tabular ─────────────────────────────

    /// Return all indexed chunks for `doc_id` in document order.
    ///
    /// Thin delegation to [`anno_rag::store::Store::chunks_by_doc`].
    ///
    /// # Errors
    /// Returns [`Error`] if the LanceDB scan fails.
    pub async fn chunks_by_doc(&self, doc_id: uuid::Uuid) -> Result<Vec<crate::store::SearchHit>> {
        self.store.chunks_by_doc(doc_id).await
    }

    /// Return a single chunk by its UUID.
    ///
    /// Thin delegation to [`anno_rag::store::Store::chunk_by_id`].
    ///
    /// # Errors
    /// Returns [`Error`] if the LanceDB scan fails.
    pub async fn chunk_by_id(
        &self,
        chunk_id: uuid::Uuid,
    ) -> Result<Option<crate::store::SearchHit>> {
        self.store.chunk_by_id(chunk_id).await
    }

    // ── D5: field validation ──────────────────────────────────────────────

    /// Record a human (or automated) validation of an extracted fact.
    ///
    /// Writes a `Validation` node to the KG linked to the source chunk.
    ///
    /// # Errors
    /// Propagates KG errors.
    pub async fn legal_validate_field(
        &self,
        chunk_id: Uuid,
        field_name: String,
        action: ValidationAction,
        corrected_value: Option<String>,
        note: Option<String>,
        actor: Option<String>,
    ) -> Result<ValidationAck> {
        let validation_id = Uuid::new_v4();
        let validated_at = chrono::Utc::now();
        let action_str = action.as_str().to_string();

        crate::legal::audit::audit_validation(actor.as_deref(), &action_str);

        let mut nodes = crate::legal::kg::NodeBatch::new();
        nodes.nodes.push(crate::legal::kg::NodeWrite::Validation {
            validation_id,
            chunk_id,
            field_name: field_name.clone(),
            action: action_str.clone(),
            corrected_value: corrected_value.clone(),
            note: note.clone(),
            validated_at,
            actor: actor.clone(),
        });

        let mut edges = crate::legal::kg::EdgeBatch::new();
        edges.edges.push(crate::legal::kg::EdgeWrite {
            from_label: "Validation",
            from_key: validation_id.to_string(),
            to_label: "Chunk",
            to_key: chunk_id.to_string(),
            edge_type: "VALIDATES",
            props: std::collections::HashMap::new(),
        });

        self.legal_kg.upsert_batch(&nodes, &edges).await?;

        Ok(ValidationAck {
            validation_id,
            chunk_id,
            field_name,
            action: action_str,
            corrected_value,
            validated_at,
        })
    }
}

// ── D3 result type ────────────────────────────────────────────────────────────

/// Result of a mandatory-clause audit for one document.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MandatoryAuditResult {
    /// Document UUID audited.
    pub doc_id: Uuid,
    /// Document type used for checklist selection.
    pub doc_type: String,
    /// Per-requirement check results.
    pub checks: Vec<crate::legal::mandatory::MandatoryCheck>,
    /// Aggregate status: `"complete"`, `"partial"`, or `"missing"`.
    pub status: String,
}

// ── D5 types ──────────────────────────────────────────────────────────────────

/// Action taken on a validated field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationAction {
    /// Confirm the extracted value is correct.
    Confirm,
    /// Reject the extracted value as wrong.
    Reject,
    /// Replace the extracted value with a corrected one.
    Correct,
}

impl ValidationAction {
    /// Stable string representation stored in the KG.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Reject => "reject",
            Self::Correct => "correct",
        }
    }
}

/// Acknowledgement returned by [`Pipeline::legal_validate_field`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationAck {
    /// Freshly generated validation UUID.
    pub validation_id: Uuid,
    /// Chunk that contains the validated fact.
    pub chunk_id: Uuid,
    /// Field name validated.
    pub field_name: String,
    /// Action that was recorded.
    pub action: String,
    /// Corrected value, if action was `"correct"`.
    pub corrected_value: Option<String>,
    /// Timestamp at which the validation was recorded.
    pub validated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a Pipeline rooted at `dir` (tempdir-friendly). `Pipeline::new`
    /// opens LanceDB, which takes ~30 s — these tests are gated behind
    /// `--ignored` to keep `cargo test` snappy on every run.
    async fn pipeline_in(dir: &Path) -> Pipeline {
        let cfg = AnnoRagConfig {
            data_dir: dir.to_path_buf(),
            ..Default::default()
        };
        Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline opens")
    }

    async fn memory_pipeline_in(dir: &Path, mode: MemoryNerMode) -> Pipeline {
        let cfg = AnnoRagConfig {
            data_dir: dir.to_path_buf(),
            memory_ner_mode: mode,
            ..Default::default()
        };
        Pipeline::new(cfg, [0u8; 32]).await.expect("pipeline opens")
    }

    #[tokio::test]
    #[ignore = "opens Pipeline/LanceDB; run with --ignored"]
    async fn forget_legal_folder_path_on_empty_store_is_noop() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pipeline = match Pipeline::new(
            AnnoRagConfig {
                data_dir: dir.path().to_path_buf(),
                ..Default::default()
            },
            [0u8; 32],
        )
        .await
        {
            Ok(pipeline) => pipeline,
            Err(e) => {
                eprintln!("skipping: {e}");
                return;
            }
        };

        let removed = pipeline
            .forget_legal_folder_path("C:/does/not/exist")
            .await
            .expect("forget folder");
        assert_eq!(removed, 0);
    }

    #[tokio::test]
    #[ignore = "loads embedder and opens LanceDB; opt in via --ignored"]
    async fn save_memory_async_row_exists_before_ner() {
        let tmp = tempfile::tempdir().unwrap();
        let p = memory_pipeline_in(tmp.path(), MemoryNerMode::Async).await;

        let saved = p
            .save_memory(
                "Antoine Lefebvre approved the report.",
                Some(crate::memory::MemoryKind::Context),
                Some("s1".into()),
            )
            .await
            .expect("save memory");

        let row = p
            .store
            .memory_get(&saved.id)
            .await
            .expect("get row")
            .expect("row exists before background NER");
        assert_eq!(saved.ner_mode, MemoryNerMode::Async);
        assert_eq!(row.text, "Antoine Lefebvre approved the report.");
        assert!(row.token_refs.is_empty());
        assert!(row.entity_refs.is_empty());
        assert!(!p.detector_loaded());
    }

    #[tokio::test]
    #[ignore = "loads embedder, detector, and opens LanceDB; opt in via --ignored"]
    async fn save_memory_async_ner_enriches_row() {
        let tmp = tempfile::tempdir().unwrap();
        let p = memory_pipeline_in(tmp.path(), MemoryNerMode::Async).await;
        let text = "Send the contract draft to c.moreau@nexacorp.fr before end of day.";

        let saved = p
            .save_memory(
                text,
                Some(crate::memory::MemoryKind::Reference),
                Some("s1".into()),
            )
            .await
            .expect("save memory");
        assert!(saved.token_refs.is_empty());
        assert!(saved.entity_refs.is_empty());

        p.save_memory_ner_task(
            saved.id.clone(),
            text.to_string(),
            Some(crate::memory::MemoryKind::Reference),
            Some("s1".into()),
        )
        .await
        .expect("ner task");

        let row = p
            .store
            .memory_get(&saved.id)
            .await
            .expect("get row")
            .expect("row exists after NER");
        assert_eq!(row.text, text);
        assert!(row.token_refs.iter().any(|r| r.token.starts_with("EMAIL_")));
        assert!(row.entity_refs.iter().any(|r| r.contains("EMAIL_")));
    }

    #[tokio::test]
    #[ignore = "loads embedder and opens LanceDB; opt in via --ignored"]
    async fn save_memory_disabled_skips_detector_and_refs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = memory_pipeline_in(tmp.path(), MemoryNerMode::Disabled).await;

        let saved = p
            .save_memory(
                "Antoine Lefebvre approved the report.",
                Some(crate::memory::MemoryKind::Context),
                None,
            )
            .await
            .expect("save memory");

        assert_eq!(saved.ner_mode, MemoryNerMode::Disabled);
        assert_eq!(saved.stored_text, "Antoine Lefebvre approved the report.");
        assert!(saved.token_refs.is_empty());
        assert!(saved.entity_refs.is_empty());
        assert!(!p.detector_loaded());
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
    fn doc_uuid_is_deterministic_and_content_sensitive() {
        let a1 = super::doc_uuid(b"hello world");
        let a2 = super::doc_uuid(b"hello world");
        let b = super::doc_uuid(b"hello world!");
        assert_eq!(a1, a2, "same bytes => same doc_id");
        assert_ne!(a1, b, "different bytes => different doc_id");
    }

    #[test]
    fn scoped_doc_uuid_distinguishes_same_bytes_across_corpora() {
        use anno_corpus_core::{ContentId, CorpusId};

        let bytes = b"same contract";
        let content = ContentId::from_bytes(bytes);
        let corpus_a = CorpusId::from_normalized_root("c:/clients/a");
        let corpus_b = CorpusId::from_normalized_root("c:/clients/b");

        let doc_a = super::scoped_doc_uuid(corpus_a, "contract.pdf", &content);
        let doc_b = super::scoped_doc_uuid(corpus_b, "contract.pdf", &content);

        assert_ne!(doc_a, doc_b);
        assert_eq!(super::doc_uuid(bytes), super::doc_uuid(bytes));
    }

    #[test]
    fn legal_ingest_candidate_paths_skip_anno_generated_outputs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("anon");
        std::fs::create_dir_all(&out).expect("anon dir");
        std::fs::write(dir.path().join("source.md"), b"# source").expect("source");
        std::fs::write(out.join("source.anon.md"), b"# generated").expect("generated");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested dir");
        std::fs::write(
            dir.path().join("nested").join("contract.anon.md"),
            b"# generated nested",
        )
        .expect("generated nested");
        std::fs::create_dir_all(dir.path().join("nested").join("anon")).expect("nested anon dir");
        std::fs::write(
            dir.path().join("nested").join("anon").join("ignored.md"),
            b"# generated nested anon",
        )
        .expect("generated nested anon");
        std::fs::create_dir_all(dir.path().join("outputs")).expect("outputs dir");
        std::fs::write(
            dir.path().join("outputs").join("kept.md"),
            b"# legitimate client output",
        )
        .expect("client outputs");
        std::fs::create_dir_all(dir.path().join(".anno")).expect(".anno dir");
        std::fs::write(
            dir.path().join(".anno").join("ignored.md"),
            b"# generated .anno",
        )
        .expect("generated .anno");
        std::fs::create_dir_all(dir.path().join(".anno-rag")).expect(".anno-rag dir");
        std::fs::write(
            dir.path().join(".anno-rag").join("ignored.md"),
            b"# generated .anno-rag",
        )
        .expect("generated .anno-rag");
        std::fs::write(dir.path().join("nested").join("contract.md"), b"# contract")
            .expect("nested source");

        let paths = legal_ingest_candidate_paths(dir.path(), true, &out);
        let names: Vec<String> = paths
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap()
                    .to_string()
            })
            .collect();

        assert_eq!(names, vec!["contract.md", "kept.md", "source.md"]);
    }

    #[test]
    fn generated_output_filter_skips_vault_workspace() {
        let root = std::path::Path::new("C:/Clients/Matter X");
        assert!(super::is_anno_generated_output(
            root,
            std::path::Path::new("C:/Clients/Matter X/vault/01-working-documents/a.docx"),
            std::path::Path::new("C:/Clients/Matter X/anon"),
        ));
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

    #[test]
    fn saved_memory_serializes_stored_text_and_legacy_redacted_text() {
        let r = SavedMemory {
            id: crate::memory::MemoryId::new(),
            stored_text: "Antoine Lefebvre approved the report.".into(),
            redacted_text: "Antoine Lefebvre approved the report.".into(),
            token_refs: Vec::new(),
            entity_refs: Vec::new(),
            invalidated_ids: Vec::new(),
            ner_mode: crate::config::MemoryNerMode::Async,
        };

        let s = serde_json::to_string(&r).unwrap();

        assert!(s.contains("\"stored_text\":\"Antoine Lefebvre approved the report.\""));
        assert!(s.contains("\"redacted_text\":\"Antoine Lefebvre approved the report.\""));
        assert!(s.contains("\"ner_mode\":\"async\""));
    }

    #[test]
    fn deferred_or_empty_extractions_are_not_indexable() {
        let deferred = ingest::ExtractedDoc {
            source_path: "scan.pdf".into(),
            content: String::new(),
            chunks: Vec::new(),
            class: ingest::DocClass::ScannedPdf,
            ocr_status: ingest::OcrStatus::Deferred(ingest::OcrDeferredReason::Disabled),
        };
        assert!(!should_index_extracted_doc(&deferred));

        let empty_text = ingest::ExtractedDoc {
            source_path: "empty.txt".into(),
            content: String::new(),
            chunks: Vec::new(),
            class: ingest::DocClass::Empty,
            ocr_status: ingest::OcrStatus::NotRequired,
        };
        assert!(!should_index_extracted_doc(&empty_text));

        let indexable = ingest::ExtractedDoc {
            source_path: "doc.md".into(),
            content: "Article 1".into(),
            chunks: vec![ingest::ExtractedChunk {
                idx: 0,
                text: "Article 1".into(),
                char_start: 0,
                char_end: 9,
                page: None,
                page_end: None,
                heading_context: Vec::new(),
            }],
            class: ingest::DocClass::TextLayer,
            ocr_status: ingest::OcrStatus::NotRequired,
        };
        assert!(should_index_extracted_doc(&indexable));
    }

    #[test]
    #[allow(deprecated)] // NER_MODEL_ID kept for backward compat; test verifies the static value
    fn predetected_legal_extractor_reports_gliner_model_id() {
        let extractor = PreDetectedLegalExtractor;
        assert_eq!(extractor.model_id(), crate::detect::NER_MODEL_ID);
        assert_eq!(extractor.extractor_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn exhausted_ocr_budget_disables_runtime_ocr_for_next_doc() {
        let cfg = AnnoRagConfig {
            ocr_mode: crate::config::OcrMode::AutoEmbedded,
            ocr_batch_budget_secs: Some(10),
            ..Default::default()
        };

        let next_cfg =
            cfg_for_ocr_budget(&cfg, Some(Duration::from_secs(10)), Duration::from_secs(10));

        assert_eq!(next_cfg.effective_ocr_mode(), crate::config::OcrMode::Off);
    }

    /// Drain an empty backlog — should return an all-zero report without
    /// hitting LanceDB (the enrichment_status table will be empty).
    #[tokio::test]
    #[ignore = "Pipeline::new opens LanceDB (~30s) — opt in via --ignored"]
    async fn drain_enrichment_backlog_empty_backlog_is_noop() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let p = pipeline_in(dir.path()).await;
        let report = p.drain_enrichment_backlog(64).await.expect("drain ok");
        assert_eq!(report.ok, 0);
        assert_eq!(report.still_pending, 0);
        assert_eq!(report.failed_max_retries, 0);
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

/// Decode a canonical entity id into `(kind, display)` for the
/// graph-recall wire shape. `display` is the rehydrated plaintext for
/// PII tokens (best-effort — falls back to the token id if the vault
/// is currently locked), or the lowercase tail for named entities.
fn entity_id_display(
    id: &str,
    vault: &crate::vault::Vault,
) -> (crate::memory::EntityKindWire, String) {
    use crate::memory::EntityKindWire;
    if let Some(rest) = id.strip_prefix("pii:") {
        // pii:<LABEL>:<TOKEN>
        let token = rest.split_once(':').map(|x| x.1).unwrap_or("");
        let display = vault
            .lookup_blocking(token)
            .unwrap_or_else(|| token.to_string());
        (EntityKindWire::PiiToken, display)
    } else if let Some(rest) = id.strip_prefix("ent:") {
        let display = rest
            .split_once(':')
            .map(|x| x.1)
            .unwrap_or(rest)
            .to_string();
        (EntityKindWire::NamedEntity, display)
    } else {
        (EntityKindWire::NamedEntity, id.to_string())
    }
}

/// Convert a microsecond UTC timestamp into an RFC 3339 string. Used by
/// the v0.2 `MemoryHit` builders to surface `valid_from` / `valid_to` in
/// the form the MCP client expects.
fn ts_us_to_rfc3339(micros: i64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_micros(micros)
        .single()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| String::from("1970-01-01T00:00:00Z"))
}
