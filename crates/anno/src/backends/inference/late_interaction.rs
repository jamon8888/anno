//! Late interaction scoring for span-level entity matching.

#[allow(unused_imports)]
use crate::Entity;

/// | Strategy    | Formula  | Speed  | Quality  | Use case                |
/// |-------------|----------|--------|----------|-------------------------|
/// | DotProduct  | s·l      | Fast   | Good     | General purpose         |
/// | MaxSim      | max(s·l) | Medium | Better   | Multi-token labels      |
/// | Bilinear    | s·W·l    | Slow   | Best     | When accuracy critical  |
///
/// # Example
///
/// ```ignore
/// use anno::{LateInteraction, DotProductInteraction};
///
/// let interaction = DotProductInteraction::with_temperature(20.0);
///
/// // Span embeddings: 3 spans × 768 dim
/// let span_embs: Vec<f32> = get_span_embeddings(&tokens, &candidates);
///
/// // Label embeddings: 5 labels × 768 dim
/// let label_embs: Vec<f32> = registry.all_embeddings();
///
/// // Compute 3×5 = 15 similarity scores
/// let mut scores = interaction.compute_similarity(
///     &span_embs, 3, &label_embs, 5, 768
/// );
/// interaction.apply_sigmoid(&mut scores);
///
/// // scores[i*5 + j] = similarity between span i and label j
/// ```
pub trait LateInteraction: Send + Sync {
    /// Compute similarity scores between span and label embeddings.
    ///
    /// # Arguments
    /// * `span_embeddings` - Shape: [num_spans, hidden_dim]
    /// * `label_embeddings` - Shape: [num_labels, hidden_dim]
    ///
    /// # Returns
    /// Similarity matrix of shape: [num_spans, num_labels]
    fn compute_similarity(
        &self,
        span_embeddings: &[f32],
        num_spans: usize,
        label_embeddings: &[f32],
        num_labels: usize,
        hidden_dim: usize,
    ) -> Vec<f32>;

    /// Apply sigmoid activation to scores.
    fn apply_sigmoid(&self, scores: &mut [f32]) {
        for s in scores.iter_mut() {
            *s = 1.0 / (1.0 + (-*s).exp());
        }
    }
}

/// Dot product interaction (default, fast).
#[derive(Debug, Clone, Copy, Default)]
pub struct DotProductInteraction {
    /// Temperature scaling (higher = sharper distribution)
    pub temperature: f32,
}

impl DotProductInteraction {
    /// Create with default temperature (1.0).
    pub fn new() -> Self {
        Self { temperature: 1.0 }
    }

    /// Create with custom temperature.
    #[must_use]
    pub fn with_temperature(temperature: f32) -> Self {
        Self { temperature }
    }
}

impl LateInteraction for DotProductInteraction {
    fn compute_similarity(
        &self,
        span_embeddings: &[f32],
        num_spans: usize,
        label_embeddings: &[f32],
        num_labels: usize,
        hidden_dim: usize,
    ) -> Vec<f32> {
        let mut scores = vec![0.0f32; num_spans * num_labels];

        for s in 0..num_spans {
            let span_start = s * hidden_dim;
            let span_end = span_start + hidden_dim;
            let span_vec = &span_embeddings[span_start..span_end];

            for l in 0..num_labels {
                let label_start = l * hidden_dim;
                let label_end = label_start + hidden_dim;
                let label_vec = &label_embeddings[label_start..label_end];

                // Dot product
                let mut dot: f32 = span_vec
                    .iter()
                    .zip(label_vec.iter())
                    .map(|(a, b)| a * b)
                    .sum();

                // Temperature scaling
                dot *= self.temperature;

                scores[s * num_labels + l] = dot;
            }
        }

        scores
    }
}

/// MaxSim interaction (ColBERT-style, better for phrases).
#[derive(Debug, Clone, Copy, Default)]
pub struct MaxSimInteraction {
    /// Temperature scaling
    pub temperature: f32,
}

impl MaxSimInteraction {
    /// Create with default settings.
    pub fn new() -> Self {
        Self { temperature: 1.0 }
    }
}

impl LateInteraction for MaxSimInteraction {
    fn compute_similarity(
        &self,
        span_embeddings: &[f32],
        num_spans: usize,
        label_embeddings: &[f32],
        num_labels: usize,
        hidden_dim: usize,
    ) -> Vec<f32> {
        // For single-vector embeddings, MaxSim degrades to dot product
        // True MaxSim requires multi-vector representations
        DotProductInteraction::new().compute_similarity(
            span_embeddings,
            num_spans,
            label_embeddings,
            num_labels,
            hidden_dim,
        )
    }
}

// =============================================================================
// Span Representation
// =============================================================================
