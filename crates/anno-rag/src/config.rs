//! Runtime configuration for `anno-rag`.
//!
//! v0.1: hard-coded defaults sourced via [`AnnoRagConfig::default`].
//! v0.2: TOML file loading + env var pipeline via [`AnnoRagConfig::load`].

use crate::accelerator::AcceleratorPreference;
use crate::layers::GdprLayerSet;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;

/// Errors that can occur when loading `AnnoRagConfig`.
#[derive(Debug)]
pub enum ConfigLoadError {
    /// An I/O error reading the config file.
    Io(String),
    /// A TOML parse error in the config file.
    Toml(String),
}

impl std::fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "config I/O error: {e}"),
            Self::Toml(e) => write!(f, "config parse error: {e}"),
        }
    }
}

impl std::error::Error for ConfigLoadError {}

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
    OcrMode::AutoEmbedded
}

impl std::str::FromStr for OcrMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "auto_embedded" => Ok(Self::AutoEmbedded),
            other => Err(format!(
                "unknown ocr-mode '{}'; valid: off, auto_embedded",
                other
            )),
        }
    }
}
impl std::fmt::Display for OcrMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::AutoEmbedded => write!(f, "auto_embedded"),
        }
    }
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

impl std::str::FromStr for AdvancedPdfNativeMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "structured" => Ok(Self::Structured),
            other => Err(format!(
                "unknown advanced-pdf-native '{}'; valid: off, structured",
                other
            )),
        }
    }
}
impl std::fmt::Display for AdvancedPdfNativeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Structured => write!(f, "structured"),
        }
    }
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

impl std::str::FromStr for MemoryNerMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MemoryNerMode::from_env_value(s).ok_or_else(|| {
            format!(
                "unknown memory-ner-mode '{}'; valid: disabled, async, sync",
                s
            )
        })
    }
}
impl std::fmt::Display for MemoryNerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Async => write!(f, "async"),
            Self::Sync => write!(f, "sync"),
        }
    }
}

fn default_memory_ner_mode() -> MemoryNerMode {
    MemoryNerMode::Async
}

fn default_ner_warmup_model() -> Option<String> {
    Some("fastino/gliner2-multi-v1".to_string())
}

fn default_ner_model_id() -> String {
    "SemplificaAI/gliner2-multi-v1-onnx".to_string()
}

fn default_ner_onnx_precision() -> String {
    "fp16".to_string()
}

fn default_index_distance() -> String {
    "cosine".to_string()
}

fn default_search_nprobes() -> usize {
    20
}

fn default_search_refine_factor() -> u32 {
    10
}

fn default_ner_candle_model_id() -> String {
    "fastino/gliner2-multi-v1".to_string()
}

fn default_embed_model() -> String {
    "OrdalieTech/Solon-embeddings-large-0.1".to_string()
}

fn default_embed_dim() -> usize {
    1024
}

fn default_default_top_k() -> usize {
    10
}

fn default_chunk_max_chars() -> usize {
    2048
}

fn default_chunk_overlap() -> usize {
    256
}

