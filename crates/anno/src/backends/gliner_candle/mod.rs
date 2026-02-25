//! GLiNER implementation using Candle (pure Rust ML) with Metal/CUDA support.
//!
//! Zero-shot NER using bi-encoder architecture: match text spans to entity labels.
//!
//! # Architecture
//!
//! ```text
//! Text Input     Label Input
//!     |              |
//!     v              v
//! [Tokenizer]   [Tokenizer]
//!     |              |
//!     v              v
//! [Transformer Encoder] (shared)
//!     |              |
//!     v              v
//! [SpanRepLayer]  [LabelEncoder]
//!     |              |
//!     +------+-------+
//!            |
//!            v
//!     [SpanLabelMatcher]
//!            |
//!            v
//!       [Entities]
//! ```
//!
//! # GPU Support
//!
//! - **Metal** (Apple Silicon): `cargo build --features candle,metal`
//! - **CUDA** (NVIDIA): `cargo build --features candle,cuda`
//! - **CPU**: Always available as fallback
//!
//! # Example
//!
//! ```rust,ignore
//! use anno::backends::gliner_candle::GLiNERCandle;
//!
//! let model = GLiNERCandle::from_pretrained("urchade/gliner_small-v2.1")?;
//! let entities = model.extract(
//!     "Steve Jobs founded Apple in California.",
//!     &["person", "organization", "location"],
//!     0.5,
//! )?;
//! ```

#![allow(dead_code)] // Token constants for future prompt encoding

use crate::{Entity, EntityType, Error, Result};
use std::path::{Path, PathBuf};

#[cfg(feature = "candle")]
use {
    super::encoder_candle::{CandleEncoder, TextEncoder},
    candle_core::{DType, Device, IndexOp, Module, Tensor, D},
    candle_nn::{linear, Linear, VarBuilder},
    tokenizers::Tokenizer,
};

/// Maximum span width for entity candidates.
const MAX_SPAN_WIDTH: usize = 12;

/// Special tokens for GLiNER models.
#[cfg(feature = "candle")]
const TOKEN_START: u32 = 1;
#[cfg(feature = "candle")]
const TOKEN_END: u32 = 2;
#[cfg(feature = "candle")]
const TOKEN_ENT: u32 = 128002;
#[cfg(feature = "candle")]
const TOKEN_SEP: u32 = 128003;

// =============================================================================
// Device Selection
// =============================================================================

/// Get the best available compute device.
#[cfg(feature = "candle")]
pub fn best_device() -> Result<Device> {
    #[cfg(all(target_os = "macos", feature = "metal"))]
    {
        if let Ok(device) = Device::new_metal(0) {
            log::info!("[GLiNER-Candle] Using Metal GPU");
            return Ok(device);
        }
    }

    #[cfg(feature = "cuda")]
    {
        if let Ok(device) = Device::new_cuda(0) {
            log::info!("[GLiNER-Candle] Using CUDA GPU");
            return Ok(device);
        }
    }

    log::info!("[GLiNER-Candle] Using CPU");
    Ok(Device::Cpu)
}

// =============================================================================
// Span Representation Layer (SpanMarker style)
// =============================================================================

/// Span representation using the SpanMarker approach from GLiNER.
/// Projects start and end positions separately and combines them.
#[cfg(feature = "candle")]
pub mod layers;
pub use layers::*;

// =============================================================================
// GLiNER Candle Model
// =============================================================================

/// GLiNER zero-shot NER using pure Rust Candle backend.
///
/// Matches text spans to entity type descriptions using a bi-encoder.
/// Supports Metal (Apple Silicon) and CUDA (NVIDIA) GPU acceleration.
mod inference;
#[cfg(feature = "candle")]
pub(crate) use inference::convert_pytorch_to_safetensors;
#[cfg(feature = "candle")]
pub use inference::GLiNERCandle;

const DEFAULT_GLINER_LABELS: &[&str] = &[
    "person",
    "organization",
    "location",
    "date",
    "time",
    "money",
    "percent",
    "product",
    "event",
    "facility",
    "work_of_art",
    "law",
    "language",
];

#[cfg(feature = "candle")]
impl crate::Model for GLiNERCandle {
    fn extract_entities(&self, text: &str, _language: Option<&str>) -> Result<Vec<Entity>> {
        // Use lower threshold for smaller models (NeuML/gliner-bert-tiny)
        // The threshold may need tuning based on the specific model
        self.extract(text, DEFAULT_GLINER_LABELS, 0.3)
    }

    fn supported_types(&self) -> Vec<EntityType> {
        DEFAULT_GLINER_LABELS
            .iter()
            .map(|label| Self::map_label(label))
            .collect()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "GLiNER-Candle"
    }

    fn description(&self) -> &'static str {
        "Zero-shot NER using GLiNER bi-encoder (pure Rust with Metal/CUDA support)"
    }

    fn capabilities(&self) -> crate::ModelCapabilities {
        crate::ModelCapabilities {
            batch_capable: true,
            streaming_capable: true,
            gpu_capable: true,
            dynamic_labels: true,
            ..Default::default()
        }
    }
}

impl crate::NamedEntityCapable for GLiNERCandle {}

