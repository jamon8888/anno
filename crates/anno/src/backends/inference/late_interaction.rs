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
#[derive(Debug, Clone, Copy)]
pub struct DotProductInteraction {
    /// Temperature scaling (higher = sharper distribution)
    pub temperature: f32,
}

impl Default for DotProductInteraction {
    fn default() -> Self {
        Self { temperature: 1.0 }
    }
}

impl DotProductInteraction {
    /// Create with default temperature (1.0).
    pub fn new() -> Self {
        Self::default()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: assert two f32 slices are approximately equal.
    fn assert_approx_eq(actual: &[f32], expected: &[f32], tol: f32) {
        assert_eq!(actual.len(), expected.len(), "length mismatch");
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert!(
                (a - e).abs() < tol,
                "index {i}: actual={a}, expected={e}, diff={}",
                (a - e).abs()
            );
        }
    }

    #[test]
    fn dot_product_identity_vectors() {
        // 2 spans x 3 dim, 2 labels x 3 dim
        // span0 = [1, 0, 0], span1 = [0, 1, 0]
        // label0 = [1, 0, 0], label1 = [0, 0, 1]
        let spans = vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let labels = vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let interaction = DotProductInteraction::new();
        let scores = interaction.compute_similarity(&spans, 2, &labels, 2, 3);
        // span0 . label0 = 1, span0 . label1 = 0
        // span1 . label0 = 0, span1 . label1 = 0
        assert_approx_eq(&scores, &[1.0, 0.0, 0.0, 0.0], 1e-6);
    }

    #[test]
    fn dot_product_with_temperature() {
        let spans = vec![1.0, 2.0, 3.0];
        let labels = vec![4.0, 5.0, 6.0];
        let interaction = DotProductInteraction::with_temperature(2.0);
        let scores = interaction.compute_similarity(&spans, 1, &labels, 1, 3);
        // dot = 1*4 + 2*5 + 3*6 = 32, then * 2.0 = 64
        assert_approx_eq(&scores, &[64.0], 1e-6);
    }

    #[test]
    fn dot_product_multiple_spans_labels() {
        // 2 spans x 2 dim, 3 labels x 2 dim
        let spans = vec![1.0, 0.0, 0.0, 1.0];
        let labels = vec![1.0, 1.0, 2.0, 0.0, 0.0, 3.0];
        let interaction = DotProductInteraction::new();
        let scores = interaction.compute_similarity(&spans, 2, &labels, 3, 2);
        // span0=[1,0]: dot with [1,1]=1, [2,0]=2, [0,3]=0
        // span1=[0,1]: dot with [1,1]=1, [2,0]=0, [0,3]=3
        assert_approx_eq(&scores, &[1.0, 2.0, 0.0, 1.0, 0.0, 3.0], 1e-6);
    }

    #[test]
    fn maxsim_delegates_to_dot_product() {
        let spans = vec![1.0, 2.0, 0.5, 0.5];
        let labels = vec![1.0, 0.0, 0.0, 1.0];
        let dot = DotProductInteraction::new();
        let maxsim = MaxSimInteraction::new();
        let dot_scores = dot.compute_similarity(&spans, 2, &labels, 2, 2);
        let max_scores = maxsim.compute_similarity(&spans, 2, &labels, 2, 2);
        assert_approx_eq(&max_scores, &dot_scores, 1e-6);
    }

    #[test]
    fn apply_sigmoid_known_values() {
        let interaction = DotProductInteraction::new();
        let mut scores = vec![0.0, 1.0, -1.0, 100.0, -100.0];
        interaction.apply_sigmoid(&mut scores);
        // sigmoid(0) = 0.5
        assert!((scores[0] - 0.5).abs() < 1e-6);
        // sigmoid(1) ~ 0.7310586
        assert!((scores[1] - 0.7310586).abs() < 1e-5);
        // sigmoid(-1) ~ 0.2689414
        assert!((scores[2] - 0.2689414).abs() < 1e-5);
        // sigmoid(100) ~ 1.0
        assert!((scores[3] - 1.0).abs() < 1e-6);
        // sigmoid(-100) ~ 0.0
        assert!(scores[4].abs() < 1e-6);
    }

    #[test]
    fn dot_product_default_temperature_is_one() {
        let d = DotProductInteraction::default();
        assert!((d.temperature - 1.0).abs() < 1e-6);
    }

    #[test]
    fn empty_inputs() {
        let interaction = DotProductInteraction::new();
        let scores = interaction.compute_similarity(&[], 0, &[], 0, 4);
        assert!(scores.is_empty());
    }
}