/// Runtime configuration: data paths, model IDs, chunking defaults.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    anno_config_meta::ConfigMeta,
    anno_config_meta::ConfigCliArgs,
)]
pub struct AnnoRagConfig {
    /// Root directory for `vault.enc`, `index.lance`, and cached model weights.
    #[config_meta(
        env = "ANNO_RAG_DATA_DIR",
        cli = "--data-dir",
        doc = "Root directory for vault, index, and model weights. Default: ~/.anno-rag",
        since = "0.1"
    )]
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// HuggingFace model ID for the embedder.
    #[config_meta(
        env = "ANNO_RAG_EMBED_MODEL",
        cli = "--embed-model",
        doc = "HuggingFace model ID for the embedder. Default: OrdalieTech/Solon-embeddings-large-0.1",
        since = "0.1"
    )]
    #[serde(default = "default_embed_model")]
    pub embed_model: String,
    /// Vector dimension. Must match the embedder's output size.
    #[config_meta(
        env = "ANNO_RAG_EMBED_DIM",
        cli = "--embed-dim",
        doc = "Vector dimension; must match embedder output. Default: 1024",
        since = "0.1"
    )]
    #[serde(default = "default_embed_dim")]
    pub embed_dim: usize,
    /// Default top-K returned by `search`.
    #[config_meta(
        env = "ANNO_RAG_DEFAULT_TOP_K",
        cli = "--default-top-k",
        doc = "Default number of results returned by search. Default: 10",
        since = "0.1"
    )]
    #[serde(default = "default_default_top_k")]
    pub default_top_k: usize,
    /// Max chunk size in characters (passed to kreuzberg's chunker).
    #[config_meta(
        env = "ANNO_RAG_CHUNK_MAX_CHARS",
        cli = "--chunk-max-chars",
        doc = "Max chunk size in characters. Default: 2048",
        since = "0.1"
    )]
    #[serde(default = "default_chunk_max_chars")]
    pub chunk_max_chars: usize,
    /// Chunk overlap in characters.
    #[config_meta(
        env = "ANNO_RAG_CHUNK_OVERLAP",
        cli = "--chunk-overlap",
        doc = "Chunk overlap in characters. Default: 256",
        since = "0.1"
    )]
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,

    /// GDPR detection layer set. Default: `defense`.
    /// Overrideable via `ANNO_GDPR_LAYERS` env var.
    #[config_meta(
        env = "ANNO_GDPR_LAYERS",
        cli = "--gdpr-layers",
        doc = "PII detection layer set: basic|defense|shadow|full. Default: defense",
        since = "0.10"
    )]
    #[serde(default)]
    pub gdpr_layers: GdprLayerSet,

    /// Threshold (chunk count) at which `Store::maybe_build_index` will
    /// build an IVF_HNSW_SQ index on the vector column. Below this, flat
    /// scan suffices. Default: 1000.
    #[config_meta(
        env = "ANNO_RAG_VECTOR_INDEX_THRESHOLD",
        cli = "--vector-index-threshold",
        doc = "Chunk count above which IVF_HNSW_SQ index is built. Default: 1000",
        since = "0.5"
    )]
    #[serde(default = "default_vector_index_threshold")]
    pub vector_index_threshold: usize,

    /// HF Hub model ID for the NER model to pre-warm. `None` skips NER
    /// warmup. Default: `None` (callers/warmup script pick the candidate).
    #[config_meta(
        env = "ANNO_RAG_NER_WARMUP_MODEL",
        cli = "--ner-warmup-model",
        doc = "HF Hub model ID to pre-warm on startup. Default: fastino/gliner2-multi-v1",
        since = "0.6"
    )]
    #[serde(default = "default_ner_warmup_model")]
    pub ner_warmup_model: Option<String>,

    /// HuggingFace model ID for the ONNX GLiNER2 detector.
    #[config_meta(
        env = "ANNO_RAG_NER_MODEL",
        cli = "--ner-model",
        doc = "HuggingFace model ID for the ONNX NER detector. Default: SemplificaAI/gliner2-multi-v1-onnx",
        since = "0.12"
    )]
    #[serde(default = "default_ner_model_id")]
    pub ner_model_id: String,

    /// ONNX graph precision for the NER detector: "fp16" (default, ~250 MB)
    /// or "fp32" (~500 MB, exact). fp16 is near-lossless for inference.
    #[config_meta(
        env = "ANNO_RAG_NER_ONNX_PRECISION",
        cli = "--ner-onnx-precision",
        doc = "ONNX graph precision for NER: fp16 (default, ~250 MB) or fp32 (~500 MB). Default: fp16",
        since = "0.13"
    )]
    #[serde(default = "default_ner_onnx_precision")]
    pub ner_onnx_precision: String,

    /// Vector index distance metric: "cosine" (default), "l2", or "dot".
    /// Embeddings are L2-normalized so cosine and dot give the same ordering;
    /// setting it explicitly guards against a future non-normalized embedder.
    #[config_meta(
        env = "ANNO_RAG_INDEX_DISTANCE",
        cli = "--index-distance",
        doc = "Vector index distance: cosine (default), l2, or dot. Default: cosine",
        since = "0.13"
    )]
    #[serde(default = "default_index_distance")]
    pub index_distance: String,

    /// IVF partition count for the vector index. `None` = LanceDB auto
    /// (approximately sqrt of the row count).
    #[config_meta(
        env = "ANNO_RAG_INDEX_NUM_PARTITIONS",
        cli = "--index-num-partitions",
        doc = "IVF partitions for the vector index. Default: auto (unset)",
        since = "0.13"
    )]
    #[serde(default)]
    pub index_num_partitions: Option<usize>,

    /// IVF partitions probed per query. Default: 20.
    /// Higher values recover recall at the cost of latency; tune against
    /// `bench_recall` once an IVF index exists (table ≥ vector_index_threshold).
    #[config_meta(
        env = "ANNO_RAG_SEARCH_NPROBES",
        cli = "--search-nprobes",
        doc = "IVF partitions probed per query (recall vs speed). Default: 20",
        since = "0.13"
    )]
    #[serde(default = "default_search_nprobes")]
    pub search_nprobes: usize,

    /// Refine-factor for IVF_HNSW_SQ queries. Default: 10.
    /// The SQ index stores vectors as int8; `refine_factor=N` fetches N×
    /// more candidates then reranks with the original float32 values to
    /// recover the ~5 % recall gap from quantization. Set to 1 to disable.
    #[config_meta(
        env = "ANNO_RAG_SEARCH_REFINE_FACTOR",
        cli = "--search-refine-factor",
        doc = "SQ refine factor (1 = off, 10 = default). Default: 10",
        since = "0.13"
    )]
    #[serde(default = "default_search_refine_factor")]
    pub search_refine_factor: u32,

    /// HuggingFace model ID for the Candle GLiNER2 detector backend.
    #[config_meta(
        env = "ANNO_RAG_NER_CANDLE_MODEL",
        cli = "--ner-candle-model",
        doc = "HuggingFace model ID for the Candle NER detector. Default: fastino/gliner2-multi-v1",
        since = "0.12"
    )]
    #[serde(default = "default_ner_candle_model_id")]
    pub ner_candle_model_id: String,

    /// MCP server name advertised on `initialize`. Default: `"anno-rag"`.
    #[config_meta(
        env = "ANNO_RAG_MCP_SERVER_NAME",
        cli = "--mcp-server-name",
        doc = "MCP server name advertised on initialize. Default: anno-rag",
        since = "0.3"
    )]
    #[serde(default = "default_mcp_server_name")]
    pub mcp_server_name: String,

    /// Runtime OCR mode. Default: `off`.
    #[config_meta(
        env = "ANNO_RAG_OCR_MODE",
        cli = "--ocr-mode",
        doc = "OCR mode: off|auto_embedded. Default: auto_embedded",
        since = "0.11"
    )]
    #[serde(default = "default_ocr_mode")]
    pub ocr_mode: OcrMode,

    /// Legacy OCR flag retained for older config files and CLI compatibility.
    /// When true, [`Self::effective_ocr_mode`] maps it to
    /// [`OcrMode::AutoEmbedded`].
    #[config_meta(
        env = "ANNO_RAG_ENABLE_OCR",
        cli = "--enable-ocr",
        doc = "[DEPRECATED] Use --ocr-mode auto_embedded instead. Default: false",
        since = "0.4"
    )]
    #[serde(default)]
    pub enable_ocr: bool,

    /// Legacy path to the system `tesseract` binary. The embedded OCR path does
    /// not use this field; it is retained while the external fallback is phased
    /// out.
    #[config_meta(
        env = "ANNO_RAG_TESSERACT_PATH",
        cli = "--tesseract-path",
        doc = "[DEPRECATED] Legacy path to tesseract binary; ignored by embedded OCR. Default: none",
        since = "0.4"
    )]
    #[serde(default)]
    pub tesseract_path: Option<PathBuf>,

    /// Optional per-folder OCR wall-clock budget in seconds. When exhausted,
    /// additional scanned PDFs/pages are deferred instead of being OCR'd.
    #[config_meta(
        env = "ANNO_RAG_OCR_BATCH_BUDGET_SECS",
        cli = "--ocr-batch-budget-secs",
        doc = "Per-folder OCR wall-clock budget in seconds. Default: none (unlimited)",
        since = "0.11"
    )]
    #[serde(default)]
    pub ocr_batch_budget_secs: Option<u64>,

    /// Whether kreuzberg's extraction cache is enabled for OCR calls.
    /// Default: `true`. Set to `false` for deterministic test behavior
    /// or debugging cache issues.
    #[config_meta(
        env = "ANNO_RAG_OCR_CACHE_ENABLED",
        cli = "--ocr-cache-enabled",
        doc = "Enable kreuzberg extraction cache. Default: true",
        since = "0.11"
    )]
    #[serde(default = "default_ocr_cache_enabled")]
    pub ocr_cache_enabled: bool,

    /// Override the primary OCR backend passed to kreuzberg. Default: `None`
    /// (kreuzberg uses `"tesseract"`). Set to `"paddleocr"` to use PaddleOCR
    /// as primary instead of fallback.
    #[config_meta(
        env = "ANNO_RAG_OCR_BACKEND",
        cli = "--ocr-backend",
        doc = "Primary OCR backend passed to kreuzberg (e.g. paddleocr). Default: none (tesseract)",
        since = "0.12"
    )]
    #[serde(default)]
    pub ocr_backend: Option<String>,

    /// Native text-layer PDF extraction profile. Default: `off`.
    #[config_meta(
        env = "ANNO_RAG_ADVANCED_PDF_NATIVE",
        cli = "--advanced-pdf-native",
        doc = "Native PDF extraction profile: off|structured. Default: off",
        since = "0.11"
    )]
    #[serde(default = "default_advanced_pdf_native")]
    pub advanced_pdf_native: AdvancedPdfNativeMode,

    /// Preserve running headers in advanced native PDF extraction.
    #[config_meta(
        env = "ANNO_RAG_PDF_KEEP_HEADERS",
        cli = "--pdf-keep-headers",
        doc = "Preserve running headers in advanced native PDF. Default: false",
        since = "0.11"
    )]
    #[serde(default)]
    pub pdf_keep_headers: bool,

    /// Preserve running footers in advanced native PDF extraction.
    #[config_meta(
        env = "ANNO_RAG_PDF_KEEP_FOOTERS",
        cli = "--pdf-keep-footers",
        doc = "Preserve running footers in advanced native PDF. Default: false",
        since = "0.11"
    )]
    #[serde(default)]
    pub pdf_keep_footers: bool,

    /// Extract PDF annotations in advanced native PDF extraction.
    #[config_meta(
        env = "ANNO_RAG_PDF_EXTRACT_ANNOTATIONS",
        cli = "--pdf-extract-annotations",
        doc = "Extract PDF annotations in advanced native PDF. Default: false",
        since = "0.11"
    )]
    #[serde(default)]
    pub pdf_extract_annotations: bool,

    /// Font-size cluster count for Kreuzberg PDF hierarchy detection.
    #[config_meta(
        env = "ANNO_RAG_PDF_HIERARCHY_CLUSTERS",
        cli = "--pdf-hierarchy-clusters",
        doc = "Font-size cluster count for PDF hierarchy (1-7). Default: 6",
        since = "0.11"
    )]
    #[serde(default = "default_pdf_hierarchy_clusters")]
    pub pdf_hierarchy_clusters: usize,

    /// Allow single-column pseudo-tables in native PDF table extraction.
    #[config_meta(
        env = "ANNO_RAG_PDF_ALLOW_SINGLE_COLUMN_TABLES",
        cli = "--pdf-allow-single-column-tables",
        doc = "Allow single-column pseudo-tables in PDF extraction. Default: false",
        since = "0.11"
    )]
    #[serde(default)]
    pub pdf_allow_single_column_tables: bool,

    /// Emit diagnostic structured sidecar data for advanced native PDFs.
    #[config_meta(
        env = "ANNO_RAG_PDF_STRUCTURED_SIDECAR",
        cli = "--pdf-structured-sidecar",
        doc = "Emit diagnostic structured sidecar for advanced PDFs. Default: false",
        since = "0.12"
    )]
    #[serde(default)]
    pub pdf_structured_sidecar: bool,

    /// Embedder weight dtype. `"f32"` (default) or `"f16"` (experimental
    /// opt-in). Read by `Embedder::load`. `None` → `"f32"`. F16 halves
    /// embedder RSS (~236 MB) but the e5-small BERT forward can produce
    /// degenerate (NaN) vectors on CPU — opt-in until numerically stable.
    #[config_meta(
        env = "ANNO_RAG_EMBEDDER_DTYPE",
        cli = "--embedder-dtype",
        doc = "Embedder weight dtype: f32 (default) or f16 (experimental). Default: none (f32)",
        since = "0.12"
    )]
    #[serde(default)]
    pub embedder_dtype: Option<String>,

    /// Runtime accelerator preference. Defaults to `auto`; `ANNO_ACCELERATOR`
    /// overrides this at process start.
    #[config_meta(
        env = "ANNO_ACCELERATOR",
        cli = "--accelerator",
        doc = "Runtime accelerator: auto|cpu|metal|cuda. Default: auto",
        since = "0.10"
    )]
    #[serde(default)]
    pub accelerator: AcceleratorPreference,

    /// Reranker repo id on HuggingFace Hub (cross-encoder, opt-in).
    #[config_meta(
        env = "ANNO_RAG_RERANK_MODEL",
        cli = "--rerank-model",
        doc = "HF Hub model ID for the cross-encoder reranker. Default: onnx-community/bge-reranker-v2-m3-ONNX",
        since = "0.12"
    )]
    #[serde(default = "default_rerank_model")]
    pub rerank_model: String,

    /// ONNX file within `rerank_model`. INT8 by default; point at
    /// "onnx/model_q4f16.onnx" (702 MB) if INT8 regresses on your corpus.
    #[config_meta(
        env = "ANNO_RAG_RERANK_ONNX_FILE",
        cli = "--rerank-onnx-file",
        doc = "ONNX file within rerank_model. Default: onnx/model_int8.onnx",
        since = "0.12"
    )]
    #[serde(default = "default_rerank_onnx_file")]
    pub rerank_onnx_file: String,

    /// RRF candidates to over-fetch before reranking. Default 30.
    #[config_meta(
        env = "ANNO_RAG_RERANK_POOL_SIZE",
        cli = "--rerank-pool-size",
        doc = "RRF candidates to over-fetch before reranking. Default: 30",
        since = "0.12"
    )]
    #[serde(default = "default_rerank_pool_size")]
    pub rerank_pool_size: usize,

    /// Max (query,passage) pairs per ONNX forward batch. Default 8.
    #[config_meta(
        env = "ANNO_RAG_RERANK_BATCH_SIZE",
        cli = "--rerank-batch-size",
        doc = "Max (query,passage) pairs per ONNX reranker batch. Default: 8",
        since = "0.12"
    )]
    #[serde(default = "default_rerank_batch_size")]
    pub rerank_batch_size: usize,

    /// Name of the LanceDB collection that stores memories (v0.1 default
    /// `"memories"`). Lives alongside the `chunks` documents table.
    #[config_meta(
        env = "ANNO_RAG_MEMORY_COLLECTION_NAME",
        cli = "--memory-collection-name",
        doc = "LanceDB collection name for memories. Default: memories",
        since = "0.8"
    )]
    #[serde(default = "default_memory_collection_name")]
    pub memory_collection_name: String,

    /// Embedding dimension for memory vectors. Matches `embed_dim` by
    /// default (384 for e5-small) but kept independent so the memory
    /// store can migrate to a different embedder than documents.
    #[config_meta(
        env = "ANNO_RAG_MEMORY_EMBEDDING_DIM",
        cli = "--memory-embedding-dim",
        doc = "Embedding dimension for memory vectors. Default: 1024",
        since = "0.8"
    )]
    #[serde(default = "default_memory_embedding_dim")]
    pub memory_embedding_dim: usize,

    /// NER mode for `memory_save`. Default: async.
    ///
    /// - `disabled`: embed + store raw text only.
    /// - `async`: embed + store immediately, then enrich NER fields in background.
    /// - `sync`: full legacy pipeline inline.
    #[config_meta(
        env = "ANNO_RAG_MEMORY_NER_MODE",
        cli = "--memory-ner-mode",
        doc = "NER mode for memory_save: disabled|async|sync. Default: async",
        since = "0.9"
    )]
    #[serde(default = "default_memory_ner_mode")]
    pub memory_ner_mode: MemoryNerMode,

    /// Interval between background compactions (seconds). Default: 24h.
    /// Drives the GDPR Art. 17 erasure SLO — physical bytes reclaim
    /// happens within at most this interval after `forget_memory`.
    #[config_meta(
        env = "ANNO_RAG_COMPACTION_INTERVAL_SECS",
        cli = "--compaction-interval-secs",
        doc = "Seconds between background compactions. Default: 86400 (24h)",
        since = "0.9"
    )]
    #[serde(default = "default_compaction_interval_secs")]
    pub compaction_interval_secs: u64,

    /// Minimum age of a tombstone before compaction reclaims it (seconds).
    /// Default: 1h. Prevents thrashing on a hot delete-then-write loop.
    #[config_meta(
        env = "ANNO_RAG_COMPACTION_MIN_AGE_SECS",
        cli = "--compaction-min-age-secs",
        doc = "Minimum tombstone age before compaction (seconds). Default: 3600",
        since = "0.9"
    )]
    #[serde(default = "default_compaction_min_age_secs")]
    pub compaction_min_age_secs: u64,

    /// Per-tenant entity alias map applied during canonicalisation
    /// (`canonicalize_entity`). Keys are the already-canonical surface
    /// form (lowercase + diacritic strip + punct strip + whitespace
    /// collapse); values are the substituted form. Empty by default —
    /// each cabinet builds its own (e.g. `"me dupont" → "dupont"`).
    /// v0.6 candidate: load from a TOML file alongside the vault.
    #[config_meta(
        env = "ANNO_RAG_ENTITY_ALIASES",
        cli = "--entity-aliases",
        doc = "JSON object mapping canonical entity surface forms to substituted forms. Default: {}",
        since = "0.10"
    )]
    #[serde(default)]
    pub entity_aliases: std::collections::HashMap<String, String>,

    /// Cosine-similarity threshold above which two `Preference` /
    /// `Reference` memories with a shared entity are treated as a
    /// conflict (the prior is auto-invalidated on save). Default 0.85.
    /// Tune up to reduce false-positive invalidation; tune down to
    /// catch more re-statements.
    #[config_meta(
        env = "ANNO_RAG_CONFLICT_COSINE_THRESHOLD",
        cli = "--conflict-cosine-threshold",
        doc = "Cosine threshold for memory conflict detection (0.0-1.0). Default: 0.85",
        since = "0.10"
    )]
    #[serde(default = "default_conflict_cosine_threshold")]
    pub conflict_cosine_threshold: f64,

    /// Maximum hop count for `Pipeline::graph_recall`. Default 2.
    /// Caps the BFS depth over `entity_refs`; higher values risk
    /// exponential expansion on popular-entity graphs.
    #[config_meta(
        env = "ANNO_RAG_GRAPH_MAX_HOPS",
        cli = "--graph-max-hops",
        doc = "Maximum BFS hop count for graph_recall. Default: 2",
        since = "0.10"
    )]
    #[serde(default = "default_graph_max_hops")]
    pub graph_max_hops: u8,

    /// Per-hop row limit for `Pipeline::graph_recall`. Default 50.
    /// Bounds the candidate set scanned at each BFS hop to keep
    /// graph recall sub-quadratic on hot entities.
    #[config_meta(
        env = "ANNO_RAG_GRAPH_PER_HOP_LIMIT",
        cli = "--graph-per-hop-limit",
        doc = "Max candidates per BFS hop in graph_recall. Default: 50",
        since = "0.10"
    )]
    #[serde(default = "default_graph_per_hop_limit")]
    pub graph_per_hop_limit: usize,
}

