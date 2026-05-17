//! Cross-encoder reranker: BGE-reranker-v2-m3, pre-quantized INT8 ONNX,
//! run via `ort`. Scores (query, passage) pairs; higher = more relevant.
//!
//! Owns the ONNX session + tokenizer. Loaded once per process via
//! `Pipeline::reranker()` (lazy `OnceCell`). Depends only on ort,
//! ndarray, tokenizers, hf-hub — NOT on store/pipeline/vault (spec §7).

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use std::sync::Mutex;
use tokenizers::Tokenizer;

/// Loaded cross-encoder reranker.
pub struct Reranker {
    /// `ort::session::Session::run` takes `&mut self`; the session is
    /// behind a `Mutex` so `score_pairs` can take `&self`.
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
    /// Hard cap on combined (query+passage) token length. 512 for
    /// BGE-reranker-v2-m3.
    max_seq_len: usize,
}

impl Reranker {
    /// Fetch the INT8 ONNX + tokenizer from the Hub (cached, same as
    /// `Embedder::load`) and build the ort session.
    ///
    /// # Errors
    /// [`Error::Rerank`] on hub fetch, tokenizer parse, or session build.
    pub async fn load(cfg: &AnnoRagConfig) -> Result<Self> {
        use hf_hub::api::tokio::Api;
        use ort::session::{builder::GraphOptimizationLevel, Session};

        let api = Api::new().map_err(|e| Error::Rerank(format!("hf-hub init: {e}")))?;
        let repo = api.model(cfg.rerank_model.clone());

        let onnx_path = repo
            .get(&cfg.rerank_onnx_file)
            .await
            .map_err(|e| Error::Rerank(format!("onnx fetch {}: {e}", cfg.rerank_onnx_file)))?;
        let tok_path = repo
            .get("tokenizer.json")
            .await
            .map_err(|e| Error::Rerank(format!("tokenizer.json fetch: {e}")))?;

        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| Error::Rerank(format!("tokenizer load: {e}")))?;

        let session = Session::builder()
            .map_err(|e| Error::Rerank(format!("session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| Error::Rerank(format!("opt level: {e}")))?
            .commit_from_file(&onnx_path)
            .map_err(|e| Error::Rerank(format!("commit onnx: {e}")))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            max_seq_len: 512,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "downloads ~571 MB (cached after first run)"]
    async fn load_succeeds() {
        let cfg = AnnoRagConfig::default();
        let r = Reranker::load(&cfg).await.expect("reranker loads");
        assert_eq!(r.max_seq_len, 512);
    }
}
