//! Runtime configuration for `anno-rag`.
//!
//! v0.1: hard-coded defaults sourced via [`AnnoRagConfig::default`].
//! TOML file loading lands in v0.2.

use crate::accelerator::AcceleratorPreference;
use crate::layers::GdprLayerSet;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;

/// Runtime OCR mode.
///
/// Build-time support is controlled separately by the `embedded-ocr` Cargo
/// feature. This mode only decides whether a build that can OCR is allowed to
/// do so for scanned PDFs/pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    /// Never OCR. Scanned PDFs/pages are deferred.
    Off,
    /// Run embedded Kreuzberg OCR only after scanned-PDF/page classification.
    AutoEmbedded,
}

fn default_ocr_mode() -> OcrMode {
    OcrMode::Off
}

/// Native text-layer PDF extraction profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancedPdfNativeMode {
    /// Keep the current fast native extraction behavior.
    Off,
    /// Enable structured native PDF extraction for better RAG provenance.
    Structured,
}

impl AdvancedPdfNativeMode {
    /// Returns true when the advanced native PDF profile should be used.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Structured)
    }
}

fn default_advanced_pdf_native() -> AdvancedPdfNativeMode {
    AdvancedPdfNativeMode::Off
}

fn default_ocr_cache_enabled() -> bool {
    true
}

fn default_pdf_hierarchy_clusters() -> usize {
    6
}

/// NER mode for `memory_save`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNerMode {
    /// Embed and store raw text; never run NER enrichment.
    Disabled,
    /// Embed and store raw text immediately; enrich NER fields in background.
    Async,
    /// Preserve the legacy inline detect + vault + tokenized storage path.
    Sync,
}

impl MemoryNerMode {
    /// Parse an environment/config string. Unknown values are rejected so
    /// callers can fall back to their existing default.
    #[must_use]
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.trim() {
            "disabled" => Some(Self::Disabled),
            "async" => Some(Self::Async),
            "sync" => Some(Self::Sync),
            _ => None,
        }
    }
}

fn default_memory_ner_mode() -> MemoryNerMode {
    MemoryNerMode::Async
}

/// Runtime configuration: data paths, model IDs, chunking defaults.
#[derive(Debug, Clone, Serialize, Deserialize, anno_config_meta::ConfigMeta)]
pub struct AnnoRagConfig {
    /// Root directory for `vault.enc`, `index.lance`, and cached model weights.
    #[config_meta(env="ANNO_RAG_DATA_DIR", cli="--data-dir", doc="Root directory for vault, index, and model weights. Default: ~/.anno-rag", since="0.1")]
    pub data_dir: PathBuf,
    /// HuggingFace model ID for the embedder.
    #[config_meta(env="ANNO_RAG_EMBED_MODEL", cli="--embed-model", doc="HuggingFace model ID for the embedder. Default: intfloat/multilingual-e5-small", since="0.1")]
    pub embed_model: String,
    /// Vector dimension. Must match the embedder's output size.
    #[config_meta(env="ANNO_RAG_EMBED_DIM", cli="--embed-dim", doc="Vector dimension; must match embedder output. Default: 384", since="0.1")]
    pub embed_dim: usize,
    /// Default top-K returned by `search`.
    #[config_meta(env="ANNO_RAG_DEFAULT_TOP_K", cli="--default-top-k", doc="Default number of results returned by search. Default: 10", since="0.1")]
    pub default_top_k: usize,
    /// Max chunk size in characters (passed to kreuzberg's chunker).
    #[config_meta(env="ANNO_RAG_CHUNK_MAX_CHARS", cli="--chunk-max-chars", doc="Max chunk size in characters. Default: 2048", since="0.1")]
    pub chunk_max_chars: usize,
    /// Chunk overlap in characters.
    #[config_meta(env="ANNO_RAG_CHUNK_OVERLAP", cli="--chunk-overlap", doc="Chunk overlap in characters. Default: 256", since="0.1")]
    pub chunk_overlap: usize,

    /// GDPR detection layer set. Default: `defense`.
    /// Overrideable via `ANNO_GDPR_LAYERS` env var.
    #[config_meta(env="ANNO_GDPR_LAYERS", cli="--gdpr-layers", doc="PII detection layer set: basic|defense|shadow|full. Default: defense", since="0.10")]
    #[serde(default)]
    pub gdpr_layers: GdprLayerSet,

