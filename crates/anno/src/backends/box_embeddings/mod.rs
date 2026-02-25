//! Box embeddings for coreference resolution.
//!
//! This module implements geometric representations (box embeddings) that encode
//! logical invariants of coreference resolution, addressing limitations of
//! vector-based approaches.
//!
//! **Note**: Training code is in `box_embeddings_training.rs`. The [matryoshka-box](https://github.com/arclabs561/matryoshka-box)
//! research project extends training with matryoshka-specific features (variable dimensions, etc.).
//!
//! # Key Concepts
//!
//! - **Box Embeddings**: Entities represented as axis-aligned hyperrectangles
//! - **Conditional Probability**: Coreference = high mutual overlap
//! - **Temporal Boxes**: Entities that evolve over time
//! - **Uncertainty-Aware**: Box volume = confidence
//!
//! # Research Background
//!
//! This implementation is related to the **matryoshka-box** research project (not yet published),
//! which combines matryoshka embeddings (variable dimensions) with box embeddings (hierarchical reasoning).
//! Standard training is in `box_embeddings_training.rs`; matryoshka-box extends it with research features.
//!
//! Based on research from:
//! - Vilnis et al. (2018): "Probabilistic Embedding of Knowledge Graphs with Box Lattice Measures"
//! - Lee et al. (2022): "Box Embeddings for Event-Event Relation Extraction" (BERE)
//! - Messner et al. (2022): "Temporal Knowledge Graph Completion with Box Embeddings" (BoxTE)
//! - Chen et al. (2021): "Uncertainty-Aware Knowledge Graph Embeddings" (UKGE)
//!
//! # Complementary Geometric Representations
//!
//! Box embeddings are one of several geometric approaches available in Anno.
//! See `archive/geometric-2024-12/` for alternatives:
//!
//! | Representation | Best For | Module |
//! |---------------|----------|--------|
//! | **Box embeddings** | Temporal, uncertainty | This module |
//! | Hyperbolic (Poincaré) | Deep type hierarchies | `archive/geometric-2024-12/hyperbolic.rs` |
//! | Sheaf NN | Gradient-level transitivity | `archive/geometric-2024-12/sheaf.rs` |
//! | TDA | Structural diagnostics | `archive/geometric-2024-12/tda.rs` |
//!
//! These approaches are **complementary**, not competing. Use boxes when you need:
//! - Explicit uncertainty (volume = confidence)
//! - Temporal evolution (min/max with velocity)
//! - Easy visualization and debugging

use serde::{Deserialize, Serialize};
use std::f32;

/// A box embedding representing an entity in d-dimensional space.
///
/// Boxes are axis-aligned hyperrectangles defined by min/max bounds in each dimension.
/// Coreference is modeled as high mutual conditional probability (overlap).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxEmbedding {
    /// Lower bound in each dimension (d-dimensional vector).
    pub min: Vec<f32>,
    /// Upper bound in each dimension (d-dimensional vector).
    pub max: Vec<f32>,
}

impl BoxEmbedding {
    /// Create a new box embedding.
    ///
    /// # Panics
    ///
    /// Panics if `min.len() != max.len()` or if any `min[i] > max[i]`.
    pub fn new(min: Vec<f32>, max: Vec<f32>) -> Self {
        assert_eq!(min.len(), max.len(), "min and max must have same dimension");
        for (i, (&m, &max_val)) in min.iter().zip(max.iter()).enumerate() {
            assert!(
                m <= max_val,
                "min[{}] = {} must be <= max[{}] = {}",
                i,
                m,
                i,
                max_val
            );
        }
        Self { min, max }
    }

