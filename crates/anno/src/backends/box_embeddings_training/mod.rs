//! Training system for box embeddings in coreference resolution.
//!
//! This module implements the mathematical framework for learning box embeddings
//! from coreference annotations, based on research from BERE, BoxTE, and UKGE.
//!
//! **Note**: This is the canonical training implementation. The [matryoshka-box](https://github.com/arclabs561/matryoshka-box)
//! research project extends this with matryoshka-specific features (variable dimensions, etc.).
//! For standard box embedding training, use this module. For research extensions, see matryoshka-box.
//!
//! # Mathematical Foundation
//!
//! ## Objective Function
//!
//! We maximize the conditional probability for positive pairs (entities that corefer)
//! and minimize it for negative pairs (entities that don't corefer):
//!
//! ```text
//! L = -Σ log P(box_i | box_j) for positive pairs (i,j)
//!     + λ * Σ max(0, margin - P(box_i | box_j)) for negative pairs (i,j)
//! ```
//!
//! Where:
//! - P(box_i | box_j) = Vol(box_i ∩ box_j) / Vol(box_j) (conditional probability)
//! - Positive pairs: entities in the same coreference chain
//! - Negative pairs: entities in different chains
//! - λ: negative sampling weight
//! - margin: minimum separation for negative pairs
//!
//! ## Gradient Computation
//!
//! We use **analytical gradients** (not finite differences) for efficiency:
//!
//! For a box with min = μ - exp(δ)/2, max = μ + exp(δ)/2:
//!
//! ∂Vol/∂μᵢ = 0 (volume doesn't depend on center position)
//! ∂Vol/∂δᵢ = Vol * 1 (since Vol = ∏exp(δᵢ))
//!
//! For intersection volume, gradients flow through each dimension's overlap.
//!
//! ## Box Constraints
//!
//! We enforce mᵢ ≤ Mᵢ for all dimensions using:
//! - Reparameterization: Mᵢ = mᵢ + exp(δᵢ) where δᵢ is the learned parameter

#[allow(unused_imports)]
use crate::backends::box_embeddings::BoxEmbedding;

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
pub mod types;
pub use types::*;

pub mod algorithm;
pub use algorithm::{BoxEmbeddingTrainer, split_train_val};
