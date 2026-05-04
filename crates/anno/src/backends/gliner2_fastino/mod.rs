//! gliner2_fastino — fastino-ai GLiNER2 backend (issue #18).
//!
//! **Status:** experimental / WIP. No API stability guarantees in Phase 1.
//!
//! Loads `fastino/gliner2-*` ONNX models (Zaratiana et al. 2025,
//! arXiv:2507.18546). Distinct from `gliner_multitask` (which loads GLiNER v1
//! multi-task models with hardcoded `<<ENT>>=128002` IDs and rejects any
//! `fastino/*` model id at the discovery layer).
//!
//! # Architecture deltas vs `gliner_multitask`
//!
//! - Special-token vocabulary: `[P]`, `[E]`, `[C]`, `[L]`, `[R]`,
//!   `[SEP_STRUCT]`, `[SEP_TEXT]`. IDs read from `tokenizer.json` at load
//!   time; never hardcoded.
//! - Prompt format: `( [P] task_name ( [E] label1 [E] label2 ) ) [SEP_TEXT] tokens...`
//! - Span scoring: dot-product similarity (Eq. 1 of arXiv:2507.18546).
//!
//! # LoRA
//!
//! Phase 1 does **not** support runtime LoRA adapter loading. To use a
//! LoRA-fine-tuned model, merge the adapter into the base weights and
//! re-export to ONNX:
//!
//! ```bash
//! python scripts/gliner2_export_onnx.py \
//!     --base fastino/gliner2-multi-v1 \
//!     --lora-adapter ./my_adapter \
//!     --output ./my_merged.onnx
//! ```
//!
//! Pointing `from_local` at a directory containing `adapter_config.json`
//! returns [`errors::Error::LoraAdapterNotSupported`].
//!
//! # Source attribution
//!
//! `processor.rs` is adapted from SemplificaAI/gliner2-rs (Apache-2.0):
//! <https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/processor.rs>

#![cfg(feature = "gliner2-fastino")]

pub mod errors;
pub(crate) mod config;
pub(crate) mod decoder;
pub(crate) mod processor;
pub(crate) mod session;

/// fastino-ai GLiNER2 model.
///
/// **Experimental.** API may change without semver bump.
pub struct GLiNER2Fastino {
    pub(crate) tokenizer: tokenizers::Tokenizer,
    pub(crate) special: processor::SpecialTokenIds,
    pub(crate) transformer: processor::SchemaTransformer,
    pub(crate) config: config::FastinoConfig,
    pub(crate) session: session::Session,
    pub(crate) model_id: String,
}

impl std::fmt::Debug for GLiNER2Fastino {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLiNER2Fastino")
            .field("model_id", &self.model_id)
            .field("hidden_size", &self.config.hidden_size)
            .finish()
    }
}

use std::path::Path;

impl GLiNER2Fastino {
    /// Load a fastino GLiNER2 model from a local directory.
    pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
        if model_dir.join("adapter_config.json").exists() {
            return Err(errors::Error::LoraAdapterNotSupported {
                path: model_dir.to_path_buf(),
            }
            .into());
        }