#[cfg(feature = "candle")]
impl crate::DynamicLabels for GLiNERCandle {
    fn extract_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        _language: Option<&str>,
    ) -> crate::Result<Vec<Entity>> {
        <Self as crate::backends::inference::ZeroShotNER>::extract_with_types(
            self, text, labels, 0.3,
        )
    }
}

#[cfg(feature = "candle")]
impl crate::backends::inference::ZeroShotNER for GLiNERCandle {
    fn extract_with_types(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: f32,
    ) -> Result<Vec<Entity>> {
        self.extract(text, entity_types, threshold)
    }

    fn extract_with_descriptions(
        &self,
        text: &str,
        descriptions: &[&str],
        threshold: f32,
    ) -> Result<Vec<Entity>> {
        // GLiNER can use descriptions directly as label text
        self.extract(text, descriptions, threshold)
    }

    fn default_types(&self) -> &[&'static str] {
        &["person", "organization", "location", "date", "event"]
    }
}

// =============================================================================
// Non-candle stub
// =============================================================================

#[cfg(not(feature = "candle"))]
#[derive(Debug)]
pub struct GLiNERCandle {
    _private: (),
}

#[cfg(not(feature = "candle"))]
impl GLiNERCandle {
    /// Create GLiNER (requires candle feature).
    pub fn new(_model_name: &str) -> Result<Self> {
        Err(Error::FeatureNotAvailable(
            "GLiNER-Candle requires the 'candle' feature. \
             Build with: cargo build --features candle\n\
             Alternative: Use GLiNEROnnx with the 'onnx' feature for similar functionality."
                .to_string(),
        ))
    }

    /// Load from pretrained (requires candle feature).
    pub fn from_pretrained(_model_id: &str) -> Result<Self> {
        Self::new("")
    }
}

#[cfg(not(feature = "candle"))]
impl crate::Model for GLiNERCandle {
    fn extract_entities(&self, _text: &str, _language: Option<&str>) -> Result<Vec<Entity>> {
        Err(Error::FeatureNotAvailable(
            "GLiNER-Candle requires the 'candle' feature".to_string(),
        ))
    }

    fn supported_types(&self) -> Vec<EntityType> {
        vec![]
    }

    fn is_available(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "GLiNER-Candle (unavailable)"
    }

    fn description(&self) -> &'static str {
        "Zero-shot NER with Candle - requires 'candle' feature"
    }
}

#[cfg(not(feature = "candle"))]
impl crate::backends::inference::ZeroShotNER for GLiNERCandle {
    fn extract_with_types(
        &self,
        _text: &str,
        _entity_types: &[&str],
        _threshold: f32,
    ) -> Result<Vec<Entity>> {
        Err(Error::FeatureNotAvailable(
            "GLiNER-Candle requires the 'candle' feature".to_string(),
        ))
    }

    fn extract_with_descriptions(
        &self,
        _text: &str,
        _descriptions: &[&str],
        _threshold: f32,
    ) -> Result<Vec<Entity>> {
        Err(Error::FeatureNotAvailable(
            "GLiNER-Candle requires the 'candle' feature".to_string(),
        ))
    }
}

// =============================================================================
// BatchCapable Trait Implementation
// =============================================================================

#[cfg(feature = "candle")]
impl crate::BatchCapable for GLiNERCandle {
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        _language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Pre-compute label embeddings for efficiency
        let _ = self.extract(texts[0], DEFAULT_GLINER_LABELS, 0.5)?;

        // Process texts - label embeddings are now cached internally
        texts
            .iter()
            .map(|text| self.extract(text, DEFAULT_GLINER_LABELS, 0.5))
            .collect()
    }

    fn optimal_batch_size(&self) -> Option<usize> {
        Some(8)
    }
}

#[cfg(not(feature = "candle"))]
impl crate::BatchCapable for GLiNERCandle {
    fn extract_entities_batch(
        &self,
        _texts: &[&str],
        _language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        Err(Error::FeatureNotAvailable(
            "GLiNER-Candle requires the 'candle' feature".to_string(),
        ))
    }

    fn optimal_batch_size(&self) -> Option<usize> {
        None
    }
}

// =============================================================================
// StreamingCapable Trait Implementation
// =============================================================================

#[cfg(feature = "candle")]
impl crate::StreamingCapable for GLiNERCandle {
    fn recommended_chunk_size(&self) -> usize {
        4096 // Characters - translates to roughly a few hundred words
    }
}

#[cfg(not(feature = "candle"))]
impl crate::StreamingCapable for GLiNERCandle {
    fn recommended_chunk_size(&self) -> usize {
        4096
    }
}

// =============================================================================
// GpuCapable Trait Implementation
// =============================================================================

#[cfg(feature = "candle")]
impl crate::GpuCapable for GLiNERCandle {
    fn is_gpu_active(&self) -> bool {
        matches!(&self.device, Device::Metal(_) | Device::Cuda(_))
    }

    fn device(&self) -> &str {
        // Use the existing device() method but return &str
        // We'll need to store this as a static or use a different approach
        match &self.device {
            Device::Cpu => "cpu",
            Device::Metal(_) => "metal",
            Device::Cuda(_) => "cuda",
        }
    }
}

#[cfg(not(feature = "candle"))]
impl crate::GpuCapable for GLiNERCandle {
    fn is_gpu_active(&self) -> bool {
        false
    }

    fn device(&self) -> &str {
        "cpu"
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests;
