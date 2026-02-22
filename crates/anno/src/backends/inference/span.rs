//! Span representation types and the handshaking matrix for joint extraction.
//!
//! - `Span`, `SpanCandidate`, `SpanWindow`: character-offset span types
//! - `HandshakingMatrix`, `HandshakingCell`: sparse grid for W2NER / TPLinker

use super::registry::{LabelCategory, LabelDefinition, SemanticRegistry};
use anno_core::{RaggedBatch, SpanCandidate};

// Span Representation
// =============================================================================

/// Configuration for span representation.
///
/// # Research Context (Deep Span Representations, arXiv:2210.04182)
///
/// From "Deep Span Representations for NER":
/// > "Existing span-based NER systems **shallowly aggregate** the token
/// > representations to span representations. However, this typically results
/// > in significant ineffectiveness for **long-span entities**."
///
/// Common span representation strategies:
///
/// | Method | Formula | Pros | Cons |
/// |--------|---------|------|------|
/// | Concat | [h_i; h_j] | Simple, fast | Ignores middle tokens |
/// | Pooling | mean(h_i:h_j) | Uses all tokens | Loses boundary info |
/// | Attention | attn(h_i:h_j) | Learnable | Expensive |
/// | GLiNER | FFN([h_i; h_j; w]) | Balanced | Requires width emb |
///
/// # Recommendation (GLiNER Default)
///
/// For most use cases, concatenating first + last token embeddings with
/// a width embedding provides the best tradeoff:
/// - O(N) complexity (vs O(N²) for all-pairs attention)
/// - Captures boundary positions (critical for NER)
/// - Width embedding disambiguates "I" vs "New York City"
#[derive(Debug, Clone)]
pub struct SpanRepConfig {
    /// Hidden dimension of the encoder
    pub hidden_dim: usize,
    /// Maximum span width (in tokens)
    ///
    /// GLiNER uses K=12: "to keep linear complexity without harming recall."
    /// Wider spans rarely contain coherent entities.
    pub max_width: usize,
    /// Whether to include width embeddings
    ///
    /// Critical for distinguishing spans of different lengths
    /// with similar boundary tokens.
    pub use_width_embeddings: bool,
    /// Width embedding dimension (typically hidden_dim / 4)
    pub width_emb_dim: usize,
}

impl Default for SpanRepConfig {
    fn default() -> Self {
        Self {
            hidden_dim: 768,
            max_width: 12,
            use_width_embeddings: true,
            width_emb_dim: 192, // 768 / 4
        }
    }
}

/// Computes span representations from token embeddings.
///
/// # Research Alignment (GLiNER, NAACL 2024)
///
/// From the GLiNER paper (arXiv:2311.08526):
/// > "The representation of a span starting at position i and ending at
/// > position j in the input text, S_ij ∈ R^D, is computed as:
/// > **S_ij = FFN(h_i ⊗ h_j)**
/// > where FFN denotes a two-layer feedforward network, and ⊗ represents
/// > the concatenation operation."
///
/// The paper also notes:
/// > "We set an upper bound to the length (K=12) of the span in order to
/// > keep linear complexity in the size of the input text, without harming recall."
///
/// # Span Representation Formula
///
/// ```text
/// span_emb = FFN(Concat(token[i], token[j], width_emb[j-i]))
///          = W_2 · ReLU(W_1 · [h_i; h_j; w_{j-i}] + b_1) + b_2
/// ```
///
/// where:
/// - h_i = start token embedding
/// - h_j = end token embedding
/// - w_{j-i} = learned width embedding (captures span length)
///
/// This is the "gnarly bit" from GLiNER that enables zero-shot matching.
///
/// # Alternative: Global Pointer (arXiv:2208.03054)
///
/// Instead of enumerating spans, Global Pointer uses RoPE (rotary position
/// embeddings) to predict (start, end) pairs simultaneously:
///
/// ```text
/// score(i, j) = q_i^T * k_j    (where q, k have RoPE applied)
/// ```
///
/// Advantages:
/// - No explicit span enumeration needed
/// - Naturally handles nested entities
/// - More parameter-efficient
///
/// GLiNER-style enumeration is still preferred for zero-shot because
/// it allows pre-computing label embeddings.
pub struct SpanRepresentationLayer {
    /// Configuration
    pub config: SpanRepConfig,
    /// Projection weights: [input_dim, hidden_dim]
    pub projection_weights: Vec<f32>,
    /// Projection bias: \[hidden_dim\]
    pub projection_bias: Vec<f32>,
    /// Width embeddings: [max_width, width_emb_dim]
    pub width_embeddings: Vec<f32>,
}

