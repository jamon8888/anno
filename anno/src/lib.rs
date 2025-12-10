//! # anno
//!
//! Information extraction for Rust: NER, coreference resolution, and evaluation.
//!
//! ## The Three-Level Hierarchy
//!
//! Anno organizes entity processing into three distinct levels, each answering
//! a different question:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │ Level 3: Identity — "Which knowledge base entry is this?"               │
//! │ Cross-document entity, may have KB link (Wikidata Q-ID, etc.)           │
//! │ Type: anno_core::grounded::Identity                                     │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │ Level 2: Track — "Are these mentions the SAME entity?"                  │
//! │ Within-document coreference chain                                       │
//! │ Type: anno_core::grounded::Track                                        │
//! │ Backend: backends::mention_ranking::MentionRankingCoref                 │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │ Level 1: Signal — "What entity mentions exist here?"                    │
//! │ Single mention detection (NER)                                          │
//! │ Types: anno_core::grounded::Signal, anno_core::Entity                   │
//! │ Backends: RegexNER, HeuristicNER, GLiNER, NuNER, etc.                   │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Don't Confuse These
//!
//! | Concept | What it is | What it's NOT |
//! |---------|------------|---------------|
//! | **NER** | What mentions exist? | Coreference (same entity?) |
//! | **Coreference** | Same entity? | Salience (how important?) |
//! | **Salience** | How important? | Coreference (not a substitute!) |
//!
//! ## Core Capabilities
//!
//! - **NER**: Multiple backends (Regex, BERT, GLiNER, NuNER, W2NER)
//! - **Coreference**: Resolution (mention-ranking, rule-based) and metrics (MUC, B³, CEAF, LEA, BLANC)
//! - **Salience**: Entity importance ranking (TextRank, YAKE, TF-IDF, Position)
//! - **Evaluation**: Comprehensive benchmarking framework with bias analysis
//!
//! ## Discourse Analysis (feature = "discourse")
//!
//! Handles phenomena beyond sentence-level NER:
//!
//! - **Centering theory**: Track what the discourse is "about" through forward/backward-looking
//!   centers. Use for pronoun resolution and coherence analysis.
//! - **Uncertain reference**: Defer resolution when antecedent is ambiguous, maintaining
//!   a distribution over candidates. Handles cataphora and bridging.
//! - **Abstract anaphora**: Resolve pronouns like "this" that refer to events, propositions,
//!   or facts—not just named entities.
//!
//! ## Quick Start
//!
//! ```rust
//! use anno::{Model, StackedNER};
//!
//! // Level 1: Detect entity mentions
//! let ner = StackedNER::default();
//! let entities = ner.extract_entities("John met Mary in Paris.", None).unwrap();
//! ```
//!
//! For coreference (Level 2):
//!
//! ```rust,ignore
//! use anno::backends::mention_ranking::MentionRankingCoref;
//!
//! let coref = MentionRankingCoref::new();
//! let (signals, tracks) = coref.resolve_to_grounded("John saw Mary. He waved.")?;
//! // signals: individual mentions, tracks: coreference chains
//! ```
//!
//! Core types (Entity, GroundedDocument, Signal, Track, etc.) are in `anno-core` and re-exported here.

#![warn(missing_docs)]

// Module declarations (core types are in anno-core, not declared here)
pub mod backends;
/// Edit distance algorithms.
pub mod edit_distance;
pub mod error;
pub mod eval;
pub mod ingest;
/// Joint inference for coreference resolution and entity linking.
pub mod joint;
pub mod lang;
/// Entity linking to knowledge bases.
pub mod linking;
pub mod offset;
/// Preprocessing for mention detection.
pub mod preprocess;
pub mod schema;
pub mod similarity;
pub mod sync;
pub mod types;

#[cfg(feature = "cli")]
pub mod cli;

/// Discourse-level analysis for coreference resolution.
///
/// Provides infrastructure for handling phenomena that span sentence boundaries:
///
/// - **Centering theory**: Track discourse focus through forward/backward-looking centers
/// - **Uncertain reference**: Deferred resolution using epsilon-term semantics
/// - **Abstract anaphora**: Pronouns referring to events, propositions, facts
/// - **Shell nouns**: Abstract nouns like "problem", "issue", "fact"
///
/// Enable with the `discourse` feature.
///
/// See [`discourse::centering`] for salience-based pronoun resolution and
/// [`discourse::uncertain_reference`] for handling ambiguous references.
#[cfg(feature = "discourse")]
pub mod discourse;

// Re-export error types
pub use error::{Error, Result};