    /// Threshold (chunk count) at which `Store::maybe_build_index` will
    /// build an IVF_HNSW_SQ index on the vector column. Below this, flat
    /// scan suffices. Default: 1000.
    #[config_meta(env="ANNO_RAG_VECTOR_INDEX_THRESHOLD", cli="--vector-index-threshold", doc="Chunk count above which IVF_HNSW_SQ index is built. Default: 1000", since="0.5")]
    #[serde(default = "default_vector_index_threshold")]
    pub vector_index_threshold: usize,

    /// HF Hub model ID for the NER model to pre-warm. `None` skips NER
    /// warmup. Default: `None` (callers/warmup script pick the candidate).
    #[config_meta(env="ANNO_RAG_NER_WARMUP_MODEL", cli="--ner-warmup-model", doc="HF Hub model ID to pre-warm on startup. Default: none", since="0.6")]
    #[serde(default)]
    pub ner_warmup_model: Option<String>,

    /// MCP server name advertised on `initialize`. Default: `"anno-rag"`.
    #[config_meta(env="ANNO_RAG_MCP_SERVER_NAME", cli="--mcp-server-name", doc="MCP server name advertised on initialize. Default: anno-rag", since="0.3")]
    #[serde(default = "default_mcp_server_name")]
    pub mcp_server_name: String,

    /// Runtime OCR mode. Default: `off`.
    #[config_meta(env="ANNO_RAG_OCR_MODE", cli="--ocr-mode", doc="OCR mode: off|auto_embedded. Default: off", since="0.11")]
    #[serde(default = "default_ocr_mode")]
    pub ocr_mode: OcrMode,

    /// Legacy OCR flag retained for older config files and CLI compatibility.
    /// When true, [`Self::effective_ocr_mode`] maps it to
    /// [`OcrMode::AutoEmbedded`].
    #[config_meta(env="ANNO_RAG_ENABLE_OCR", cli="--enable-ocr", doc="[DEPRECATED] Use --ocr-mode auto_embedded instead. Default: false", since="0.4")]
    #[serde(default)]
    pub enable_ocr: bool,

    /// Legacy path to the system `tesseract` binary. The embedded OCR path does
    /// not use this field; it is retained while the external fallback is phased
    /// out.
    #[config_meta(env="ANNO_RAG_TESSERACT_PATH", cli="--tesseract-path", doc="[DEPRECATED] Legacy path to tesseract binary; ignored by embedded OCR. Default: none", since="0.4")]
    #[serde(default)]
    pub tesseract_path: Option<PathBuf>,

    /// Optional per-folder OCR wall-clock budget in seconds. When exhausted,
    /// additional scanned PDFs/pages are deferred instead of being OCR'd.
    #[config_meta(env="ANNO_RAG_OCR_BATCH_BUDGET_SECS", cli="--ocr-batch-budget-secs", doc="Per-folder OCR wall-clock budget in seconds. Default: none (unlimited)", since="0.11")]
    #[serde(default)]
    pub ocr_batch_budget_secs: Option<u64>,

    /// Whether kreuzberg's extraction cache is enabled for OCR calls.
    /// Default: `true`. Set to `false` for deterministic test behavior
    /// or debugging cache issues.
    #[config_meta(env="ANNO_RAG_OCR_CACHE_ENABLED", cli="--ocr-cache-enabled", doc="Enable kreuzberg extraction cache. Default: true", since="0.11")]
    #[serde(default = "default_ocr_cache_enabled")]
    pub ocr_cache_enabled: bool,

    /// Override the primary OCR backend passed to kreuzberg. Default: `None`
    /// (kreuzberg uses `"tesseract"`). Set to `"paddleocr"` to use PaddleOCR
    /// as primary instead of fallback.
    #[config_meta(env="ANNO_RAG_OCR_BACKEND", cli="--ocr-backend", doc="Primary OCR backend passed to kreuzberg (e.g. paddleocr). Default: none (tesseract)", since="0.12")]
    #[serde(default)]
    pub ocr_backend: Option<String>,

    /// Native text-layer PDF extraction profile. Default: `off`.
    #[config_meta(env="ANNO_RAG_ADVANCED_PDF_NATIVE", cli="--advanced-pdf-native", doc="Native PDF extraction profile: off|structured. Default: off", since="0.11")]
    #[serde(default = "default_advanced_pdf_native")]
    pub advanced_pdf_native: AdvancedPdfNativeMode,

    /// Preserve running headers in advanced native PDF extraction.
    #[config_meta(env="ANNO_RAG_PDF_KEEP_HEADERS", cli="--pdf-keep-headers", doc="Preserve running headers in advanced native PDF. Default: false", since="0.11")]
    #[serde(default)]
    pub pdf_keep_headers: bool,

