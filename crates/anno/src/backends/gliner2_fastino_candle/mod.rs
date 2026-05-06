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

    // Constructors are added in M4; load_adapter / unload_adapter in M8.
    // For now the only construction path is via `encoder::Encoder::from_safetensors`
    // directly (used by M3.2's smoke test).
    #[doc(hidden)]
    pub fn _from_local_minimal(
        model_dir: &Path,
        device: &Device,
    ) -> crate::Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");
        let weights_path = model_dir.join("model.safetensors");
        let config_path = model_dir.join("config.json");

        let tokenizer = crate::backends::hf_loader::load_tokenizer(&tokenizer_path)
            .map_err(|e| crate::Error::Backend(format!("tokenizer: {e}")))?;
        let encoder = encoder::Encoder::from_safetensors(&weights_path, &config_path, device)?;
        let heads = heads::AllHeads::stub();
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