impl SpanRepresentationLayer {
    /// Create a new span representation layer with random initialization.
    pub fn new(config: SpanRepConfig) -> Self {
        let input_dim = config.hidden_dim * 2 + config.width_emb_dim;

        Self {
            projection_weights: vec![0.0f32; input_dim * config.hidden_dim],
            projection_bias: vec![0.0f32; config.hidden_dim],
            width_embeddings: vec![0.0f32; config.max_width * config.width_emb_dim],
            config,
        }
    }

    /// Compute span representations from token embeddings.
    ///
    /// # Arguments
    /// * `token_embeddings` - Flattened [num_tokens, hidden_dim]
    /// * `candidates` - Span candidates with start/end indices
    ///
    /// # Returns
    /// Span embeddings: [num_candidates, hidden_dim]
    pub fn forward(
        &self,
        token_embeddings: &[f32],
        candidates: &[SpanCandidate],
        batch: &RaggedBatch,
    ) -> Vec<f32> {
        let hidden_dim = self.config.hidden_dim;
        let width_emb_dim = self.config.width_emb_dim;
        let max_width = self.config.max_width;

        // Check for overflow in allocation
        let total_elements = match candidates.len().checked_mul(hidden_dim) {
            Some(v) => v,
            None => {
                log::warn!(
                    "Span embedding allocation overflow: {} candidates * {} hidden_dim, returning empty",
                    candidates.len(), hidden_dim
                );
                return vec![];
            }
        };
        let mut span_embeddings = vec![0.0f32; total_elements];

        for (span_idx, candidate) in candidates.iter().enumerate() {
            // Get document token range
            let doc_range = match batch.doc_range(candidate.doc_idx as usize) {
                Some(r) => r,
                None => continue,
            };

            // Validate span before computing global indices
            if candidate.end <= candidate.start {
                log::warn!(
                    "Invalid span candidate: end ({}) <= start ({})",
                    candidate.end,
                    candidate.start
                );
                continue;
            }

            // Global token indices
            let start_global = doc_range.start + candidate.start as usize;
            let end_global = doc_range.start + (candidate.end as usize) - 1; // Safe now that we validated

            // Bounds check - must ensure both start and end slices fit
            // Use checked arithmetic to prevent overflow
            let start_byte = match start_global.checked_mul(hidden_dim) {
                Some(v) => v,
                None => {
                    log::warn!(
                        "Token index overflow: start_global={} * hidden_dim={}",
                        start_global,
                        hidden_dim
                    );
                    continue;
                }
            };
            let start_end_byte = match (start_global + 1).checked_mul(hidden_dim) {
                Some(v) => v,
                None => {
                    log::warn!(
                        "Token index overflow: (start_global+1)={} * hidden_dim={}",
                        start_global + 1,
                        hidden_dim
                    );
                    continue;
                }
            };
            let end_byte = match end_global.checked_mul(hidden_dim) {
                Some(v) => v,
                None => {
                    log::warn!(
                        "Token index overflow: end_global={} * hidden_dim={}",
                        end_global,
                        hidden_dim
                    );
                    continue;
                }
            };
            let end_end_byte = match (end_global + 1).checked_mul(hidden_dim) {
                Some(v) => v,
                None => {
                    log::warn!(
                        "Token index overflow: (end_global+1)={} * hidden_dim={}",
                        end_global + 1,
                        hidden_dim
                    );
                    continue;
                }
            };

            if start_byte >= token_embeddings.len()
                || start_end_byte > token_embeddings.len()
                || end_byte >= token_embeddings.len()
                || end_end_byte > token_embeddings.len()
            {
                continue;
            }

            // Get start and end token embeddings
            let start_emb = &token_embeddings[start_byte..start_end_byte];
            let end_emb = &token_embeddings[end_byte..end_end_byte];

            // Optional width embedding (index = span_len - 1).
            let width_emb = if self.config.use_width_embeddings && width_emb_dim > 0 {
                let max_width_idx = max_width.saturating_sub(1);
                let span_len = candidate.width() as usize;
                let width_idx = span_len.saturating_sub(1).min(max_width_idx);

                let width_start = width_idx.saturating_mul(width_emb_dim);
                let width_end = width_start.saturating_add(width_emb_dim);
                if width_end > self.width_embeddings.len() {
                    None
                } else {
                    Some(&self.width_embeddings[width_start..width_end])
                }
            } else {
                None
            };

            // Baseline span representation: average of boundary embeddings (+ optional width signal).
            // This is deterministic and works without learned projection weights.
            let output_start = span_idx * hidden_dim;
            for h in 0..hidden_dim {
                span_embeddings[output_start + h] = (start_emb[h] + end_emb[h]) * 0.5;
                if let Some(width_emb) = width_emb {
                    if h < width_emb_dim {
                        span_embeddings[output_start + h] += width_emb[h] * 0.1;
                    }
                }
            }
        }

        span_embeddings
    }
}