    /// Preserve running footers in advanced native PDF extraction.
    #[config_meta(env="ANNO_RAG_PDF_KEEP_FOOTERS", cli="--pdf-keep-footers", doc="Preserve running footers in advanced native PDF. Default: false", since="0.11")]
    #[serde(default)]
    pub pdf_keep_footers: bool,

    /// Extract PDF annotations in advanced native PDF extraction.
    #[config_meta(env="ANNO_RAG_PDF_EXTRACT_ANNOTATIONS", cli="--pdf-extract-annotations", doc="Extract PDF annotations in advanced native PDF. Default: false", since="0.11")]
    #[serde(default)]
    pub pdf_extract_annotations: bool,

    /// Font-size cluster count for Kreuzberg PDF hierarchy detection.
    #[config_meta(env="ANNO_RAG_PDF_HIERARCHY_CLUSTERS", cli="--pdf-hierarchy-clusters", doc="Font-size cluster count for PDF hierarchy (1-7). Default: 6", since="0.11")]
    #[serde(default = "default_pdf_hierarchy_clusters")]
    pub pdf_hierarchy_clusters: usize,

    /// Allow single-column pseudo-tables in native PDF table extraction.
    #[config_meta(env="ANNO_RAG_PDF_ALLOW_SINGLE_COLUMN_TABLES", cli="--pdf-allow-single-column-tables", doc="Allow single-column pseudo-tables in PDF extraction. Default: false", since="0.11")]
    #[serde(default)]
    pub pdf_allow_single_column_tables: bool,

    /// Emit diagnostic structured sidecar data for advanced native PDFs.
    #[config_meta(env="ANNO_RAG_PDF_STRUCTURED_SIDECAR", cli="--pdf-structured-sidecar", doc="Emit diagnostic structured sidecar for advanced PDFs. Default: false", since="0.12")]
    #[serde(default)]
    pub pdf_structured_sidecar: bool,

    /// Embedder weight dtype. `"f32"` (default) or `"f16"` (experimental
    /// opt-in). Read by `Embedder::load`. `None` → `"f32"`. F16 halves
    /// embedder RSS (~236 MB) but the e5-small BERT forward can produce
    /// degenerate (NaN) vectors on CPU — opt-in until numerically stable.
    #[config_meta(env="ANNO_RAG_EMBEDDER_DTYPE", cli="--embedder-dtype", doc="Embedder weight dtype: f32 (default) or f16 (experimental). Default: none (f32)", since="0.12")]
    #[serde(default)]
    pub embedder_dtype: Option<String>,

    /// Runtime accelerator preference. Defaults to `auto`; `ANNO_ACCELERATOR`
    /// overrides this at process start.
    #[config_meta(env="ANNO_ACCELERATOR", cli="--accelerator", doc="Runtime accelerator: auto|cpu|metal|cuda. Default: auto", since="0.10")]
    #[serde(default)]
    pub accelerator: AcceleratorPreference,

    /// Reranker repo id on HuggingFace Hub (cross-encoder, opt-in).
    #[config_meta(env="ANNO_RAG_RERANK_MODEL", cli="--rerank-model", doc="HF Hub model ID for the cross-encoder reranker. Default: onnx-community/bge-reranker-v2-m3-ONNX", since="0.12")]
    #[serde(default = "default_rerank_model")]
    pub rerank_model: String,

    /// ONNX file within `rerank_model`. INT8 by default; point at
    /// "onnx/model_q4f16.onnx" (702 MB) if INT8 regresses on your corpus.
    #[config_meta(env="ANNO_RAG_RERANK_ONNX_FILE", cli="--rerank-onnx-file", doc="ONNX file within rerank_model. Default: onnx/model_int8.onnx", since="0.12")]
    #[serde(default = "default_rerank_onnx_file")]
    pub rerank_onnx_file: String,

    /// RRF candidates to over-fetch before reranking. Default 30.
    #[config_meta(env="ANNO_RAG_RERANK_POOL_SIZE", cli="--rerank-pool-size", doc="RRF candidates to over-fetch before reranking. Default: 30", since="0.12")]
    #[serde(default = "default_rerank_pool_size")]
    pub rerank_pool_size: usize,

