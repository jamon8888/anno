//! Runtime configuration for `anno-rag`.
//!
//! v0.1: hard-coded defaults sourced via [`AnnoRagConfig::default`].
//! TOML file loading lands in v0.2.

use crate::accelerator::AcceleratorPreference;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnoRagConfig {
    /// Root directory for `vault.enc`, `index.lance`, and cached model weights.
    pub data_dir: PathBuf,
    /// HuggingFace model ID for the embedder.
    pub embed_model: String,
    /// Vector dimension. Must match the embedder's output size.
    pub embed_dim: usize,
    /// Default top-K returned by `search`.
    pub default_top_k: usize,
    /// Max chunk size in characters (passed to kreuzberg's chunker).
    pub chunk_max_chars: usize,
    /// Chunk overlap in characters.
    pub chunk_overlap: usize,

    /// Threshold (chunk count) at which `Store::maybe_build_index` will
    /// build an IVF_HNSW_SQ index on the vector column. Below this, flat
    /// scan suffices. Default: 1000.
    #[serde(default = "default_vector_index_threshold")]
    pub vector_index_threshold: usize,

    /// HF Hub model ID for the NER model to pre-warm. `None` skips NER
    /// warmup. Default: `None` (callers/warmup script pick the candidate).
    #[serde(default)]
    pub ner_warmup_model: Option<String>,

    /// MCP server name advertised on `initialize`. Default: `"anno-rag"`.
    #[serde(default = "default_mcp_server_name")]
    pub mcp_server_name: String,

    /// Runtime OCR mode. Default: `off`.
    #[serde(default = "default_ocr_mode")]
    pub ocr_mode: OcrMode,

    /// Legacy OCR flag retained for older config files and CLI compatibility.
    /// When true, [`Self::effective_ocr_mode`] maps it to
    /// [`OcrMode::AutoEmbedded`].
    #[serde(default)]
    pub enable_ocr: bool,

    /// Legacy path to the system `tesseract` binary. The embedded OCR path does
    /// not use this field; it is retained while the external fallback is phased
    /// out.
    #[serde(default)]
    pub tesseract_path: Option<PathBuf>,

    /// Optional per-folder OCR wall-clock budget in seconds. When exhausted,
    /// additional scanned PDFs/pages are deferred instead of being OCR'd.
    #[serde(default)]
    pub ocr_batch_budget_secs: Option<u64>,

    /// Native text-layer PDF extraction profile. Default: `off`.
    #[serde(default = "default_advanced_pdf_native")]
    pub advanced_pdf_native: AdvancedPdfNativeMode,

    /// Preserve running headers in advanced native PDF extraction.
    #[serde(default)]
    pub pdf_keep_headers: bool,

    /// Preserve running footers in advanced native PDF extraction.
    #[serde(default)]
    pub pdf_keep_footers: bool,

    /// Extract PDF annotations in advanced native PDF extraction.
    #[serde(default)]
    pub pdf_extract_annotations: bool,

    /// Font-size cluster count for Kreuzberg PDF hierarchy detection.
    #[serde(default = "default_pdf_hierarchy_clusters")]
    pub pdf_hierarchy_clusters: usize,

    /// Allow single-column pseudo-tables in native PDF table extraction.
    #[serde(default)]
    pub pdf_allow_single_column_tables: bool,

    /// Emit diagnostic structured sidecar data for advanced native PDFs.
    #[serde(default)]
    pub pdf_structured_sidecar: bool,

    /// Embedder weight dtype. `"f32"` (default) or `"f16"` (experimental
    /// opt-in). Read by `Embedder::load`. `None` → `"f32"`. F16 halves
    /// embedder RSS (~236 MB) but the e5-small BERT forward can produce
    /// degenerate (NaN) vectors on CPU — opt-in until numerically stable.
    #[serde(default)]
    pub embedder_dtype: Option<String>,

    /// Runtime accelerator preference. Defaults to `auto`; `ANNO_ACCELERATOR`
    /// overrides this at process start.
    #[serde(default)]
    pub accelerator: AcceleratorPreference,

    /// Reranker repo id on HuggingFace Hub (cross-encoder, opt-in).
    #[serde(default = "default_rerank_model")]
    pub rerank_model: String,

    /// ONNX file within `rerank_model`. INT8 by default; point at
    /// "onnx/model_q4f16.onnx" (702 MB) if INT8 regresses on your corpus.
    #[serde(default = "default_rerank_onnx_file")]
    pub rerank_onnx_file: String,

    /// RRF candidates to over-fetch before reranking. Default 30.
    #[serde(default = "default_rerank_pool_size")]
    pub rerank_pool_size: usize,

    /// Max (query,passage) pairs per ONNX forward batch. Default 8.
    #[serde(default = "default_rerank_batch_size")]
    pub rerank_batch_size: usize,

    /// Name of the LanceDB collection that stores memories (v0.1 default
    /// `"memories"`). Lives alongside the `chunks` documents table.
    #[serde(default = "default_memory_collection_name")]
    pub memory_collection_name: String,

    /// Embedding dimension for memory vectors. Matches `embed_dim` by
    /// default (384 for e5-small) but kept independent so the memory
    /// store can migrate to a different embedder than documents.
    #[serde(default = "default_memory_embedding_dim")]
    pub memory_embedding_dim: usize,

    /// NER mode for `memory_save`. Default: async.
    ///
    /// - `disabled`: embed + store raw text only.
    /// - `async`: embed + store immediately, then enrich NER fields in background.
    /// - `sync`: full legacy pipeline inline.
    #[serde(default = "default_memory_ner_mode")]
    pub memory_ner_mode: MemoryNerMode,

    /// Interval between background compactions (seconds). Default: 24h.
    /// Drives the GDPR Art. 17 erasure SLO — physical bytes reclaim
    /// happens within at most this interval after `forget_memory`.
    #[serde(default = "default_compaction_interval_secs")]
    pub compaction_interval_secs: u64,

    /// Minimum age of a tombstone before compaction reclaims it (seconds).
    /// Default: 1h. Prevents thrashing on a hot delete-then-write loop.
    #[serde(default = "default_compaction_min_age_secs")]
    pub compaction_min_age_secs: u64,

    /// Per-tenant entity alias map applied during canonicalisation
    /// (`canonicalize_entity`). Keys are the already-canonical surface
    /// form (lowercase + diacritic strip + punct strip + whitespace
    /// collapse); values are the substituted form. Empty by default —
    /// each cabinet builds its own (e.g. `"me dupont" → "dupont"`).
    /// v0.6 candidate: load from a TOML file alongside the vault.
    #[serde(default)]
    pub entity_aliases: std::collections::HashMap<String, String>,

    /// Cosine-similarity threshold above which two `Preference` /
    /// `Reference` memories with a shared entity are treated as a
    /// conflict (the prior is auto-invalidated on save). Default 0.85.
    /// Tune up to reduce false-positive invalidation; tune down to
    /// catch more re-statements.
    #[serde(default = "default_conflict_cosine_threshold")]
    pub conflict_cosine_threshold: f32,

    /// Maximum hop count for `Pipeline::graph_recall`. Default 2.
    /// Caps the BFS depth over `entity_refs`; higher values risk
    /// exponential expansion on popular-entity graphs.
    #[serde(default = "default_graph_max_hops")]
    pub graph_max_hops: u8,

    /// Per-hop row limit for `Pipeline::graph_recall`. Default 50.
    /// Bounds the candidate set scanned at each BFS hop to keep
    /// graph recall sub-quadratic on hot entities.
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
            vector_index_threshold: default_vector_index_threshold(),
            ner_warmup_model: None,
            mcp_server_name: default_mcp_server_name(),
            ocr_mode: default_ocr_mode(),
            enable_ocr: false,
            tesseract_path: None,
            ocr_batch_budget_secs: None,
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
    #[must_use]
    pub fn effective_ocr_mode(&self) -> OcrMode {
        if self.enable_ocr && self.ocr_mode == OcrMode::Off {
            OcrMode::AutoEmbedded
        } else {
            self.ocr_mode
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
}
