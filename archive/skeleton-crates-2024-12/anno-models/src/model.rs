//! Model trait and types for NER.

use crate::runtime::Runtime;
use anno_core::Entity;
use std::fmt::Debug;

/// Error type for model operations.
#[derive(Debug, Clone)]
pub enum ModelError {
    /// Model initialization failed.
    InitError(String),
    /// Tokenization failed.
    TokenizationError(String),
    /// Inference failed.
    InferenceError(String),
    /// Invalid input.
    InvalidInput(String),
    /// Model not loaded.
    NotLoaded,
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitError(msg) => write!(f, "Init error: {}", msg),
            Self::TokenizationError(msg) => write!(f, "Tokenization error: {}", msg),
            Self::InferenceError(msg) => write!(f, "Inference error: {}", msg),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            Self::NotLoaded => write!(f, "Model not loaded"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Information about a model.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model name (e.g., "GLiNER").
    pub name: String,
    /// Model variant (e.g., "gliner_base").
    pub variant: String,
    /// Supported entity types (empty = zero-shot).
    pub supported_types: Vec<String>,
    /// Maximum sequence length.
    pub max_length: usize,
    /// Whether the model supports zero-shot NER.
    pub zero_shot: bool,
}

/// Configuration for model loading.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// Model identifier (path or HuggingFace repo).
    pub model_id: String,
    /// Maximum sequence length.
    pub max_length: usize,
    /// Confidence threshold for entity extraction.
    pub threshold: f32,
    /// Batch size for inference.
    pub batch_size: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            max_length: 512,
            threshold: 0.5,
            batch_size: 8,
        }
    }
}

/// NER model trait.
///
/// This is the core abstraction for entity extraction models.
/// Models are parameterized by a `Runtime` to support multiple backends.
pub trait Model: Debug + Send + Sync {
    /// The runtime type this model uses.
    type Runtime: Runtime;

    /// Get model information.
    fn info(&self) -> &ModelInfo;

    /// Get the runtime.
    fn runtime(&self) -> &Self::Runtime;

    /// Extract entities from text.
    ///
    /// # Arguments
    /// - `text`: Input text
    /// - `entity_types`: Optional entity types to extract (for zero-shot models)
    ///
    /// # Returns
    /// Vector of extracted entities with spans and confidence scores.
    fn extract_entities(
        &self,
        text: &str,
        entity_types: Option<&[&str]>,
    ) -> Result<Vec<Entity>, ModelError>;

    /// Extract entities from multiple texts (batch).
    ///
    /// Default implementation calls `extract_entities` for each text.
    /// Models can override for more efficient batched inference.
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        entity_types: Option<&[&str]>,
    ) -> Result<Vec<Vec<Entity>>, ModelError> {
        texts
            .iter()
            .map(|text| self.extract_entities(text, entity_types))
            .collect()
    }

    /// Check if the model is available (loaded and ready).
    fn is_available(&self) -> bool;

    /// Get supported entity types.
    ///
    /// Returns empty slice for zero-shot models.
    fn supported_types(&self) -> &[String] {
        &self.info().supported_types
    }

    /// Check if this is a zero-shot model.
    fn is_zero_shot(&self) -> bool {
        self.info().zero_shot
    }
}

/// Extension trait for models that support streaming inference.
pub trait StreamingModel: Model {
    /// Iterator type for streaming results.
    type EntityIterator<'a>: Iterator<Item = Result<Entity, ModelError>>
    where
        Self: 'a;

    /// Stream entities as they are extracted.
    fn stream_entities<'a>(
        &'a self,
        text: &'a str,
        entity_types: Option<&'a [&'a str]>,
    ) -> Self::EntityIterator<'a>;
}

/// Extension trait for models that support confidence calibration.
pub trait CalibratedModel: Model {
    /// Apply temperature scaling to confidence scores.
    fn set_temperature(&mut self, temperature: f32);

    /// Get the current temperature.
    fn temperature(&self) -> f32;
}