// Re-export anno-core types for backward compatibility
pub use anno_core::{
    Corpus, DiscontinuousSpan, Entity, EntityBuilder, EntityCategory, EntityType, EntityViewport,
    ExtractionMethod, GraphDocument, GraphEdge, GraphExportFormat, GraphNode, GroundedDocument,
    HashMapLexicon, HierarchicalConfidence, Identity, IdentityId, IdentitySource, Lexicon,
    Location, Modality, Provenance, Quantifier, RaggedBatch, Relation, Signal, SignalId, SignalRef,
    Span, SpanCandidate, Track, TrackId, TrackRef, TypeMapper, ValidationIssue,
};

/// Re-export graph module for backward compatibility (anno::graph::*)
///
/// This module re-exports all graph-related types from `anno-core`:
/// - `GraphNode`, `GraphEdge`, `GraphDocument`
/// - Graph export formats and utilities
pub mod graph {
    pub use anno_core::graph::*;
}

/// Re-export grounded module for backward compatibility (anno::grounded::*)
///
/// This module re-exports all grounded document types from `anno-core`:
/// - `GroundedDocument`, `Signal`, `Track`, `Identity`
/// - Coreference resolution types and utilities
pub mod grounded {
    pub use anno_core::grounded::*;
}

// Re-export commonly used types
pub use lang::{detect_language, Language};
pub use offset::{
    bytes_to_chars, chars_to_bytes, is_ascii, OffsetMapping, SpanConverter, TextSpan, TokenSpan,
};
pub use schema::*;
pub use similarity::*;
pub use sync::*;
pub use types::*;

// =============================================================================
// Sealed Trait Pattern
// =============================================================================

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::RegexNER {}
    impl Sealed for super::HeuristicNER {}
    impl Sealed for super::StackedNER {}
    impl Sealed for super::NuNER {}
    impl Sealed for super::W2NER {}
    impl Sealed for super::NERExtractor {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::BertNEROnnx {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::GLiNEROnnx {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::backends::albert::ALBERTNER {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::backends::deberta_v3::DeBERTaV3NER {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::backends::gliner_poly::GLiNERPoly {}

    #[cfg(feature = "onnx")]
    impl Sealed for super::backends::gliner2::GLiNER2Onnx {}

    #[cfg(feature = "candle")]
    impl Sealed for super::CandleNER {}

    impl Sealed for super::backends::tplinker::TPLinker {}
    impl Sealed for super::backends::universal_ner::UniversalNER {}

    #[allow(deprecated)]
    impl Sealed for super::backends::rule::RuleBasedNER {}

    impl Sealed for super::MockModel {}
    impl Sealed for super::joint::JointModel {}
}

/// Trait for NER model backends.
pub trait Model: sealed::Sealed + Send + Sync {
    /// Extract entities from text.
    fn extract_entities(&self, text: &str, language: Option<&str>) -> Result<Vec<Entity>>;

    /// Get supported entity types.
    fn supported_types(&self) -> Vec<EntityType>;

    /// Check if model is available and ready.
    fn is_available(&self) -> bool;

    /// Get the model name/identifier.
    fn name(&self) -> &'static str {
        "unknown"
    }

    /// Get a description of the model.
    fn description(&self) -> &'static str {
        "Unknown NER model"
    }
}

// =============================================================================
// Capability Marker Traits
// =============================================================================

/// Trait for models that support batch processing.
///
/// Models implementing this trait can process multiple texts efficiently,
/// potentially using parallel processing or optimized batch operations.
pub trait BatchCapable: Model {
    /// Extract entities from multiple texts in a batch.
    ///
    /// # Arguments
    /// * `texts` - Slice of text strings to process
    /// * `language` - Optional language hint for the texts
    ///
    /// # Returns
    /// A vector of entity vectors, one per input text
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        texts
            .iter()
            .map(|text| self.extract_entities(text, language))
            .collect()
    }

    /// Get the optimal batch size for this model, if applicable.
    ///
    /// Returns `None` if the model doesn't have a specific optimal batch size,
    /// or `Some(n)` if there's a recommended batch size for best performance.
    fn optimal_batch_size(&self) -> Option<usize> {
        None
    }
}

/// Trait for models that support GPU acceleration.
///
/// Models implementing this trait can report whether GPU is active
/// and which device they're using.
pub trait GpuCapable: Model {
    /// Check if GPU acceleration is currently active.
    ///
    /// Returns `true` if the model is using GPU, `false` if using CPU.
    fn is_gpu_active(&self) -> bool;

    /// Get the device identifier (e.g., "cuda:0", "cpu").
    ///
    /// Returns a string describing the compute device being used.
    fn device(&self) -> &str;
}

/// Trait for models that support streaming/chunked extraction.
///
/// Useful for processing very long documents by splitting them into chunks
/// and extracting entities from each chunk with proper offset tracking.
pub trait StreamingCapable: Model {
    /// Extract entities from a chunk of text, adjusting offsets by the chunk's position.
    ///
    /// # Arguments
    ///
    /// * `chunk` - A portion of the full document text
    /// * `offset` - Character offset of this chunk within the full document
    ///
    /// # Returns
    ///
    /// Entities with offsets adjusted to their position in the full document.
    fn extract_entities_streaming(&self, chunk: &str, offset: usize) -> Result<Vec<Entity>> {
        let entities = self.extract_entities(chunk, None)?;
        Ok(entities
            .into_iter()
            .map(|mut e| {
                e.start += offset;
                e.end += offset;
                e
            })
            .collect())
    }

    /// Get the recommended chunk size for streaming extraction.
    ///
    /// Returns the optimal number of characters per chunk for this model.
    /// Default implementation returns 10,000 characters.
    fn recommended_chunk_size(&self) -> usize {
        10_000
    }
}

/// Marker trait for models that extract named entities (persons, organizations, locations).
///
/// This is a marker trait used for type-level distinctions between different
/// model capabilities. All NER models should implement this.
pub trait NamedEntityCapable: Model {}

/// Marker trait for models that extract structured entities (dates, times, money, etc.).
///
/// This is a marker trait used for type-level distinctions between different
/// model capabilities. Models that extract structured data (like `RegexNER`) should implement this.
pub trait StructuredEntityCapable: Model {}

/// Trait for models that can extract relations between entities.
///
/// Models implementing this trait can jointly extract entities and their relationships,
/// producing (head, relation_type, tail) triples.
pub trait RelationCapable: Model {
    /// Extract entities and their relations from text.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to extract from
    /// * `language` - Optional language hint (e.g., "en", "es")
    ///
    /// # Returns
    ///
    /// A tuple of (entities, relations) where relations link entities together.
    fn extract_with_relations(
        &self,
        text: &str,
        language: Option<&str>,
    ) -> Result<(Vec<Entity>, Vec<Relation>)>;
}

/// Trait for models that support dynamic/zero-shot entity type specification.
///
/// Models implementing this trait can extract entities of arbitrary types
/// specified at inference time (e.g., GLiNER, UniversalNER), rather than
/// being limited to a fixed set of pre-trained types.
pub trait DynamicLabels: Model {
    /// Extract entities with custom type labels.
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to extract from
    /// * `labels` - Custom entity type labels to extract (e.g., ["PERSON", "ORGANIZATION"])
    /// * `language` - Optional language hint (e.g., "en", "es")
    ///
    /// # Returns
    ///
    /// Entities of the specified types found in the text.
    fn extract_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        language: Option<&str>,
    ) -> Result<Vec<Entity>>;
}

