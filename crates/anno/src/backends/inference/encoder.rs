//! Core encoder traits for GLiNER/ModernBERT-style bi-encoder extraction.

use crate::{Entity, EntityType};
use anno_core::RaggedBatch;

// Core Encoder Traits (GLiNER/ModernBERT Alignment)
// =============================================================================

/// Text encoder trait for transformer-based encoders.
///
/// # Motivation
///
/// Modern NER systems require converting raw text into dense vector representations
/// that capture semantic meaning. This trait abstracts the encoding step, allowing
/// different transformer architectures to be used interchangeably.
///
/// # Supported Architectures
///
/// | Architecture | Context | Key Features | Speed |
/// |--------------|---------|--------------|-------|
/// | ModernBERT   | 8,192   | RoPE, GeGLU, unpadded inference | 3x faster |
/// | DeBERTaV3    | 512     | Disentangled attention | Baseline |
/// | BERT/RoBERTa | 512     | Classic, widely available | Baseline |
///
/// # Research Alignment (ModernBERT, Dec 2024)
///
/// From ModernBERT paper (arXiv:2412.13663):
/// > "Pareto improvements to BERT... encoder-only models offer great
/// > performance-size tradeoff for retrieval and classification."
///
/// Key innovations:
/// - **Alternating Attention**: Global attention every 3 layers, local (128-token
///   window) elsewhere. Reduces complexity for long sequences.
/// - **Unpadding**: "ModernBERT unpads inputs *before* the token embedding layer
///   and optionally repads model outputs leading to a 10-to-20 percent
///   performance improvement over previous methods."
/// - **RoPE**: Rotary positional embeddings enable extrapolation to longer sequences.
/// - **GeGLU**: Gated activation function improves over GELU.
///
/// # Example
///
/// ```ignore
/// use anno::TextEncoder;
///
/// fn process_document(encoder: &dyn TextEncoder, text: &str) {
///     let output = encoder.encode(text).unwrap();
///     println!("Encoded {} tokens into {} dimensions",
///              output.num_tokens, output.hidden_dim);
///
///     // Token offsets map back to character positions
///     for (i, (start, end)) in output.token_offsets.iter().enumerate() {
///         println!("Token {}: chars {}..{}", i, start, end);
///     }
/// }
/// ```
pub trait TextEncoder: Send + Sync {
    /// Encode text into token embeddings.
    ///
    /// # Arguments
    /// * `text` - Input text to encode
    ///
    /// # Returns
    /// * Token embeddings as flattened [num_tokens, hidden_dim]
    /// * Attention mask indicating valid tokens
    fn encode(&self, text: &str) -> crate::Result<EncoderOutput>;

    /// Encode a batch of texts.
    ///
    /// # Arguments
    /// * `texts` - Batch of input texts
    ///
    /// # Returns
    /// * RaggedBatch containing all embeddings with document boundaries
    fn encode_batch(&self, texts: &[&str]) -> crate::Result<(Vec<f32>, RaggedBatch)>;

    /// Get the hidden dimension of the encoder.
    fn hidden_dim(&self) -> usize;

    /// Get the maximum sequence length.
    fn max_length(&self) -> usize;

    /// Get the encoder architecture name.
    fn architecture(&self) -> &'static str;
}

/// Output from text encoding.
#[derive(Debug, Clone)]
pub struct EncoderOutput {
    /// Token embeddings: [num_tokens, hidden_dim]
    pub embeddings: Vec<f32>,
    /// Number of tokens
    pub num_tokens: usize,
    /// Hidden dimension
    pub hidden_dim: usize,
    /// Token-to-character mapping (for span recovery)
    pub token_offsets: Vec<(usize, usize)>,
}

/// Label encoder trait for encoding entity type descriptions.
///
/// # Motivation
///
/// Zero-shot NER works by encoding entity type *descriptions* into the same
/// vector space as text spans. Instead of training separate classifiers for
/// each entity type, we compute similarity between spans and label embeddings.
///
/// This enables:
/// - **Unlimited entity types** at inference (no retraining needed)
/// - **Faster inference** when labels are pre-computed
/// - **Better generalization** to unseen entity types via semantic similarity
///
/// # Research Alignment
///
/// From GLiNER bi-encoder (knowledgator/modern-gliner-bi-base-v1.0):
/// > "textual encoder is ModernBERT-base and entity label encoder is
/// > sentence transformer - BGE-small-en."
///
/// # Example
///
/// ```ignore
/// use anno::LabelEncoder;
///
/// fn setup_custom_types(encoder: &dyn LabelEncoder) {
///     // Encode rich descriptions for better matching
///     let labels = &[
///         "a named individual human being",
///         "a company, institution, or organized group",
///         "a geographical location, city, country, or region",
///     ];
///
///     let embeddings = encoder.encode_labels(labels).unwrap();
///     // Store embeddings in SemanticRegistry for fast lookup
/// }
/// ```
pub trait LabelEncoder: Send + Sync {
    /// Encode a single label description.
    ///
    /// # Arguments
    /// * `label` - Label description (e.g., "a named individual human being")
    fn encode_label(&self, label: &str) -> crate::Result<Vec<f32>>;