fn default_memory_collection_name() -> String {
    "memories".to_string()
}

fn default_memory_embedding_dim() -> usize {
    1024
}

fn default_compaction_interval_secs() -> u64 {
    24 * 3600
}

fn default_compaction_min_age_secs() -> u64 {
    3600
}

fn default_conflict_cosine_threshold() -> f64 {
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
            embed_model: default_embed_model(),
            embed_dim: default_embed_dim(),
            default_top_k: 10,
            chunk_max_chars: 2048,
            chunk_overlap: 256,
            gdpr_layers: GdprLayerSet::Defense,
            vector_index_threshold: default_vector_index_threshold(),
            ner_warmup_model: default_ner_warmup_model(),
            ner_model_id: default_ner_model_id(),
            ner_onnx_precision: default_ner_onnx_precision(),
            index_distance: default_index_distance(),
            index_num_partitions: None,
            search_nprobes: default_search_nprobes(),
            search_refine_factor: default_search_refine_factor(),
            ner_candle_model_id: default_ner_candle_model_id(),
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
    /// Load configuration: defaults → TOML file → env vars → overrides.
    ///
    /// If `file_path` is `None` or the path does not exist the defaults are
    /// used as base and only env vars (and optional overrides) are applied.
    pub fn load_from_file(
        file_path: Option<&std::path::Path>,
        overrides: Option<&ConfigOverrides>,
    ) -> Result<Self, ConfigLoadError> {
        let mut cfg = Self::default();

        if let Some(path) = file_path {
            if path.exists() {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| ConfigLoadError::Io(format!("read {}: {e}", path.display())))?;
                let from_file: Self = toml::from_str(&contents)
                    .map_err(|e| ConfigLoadError::Toml(format!("{}: {e}", path.display())))?;
                cfg = from_file;
            }
        }

        cfg.apply_env();

        if let Some(ov) = overrides {
            cfg.apply_overrides(ov);
        }

        Ok(cfg)
    }

    /// Load from `~/.anno-rag/config.toml` (default path).
    ///
    /// Falls back to `load_from_file(None, overrides)` when the home
    /// directory cannot be determined.
    pub fn load(overrides: Option<&ConfigOverrides>) -> Result<Self, ConfigLoadError> {
        let default_path = Self::default_config_path();
        Self::load_from_file(default_path.as_deref(), overrides)
    }

    /// Returns `~/.anno-rag/config.toml` if home dir is determinable.
    #[must_use]
    pub fn default_config_path() -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|h| h.join(".anno-rag").join("config.toml"))
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("ANNO_RAG_DATA_DIR") {
            if !v.is_empty() {
                self.data_dir = std::path::PathBuf::from(v);
            }
        }
        if let Ok(v) = std::env::var("ANNO_GDPR_LAYERS") {
            if let Ok(l) = v.parse::<crate::layers::GdprLayerSet>() {
                self.gdpr_layers = l;
            }
        }
        if let Ok(v) = std::env::var("ANNO_ACCELERATOR") {
            if let Some(a) = crate::accelerator::AcceleratorPreference::from_env_value(&v) {
                self.accelerator = a;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_MEMORY_NER_MODE") {
            if let Some(m) = MemoryNerMode::from_env_value(&v) {
                self.memory_ner_mode = m;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_OCR_MODE") {
            match v.as_str() {
                "off" => self.ocr_mode = OcrMode::Off,
                "auto_embedded" => self.ocr_mode = OcrMode::AutoEmbedded,
                _ => {}
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_EMBED_MODEL") {
            self.embed_model = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_NER_MODEL") {
            self.ner_model_id = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_NER_ONNX_PRECISION") {
            self.ner_onnx_precision = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_INDEX_DISTANCE") {
            self.index_distance = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_INDEX_NUM_PARTITIONS") {
            if let Ok(n) = v.parse::<usize>() {
                self.index_num_partitions = Some(n);
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_SEARCH_NPROBES") {
            if let Ok(n) = v.parse::<usize>() {
                self.search_nprobes = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_SEARCH_REFINE_FACTOR") {
            if let Ok(n) = v.parse::<u32>() {
                self.search_refine_factor = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_NER_CANDLE_MODEL") {
            self.ner_candle_model_id = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_EMBED_DIM") {
            if let Ok(n) = v.parse() {
                self.embed_dim = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_DEFAULT_TOP_K") {
            if let Ok(n) = v.parse() {
                self.default_top_k = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_CHUNK_MAX_CHARS") {
            if let Ok(n) = v.parse() {
                self.chunk_max_chars = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_CHUNK_OVERLAP") {
            if let Ok(n) = v.parse() {
                self.chunk_overlap = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_EMBEDDER_DTYPE") {
            self.embedder_dtype = Some(v);
        }
        if let Ok(v) = std::env::var("ANNO_RAG_RERANK_MODEL") {
            self.rerank_model = v;
        }
        if let Ok(v) = std::env::var("ANNO_RAG_RERANK_BATCH_SIZE") {
            if let Ok(n) = v.parse() {
                self.rerank_batch_size = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_GRAPH_MAX_HOPS") {
            if let Ok(n) = v.parse() {
                self.graph_max_hops = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_GRAPH_PER_HOP_LIMIT") {
            if let Ok(n) = v.parse() {
                self.graph_per_hop_limit = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_CONFLICT_COSINE_THRESHOLD") {
            if let Ok(n) = v.parse() {
                self.conflict_cosine_threshold = n;
            }
        }
        if let Ok(v) = std::env::var("ANNO_RAG_PDF_STRUCTURED_SIDECAR") {
            self.pdf_structured_sidecar =
                matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes");
        }
        if let Ok(v) = std::env::var("ANNO_RAG_OCR_CACHE_ENABLED") {
            self.ocr_cache_enabled =
                !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no");
        }
        if let Ok(v) = std::env::var("ANNO_RAG_OCR_BACKEND") {
            self.ocr_backend = Some(v);
        }
    }

    fn apply_overrides(&mut self, ov: &ConfigOverrides) {
        if let Some(v) = ov.data_dir.clone() {
            self.data_dir = v;
        }
        if let Some(v) = ov.embed_model.clone() {
            self.embed_model = v;
        }
        if let Some(v) = ov.embed_dim {
            self.embed_dim = v;
        }
        if let Some(v) = ov.default_top_k {
            self.default_top_k = v;
        }
        if let Some(v) = ov.chunk_max_chars {
            self.chunk_max_chars = v;
        }
        if let Some(v) = ov.chunk_overlap {
            self.chunk_overlap = v;
        }
        if let Some(v) = ov.gdpr_layers {
            self.gdpr_layers = v;
        }
        if let Some(v) = ov.vector_index_threshold {
            self.vector_index_threshold = v;
        }
        // ner_warmup_model: Option<String> in source → Option<String> in overrides
        if let Some(v) = ov.ner_warmup_model.clone() {
            self.ner_warmup_model = Some(v);
        }
        if let Some(v) = ov.ner_model_id.clone() {
            self.ner_model_id = v;
        }
        if let Some(v) = ov.ner_onnx_precision.clone() {
            self.ner_onnx_precision = v;
        }
        if let Some(v) = ov.index_distance.clone() {
            self.index_distance = v;
        }
        if let Some(v) = ov.index_num_partitions {
            self.index_num_partitions = Some(v);
        }
        if let Some(v) = ov.search_nprobes {
            self.search_nprobes = v;
        }
        if let Some(v) = ov.search_refine_factor {
            self.search_refine_factor = v;
        }
        if let Some(v) = ov.ner_candle_model_id.clone() {
            self.ner_candle_model_id = v;
        }
        if let Some(v) = ov.mcp_server_name.clone() {
            self.mcp_server_name = v;
        }
        if let Some(v) = ov.ocr_mode {
            self.ocr_mode = v;
        }
        if let Some(v) = ov.enable_ocr {
            self.enable_ocr = v;
        }
        // tesseract_path: Option<PathBuf> in source → Option<PathBuf> in overrides
        if let Some(v) = ov.tesseract_path.clone() {
            self.tesseract_path = Some(v);
        }
        // ocr_batch_budget_secs: Option<u64> in source → Option<u64> in overrides
        if let Some(v) = ov.ocr_batch_budget_secs {
            self.ocr_batch_budget_secs = Some(v);
        }
        if let Some(v) = ov.ocr_cache_enabled {
            self.ocr_cache_enabled = v;
        }
        // ocr_backend: Option<String> in source → Option<String> in overrides
        if let Some(v) = ov.ocr_backend.clone() {
            self.ocr_backend = Some(v);
        }
        if let Some(v) = ov.advanced_pdf_native {
            self.advanced_pdf_native = v;
        }
        if let Some(v) = ov.pdf_keep_headers {
            self.pdf_keep_headers = v;
        }
        if let Some(v) = ov.pdf_keep_footers {
            self.pdf_keep_footers = v;
        }
        if let Some(v) = ov.pdf_extract_annotations {
            self.pdf_extract_annotations = v;
        }
        if let Some(v) = ov.pdf_hierarchy_clusters {
            self.pdf_hierarchy_clusters = v;
        }
        if let Some(v) = ov.pdf_allow_single_column_tables {
            self.pdf_allow_single_column_tables = v;
        }
        if let Some(v) = ov.pdf_structured_sidecar {
            self.pdf_structured_sidecar = v;
        }
        // embedder_dtype: Option<String> in source → Option<String> in overrides
        if let Some(v) = ov.embedder_dtype.clone() {
            self.embedder_dtype = Some(v);
        }
        if let Some(v) = ov.accelerator {
            self.accelerator = v;
        }
        if let Some(v) = ov.rerank_model.clone() {
            self.rerank_model = v;
        }
        if let Some(v) = ov.rerank_onnx_file.clone() {
            self.rerank_onnx_file = v;
        }
        if let Some(v) = ov.rerank_pool_size {
            self.rerank_pool_size = v;
        }
        if let Some(v) = ov.rerank_batch_size {
            self.rerank_batch_size = v;
        }
        if let Some(v) = ov.memory_collection_name.clone() {
            self.memory_collection_name = v;
        }
        if let Some(v) = ov.memory_embedding_dim {
            self.memory_embedding_dim = v;
        }
        if let Some(v) = ov.memory_ner_mode {
            self.memory_ner_mode = v;
        }
        if let Some(v) = ov.compaction_interval_secs {
            self.compaction_interval_secs = v;
        }
        if let Some(v) = ov.compaction_min_age_secs {
            self.compaction_min_age_secs = v;
        }
        // entity_aliases: HashMap — skipped by ConfigCliArgs (#[arg(skip)]), never set from CLI
        if let Some(v) = ov.conflict_cosine_threshold {
            self.conflict_cosine_threshold = v;
        }
        if let Some(v) = ov.graph_max_hops {
            self.graph_max_hops = v;
        }
        if let Some(v) = ov.graph_per_hop_limit {
            self.graph_per_hop_limit = v;
        }
    }

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

    /// Full HF repo ID used as a two-level cache path.
    ///
    /// Example: `"SemplificaAI/gliner2-multi-v1-onnx"` → `"SemplificaAI/gliner2-multi-v1-onnx"`
    /// Edge case: `"local-model"` (no `/`) → `"local-model"` (single-level, unchanged)
    #[must_use]
    pub fn ner_onnx_dir(&self) -> String {
        self.ner_model_id.clone()
    }

    /// Full candle model ID with `"-candle"` appended.
    ///
    /// Example: `"fastino/gliner2-multi-v1"` → `"fastino/gliner2-multi-v1-candle"`
    #[must_use]
    pub fn ner_candle_dir(&self) -> String {
        format!("{}-candle", self.ner_candle_model_id)
    }

    /// Full embed model ID used as a two-level cache path.
    ///
    /// Example: `"OrdalieTech/Solon-embeddings-large-0.1"` → `"OrdalieTech/Solon-embeddings-large-0.1"`
    #[must_use]
    pub fn embedder_dir(&self) -> String {
        self.embed_model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.embed_dim, 1024);
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
        // absent field → current default (fastino/gliner2-multi-v1)
        assert_eq!(
            c.ner_warmup_model.as_deref(),
            Some("fastino/gliner2-multi-v1")
        );
        assert_eq!(c.mcp_server_name, "anno-rag");
        assert!(!c.enable_ocr);
        assert!(c.tesseract_path.is_none());
        // absent field → current default (auto_embedded)
        assert_eq!(c.ocr_mode, OcrMode::AutoEmbedded);
        assert_eq!(c.effective_ocr_mode(), OcrMode::AutoEmbedded);
        assert_eq!(c.ocr_batch_budget_secs, None);
        assert!(c.embedder_dtype.is_none());
        assert_eq!(c.memory_collection_name, "memories");
        // absent field → current default (1024 for Solon-large)
        assert_eq!(c.memory_embedding_dim, 1024);
        assert!(c.ocr_cache_enabled);
        assert!(c.ocr_backend.is_none());
    }

    #[test]
    fn defaults_include_new_fields() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.vector_index_threshold, 1000);
        assert_eq!(
            c.ner_warmup_model.as_deref(),
            Some("fastino/gliner2-multi-v1")
        );
        assert_eq!(c.mcp_server_name, "anno-rag");
        assert_eq!(c.ocr_mode, OcrMode::AutoEmbedded);
        assert_eq!(c.effective_ocr_mode(), OcrMode::AutoEmbedded);
        assert!(!c.enable_ocr);
        assert!(c.tesseract_path.is_none());
        assert_eq!(c.ocr_batch_budget_secs, None);
        assert!(c.embedder_dtype.is_none());
        assert_eq!(c.memory_collection_name, "memories");
        assert_eq!(c.memory_embedding_dim, 1024);
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
            ocr_mode: OcrMode::Off, // explicitly exercise the legacy compat path
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
        assert!(
            s.contains(r#""gdpr_layers":"basic""#),
            "wire format should be lowercase snake_case"
        );
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
        assert!(
            s.contains("gdpr_layers = \"shadow\""),
            "toml wire format should be lowercase snake_case: {s}"
        );
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

    #[test]
    fn load_from_toml_overrides_defaults() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let toml_path = dir.path().join("config.toml");
        std::fs::write(
            &toml_path,
            r#"
            data_dir = "/tmp/anno-test"
            default_top_k = 42
        "#,
        )
        .expect("write");

        let cfg = AnnoRagConfig::load_from_file(Some(&toml_path), None).expect("load");

        assert_eq!(cfg.default_top_k, 42);
        assert_eq!(cfg.embed_dim, 1024); // unchanged default
    }

    #[test]
    fn load_with_no_file_returns_defaults() {
        let cfg = AnnoRagConfig::load_from_file(None, None).expect("load");
        assert_eq!(cfg.embed_dim, 1024);
        assert_eq!(cfg.default_top_k, 10);
    }

    #[test]
    fn ner_onnx_dir_is_full_model_id() {
        let cfg = AnnoRagConfig::default();
        assert_eq!(cfg.ner_onnx_dir(), "SemplificaAI/gliner2-multi-v1-onnx");
    }

    #[test]
    fn ner_candle_dir_is_full_model_id_with_candle_suffix() {
        let cfg = AnnoRagConfig::default();
        assert_eq!(cfg.ner_candle_dir(), "fastino/gliner2-multi-v1-candle");
    }

    #[test]
    fn ner_onnx_dir_uses_custom_model_id() {
        let mut cfg = AnnoRagConfig::default();
        cfg.ner_model_id = "myorg/my-gliner-onnx".to_string();
        assert_eq!(cfg.ner_onnx_dir(), "myorg/my-gliner-onnx");
    }

    #[test]
    fn ner_candle_dir_uses_custom_candle_model_id() {
        let mut cfg = AnnoRagConfig::default();
        cfg.ner_candle_model_id = "myorg/my-gliner-pt".to_string();
        assert_eq!(cfg.ner_candle_dir(), "myorg/my-gliner-pt-candle");
    }

    #[test]
    fn embedder_dir_is_full_embed_model_id() {
        let cfg = AnnoRagConfig::default();
        assert_eq!(cfg.embedder_dir(), cfg.embed_model.clone());
    }

    #[test]
    fn embedder_dir_uses_custom_embed_model() {
        let mut cfg = AnnoRagConfig::default();
        cfg.embed_model = "OrdalieTech/Solon-embeddings-large-0.1".to_string();
        assert_eq!(cfg.embedder_dir(), "OrdalieTech/Solon-embeddings-large-0.1");
    }

    #[test]
    fn embedder_dir_no_slash_returns_whole_string() {
        let mut cfg = AnnoRagConfig::default();
        cfg.embed_model = "local-model".to_string();
        assert_eq!(cfg.embedder_dir(), "local-model");
    }

    #[test]
    fn ner_onnx_precision_defaults_to_fp16() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.ner_onnx_precision, "fp16");
    }

    #[test]
    fn ner_onnx_precision_round_trips_fp32() {
        let json = r#"{"ner_onnx_precision":"fp32"}"#;
        let c: AnnoRagConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.ner_onnx_precision, "fp32");
    }

    #[test]
    fn index_tuning_defaults() {
        let c = AnnoRagConfig::default();
        assert_eq!(c.index_distance, "cosine");
        assert_eq!(c.index_num_partitions, None);
        assert_eq!(c.search_nprobes, 20);
        assert_eq!(c.search_refine_factor, 10);
    }
}
