//! Training data types and configuration for box embedding training.

#[allow(unused_imports)]
use super::*;

use crate::backends::box_embeddings::BoxEmbedding;
use anno_core::Entity;
use anno_core::{CorefChain, CorefDocument};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Trainable Box Embedding
// =============================================================================

/// A trainable box embedding with learnable parameters.
///
/// Uses reparameterization to ensure min <= max:
/// - min = mu - exp(delta)/2
/// - max = mu + exp(delta)/2
///
/// This ensures boxes are always valid (min <= max).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainableBox {
    /// Mean position in each dimension (d-dimensional vector).
    pub mu: Vec<f32>,
    /// Log-width in each dimension (width = exp(delta)).
    pub delta: Vec<f32>,
    /// Dimension
    pub dim: usize,
}

impl TrainableBox {
    /// Create a new trainable box.
    ///
    /// # Arguments
    ///
    /// * `mu` - Mean position (center of box)
    /// * `delta` - Log-width (width = exp(delta))
    ///
    /// The box will have:
    /// - min = mu - exp(delta) / 2
    /// - max = mu + exp(delta) / 2
    #[must_use]
    pub fn new(mu: Vec<f32>, delta: Vec<f32>) -> Self {
        assert_eq!(
            mu.len(),
            delta.len(),
            "mu and delta must have same dimension"
        );
        let dim = mu.len();
        Self { mu, delta, dim }
    }

    /// Initialize from a vector embedding.
    ///
    /// Creates a small box around the vector with initial width `init_width`.
    #[must_use]
    pub fn from_vector(vector: &[f32], init_width: f32) -> Self {
        let mu = vector.to_vec();
        let delta: Vec<f32> = vec![init_width.ln(); mu.len()];
        Self::new(mu, delta)
    }

    /// Convert to a BoxEmbedding (for inference).
    #[must_use]
    pub fn to_box(&self) -> BoxEmbedding {
        let min: Vec<f32> = self
            .mu
            .iter()
            .zip(self.delta.iter())
            .map(|(&m, &d)| m - (d.exp() / 2.0))
            .collect();
        let max: Vec<f32> = self
            .mu
            .iter()
            .zip(self.delta.iter())
            .map(|(&m, &d)| m + (d.exp() / 2.0))
            .collect();
        BoxEmbedding::new(min, max)
    }
}

// =============================================================================
// Training Data Structures
// =============================================================================

/// A single training example (one document with coreference chains).
///
/// Each example contains:
/// - Entities: All mentions in the document
/// - Chains: Groups of entities that corefer
#[derive(Debug, Clone)]
pub struct TrainingExample {
    /// All entity mentions in the document
    pub entities: Vec<Entity>,
    /// Coreference chains (groups of entity IDs that refer to the same entity)
    pub chains: Vec<CorefChain>,
}

/// Convert a CorefDocument to a TrainingExample.
///
/// This extracts all mentions as entities and preserves the coreference chains.
impl From<&CorefDocument> for TrainingExample {
    fn from(doc: &CorefDocument) -> Self {
        // Collect all mentions as entities
        let mut entities = Vec::new();
        let mut mention_to_entity_id = HashMap::new();

        for chain in &doc.chains {
            for mention in &chain.mentions {
                // Use character offset as entity ID (unique per document)
                let entity_id = mention.start;

                // Convert mention entity_type (Option<String>) to EntityType
                // Default to Person if type is unknown
                let entity_type = mention
                    .entity_type
                    .as_ref()
                    .and_then(|s| match s.as_str() {
                        "PER" | "Person" | "person" => Some(anno_core::EntityType::Person),
                        "ORG" | "Organization" | "organization" => {
                            Some(anno_core::EntityType::Organization)
                        }
                        "LOC" | "Location" | "location" => Some(anno_core::EntityType::Location),
                        _ => None,
                    })
                    .unwrap_or(anno_core::EntityType::Person);

                // Create Entity from Mention
                let entity = Entity::new(
                    mention.text.clone(),
                    entity_type,
                    entity_id,
                    mention.end,
                    1.0,
                );

                entities.push(entity);
                mention_to_entity_id.insert((mention.start, mention.end), entity_id);
            }
        }

        // Rebuild chains with same mentions (they already have correct offsets)
        let chains = doc.chains.clone();

        Self { entities, chains }
    }
}

/// Convert multiple CorefDocuments to TrainingExamples.
pub fn coref_documents_to_training_examples(docs: &[CorefDocument]) -> Vec<TrainingExample> {
    docs.iter().map(TrainingExample::from).collect()
}

// =============================================================================
// Training Configuration
// =============================================================================

/// Training configuration for box embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// Learning rate
    pub learning_rate: f32,
    /// Weight for negative pairs
    pub negative_weight: f32,
    /// Margin for negative pairs
    pub margin: f32,
    /// L2 regularization weight
    pub regularization: f32,
    /// Number of training epochs
    pub epochs: usize,
    /// Batch size (for mini-batch training)
    pub batch_size: usize,
    /// Warmup epochs (linear increase from 0.1*lr to lr)
    pub warmup_epochs: usize,
    /// Use self-adversarial negative sampling
    pub use_self_adversarial: bool,
    /// Temperature for self-adversarial sampling
    pub adversarial_temperature: f32,
    /// Early stopping patience (stop if no improvement for N epochs)
    pub early_stopping_patience: Option<usize>,
    /// Minimum improvement for early stopping (relative)
    pub early_stopping_min_delta: f32,
    /// Multi-stage training: focus on positives first (epochs), then negatives
    pub positive_focus_epochs: Option<usize>,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.001,
            negative_weight: 0.5,
            margin: 0.3,
            regularization: 0.0001,
            epochs: 100,
            batch_size: 32,
            warmup_epochs: 10,
            use_self_adversarial: true,
            adversarial_temperature: 1.0,
            early_stopping_patience: Some(10),
            early_stopping_min_delta: 0.001,
            positive_focus_epochs: None,
        }
    }
}

// =============================================================================
// AMSGrad Optimizer State
// =============================================================================

/// AMSGrad optimizer state for a single box.
#[derive(Debug, Clone)]
pub struct AMSGradState {
    /// First moment estimate (m)
    pub m: Vec<f32>,
    /// Second moment estimate (v)
    pub v: Vec<f32>,
    /// Max second moment estimate (v_hat)
    pub v_hat: Vec<f32>,
    /// Iteration counter
    pub t: usize,
    /// Learning rate
    pub lr: f32,
    /// Beta1 (momentum)
    pub beta1: f32,
    /// Beta2 (RMSprop)
    pub beta2: f32,
    /// Epsilon (numerical stability)
    pub epsilon: f32,
}

impl AMSGradState {
    /// Create new AMSGrad state.
    pub fn new(dim: usize, learning_rate: f32) -> Self {
        Self {
            m: vec![0.0; dim],
            v: vec![0.0; dim],
            v_hat: vec![0.0; dim],
            t: 0,
            lr: learning_rate,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
        }
    }

    /// Set learning rate.
    pub fn set_lr(&mut self, lr: f32) {
        self.lr = lr;
    }
}

// =============================================================================
// Trainer
// =============================================================================
