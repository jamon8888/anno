//! GLiNER-based NER implementation using ONNX Runtime.
//!
//! GLiNER (Generalist and Lightweight Model for Named Entity Recognition) is
//! a popular approach to “open/zero-shot” NER. This implementation follows the GLiNER prompt format
//! and common community conventions.
//!
//! ## Prompt Format
//!
//! GLiNER uses a special prompt format:
//!
//! ```text
//! [START] <<ENT>> type1 <<ENT>> type2 <<SEP>> word1 word2 ... [END]
//! ```
//!
//! Token IDs (for GLiNER tokenizer):
//! - START = 1
//! - END = 2
//! - `<<ENT>>` = 128002
//! - `<<SEP>>` = 128003
//!
//! ## Key Insight
//!
//! Each word is encoded SEPARATELY, preserving word boundaries.
//! Output shape: [batch, num_words, max_width, num_entity_types]

#![allow(missing_docs)] // Stub implementation
#![allow(dead_code)] // Placeholder constants
#![allow(clippy::type_complexity)] // Complex return tuples
#![allow(clippy::manual_contains)] // Shape check style
#![allow(unused_variables)] // Feature-gated code
#![allow(clippy::items_after_test_module)] // Large file; keep local tests near helpers
#![allow(unused_imports)] // EntityType used conditionally

#[cfg(feature = "onnx")]
use crate::sync::{lock, try_lock, Mutex};
use crate::{Entity, Error, Result};
use anno_core::{EntityCategory, EntityType};

/// Special token IDs for GLiNER models
const TOKEN_START: u32 = 1;
const TOKEN_END: u32 = 2;
const TOKEN_ENT: u32 = 128002;
const TOKEN_SEP: u32 = 128003;

/// Default max span width from GLiNER config
const MAX_SPAN_WIDTH: usize = 12;

/// Configuration for GLiNER model loading.
#[cfg(feature = "onnx")]
pub mod config;
pub use config::*;

pub struct GLiNEROnnx {
    session: Mutex<ort::session::Session>,
    /// Arc-wrapped tokenizer for cheap cloning across threads.
    tokenizer: std::sync::Arc<tokenizers::Tokenizer>,
    /// HuggingFace model identifier (e.g., "onnx-community/gliner_small-v2.1").
    model_name: String,
    /// Whether a quantized model was loaded.
    is_quantized: bool,
    /// LRU cache for prompt encodings (keyed by text + entity types).
    prompt_cache: Option<Mutex<lru::LruCache<PromptCacheKey, PromptCacheValue>>>,
}

#[cfg(feature = "onnx")]
mod inference;
pub(crate) use inference::expand_ner_label;
#[cfg(feature = "onnx")]
pub(crate) use inference::looks_like_company_name;
use inference::DEFAULT_GLINER_LABELS;
impl crate::Model for GLiNEROnnx {
    fn extract_entities(&self, text: &str, _language: Option<&str>) -> crate::Result<Vec<Entity>> {
        // Use default labels for the Model trait interface
        // For custom labels, use the extract(text, labels, threshold) method directly
        self.extract(text, DEFAULT_GLINER_LABELS, 0.5)
    }

    fn supported_types(&self) -> Vec<anno_core::EntityType> {
        // GLiNER supports any type via zero-shot - return the defaults
        DEFAULT_GLINER_LABELS
            .iter()
            .map(|label| anno_core::EntityType::Custom {
                name: (*label).to_string(),
                category: EntityCategory::Misc,
            })
            .collect()
    }

    fn is_available(&self) -> bool {
        true // If we got this far, it's available
    }

    fn name(&self) -> &'static str {
        "GLiNER-ONNX"
    }

    fn description(&self) -> &'static str {
        "Zero-shot NER using GLiNER with ONNX Runtime backend"
    }

    fn version(&self) -> String {
        // Version depends on the model weights and quantization status
        format!(
            "gliner-onnx-{}-{}",
            self.model_name,
            if self.is_quantized { "q" } else { "fp32" }
        )
    }
}

#[cfg(feature = "onnx")]
impl crate::backends::inference::ZeroShotNER for GLiNEROnnx {
    fn extract_with_types(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        self.extract(text, entity_types, threshold)
    }

    fn extract_with_descriptions(
        &self,
        text: &str,
        descriptions: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        // GLiNER encodes labels as text, so descriptions work the same way
        self.extract(text, descriptions, threshold)
    }

    fn default_types(&self) -> &[&'static str] {
        DEFAULT_GLINER_LABELS
    }
}

// =============================================================================
// Stub when feature disabled
// =============================================================================

#[cfg(not(feature = "onnx"))]
#[derive(Debug)]
pub struct GLiNEROnnx;

