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
}

fn default_vector_index_threshold() -> usize {
    1000
}

fn default_mcp_server_name() -> String {
    "anno-rag".to_string()
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
    }

    #[test]
    fn round_trips_through_json() {
        let c = AnnoRagConfig::default();
        let s = serde_json::to_string(&c).expect("serialize");
        let back: AnnoRagConfig = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(c.embed_dim, back.embed_dim);
        assert_eq!(c.default_top_k, back.default_top_k);
    }
}