    /// Max (query,passage) pairs per ONNX forward batch. Default 8.
    #[config_meta(env="ANNO_RAG_RERANK_BATCH_SIZE", cli="--rerank-batch-size", doc="Max (query,passage) pairs per ONNX reranker batch. Default: 8", since="0.12")]
    #[serde(default = "default_rerank_batch_size")]
    pub rerank_batch_size: usize,

    /// Name of the LanceDB collection that stores memories (v0.1 default
    /// `"memories"`). Lives alongside the `chunks` documents table.
    #[config_meta(env="ANNO_RAG_MEMORY_COLLECTION_NAME", cli="--memory-collection-name", doc="LanceDB collection name for memories. Default: memories", since="0.8")]
    #[serde(default = "default_memory_collection_name")]
    pub memory_collection_name: String,

    /// Embedding dimension for memory vectors. Matches `embed_dim` by
    /// default (384 for e5-small) but kept independent so the memory
    /// store can migrate to a different embedder than documents.
    #[config_meta(env="ANNO_RAG_MEMORY_EMBEDDING_DIM", cli="--memory-embedding-dim", doc="Embedding dimension for memory vectors. Default: 384", since="0.8")]
    #[serde(default = "default_memory_embedding_dim")]
    pub memory_embedding_dim: usize,

    /// NER mode for `memory_save`. Default: async.
    ///
    /// - `disabled`: embed + store raw text only.
    /// - `async`: embed + store immediately, then enrich NER fields in background.
    /// - `sync`: full legacy pipeline inline.
    #[config_meta(env="ANNO_RAG_MEMORY_NER_MODE", cli="--memory-ner-mode", doc="NER mode for memory_save: disabled|async|sync. Default: async", since="0.9")]
    #[serde(default = "default_memory_ner_mode")]
    pub memory_ner_mode: MemoryNerMode,

    /// Interval between background compactions (seconds). Default: 24h.
    /// Drives the GDPR Art. 17 erasure SLO — physical bytes reclaim
    /// happens within at most this interval after `forget_memory`.
    #[config_meta(env="ANNO_RAG_COMPACTION_INTERVAL_SECS", cli="--compaction-interval-secs", doc="Seconds between background compactions. Default: 86400 (24h)", since="0.9")]
    #[serde(default = "default_compaction_interval_secs")]
    pub compaction_interval_secs: u64,

    /// Minimum age of a tombstone before compaction reclaims it (seconds).
    /// Default: 1h. Prevents thrashing on a hot delete-then-write loop.
    #[config_meta(env="ANNO_RAG_COMPACTION_MIN_AGE_SECS", cli="--compaction-min-age-secs", doc="Minimum tombstone age before compaction (seconds). Default: 3600", since="0.9")]
    #[serde(default = "default_compaction_min_age_secs")]
    pub compaction_min_age_secs: u64,

    /// Per-tenant entity alias map applied during canonicalisation
    /// (`canonicalize_entity`). Keys are the already-canonical surface
    /// form (lowercase + diacritic strip + punct strip + whitespace
    /// collapse); values are the substituted form. Empty by default —
    /// each cabinet builds its own (e.g. `"me dupont" → "dupont"`).
    /// v0.6 candidate: load from a TOML file alongside the vault.
    #[config_meta(env="ANNO_RAG_ENTITY_ALIASES", cli="--entity-aliases", doc="JSON object mapping canonical entity surface forms to substituted forms. Default: {}", since="0.10")]
    #[serde(default)]
    pub entity_aliases: std::collections::HashMap<String, String>,

    /// Cosine-similarity threshold above which two `Preference` /
    /// `Reference` memories with a shared entity are treated as a
    /// conflict (the prior is auto-invalidated on save). Default 0.85.
    /// Tune up to reduce false-positive invalidation; tune down to
    /// catch more re-statements.
    #[config_meta(env="ANNO_RAG_CONFLICT_COSINE_THRESHOLD", cli="--conflict-cosine-threshold", doc="Cosine threshold for memory conflict detection (0.0-1.0). Default: 0.85", since="0.10")]
    #[serde(default = "default_conflict_cosine_threshold")]
    pub conflict_cosine_threshold: f32,

    /// Maximum hop count for `Pipeline::graph_recall`. Default 2.
    /// Caps the BFS depth over `entity_refs`; higher values risk
    /// exponential expansion on popular-entity graphs.
    #[config_meta(env="ANNO_RAG_GRAPH_MAX_HOPS", cli="--graph-max-hops", doc="Maximum BFS hop count for graph_recall. Default: 2", since="0.10")]
    #[serde(default = "default_graph_max_hops")]
    pub graph_max_hops: u8,

