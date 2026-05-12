//! Runtime configuration for `anno-rag`.
//!
//! v0.1: hard-coded defaults sourced via [`AnnoRagConfig::default`].
//! TOML file loading lands in v0.2.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Runtime configuration: data paths, model IDs, chunking defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnoRagConfig {
    /// Root directory for `vault.db`, `index.lance`, and cached model weights.
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
        }
    }
}

impl AnnoRagConfig {
    /// Path to the encrypted SqliteVault file.
    #[must_use]
    pub fn vault_path(&self) -> PathBuf {
        self.data_dir.join("vault.db")
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
        let mut c = AnnoRagConfig::default();
        c.data_dir = PathBuf::from("/tmp/anno-rag");
        assert_eq!(c.vault_path(), PathBuf::from("/tmp/anno-rag/vault.db"));
        assert_eq!(c.index_path(), PathBuf::from("/tmp/anno-rag/index.lance"));
        assert_eq!(c.models_cache(), PathBuf::from("/tmp/anno-rag/models"));
        assert_eq!(c.outputs_dir(), PathBuf::from("/tmp/anno-rag/outputs"));
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
