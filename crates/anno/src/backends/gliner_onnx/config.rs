//! GLiNER ONNX configuration and prompt cache types.

use super::*;

pub struct GLiNERConfig {
    /// Prefer quantized models (INT8) for faster CPU inference.
    pub prefer_quantized: bool,
    /// ONNX optimization level (1-3, default 3).
    pub optimization_level: u8,
    /// Number of threads for inference (0 = auto).
    pub num_threads: usize,
    /// Cache size for prompt encodings (0 = disabled, default 100).
    ///
    /// The prompt cache stores encoded prompts keyed by (text, entity_types, model_id).
    /// This can materially reduce repeated work in evaluation loops and API usage patterns
    /// where the same text is queried with multiple type sets.
    pub prompt_cache_size: usize,
}

#[cfg(feature = "onnx")]
impl Default for GLiNERConfig {
    fn default() -> Self {
        Self {
            prefer_quantized: true,
            optimization_level: 3,
            num_threads: 4,
            prompt_cache_size: 100,
        }
    }
}

/// Cache key for prompt encodings.
///
/// Keyed by (text_hash, entity_types_hash, model_id) to ensure cache hits
/// only when text, entity types, and model are identical.
#[cfg(feature = "onnx")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct PromptCacheKey {
    pub(super) text_hash: u64,
    pub(super) entity_types_hash: u64,
    pub(super) model_id: String,
}

/// Cached prompt encoding result.
#[cfg(feature = "onnx")]
#[derive(Debug, Clone)]
pub(super) struct PromptCacheValue {
    pub(super) input_ids: Vec<i64>,
    pub(super) attention_mask: Vec<i64>,
    pub(super) words_mask: Vec<i64>,
    pub(super) text_lengths: i64,
    pub(super) entity_count: usize,
}