    /// Per-hop row limit for `Pipeline::graph_recall`. Default 50.
    /// Bounds the candidate set scanned at each BFS hop to keep
    /// graph recall sub-quadratic on hot entities.
    #[config_meta(env="ANNO_RAG_GRAPH_PER_HOP_LIMIT", cli="--graph-per-hop-limit", doc="Max candidates per BFS hop in graph_recall. Default: 50", since="0.10")]
    #[serde(default = "default_graph_per_hop_limit")]
    pub graph_per_hop_limit: usize,
}

fn default_memory_collection_name() -> String {
    "memories".to_string()
}

fn default_memory_embedding_dim() -> usize {
    384
}

fn default_compaction_interval_secs() -> u64 {
    24 * 3600
}

fn default_compaction_min_age_secs() -> u64 {
    3600
}

fn default_conflict_cosine_threshold() -> f32 {
    0.85
}

fn default_graph_max_hops() -> u8 {
    2
}

fn default_graph_per_hop_limit() -> usize {
    50
}

fn default_vector_index_threshold() -> usize {
    1000
}

fn default_mcp_server_name() -> String {
    "anno-rag".to_string()
}

fn default_rerank_model() -> String {
    "onnx-community/bge-reranker-v2-m3-ONNX".to_string()
}
fn default_rerank_onnx_file() -> String {
    "onnx/model_int8.onnx".to_string()
}
fn default_rerank_pool_size() -> usize {
    30
}
fn default_rerank_batch_size() -> usize {
    8
}

fn default_data_dir() -> PathBuf {
    default_data_dir_from_env(
        std::env::var_os("ANNO_RAG_DATA_DIR"),
        std::env::var_os("HOME"),
    )
}

fn default_data_dir_from_env(
    override_dir: Option<OsString>,
    home_dir: Option<OsString>,
) -> PathBuf {
    override_dir
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home_dir
                .filter(|p| !p.is_empty())
                .map(|p| PathBuf::from(p).join(".anno-rag"))
        })
        .or_else(|| dirs::home_dir().map(|p| p.join(".anno-rag")))
        .unwrap_or_else(|| PathBuf::from(".anno-rag"))
}

impl Default for AnnoRagConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();

        Self {
            data_dir,
            embed_model: "intfloat/multilingual-e5-small".to_string(),
            embed_dim: 384,
            default_top_k: 10,
            chunk_max_chars: 2048,
            chunk_overlap: 256,
            gdpr_layers: GdprLayerSet::Defense,
            vector_index_threshold: default_vector_index_threshold(),
            ner_warmup_model: None,
            mcp_server_name: default_mcp_server_name(),
            ocr_mode: default_ocr_mode(),
            enable_ocr: false,
            tesseract_path: None,
            ocr_batch_budget_secs: None,
            ocr_cache_enabled: default_ocr_cache_enabled(),
            ocr_backend: None,
            advanced_pdf_native: default_advanced_pdf_native(),
            pdf_keep_headers: false,
            pdf_keep_footers: false,
            pdf_extract_annotations: false,
            pdf_hierarchy_clusters: default_pdf_hierarchy_clusters(),
            pdf_allow_single_column_tables: false,
            pdf_structured_sidecar: false,
            embedder_dtype: None,
            accelerator: AcceleratorPreference::Auto,
            memory_collection_name: default_memory_collection_name(),
            memory_embedding_dim: default_memory_embedding_dim(),
            memory_ner_mode: default_memory_ner_mode(),
            compaction_interval_secs: default_compaction_interval_secs(),
            compaction_min_age_secs: default_compaction_min_age_secs(),
            entity_aliases: std::collections::HashMap::new(),
            conflict_cosine_threshold: default_conflict_cosine_threshold(),
            graph_max_hops: default_graph_max_hops(),
            graph_per_hop_limit: default_graph_per_hop_limit(),
            rerank_model: default_rerank_model(),
            rerank_onnx_file: default_rerank_onnx_file(),
            rerank_pool_size: default_rerank_pool_size(),
            rerank_batch_size: default_rerank_batch_size(),
        }
    }
}

impl AnnoRagConfig {
    /// Runtime OCR mode after applying legacy compatibility flags.
    ///
    /// When `enable_ocr` is true and `ocr_mode` is `Off`, maps to
    /// `AutoEmbedded` for backward compatibility. Logs a deprecation
    /// warning so users migrate to `ocr_mode: auto_embedded`.
    #[must_use]
    pub fn effective_ocr_mode(&self) -> OcrMode {
        if self.enable_ocr && self.ocr_mode == OcrMode::Off {
            tracing::warn!(
                "config field 'enable_ocr' is deprecated; \
                 use 'ocr_mode: auto_embedded' instead"
            );
            OcrMode::AutoEmbedded
        } else {
            self.ocr_mode
        }
    }