// Re-export backends
pub use backends::{
    AutoNER, BackendType, ConflictStrategy, HeuristicNER, NERExtractor, NuNER, RegexNER,
    StackedNER, TPLinker, W2NERConfig, W2NERRelation, W2NER,
};

// Mention-ranking coreference (Bourgois & Poibeau 2025)
pub use backends::mention_ranking::{
    ClusteringStrategy, MentionCluster, MentionRankingConfig, MentionRankingCoref, Number,
    RankedMention,
};

// Re-export MockModel for testing

// Re-export Model trait and related
pub use backends::inference::*;

#[cfg(feature = "onnx")]
pub use backends::{BertNEROnnx, GLiNEROnnx};

#[cfg(feature = "candle")]
pub use backends::CandleNER;

// Constants

/// Default BERT ONNX model identifier (HuggingFace model ID).
pub const DEFAULT_BERT_ONNX_MODEL: &str = "protectai/bert-base-NER-onnx";

/// Default GLiNER ONNX model identifier (HuggingFace model ID).
pub const DEFAULT_GLINER_MODEL: &str = "onnx-community/gliner_small-v2.1";

/// Default GLiNER2 ONNX model identifier (HuggingFace model ID).
pub const DEFAULT_GLINER2_MODEL: &str = "onnx-community/gliner-multitask-large-v0.5";

/// Default Candle model identifier (HuggingFace model ID).
pub const DEFAULT_CANDLE_MODEL: &str = "dslim/bert-base-NER";

/// Default NuNER ONNX model identifier (HuggingFace model ID).
pub const DEFAULT_NUNER_MODEL: &str = "deepanwa/NuNerZero_onnx";

/// Default W2NER ONNX model identifier (HuggingFace model ID).
pub const DEFAULT_W2NER_MODEL: &str = "ljynlp/w2ner-bert-base";