        let tokenizer_path = model_dir.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(errors::Error::TokenizerMissing(tokenizer_path).into());
        }
        let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: tokenizer: {e}")))?;

        let special = processor::SpecialTokenIds::resolve(&tokenizer)?;
        let transformer = processor::SchemaTransformer::new(tokenizer.clone())?;
        let config = config::FastinoConfig::from_path(&model_dir.join("config.json"))?;

        // Try common ONNX filenames; fastino exports use `model.onnx` and
        // SemplificaAI's pin uses the same. Phase 3 will add `_iobinding` variants.
        let onnx_candidates = ["model.onnx", "onnx/model.onnx"];
        let model_path = onnx_candidates
            .iter()
            .map(|f| model_dir.join(f))
            .find(|p| p.exists())
            .ok_or_else(|| {
                crate::Error::Backend(format!(
                    "gliner2_fastino: no ONNX model in {} (tried {:?})",
                    model_dir.display(),
                    onnx_candidates
                ))
            })?;
        let session = session::Session::from_path(&model_path)?;

        Ok(Self {
            tokenizer,
            special,
            transformer,
            config,
            session,
            model_id: model_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "gliner2_fastino_local".to_string()),
        })
    }

    /// Extract entities for the given labels at the given threshold.
    ///
    /// **Phase 1.** Internal helper called by the public `Model` /
    /// `ZeroShotNER` impls in T18. Empty `types` slice short-circuits with
    /// `Ok(vec![])` without invoking the model.
    pub(crate) fn extract_ner(
        &self,
        text: &str,
        types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<crate::Entity>> {
        if types.is_empty() {
            return Ok(vec![]);
        }

        let labels: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        let task = processor::SchemaTask::Entities(labels.clone());
        let record = self.transformer.transform(text, &[task])?;

        // Build word_offsets table for the original text — used by the decoder
        // to convert (start_word, end_word) to character offsets.
        let splitter = processor::WhitespaceTokenSplitter::new()
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: splitter: {e}")))?;
        let word_offsets: Vec<(usize, usize)> = splitter
            .split_with_offsets(text)
            .into_iter()
            .map(|(_, s, e)| (s, e))
            .collect();

        // Build ndarray inputs.
        let seq_len = record.input_ids.len();
        let input_ids: ndarray::Array2<i64> =
            ndarray::Array2::from_shape_vec((1, seq_len), record.input_ids.clone()).map_err(
                |e| crate::Error::Backend(format!("gliner2_fastino: input_ids reshape: {e}")),
            )?;
        let attention_mask: ndarray::Array2<i64> =
            ndarray::Array2::from_shape_vec((1, seq_len), record.attention_mask.clone())
                .map_err(|e| {
                    crate::Error::Backend(format!("gliner2_fastino: attn reshape: {e}"))
                })?;

        let input_ids_t = crate::backends::ort_compat::tensor_from_ndarray(input_ids)
            .map_err(|e| {
                crate::Error::Backend(format!("gliner2_fastino: input_ids tensor: {e}"))
            })?;
        let attention_mask_t = crate::backends::ort_compat::tensor_from_ndarray(attention_mask)
            .map_err(|e| {
                crate::Error::Backend(format!("gliner2_fastino: attn tensor: {e}"))
            })?;

        // VERIFY against a real fastino ONNX export — input names (`input_ids`,
        // `attention_mask`) and output names (`scores`, `spans`) follow the
        // SemplificaAI export convention. Run:
        //   python -c "import onnx; m=onnx.load('model.onnx'); \
        //              print([(i.name, [d.dim_value or d.dim_param \
        //              for d in i.type.tensor_type.shape.dim]) \
        //              for i in m.graph.input + m.graph.output])"
        // and update names below if they differ.
        //
        // We run inference inside `with_session` and eagerly copy the output
        // tensors into owned Vecs so the `SessionOutputs` borrow does not
        // escape the closure (it borrows from the locked `Session`).
        // Returned as ((score_shape, score_data), (span_shape, span_data)).
        type OwnedTensors = (
            (Vec<i64>, Vec<f32>),
            (Vec<i64>, Vec<i64>),
        );
        let ((score_shape, scores_data), (span_shape, spans_data)): OwnedTensors = self
            .session
            .with_session(|s| -> crate::Result<OwnedTensors> {
                let outputs = s
                    .run(ort::inputs![
                        "input_ids" => input_ids_t.into_dyn(),
                        "attention_mask" => attention_mask_t.into_dyn(),
                    ])
                    .map_err(|e| {
                        crate::Error::Backend(format!("gliner2_fastino: run: {e}"))
                    })?;

                // Extract score and span tensors. The expected shapes are:
                //   scores: [batch=1, num_spans, num_labels] f32
                //   spans:  [batch=1, num_spans, 2]          i64 (start_word, end_word)
                // VERIFY shapes against a real export — see comment above.
                let scores_val = outputs.get("scores").ok_or_else(|| {
                    crate::Error::Backend(
                        "gliner2_fastino: missing 'scores' output".into(),
                    )
                })?;
                let spans_val = outputs.get("spans").ok_or_else(|| {
                    crate::Error::Backend("gliner2_fastino: missing 'spans' output".into())
                })?;

                let (score_shape, scores_cow) = scores_val
                    .try_extract_tensor::<f32>()
                    .map_err(|e| {
                        crate::Error::Backend(format!(
                            "gliner2_fastino: scores extract: {e}"
                        ))
                    })?;
                let (span_shape, spans_cow) = spans_val
                    .try_extract_tensor::<i64>()
                    .map_err(|e| {
                        crate::Error::Backend(format!(
                            "gliner2_fastino: spans extract: {e}"
                        ))
                    })?;

                // Copy into owned Vecs before the SessionOutputs borrow ends.
                Ok((
                    (score_shape.to_vec(), scores_cow.to_vec()),
                    (span_shape.to_vec(), spans_cow.to_vec()),
                ))
            })?;

        // Validate shapes. score_shape / span_shape are Vec<i64>.
        // Expected: scores [1, num_spans, num_labels], spans [1, num_spans, 2].
        if score_shape.len() != 3 {
            return Err(crate::Error::Backend(format!(
                "gliner2_fastino: scores shape len {} (expected 3)",
                score_shape.len()
            )));
        }
        if span_shape.len() != 3 || span_shape[2] != 2 {
            return Err(crate::Error::Backend(format!(
                "gliner2_fastino: spans shape {:?} (expected [B, N, 2])",
                span_shape
            )));
        }
        let num_spans = score_shape[1] as usize;
        let num_labels = score_shape[2] as usize;

        // Build Vec<decoder::Span> from flat tensors.
        // scores layout: [batch=0, span_idx, label_idx] → flat index = span_idx * num_labels + label_idx
        // spans layout:  [batch=0, span_idx, col]       → flat index = span_idx * 2 + col
        let mut decoded: Vec<decoder::Span> = Vec::new();
        for span_idx in 0..num_spans {
            let start_word = spans_data[span_idx * 2] as usize;
            let end_word = spans_data[span_idx * 2 + 1] as usize;
            for label_idx in 0..num_labels {
                let score = scores_data[span_idx * num_labels + label_idx];
                // Apply sigmoid here if the head produces logits — fastino's export
                // is documented as already-sigmoided per the upstream paper Eq. 1.
                // VERIFY: if entities come back saturated at 1.0, the head is logits
                // and this should be `1.0 / (1.0 + (-score).exp())`.
                decoded.push(decoder::Span {
                    start_word,
                    end_word,
                    label_idx,
                    score,
                });
            }
        }

        Ok(decoder::decode_spans(text, &word_offsets, &labels, &decoded, threshold))
    }

    /// Load a fastino GLiNER2 model by Hugging Face model id.
    ///
    /// Downloads `tokenizer.json`, `config.json`, and the ONNX model file
    /// (trying `onnx/model.onnx` then `model.onnx`) into the standard HF
    /// cache, then defers to `from_local` on the cache snapshot directory.
    ///
    /// **Phase 1 / experimental.** No retry/backoff on transient HF Hub
    /// failures beyond what `hf-hub` itself provides.
    pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
        let api = crate::backends::hf_loader::hf_api()
            .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: hf_api: {e}")))?;
        let repo = api.model(model_id.to_string());

        let _model_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["onnx/model.onnx", "model.onnx"],
        )
        .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download model: {e}")))?;
        let tokenizer_path =
            crate::backends::hf_loader::download_model_file(&repo, &["tokenizer.json"])
                .map_err(|e| {
                    crate::Error::Backend(format!("gliner2_fastino: download tokenizer: {e}"))
                })?;
        let _config_path =
            crate::backends::hf_loader::download_model_file(&repo, &["config.json"])
                .map_err(|e| {
                    crate::Error::Backend(format!("gliner2_fastino: download config: {e}"))
                })?;

        // hf_loader::download_model_file returns paths in the HF cache. Their
        // common parent is the snapshot dir.
        let snapshot_dir = tokenizer_path.parent().ok_or_else(|| {
            crate::Error::Backend("gliner2_fastino: tokenizer parent missing".into())
        })?;
        let mut model = Self::from_local(snapshot_dir)?;
        model.model_id = model_id.to_string();
        Ok(model)
    }
}

#[cfg(test)]
mod from_local_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn from_local_rejects_lora_adapter_dir() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("adapter_config.json"), "{}").unwrap();

        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("scripts/gliner2_export_onnx.py"), "missing script path: {msg}");
        assert!(msg.contains("--lora-adapter"), "missing flag: {msg}");
    }

    #[test]
    fn from_local_missing_tokenizer_returns_typed_error() {
        let dir = tempdir().unwrap();
        // Empty directory — no tokenizer.json, no adapter_config.json.
        let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
        assert!(err.to_string().contains("tokenizer"), "got {err}");
    }
}