    /// Log warnings for deprecated configuration fields.
    ///
    /// Call once at startup after loading config.
    pub fn warn_deprecated_fields(&self) {
        if self.tesseract_path.is_some() {
            tracing::warn!(
                "config field 'tesseract_path' is deprecated and ignored; \
                 embedded OCR manages its own Tesseract binary"
            );
        }
    }

    /// Kreuzberg PDF hierarchy cluster count clamped to the supported 1..=7 range.
    #[must_use]
    pub fn effective_pdf_hierarchy_clusters(&self) -> usize {
        self.pdf_hierarchy_clusters.clamp(1, 7)
    }

    /// Path to the encrypted cloakpipe Vault file (AES-256-GCM single-file format).
    #[must_use]
    pub fn vault_path(&self) -> PathBuf {
        self.data_dir.join("vault.enc")
    }

    /// Path to the LanceDB index directory.
    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.data_dir.join("index.lance")
    }

    /// Path where embedder / tokenizer weights are cached.
    #[must_use]
    pub fn models_cache(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// Path where pseudonymized markdown copies are written.
    #[must_use]
    pub fn outputs_dir(&self) -> PathBuf {
        self.data_dir.join("outputs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.embed_dim, 384);
        assert!(c.default_top_k > 0);
        assert!(c.chunk_max_chars > c.chunk_overlap);
    }

    #[test]
    fn paths_derive_from_data_dir() {
        let c = AnnoRagConfig {
            data_dir: PathBuf::from("/tmp/anno-rag"),
            ..Default::default()
        };
        assert_eq!(c.vault_path(), PathBuf::from("/tmp/anno-rag/vault.enc"));
        assert_eq!(c.index_path(), PathBuf::from("/tmp/anno-rag/index.lance"));
        assert_eq!(c.models_cache(), PathBuf::from("/tmp/anno-rag/models"));
        assert_eq!(c.outputs_dir(), PathBuf::from("/tmp/anno-rag/outputs"));
    }

    #[test]
    fn data_dir_env_override_wins_over_home_default() {
        let data_dir = default_data_dir_from_env(
            Some(OsString::from("/tmp/anno-rag-env")),
            Some(OsString::from("/tmp/anno-rag-home")),
        );

        assert_eq!(data_dir, PathBuf::from("/tmp/anno-rag-env"));
    }

    #[test]
    fn deserializes_v0_1_config_without_new_fields() {
        // Old v0.1 JSON had only the original 6 fields. Verify still parses.
        let v01_json = r#"{
            "data_dir": "/tmp/anno-rag",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(v01_json).expect("v0.1 config must parse");
        assert_eq!(c.vector_index_threshold, 1000);
        assert!(c.ner_warmup_model.is_none());
        assert_eq!(c.mcp_server_name, "anno-rag");
        assert!(!c.enable_ocr);
        assert!(c.tesseract_path.is_none());
        assert_eq!(c.ocr_mode, OcrMode::Off);
        assert_eq!(c.effective_ocr_mode(), OcrMode::Off);
        assert_eq!(c.ocr_batch_budget_secs, None);
        assert!(c.embedder_dtype.is_none());
        assert_eq!(c.memory_collection_name, "memories");
        assert_eq!(c.memory_embedding_dim, 384);
        assert!(c.ocr_cache_enabled);
        assert!(c.ocr_backend.is_none());
    }

    #[test]
    fn defaults_include_new_fields() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.vector_index_threshold, 1000);
        assert!(c.ner_warmup_model.is_none());
        assert_eq!(c.mcp_server_name, "anno-rag");
        assert_eq!(c.ocr_mode, OcrMode::Off);
        assert_eq!(c.effective_ocr_mode(), OcrMode::Off);
        assert!(!c.enable_ocr);
        assert!(c.tesseract_path.is_none());
        assert_eq!(c.ocr_batch_budget_secs, None);
        assert!(c.embedder_dtype.is_none());
        assert_eq!(c.memory_collection_name, "memories");
        assert_eq!(c.memory_embedding_dim, 384);
        assert_eq!(c.memory_ner_mode, MemoryNerMode::Async);
        assert!(c.ocr_cache_enabled);
        assert!(c.ocr_backend.is_none());
    }

    #[test]
    fn memory_ner_mode_defaults_to_async_for_old_config() {
        let v01_json = r#"{
            "data_dir": "/tmp/anno-rag",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256
        }"#;

        let c: AnnoRagConfig = serde_json::from_str(v01_json).expect("old config parses");

        assert_eq!(c.memory_ner_mode, MemoryNerMode::Async);
    }

    #[test]
    fn old_config_defaults_accelerator_to_auto() {
        let v01_json = r#"{
            "data_dir": ".anno-rag",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(v01_json).expect("old config parses");
        assert_eq!(c.accelerator, AcceleratorPreference::Auto);
    }

    #[test]
    fn memory_ner_mode_round_trips_as_snake_case() {
        for mode in [
            MemoryNerMode::Disabled,
            MemoryNerMode::Async,
            MemoryNerMode::Sync,
        ] {
            let c = AnnoRagConfig {
                memory_ner_mode: mode,
                ..Default::default()
            };

            let s = serde_json::to_string(&c).expect("serialize");
            let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");

            assert_eq!(back.memory_ner_mode, mode);
        }

        let c = AnnoRagConfig {
            memory_ner_mode: MemoryNerMode::Disabled,
            ..Default::default()
        };
        let s = serde_json::to_string(&c).expect("serialize");
        assert!(s.contains(r#""memory_ner_mode":"disabled""#));
    }

    #[test]
    fn memory_ner_mode_parses_env_values() {
        assert_eq!(
            MemoryNerMode::from_env_value("disabled"),
            Some(MemoryNerMode::Disabled)
        );
        assert_eq!(
            MemoryNerMode::from_env_value(" async "),
            Some(MemoryNerMode::Async)
        );
        assert_eq!(
            MemoryNerMode::from_env_value("sync"),
            Some(MemoryNerMode::Sync)
        );
        assert_eq!(MemoryNerMode::from_env_value("bogus"), None);
    }

    #[test]
    fn legacy_enable_ocr_maps_to_auto_embedded() {
        let c = AnnoRagConfig {
            enable_ocr: true,
            ..Default::default()
        };

        assert_eq!(c.ocr_mode, OcrMode::Off);
        assert_eq!(c.effective_ocr_mode(), OcrMode::AutoEmbedded);
    }

    #[test]
    fn ocr_mode_round_trips_as_snake_case() {
        let c = AnnoRagConfig {
            ocr_mode: OcrMode::AutoEmbedded,
            ocr_batch_budget_secs: Some(30),
            ..Default::default()
        };

        let s = serde_json::to_string(&c).expect("serialize");
        assert!(s.contains(r#""ocr_mode":"auto_embedded""#));
        let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.ocr_mode, OcrMode::AutoEmbedded);
        assert_eq!(back.ocr_batch_budget_secs, Some(30));
    }

    #[test]
    fn advanced_pdf_native_defaults_to_off() {
        let c = AnnoRagConfig::default();

        assert_eq!(c.advanced_pdf_native, AdvancedPdfNativeMode::Off);
        assert!(!c.advanced_pdf_native.is_enabled());
        assert!(!c.pdf_keep_headers);
        assert!(!c.pdf_keep_footers);
        assert!(!c.pdf_extract_annotations);
        assert_eq!(c.pdf_hierarchy_clusters, 6);
        assert!(!c.pdf_allow_single_column_tables);
        assert!(!c.pdf_structured_sidecar);
    }

    #[test]
    fn advanced_pdf_native_round_trips_as_snake_case() {
        let c = AnnoRagConfig {
            advanced_pdf_native: AdvancedPdfNativeMode::Structured,
            pdf_keep_headers: true,
            pdf_keep_footers: true,
            pdf_extract_annotations: true,
            pdf_hierarchy_clusters: 4,
            pdf_allow_single_column_tables: true,
            pdf_structured_sidecar: true,
            ..Default::default()
        };

        let s = serde_json::to_string(&c).expect("serialize");
        assert!(s.contains(r#""advanced_pdf_native":"structured""#));
        let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");

        assert_eq!(back.advanced_pdf_native, AdvancedPdfNativeMode::Structured);
        assert!(back.advanced_pdf_native.is_enabled());
        assert!(back.pdf_keep_headers);
        assert!(back.pdf_keep_footers);
        assert!(back.pdf_extract_annotations);
        assert_eq!(back.pdf_hierarchy_clusters, 4);
        assert!(back.pdf_allow_single_column_tables);
        assert!(back.pdf_structured_sidecar);
    }

    #[test]
    fn old_config_defaults_advanced_pdf_native_fields() {
        let v01_json = r#"{
            "data_dir": "/tmp/anno-rag",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256
        }"#;

        let c: AnnoRagConfig = serde_json::from_str(v01_json).expect("old config parses");

        assert_eq!(c.advanced_pdf_native, AdvancedPdfNativeMode::Off);
        assert!(!c.pdf_keep_headers);
        assert!(!c.pdf_keep_footers);
        assert!(!c.pdf_extract_annotations);
        assert_eq!(c.pdf_hierarchy_clusters, 6);
        assert!(!c.pdf_allow_single_column_tables);
        assert!(!c.pdf_structured_sidecar);
    }

    #[test]
    fn round_trips_through_json() {
        let c = AnnoRagConfig::default();
        let s = serde_json::to_string(&c).expect("serialize");
        let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(c.embed_dim, back.embed_dim);
        assert_eq!(c.default_top_k, back.default_top_k);
    }

    #[test]
    fn rerank_defaults_are_sane() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.rerank_model, "onnx-community/bge-reranker-v2-m3-ONNX");
        assert_eq!(c.rerank_onnx_file, "onnx/model_int8.onnx");
        assert_eq!(c.rerank_pool_size, 30);
        assert_eq!(c.rerank_batch_size, 8);
    }

    #[test]
    fn deprecated_fields_still_parse_and_map() {
        let json = r#"{
            "data_dir": "/tmp",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256,
            "enable_ocr": true,
            "tesseract_path": "/usr/bin/tesseract"
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(json).expect("legacy config must parse");
        assert!(c.enable_ocr);
        assert_eq!(
            c.tesseract_path,
            Some(std::path::PathBuf::from("/usr/bin/tesseract"))
        );
        assert_eq!(c.effective_ocr_mode(), OcrMode::AutoEmbedded);
    }

    #[test]
    fn ocr_cache_enabled_defaults_to_true() {
        let c = AnnoRagConfig::default();
        assert!(c.ocr_cache_enabled);
    }

    #[test]
    fn ocr_cache_enabled_parses_from_json() {
        let json = r#"{
            "data_dir": "/tmp",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256,
            "ocr_cache_enabled": false
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(json).expect("parses");
        assert!(!c.ocr_cache_enabled);
    }

    #[test]
    fn ocr_backend_defaults_to_none() {
        let c = AnnoRagConfig::default();
        assert!(c.ocr_backend.is_none());
    }

    #[test]
    fn gdpr_layers_defaults_to_defense() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.gdpr_layers, crate::layers::GdprLayerSet::Defense);
    }

    #[test]
    fn gdpr_layers_round_trips_through_json() {
        let c = AnnoRagConfig {
            gdpr_layers: crate::layers::GdprLayerSet::Basic,
            ..Default::default()
        };
        let s = serde_json::to_string(&c).expect("serialize");
        assert!(s.contains(r#""gdpr_layers":"basic""#), "wire format should be lowercase snake_case");
        let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.gdpr_layers, crate::layers::GdprLayerSet::Basic);
    }

    #[test]
    fn gdpr_layers_round_trips_through_toml() {
        // Full round-trip: serialize a complete config to TOML and read it back.
        // Partial deserialization is not tested here because data_dir and other
        // required fields have no serde default (Task 3 will add #[serde(default)]
        // to enable partial TOML files for config loading).
        let cfg = AnnoRagConfig {
            gdpr_layers: crate::layers::GdprLayerSet::Shadow,
            ..Default::default()
        };
        let s = toml::to_string(&cfg).expect("serialize to toml");
        assert!(s.contains("gdpr_layers = \"shadow\""), "toml wire format should be lowercase snake_case: {s}");
        let back: AnnoRagConfig = toml::from_str(&s).expect("deserialize from toml");
        assert_eq!(back.gdpr_layers, crate::layers::GdprLayerSet::Shadow);
    }

    #[test]
    fn ocr_backend_parses_from_json() {
        let json = r#"{
            "data_dir": "/tmp",
            "embed_model": "intfloat/multilingual-e5-small",
            "embed_dim": 384,
            "default_top_k": 10,
            "chunk_max_chars": 2048,
            "chunk_overlap": 256,
            "ocr_backend": "paddleocr"
        }"#;
        let c: AnnoRagConfig = serde_json::from_str(json).expect("parses");
        assert_eq!(c.ocr_backend.as_deref(), Some("paddleocr"));
    }
}