    /// Encode multiple labels.
    ///
    /// # Arguments
    /// * `labels` - Label descriptions
    ///
    /// # Returns
    /// Flattened embeddings: [num_labels, hidden_dim]
    fn encode_labels(&self, labels: &[&str]) -> crate::Result<Vec<f32>>;

    /// Get the hidden dimension.
    fn hidden_dim(&self) -> usize;
}

/// Bi-encoder architecture combining text and label encoders.
///
/// # Motivation
///
/// The bi-encoder architecture treats NER as a **matching problem** rather than
/// a classification problem. It encodes text spans and entity labels separately,
/// then computes similarity scores to determine matches.
///
/// ```text
/// ┌─────────────────┐         ┌─────────────────┐
/// │   Text Input    │         │  Label Desc.    │
/// │ "Steve Jobs"    │         │ "person name"   │
/// └────────┬────────┘         └────────┬────────┘
///          │                           │
///          ▼                           ▼
/// ┌─────────────────┐         ┌─────────────────┐
/// │  TextEncoder    │         │  LabelEncoder   │
/// │  (ModernBERT)   │         │  (BGE-small)    │
/// └────────┬────────┘         └────────┬────────┘
///          │                           │
///          ▼                           ▼
/// ┌─────────────────┐         ┌─────────────────┐
/// │ Span Embedding  │◄───────►│ Label Embedding │
/// │   [768]         │ cosine  │   [768]         │
/// └─────────────────┘ sim     └─────────────────┘
///                      │
///                      ▼
///               Score: 0.92
/// ```
///
/// # Trade-offs
///
/// | Aspect | Bi-Encoder | Uni-Encoder |
/// |--------|------------|-------------|
/// | Entity types | Unlimited | Fixed at training |
/// | Inference speed | Faster (pre-compute labels) | Slower |
/// | Disambiguation | Harder (no label interaction) | Better |
/// | Generalization | Better to new types | Limited |
///
/// # Research Alignment
///
/// From GLiNER: "GLiNER frames NER as a matching problem, comparing candidate
/// spans with entity type embeddings."
///
/// From knowledgator: "Bi-encoder architecture brings several advantages...
/// unlimited entities, faster inference, better generalization."
///
/// Drawback: "Lack of inter-label interactions that make it hard to
/// disambiguate semantically similar but contextually different entities."
///
/// # Example
///
/// ```ignore
/// use anno::BiEncoder;
///
/// fn extract_custom_entities(bi_enc: &dyn BiEncoder, text: &str) {
///     let labels = &["software company", "hardware manufacturer", "person"];
///     let scores = bi_enc.encode_and_match(text, labels, 8).unwrap();
///
///     for s in scores.iter().filter(|s| s.score > 0.5) {
///         println!("Found '{}' as type {} (score: {:.2})",
///                  &text[s.start..s.end], labels[s.label_idx], s.score);
///     }
/// }
/// ```
pub trait BiEncoder: Send + Sync {
    /// Get the text encoder.
    fn text_encoder(&self) -> &dyn TextEncoder;

    /// Get the label encoder.
    fn label_encoder(&self) -> &dyn LabelEncoder;

    /// Encode text and labels, compute span-label similarities.
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `labels` - Entity type descriptions
    /// * `max_span_width` - Maximum span width to consider
    ///
    /// # Returns
    /// Similarity scores for each (span, label) pair
    fn encode_and_match(
        &self,
        text: &str,
        labels: &[&str],
        max_span_width: usize,
    ) -> crate::Result<Vec<SpanLabelScore>>;
}

/// Score for a (span, label) match.
#[derive(Debug, Clone)]
pub struct SpanLabelScore {
    /// Span start (character offset)
    pub start: usize,
    /// Span end (character offset, exclusive)
    pub end: usize,
    /// Label index
    pub label_idx: usize,
    /// Similarity score (0.0 - 1.0)
    pub score: f32,
}

// =============================================================================