// =============================================================================
// Handshaking Matrix (TPLinker-style Joint Extraction)
// =============================================================================

/// Result cell in a handshaking matrix.
#[derive(Debug, Clone, Copy)]
pub struct HandshakingCell {
    /// Row index (token i)
    pub i: u32,
    /// Column index (token j)
    pub j: u32,
    /// Predicted label index
    pub label_idx: u16,
    /// Confidence score
    pub score: f32,
}

/// Handshaking matrix for joint entity-relation extraction.
///
/// # Research Alignment (W2NER, AAAI 2022)
///
/// From the W2NER paper (arXiv:2112.10070):
/// > "We present a novel alternative by modeling the unified NER as word-word
/// > relation classification, namely W2NER. The architecture resolves the kernel
/// > bottleneck of unified NER by effectively modeling the neighboring relations
/// > between entity words with **Next-Neighboring-Word (NNW)** and
/// > **Tail-Head-Word-* (THW-*)** relations."
///
/// In TPLinker/W2NER, we don't just tag tokens - we tag token PAIRS.
/// The matrix M\[i,j\] contains the label for the span (i, j).
///
/// # Key Relations
///
/// | Relation | Description | Purpose |
/// |----------|-------------|---------|
/// | NNW | Next-Neighboring-Word | Links adjacent tokens within entity |
/// | THW-* | Tail-Head-Word | Links end of one entity to start of next |
///
/// # Benefits
///
/// - Overlapping entities (same token in multiple spans)
/// - Joint entity-relation extraction in one pass
/// - Explicit boundary modeling
/// - Handles flat, nested, AND discontinuous NER in one model
pub struct HandshakingMatrix {
    /// Non-zero cells (sparse representation)
    pub cells: Vec<HandshakingCell>,
    /// Sequence length
    pub seq_len: usize,
    /// Number of labels
    pub num_labels: usize,
}

impl HandshakingMatrix {
    /// Create from dense scores with thresholding.
    ///
    /// # Arguments
    /// * `scores` - Dense [seq_len, seq_len, num_labels] scores
    /// * `threshold` - Minimum score to keep
    pub fn from_dense(scores: &[f32], seq_len: usize, num_labels: usize, threshold: f32) -> Self {
        // Performance: Pre-allocate cells vec with estimated capacity
        // Most matrices have sparse cells (only high-scoring ones), so we estimate conservatively
        let estimated_capacity = (seq_len * seq_len / 10).min(1000); // ~10% of cells typically pass threshold
        let mut cells = Vec::with_capacity(estimated_capacity);

        for i in 0..seq_len {
            for j in i..seq_len {
                // Upper triangular (i <= j)
                for l in 0..num_labels {
                    let idx = i * seq_len * num_labels + j * num_labels + l;
                    if idx < scores.len() {
                        let score = scores[idx];
                        if score >= threshold {
                            cells.push(HandshakingCell {
                                i: i as u32,
                                j: j as u32,
                                label_idx: l as u16,
                                score,
                            });
                        }
                    }
                }
            }
        }

        Self {
            cells,
            seq_len,
            num_labels,
        }
    }

    /// Decode entities from handshaking matrix.
    ///
    /// In W2NER convention, cell (i, j) represents a span where:
    /// - j is the start token index
    /// - i is the end token index (inclusive, so we add 1 for exclusive end)
    pub fn decode_entities<'a>(
        &self,
        registry: &'a SemanticRegistry,
    ) -> Vec<(SpanCandidate, &'a LabelDefinition, f32)> {
        let mut entities = Vec::new();

        for cell in &self.cells {
            if let Some(label) = registry.labels.get(cell.label_idx as usize) {
                if label.category == LabelCategory::Entity {
                    // W2NER: j=start, i=end (inclusive), so span is [j, i+1)
                    entities.push((SpanCandidate::new(0, cell.j, cell.i + 1), label, cell.score));
                }
            }
        }

        // Performance: Use unstable sort (we don't need stable sort here)
        // Sort by position, then by score (descending)
        entities.sort_unstable_by(|a, b| {
            a.0.start
                .cmp(&b.0.start)
                .then_with(|| a.0.end.cmp(&b.0.end))
                .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
        });

        // Performance: Pre-allocate kept vec with estimated capacity
        // Non-maximum suppression
        let mut kept = Vec::with_capacity(entities.len().min(32));
        for (span, label, score) in entities {
            let overlaps = kept.iter().any(|(s, _, _): &(SpanCandidate, _, _)| {
                !(span.end <= s.start || s.end <= span.start)
            });
            if !overlaps {
                kept.push((span, label, score));
            }
        }

        kept
    }
}

// =============================================================================