/// Automatically select the best available NER backend.
pub fn auto() -> Result<Box<dyn Model>> {
    #[cfg(feature = "onnx")]
    {
        if let Ok(model) = GLiNEROnnx::new(DEFAULT_GLINER_MODEL) {
            return Ok(Box::new(model));
        }
        if let Ok(model) = BertNEROnnx::new(DEFAULT_BERT_ONNX_MODEL) {
            return Ok(Box::new(model));
        }
    }
    #[cfg(feature = "candle")]
    {
        if let Ok(model) = CandleNER::from_pretrained(DEFAULT_CANDLE_MODEL) {
            return Ok(Box::new(model));
        }
    }
    Ok(Box::new(StackedNER::default()))
}

/// Check which backends are currently available.
pub fn available_backends() -> Vec<(&'static str, bool)> {
    let backends = vec![
        ("RegexNER", true),
        ("HeuristicNER", true),
        ("StackedNER", true),
    ];

    #[cfg(feature = "onnx")]
    {
        backends.push(("BertNEROnnx", true));
        backends.push(("GLiNEROnnx", true));
        backends.push(("NuNER", true));
        backends.push(("W2NER", true));
    }

    #[cfg(feature = "candle")]
    {
        backends.push(("CandleNER", true));
    }

    backends
}

/// A mock NER model for testing purposes.
///
/// This is provided so tests can create custom mock implementations
/// without breaking the sealed trait pattern.
///
/// # Entity Validation
///
/// By default, `extract_entities` validates that entity offsets are within
/// the input text bounds and that `start < end`. Set `validate = false`
/// to disable this (useful for testing error handling).
///
/// # Example
///
/// ```rust
/// use anno::{MockModel, Entity, EntityType, Result};
///
/// let mock = MockModel::new("test-mock")
///     .with_entities(vec![
///         Entity::new("John", EntityType::Person, 0, 4, 0.9),
///     ]);
///
/// // Use mock in tests
/// ```
#[derive(Clone)]
pub struct MockModel {
    /// Model name identifier.
    name: &'static str,
    /// Entities to return when `extract_entities` is called.
    entities: Vec<Entity>,
    /// Supported entity types for this mock model.
    types: Vec<EntityType>,
    /// If true, validate entity offsets against input text (default: true).
    validate: bool,
}

impl MockModel {
    /// Create a new mock model.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            entities: Vec::new(),
            types: Vec::new(),
            validate: true,
        }
    }

    /// Set entities to return on extraction.
    ///
    /// # Panics
    ///
    /// Panics if any entity has `start >= end`.
    #[must_use]
    pub fn with_entities(mut self, entities: Vec<Entity>) -> Self {
        // Basic validation on construction
        for (i, e) in entities.iter().enumerate() {
            assert!(
                e.start < e.end,
                "MockModel entity {}: start ({}) must be < end ({})",
                i,
                e.start,
                e.end
            );
            assert!(
                e.confidence >= 0.0 && e.confidence <= 1.0,
                "MockModel entity {}: confidence ({}) must be in [0.0, 1.0]",
                i,
                e.confidence
            );
        }
        self.entities = entities;
        self
    }

    /// Set supported entity types.
    #[must_use]
    pub fn with_types(mut self, types: Vec<EntityType>) -> Self {
        self.types = types;
        self
    }

    /// Disable offset validation during extraction (for testing error paths).
    #[must_use]
    pub fn without_validation(mut self) -> Self {
        self.validate = false;
        self
    }

    /// Validate that entity offsets are within text bounds.
    fn validate_entities(&self, text: &str) -> Result<()> {
        // Performance optimization: Cache text length (called once, used for all entities)
        let text_len = text.chars().count();
        for (i, e) in self.entities.iter().enumerate() {
            if e.end > text_len {
                return Err(Error::InvalidInput(format!(
                    "MockModel entity {} '{}': end offset ({}) exceeds text length ({} chars)",
                    i, e.text, e.end, text_len
                )));
            }
            // Verify text matches (using char offsets)
            // Use optimized extract_text_with_len to avoid recalculating length
            let actual_text = e.extract_text_with_len(text, text_len);
            if actual_text != e.text {
                return Err(Error::InvalidInput(format!(
                    "MockModel entity {} text mismatch: expected '{}' at [{},{}), found '{}'",
                    i, e.text, e.start, e.end, actual_text
                )));
            }
        }
        Ok(())
    }
}

impl Model for MockModel {
    fn extract_entities(&self, text: &str, _language: Option<&str>) -> Result<Vec<Entity>> {
        if self.validate && !self.entities.is_empty() {
            self.validate_entities(text)?;
        }
        Ok(self.entities.clone())
    }

    fn supported_types(&self) -> Vec<EntityType> {
        self.types.clone()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Mock NER model for testing"
    }
}