    /// Get the dimension of the box.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.min.len()
    }

    /// Compute the volume of the box.
    ///
    /// Volume = product of (max - min) for each dimension.
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.min
            .iter()
            .zip(self.max.iter())
            .map(|(&m, &max_val)| (max_val - m).max(0.0))
            .product()
    }

    /// Compute the intersection volume with another box.
    ///
    /// Returns 0.0 if boxes are disjoint.
    #[must_use]
    pub fn intersection_volume(&self, other: &Self) -> f32 {
        assert_eq!(
            self.dim(),
            other.dim(),
            "Boxes must have same dimension for intersection"
        );

        self.min
            .iter()
            .zip(self.max.iter())
            .zip(other.min.iter().zip(other.max.iter()))
            .map(|((&m1, &max1), (&m2, &max2))| {
                let intersection_min = m1.max(m2);
                let intersection_max = max1.min(max2);
                (intersection_max - intersection_min).max(0.0)
            })
            .product()
    }

    /// Compute conditional probability P(self | other).
    ///
    /// This is the BERE model's coreference metric:
    /// P(A|B) = Vol(A ∩ B) / Vol(B)
    ///
    /// Returns a value in [0.0, 1.0] where:
    /// - 1.0 = self is completely contained in other
    /// - 0.0 = boxes are disjoint
    #[must_use]
    pub fn conditional_probability(&self, other: &Self) -> f32 {
        let vol_other = other.volume();
        if vol_other == 0.0 {
            return 0.0;
        }
        self.intersection_volume(other) / vol_other
    }

    /// Compute mutual coreference score.
    ///
    /// Coreference requires high mutual conditional probability:
    /// score = (P(A|B) + P(B|A)) / 2
    ///
    /// This ensures both boxes largely contain each other (high overlap).
    #[must_use]
    pub fn coreference_score(&self, other: &Self) -> f32 {
        let p_a_given_b = self.conditional_probability(other);
        let p_b_given_a = other.conditional_probability(self);
        (p_a_given_b + p_b_given_a) / 2.0
    }

    /// Check if this box is contained in another box.
    ///
    /// Returns true if self ⊆ other (all dimensions).
    #[must_use]
    pub fn is_contained_in(&self, other: &Self) -> bool {
        assert_eq!(self.dim(), other.dim(), "Boxes must have same dimension");
        self.min
            .iter()
            .zip(self.max.iter())
            .zip(other.min.iter().zip(other.max.iter()))
            .all(|((&m1, &max1), (&m2, &max2))| m2 <= m1 && max1 <= max2)
    }

    /// Check if boxes are disjoint (no overlap).
    #[must_use]
    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.intersection_volume(other) == 0.0
    }

    /// Create a box embedding from a vector embedding.
    ///
    /// Converts a point embedding to a box by creating a small hypercube
    /// around the point. The box size is controlled by `radius`.
    ///
    /// # Arguments
    ///
    /// * `vector` - Vector embedding (point in space)
    /// * `radius` - Half-width of the box in each dimension
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let vector = vec![0.5, 0.5, 0.5];
    /// let box_embedding = BoxEmbedding::from_vector(&vector, 0.1);
    /// // Creates box: min=[0.4, 0.4, 0.4], max=[0.6, 0.6, 0.6]
    /// ```
    #[must_use]
    pub fn from_vector(vector: &[f32], radius: f32) -> Self {
        let min: Vec<f32> = vector.iter().map(|&v| v - radius).collect();
        let max: Vec<f32> = vector.iter().map(|&v| v + radius).collect();
        Self::new(min, max)
    }

    /// Create a box embedding from a vector with adaptive radius.
    ///
    /// Uses a radius proportional to the vector's magnitude, creating
    /// larger boxes for vectors further from the origin.
    ///
    /// # Arguments
    ///
    /// * `vector` - Vector embedding
    /// * `radius_factor` - Multiplier for adaptive radius (default: 0.1)
    #[must_use]
    pub fn from_vector_adaptive(vector: &[f32], radius_factor: f32) -> Self {
        let magnitude: f32 = vector.iter().map(|&v| v * v).sum::<f32>().sqrt();
        let radius = magnitude * radius_factor + 0.01; // Add small epsilon
        Self::from_vector(vector, radius)
    }

    /// Get the center point of the box.
    ///
    /// Returns the midpoint in each dimension.
    #[must_use]
    pub fn center(&self) -> Vec<f32> {
        self.min
            .iter()
            .zip(self.max.iter())
            .map(|(&m, &max_val)| (m + max_val) / 2.0)
            .collect()
    }

    /// Get the size (width) in each dimension.
    #[must_use]
    pub fn size(&self) -> Vec<f32> {
        self.min
            .iter()
            .zip(self.max.iter())
            .map(|(&m, &max_val)| (max_val - m).max(0.0))
            .collect()
    }

    /// Compute the intersection box with another box.
    ///
    /// Returns a new box representing the overlapping region.
    /// If boxes are disjoint, returns a zero-volume box.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        assert_eq!(
            self.dim(),
            other.dim(),
            "Boxes must have same dimension for intersection"
        );

        let min: Vec<f32> = self
            .min
            .iter()
            .zip(other.min.iter())
            .map(|(&a, &b)| a.max(b))
            .collect();

        let max: Vec<f32> = self
            .max
            .iter()
            .zip(other.max.iter())
            .map(|(&a, &b)| a.min(b))
            .collect();

        Self { min, max }
    }

    /// Compute the union box (bounding box containing both).
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        assert_eq!(
            self.dim(),
            other.dim(),
            "Boxes must have same dimension for union"
        );

        let min: Vec<f32> = self
            .min
            .iter()
            .zip(other.min.iter())
            .map(|(&a, &b)| a.min(b))
            .collect();

        let max: Vec<f32> = self
            .max
            .iter()
            .zip(other.max.iter())
            .map(|(&a, &b)| a.max(b))
            .collect();

        Self { min, max }
    }

    /// Compute overlap probability (Jaccard-style).
    ///
    /// P(overlap) = Vol(intersection) / Vol(union)
    #[must_use]
    pub fn overlap_prob(&self, other: &Self) -> f32 {
        let intersection_vol = self.intersection_volume(other);
        let union_vol = self.volume() + other.volume() - intersection_vol;
        if union_vol == 0.0 {
            return 0.0;
        }
        intersection_vol / union_vol
    }

    /// Compute minimum Euclidean distance between two boxes.
    ///
    /// Returns 0.0 if boxes overlap.
    #[must_use]
    pub fn distance(&self, other: &Self) -> f32 {
        assert_eq!(
            self.dim(),
            other.dim(),
            "Boxes must have same dimension for distance"
        );

        let dist_sq: f32 = self
            .min
            .iter()
            .zip(self.max.iter())
            .zip(other.min.iter().zip(other.max.iter()))
            .map(|((&min1, &max1), (&min2, &max2))| {
                // Gap in this dimension
                let gap = if max1 < min2 {
                    min2 - max1 // other is to the right
                } else if max2 < min1 {
                    min1 - max2 // other is to the left
                } else {
                    0.0 // overlap in this dimension
                };
                gap * gap
            })
            .sum();

        dist_sq.sqrt()
    }
}