#[cfg(not(feature = "onnx"))]
impl GLiNEROnnx {
    /// Create a new GLiNER model (stub - requires onnx feature).
    pub fn new(_model_name: &str) -> Result<Self> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature. \
             Build with: cargo build --features onnx"
                .to_string(),
        ))
    }

    /// Get the model name (stub).
    pub fn model_name(&self) -> &str {
        "gliner-not-enabled"
    }

    /// Extract entities (stub - requires onnx feature).
    pub fn extract(
        &self,
        _text: &str,
        _entity_types: &[&str],
        _threshold: f32,
    ) -> Result<Vec<Entity>> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature".to_string(),
        ))
    }
}

#[cfg(not(feature = "onnx"))]
impl crate::Model for GLiNEROnnx {
    fn extract_entities(&self, _text: &str, _language: Option<&str>) -> crate::Result<Vec<Entity>> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature".to_string(),
        ))
    }

    fn supported_types(&self) -> Vec<anno_core::EntityType> {
        vec![]
    }

    fn is_available(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "GLiNER-ONNX (unavailable)"
    }

    fn description(&self) -> &'static str {
        "GLiNER with ONNX Runtime backend - requires 'onnx' feature"
    }
}

#[cfg(not(feature = "onnx"))]
impl crate::backends::inference::ZeroShotNER for GLiNEROnnx {
    fn extract_with_types(
        &self,
        _text: &str,
        _entity_types: &[&str],
        _threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature".to_string(),
        ))
    }

    fn extract_with_descriptions(
        &self,
        _text: &str,
        _descriptions: &[&str],
        _threshold: f32,
    ) -> crate::Result<Vec<Entity>> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature".to_string(),
        ))
    }
}

// =============================================================================
// BatchCapable Trait Implementation
// =============================================================================

#[cfg(feature = "onnx")]
impl crate::BatchCapable for GLiNEROnnx {
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        _language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // GLiNER supports true batching with padded sequences
        // For simplicity, we reuse the session efficiently with sequential calls
        // The tokenizer and model weights stay cached
        let default_types = DEFAULT_GLINER_LABELS;
        let threshold = 0.5;

        texts
            .iter()
            .map(|text| self.extract(text, default_types, threshold))
            .collect()
    }

    fn optimal_batch_size(&self) -> Option<usize> {
        Some(16)
    }
}

#[cfg(not(feature = "onnx"))]
impl crate::BatchCapable for GLiNEROnnx {
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        _language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        Err(Error::InvalidInput(
            "GLiNER-ONNX requires the 'onnx' feature".to_string(),
        ))
    }

    fn optimal_batch_size(&self) -> Option<usize> {
        None
    }
}

// =============================================================================
// StreamingCapable Trait Implementation
// =============================================================================
// Overlap Removal
// =============================================================================

/// Remove overlapping entity spans intelligently.
///
/// Strategy:
/// 1. Prefer shorter spans when they have similar or higher confidence
///    (e.g., prefer "Department of Defense" over "The Department of Defense")
/// 2. For truly overlapping spans of similar length, keep highest confidence
/// 3. Handle comma-separated entities (e.g., "IBM, NASA" should become "IBM" + "NASA")
fn remove_overlapping_spans(mut entities: Vec<Entity>) -> Vec<Entity> {
    if entities.len() <= 1 {
        return entities;
    }

    // Performance: Use unstable sort (we don't need stable sort here)
    // Sort by span length (shorter first), then by confidence descending
    // This prefers shorter, more precise spans
    entities.sort_unstable_by(|a, b| {
        let len_a = a.end - a.start;
        let len_b = b.end - b.start;
        len_a.cmp(&len_b).then_with(|| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    let mut result: Vec<Entity> = Vec::with_capacity(entities.len());

    for entity in entities {
        // Check if this entity is FULLY CONTAINED by any already-kept entity
        // If so, skip it (we already have a more precise version)
        let is_superset_of_existing = result.iter().any(|kept| {
            // Entity fully contains kept
            entity.start <= kept.start && entity.end >= kept.end
        });

        if is_superset_of_existing {
            // Skip - we have smaller, more precise entities
            continue;
        }

        // Check if this entity overlaps (but doesn't contain) any kept entity
        let overlaps_existing = result.iter().any(|kept| {
            let entity_range = entity.start..entity.end;
            let kept_range = kept.start..kept.end;
            // Partial overlap (not full containment)
            entity_range.start < kept_range.end && kept_range.start < entity_range.end
        });

        if !overlaps_existing {
            result.push(entity);
        }
    }

    // Performance: Use unstable sort (we don't need stable sort here)
    // Re-sort by position for output
    result.sort_unstable_by_key(|e| e.start);
    result
}

// =============================================================================
// StreamingCapable
// =============================================================================

#[cfg(feature = "onnx")]
impl crate::StreamingCapable for GLiNEROnnx {
    fn recommended_chunk_size(&self) -> usize {
        4096 // Characters
    }
}

#[cfg(not(feature = "onnx"))]
impl crate::StreamingCapable for GLiNEROnnx {
    fn recommended_chunk_size(&self) -> usize {
        4096
    }
}

#[cfg(test)]
mod postprocess_tests;
