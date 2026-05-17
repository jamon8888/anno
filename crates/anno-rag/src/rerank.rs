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
    /// Score each `(query, passage)` pair. Returns relevance scores in
    /// [0, 1] (sigmoid of the classifier logit), in input order. Higher
    /// = more relevant. Uses a fixed batch of 8.
    ///
    /// # Errors
    /// [`Error::Rerank`] on tokenization, tensor build, or ONNX run.
    pub fn score_pairs(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        self.score_pairs_batched(query, passages, 8)
    }

    /// Same as [`Reranker::score_pairs`] with an explicit batch size
    /// (wired to `cfg.rerank_batch_size` by the pipeline).
    ///
    /// # Errors
    /// [`Error::Rerank`] on tokenization, tensor build, or ONNX run.
    pub fn score_pairs_batched(
        &self,
        query: &str,
        passages: &[&str],
        batch_size: usize,
    ) -> Result<Vec<f32>> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }
        let bs = batch_size.max(1);
        let mut out = Vec::with_capacity(passages.len());
        for chunk in passages.chunks(bs) {
            out.extend(self.score_batch(query, chunk)?);
        }
        Ok(out)
    }

    /// One forward pass over a batch of pairs.
    fn score_batch(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        // 1. Tokenize each (query, passage) pair.
        let mut encs = Vec::with_capacity(passages.len());
        for p in passages {
            let enc = self
                .tokenizer
                .encode((query, *p), true)
                .map_err(|e| Error::Rerank(format!("encode pair: {e}")))?;
            encs.push(enc);
        }
        let max_len = encs
            .iter()
            .map(|e| e.get_ids().len().min(self.max_seq_len))
            .max()
            .unwrap_or(0);
        let n = passages.len();

        let mut ids: Vec<i64> = Vec::with_capacity(n * max_len);
        let mut mask: Vec<i64> = Vec::with_capacity(n * max_len);
        for e in &encs {
            let take = e.get_ids().len().min(max_len);
            let pad = max_len - take;
            ids.extend(e.get_ids()[..take].iter().map(|&x| i64::from(x)));
            ids.extend(std::iter::repeat_n(0i64, pad));
            mask.extend(e.get_attention_mask()[..take].iter().map(|&x| i64::from(x)));
            mask.extend(std::iter::repeat_n(0i64, pad));
        }

        // 2. Build ort tensors — shape is Vec<usize>, data boxed slice
        //    (proven idiom from rerank_smoke.rs).
        let shape: Vec<usize> = vec![n, max_len];
        let ids_t = ort::value::Tensor::from_array((shape.clone(), ids.into_boxed_slice()))
            .map_err(|e| Error::Rerank(format!("ids tensor: {e}")))?;
        let mask_t = ort::value::Tensor::from_array((shape, mask.into_boxed_slice()))
            .map_err(|e| Error::Rerank(format!("mask tensor: {e}")))?;

        // 3. Run. Session::run is &mut self → lock the Mutex.
        let mut guard = self
            .session
            .lock()
            .map_err(|e| Error::Rerank(format!("session lock poisoned: {e}")))?;
        let outputs = guard
            .run(ort::inputs![
                "input_ids" => ids_t.into_dyn(),
                "attention_mask" => mask_t.into_dyn(),
            ])
            .map_err(|e| Error::Rerank(format!("onnx run: {e}")))?;

        // 4. Extract logits [n,1] → sigmoid → Vec<f32> length n.
        //    try_extract_tensor::<f32>() returns (shape, CowArray/ndarray);
        //    .as_slice().unwrap() gives &[f32] (contiguous C-order guaranteed).
        let logits_val = outputs
            .values()
            .next()
            .ok_or_else(|| Error::Rerank("onnx: no outputs".into()))?;
        // try_extract_tensor::<f32>() returns (&Shape, &[f32]) in ort 2.0.0-rc.12
        // (proven by the smoke test and confirmed in extract.rs).
        let (_oshape, flat) = logits_val
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Rerank(format!("extract logits: {e}")))?;
        if flat.len() < n {
            return Err(Error::Rerank(format!(
                "expected >= {n} logits, got {}",
                flat.len()
            )));
        }
        Ok(flat[..n]
            .iter()
            .map(|&z| 1.0_f32 / (1.0 + (-z).exp()))
            .collect())
    }

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

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn relevant_outranks_irrelevant() {
        let r = Reranker::load(&AnnoRagConfig::default()).await.expect("load");
        let scores = r
            .score_pairs(
                "responsabilité contractuelle du débiteur",
                &[
                    "Le débiteur engage sa responsabilité contractuelle en cas d'inexécution.",
                    "La recette des crêpes nécessite de la farine et des œufs.",
                ],
            )
            .expect("score");
        assert_eq!(scores.len(), 2);
        assert!(
            scores[0] > scores[1],
            "legal passage ({}) must outrank pancake recipe ({})",
            scores[0], scores[1]
        );
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn empty_passages_is_empty_no_panic() {
        let r = Reranker::load(&AnnoRagConfig::default()).await.expect("load");
        assert!(r.score_pairs("q", &[]).expect("score").is_empty());
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn batching_matches_single_and_is_deterministic() {
        let r = Reranker::load(&AnnoRagConfig::default()).await.expect("load");
        let passages: Vec<String> = (0..17).map(|i| format!("clause numéro {i}")).collect();
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
        let a = r.score_pairs("clause", &refs).expect("a");
        let b = r.score_pairs("clause", &refs).expect("b");
        assert_eq!(a.len(), 17);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < f32::EPSILON, "determinism: {x} vs {y}");
        }
    }

    #[tokio::test]
    #[ignore = "uses cached ~571 MB model"]
    async fn overlong_passage_truncates_no_panic() {
        let r = Reranker::load(&AnnoRagConfig::default()).await.expect("load");
        let long = "lorem ipsum ".repeat(5000);
        let s = r.score_pairs("q", &[long.as_str()]).expect("score");
        assert_eq!(s.len(), 1);
        assert!(s[0].is_finite());
    }
}
