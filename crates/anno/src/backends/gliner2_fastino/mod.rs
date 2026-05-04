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
