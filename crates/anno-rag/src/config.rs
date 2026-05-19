//! Runtime configuration for `anno-rag`.
//!
//! v0.1: hard-coded defaults sourced via [`AnnoRagConfig::default`].
//! TOML file loading lands in v0.2.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

    /// Enable OCR fallback when a PDF has no text layer. Default: false.
    /// Requires system `tesseract` binary on PATH (or `tesseract_path` set).
    #[serde(default)]
    pub enable_ocr: bool,

    /// Explicit path to the system `tesseract` binary. Default: PATH lookup.
    #[serde(default)]
    pub tesseract_path: Option<PathBuf>,

    /// Embedder weight dtype. `"f32"` (default) or `"f16"` (experimental
    /// opt-in). Read by `Embedder::load`. `None` → `"f32"`. F16 halves
    /// embedder RSS (~236 MB) but the e5-small BERT forward can produce
    /// degenerate (NaN) vectors on CPU — opt-in until numerically stable.
    #[serde(default)]
    pub embedder_dtype: Option<String>,

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

impl Default for AnnoRagConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .map(|p| p.join(".anno-rag"))
            .unwrap_or_else(|| PathBuf::from(".anno-rag"));

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
            enable_ocr: false,
            tesseract_path: None,
            embedder_dtype: None,
            memory_collection_name: default_memory_collection_name(),
            memory_embedding_dim: default_memory_embedding_dim(),
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
        assert!(!c.enable_ocr);
        assert!(c.tesseract_path.is_none());
        assert!(c.embedder_dtype.is_none());
        assert_eq!(c.memory_collection_name, "memories");
        assert_eq!(c.memory_embedding_dim, 384);
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