// =============================================================================
// Subsume Trait Implementation (optional, feature-gated)
// =============================================================================

/// Implements the subsume-core Box trait when the `subsume` feature is enabled.
///
/// This allows anno's BoxEmbedding to be used with subsume's distance metrics,
/// training utilities, and other advanced box operations.
#[cfg(feature = "subsume")]
impl subsume::Box for BoxEmbedding {
    type Scalar = f32;
    type Vector = Vec<f32>;

    fn min(&self) -> &Self::Vector {
        &self.min
    }

    fn max(&self) -> &Self::Vector {
        &self.max
    }

    fn dim(&self) -> usize {
        self.min.len()
    }

    fn volume(&self, _temperature: Self::Scalar) -> Result<Self::Scalar, subsume::BoxError> {
        // anno's BoxEmbedding doesn't use temperature (hard boxes)
        Ok(BoxEmbedding::volume(self))
    }

    fn intersection(&self, other: &Self) -> Result<Self, subsume::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }
        Ok(BoxEmbedding::intersection(self, other))
    }

    fn containment_prob(
        &self,
        other: &Self,
        _temperature: Self::Scalar,
    ) -> Result<Self::Scalar, subsume::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }
        // subsume: P(other ⊆ self) = Vol(intersection) / Vol(other)
        // This is the same as anno's conditional_probability but with swapped args
        Ok(self.conditional_probability(other))
    }

    fn overlap_prob(
        &self,
        other: &Self,
        _temperature: Self::Scalar,
    ) -> Result<Self::Scalar, subsume::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }
        Ok(BoxEmbedding::overlap_prob(self, other))
    }

    fn union(&self, other: &Self) -> Result<Self, subsume::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }
        Ok(BoxEmbedding::union(self, other))
    }

    fn center(&self) -> Result<Self::Vector, subsume::BoxError> {
        Ok(BoxEmbedding::center(self))
    }

    fn distance(&self, other: &Self) -> Result<Self::Scalar, subsume::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }
        Ok(BoxEmbedding::distance(self, other))
    }

    fn truncate(&self, k: usize) -> Result<Self, subsume::BoxError> {
        if k > self.dim() {
            return Err(subsume::BoxError::MatryoshkaMismatch {
                requested: k,
                actual: self.dim(),
            });
        }
        Ok(BoxEmbedding::new(
            self.min[..k].to_vec(),
            self.max[..k].to_vec(),
        ))
    }
}

/// Configuration for box-based coreference resolution.
pub mod extras;
pub use extras::*;
#[cfg(test)]
mod tests;
