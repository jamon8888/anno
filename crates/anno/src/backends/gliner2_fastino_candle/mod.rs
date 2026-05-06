//! # gliner2_fastino_candle (Phase 4)
//!
//! Candle backend for fastino-ai GLiNER2 with **runtime LoRA adapter
//! merge-at-load**. Loads PEFT-format adapters and merges them into the
//! base weights at `load_adapter` time, producing a fully-merged model
//! with zero per-forward overhead.
//!
//! Parallel to the ONNX-based [`crate::backends::gliner2_fastino`]. Same
//! public method shapes (Model + ZeroShotNER); users swap backends with
//! a type alias. The differentiator is `load_adapter` / `unload_adapter`.
//!
//! ## When to use this backend
//!
//! - You have multiple domain-specific LoRA adapters (e.g., legal,
//!   medical, financial) trained on the same base model.
//! - You want to switch between domains at runtime without re-exporting
//!   merged ONNX models per domain (which costs ~6 GB on disk per).
//! - Adapter swap rate is moderate (every few minutes/hours, not per
//!   request). For sub-millisecond hot-swap, see optional Phase 4.5.
//!
//! ## Architecture
//!
//! - Encoder: [`candle_transformers::models::debertav2::DebertaV2Model`]
//!   — provides DeBERTa-v2/v3 disentangled attention without anno
//!   reimplementing it ([PR #2743](https://github.com/huggingface/candle/pull/2743)).
//! - Heads: 7 small Candle modules (token_gather, span_rep, schema_gather,
//!   count_pred, count_lstm, scorer, classifier).
//! - LoRA: `W_merged = W_base + (alpha/r) * (lora_B @ lora_A)`, applied
//!   once at `load_adapter` time per target module.

#![cfg(feature = "gliner2-fastino-candle")]
#![allow(dead_code)] // Phase 4 in-progress: methods wired by M5+

pub mod decoder;
pub mod encoder;
pub mod heads;
pub mod lora;
pub mod pipeline;
pub mod processor;

use std::path::{Path, PathBuf};

use candle_core::Device;

/// Phase 4 Candle-based GLiNER2 backend with PEFT LoRA adapter
/// merge-at-load support.
pub struct GLiNER2FastinoCandle {
    pub(crate) tokenizer: tokenizers::Tokenizer,
    pub(crate) device: Device,
    /// Directory containing the base model's tokenizer.json,
    /// config.json, and model.safetensors. Used to re-merge from disk
    /// when `unload_adapter` is called or a new adapter replaces a
    /// previous one.
    pub(crate) base_model_dir: PathBuf,
    pub(crate) encoder: encoder::Encoder,
    pub(crate) heads: heads::AllHeads,
    /// Name of the currently merged adapter, or `None` if running on
    /// pure base weights.
    pub(crate) active_adapter: Option<String>,
    pub(crate) model_id: String,
}

impl std::fmt::Debug for GLiNER2FastinoCandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLiNER2FastinoCandle")
            .field("model_id", &self.model_id)
            .field("active_adapter", &self.active_adapter)
            .finish()
    }
}

impl GLiNER2FastinoCandle {
    /// Active adapter name, or `None` if running on pure base weights.
    pub fn active_adapter(&self) -> Option<&str> {
        self.active_adapter.as_deref()
    }

    /// Load from a local directory containing the PyTorch artifacts:
    /// `tokenizer.json`, `config.json`, `model.safetensors`.
    ///
    /// **Phase 4 / experimental.** No GPU device override yet — uses
    /// CPU. Phase 4.5 may add `from_local_with_config` analogous to
    /// the ONNX backend's pattern.
    pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
        Self::from_local_on_device(model_dir, &Device::Cpu)
    }

    /// Load from HuggingFace Hub. Downloads `tokenizer.json`,
    /// `config.json`, and `model.safetensors` to the local HF cache,
    /// then defers to [`Self::from_local`].
    ///
    /// **Important**: this loads the *PyTorch* repo (e.g.
    /// `fastino/gliner2-multi-v1`), NOT the SemplificaAI ONNX export.
    /// They're different artifacts.
    pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
        let api = crate::backends::hf_loader::hf_api()
            .map_err(|e| crate::Error::Backend(format!("hf_api: {e}")))?;
        let repo = api.model(model_id.to_string());

        // Touch each required file so it's in the local cache. Order
        // matters: weights last so we can use its parent as the snapshot dir.
        let _tokenizer = crate::backends::hf_loader::download_model_file(
            &repo,
            &["tokenizer.json"],
        )
        .map_err(|e| crate::Error::Backend(format!("download tokenizer: {e}")))?;
        let _config = crate::backends::hf_loader::download_model_file(
            &repo,
            &["config.json"],
        )
        .map_err(|e| crate::Error::Backend(format!("download config: {e}")))?;
        let weights_path = crate::backends::hf_loader::download_model_file(
            &repo,
            &["model.safetensors", "pytorch_model.bin"],
        )
        .map_err(|e| crate::Error::Backend(format!("download weights: {e}")))?;

        let snapshot_dir = weights_path
            .parent()
            .ok_or_else(|| crate::Error::Backend("snapshot dir resolution".into()))?;
        let mut model = Self::from_local(snapshot_dir)?;
        model.model_id = model_id.to_string();
        Ok(model)
    }

    /// Internal: like [`Self::from_local`] but explicit about the
    /// Candle device. Hot-swap on a different device requires re-
    /// loading; not exposed publicly until Phase 4.5.
    pub(crate) fn from_local_on_device(
        model_dir: &Path,
        device: &Device,
    ) -> crate::Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");
        let weights_path = model_dir.join("model.safetensors");
        let config_path = model_dir.join("config.json");

        if !weights_path.exists() {
            return Err(crate::Error::Backend(format!(
                "gliner2_fastino_candle: model.safetensors not found in {} \
                 (PyTorch fastino/gliner2-* repo expected; SemplificaAI ONNX \
                 export is a different artifact)",
                model_dir.display()
            )));
        }

        let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)
            .map_err(|e| crate::Error::Backend(format!("tokenizer: {e}")))?;
        let encoder = encoder::Encoder::from_safetensors(&weights_path, &config_path, device)?;
        let heads = heads::AllHeads::from_safetensors(&weights_path, device)?;
        let model_id = model_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "gliner2_fastino_candle_local".to_string());

        Ok(Self {
            tokenizer,
            device: device.clone(),
            base_model_dir: model_dir.to_path_buf(),
            encoder,
            heads,
            active_adapter: None,
            model_id,
        })
    }
}
