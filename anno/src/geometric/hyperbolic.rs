//! Hyperbolic embeddings for hierarchical entity type representation.
//!
//! Hyperbolic space (specifically the Poincaré ball) naturally represents
//! hierarchical structures because distances grow exponentially toward the
//! boundary—mirroring how hierarchies have exponentially more leaves than roots.
//!
//! # Why Hyperbolic for NLP?
//!
//! Entity type hierarchies are inherently tree-like:
//!
//! ```text
//!                    Entity
//!                   /      \
//!              Person       Location
//!             /  |  \         /    \
//!        Artist Athlete ...  City  Country
//! ```
//!
//! In Euclidean space, fitting such trees requires high dimensions.
//! In hyperbolic space, trees embed naturally with low distortion.
//!
//! # Mathematical Background
//!
//! The **Poincaré ball model** represents hyperbolic space as the open unit ball:
//!
//! ```text
//! B^n = { x ∈ ℝ^n : ||x|| < 1 }
//! ```
//!
//! With metric tensor scaled by the conformal factor:
//!
//! ```text
//! λ_x = 2 / (1 - ||x||²)
//! ```
//!
//! Geodesic distance between points x, y:
//!
//! ```text
//! d(x, y) = arcosh(1 + 2 * ||x - y||² / ((1 - ||x||²)(1 - ||y||²)))
//! ```
//!
//! # Integration with Anno
//!
//! Hyperbolic embeddings complement box embeddings:
//!
//! | Aspect | Box Embeddings | Hyperbolic |
//! |--------|---------------|------------|
//! | Structure | Axis-aligned rectangles | Points in Poincaré ball |
//! | Hierarchy | Containment | Distance to origin |
//! | Uncertainty | Volume | Not native (use boxes) |
//! | Training | Euclidean gradients | Riemannian gradients |
//!
//! # Example (Future)
//!
//! ```rust,ignore
//! use anno::geometric::hyperbolic::{HyperbolicEmbedding, PoincareDistance};
//!
//! let entity = HyperbolicEmbedding::new(vec![0.1, 0.2, 0.3])?;
//! let type_embedding = HyperbolicEmbedding::new(vec![0.05, 0.1, 0.15])?;
//!
//! // Distance in hyperbolic space
//! let dist = entity.distance(&type_embedding);
//!
//! // Hierarchy: closer to origin = more general
//! let specificity = entity.norm(); // Higher = more specific type
//! ```
//!
//! # References
//!
//! - Nickel & Kiela (2017): "Poincaré Embeddings for Learning Hierarchical Representations"
//! - Facebook Research implementation: <https://github.com/facebookresearch/poincare-embeddings>
//! - arXiv:2507.17787: "Hyperbolic Multi-Head Latent Attention" (2025)
//!
//! # Status: STUB
//!
//! This module provides trait definitions and placeholder implementations.
//! Full implementation requires:
//!
//! 1. Riemannian gradient computation
//! 2. Exponential/logarithmic maps
//! 3. Parallel transport for optimization
//!
//! See `docs/GEOMETRIC_FOUNDATIONS.md` for implementation roadmap.

use serde::{Deserialize, Serialize};

// ============================================================================
// Core Types
// ============================================================================

/// A point in the Poincaré ball model of hyperbolic space.
///
/// The embedding lives in the open unit ball: ||x|| < 1.
/// Points closer to the origin are "more general" (higher in hierarchy).
/// Points closer to the boundary are "more specific" (leaves).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperbolicEmbedding {
    /// Coordinates in the Poincaré ball (must satisfy ||coords|| < 1).
    coords: Vec<f32>,
}

/// Error type for hyperbolic operations.
#[derive(Debug, Clone, PartialEq)]
pub enum HyperbolicError {
    /// Point is outside the Poincaré ball (||x|| >= 1).
    OutsideBall {
        /// The norm of the point that was outside the ball.
        norm: f32,
    },
    /// Dimension mismatch between embeddings.
    DimensionMismatch {
        /// Expected dimension.
        expected: usize,
        /// Actual dimension received.
        got: usize,
    },
    /// Numerical instability detected.
    NumericalInstability(String),
}

impl std::fmt::Display for HyperbolicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideBall { norm } => {
                write!(f, "Point outside Poincaré ball: ||x|| = {} >= 1", norm)
            }
            Self::DimensionMismatch { expected, got } => {
                write!(f, "Dimension mismatch: expected {}, got {}", expected, got)
            }
            Self::NumericalInstability(msg) => {
                write!(f, "Numerical instability: {}", msg)
            }
        }
    }
}

impl std::error::Error for HyperbolicError {}

impl HyperbolicEmbedding {
    /// Maximum allowed norm (slightly less than 1 for numerical stability).
    const MAX_NORM: f32 = 0.99999;

    /// Epsilon for numerical stability.
    const EPS: f32 = 1e-7;

    /// Create a new hyperbolic embedding.
    ///
    /// Returns error if ||coords|| >= 1.
    pub fn new(coords: Vec<f32>) -> Result<Self, HyperbolicError> {
        let norm = Self::compute_norm(&coords);
        if norm >= 1.0 {
            return Err(HyperbolicError::OutsideBall { norm });
        }
        Ok(Self { coords })
    }

    /// Create embedding at the origin (most general point).
    pub fn origin(dim: usize) -> Self {
        Self {
            coords: vec![0.0; dim],
        }
    }

    /// Get the dimension of the embedding.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.coords.len()
    }

    /// Get the Euclidean norm of the embedding.
    ///
    /// In the Poincaré ball, norm indicates hierarchy level:
    /// - 0.0 = root (most general)
    /// - → 1.0 = leaf (most specific)
    #[must_use]
    pub fn norm(&self) -> f32 {
        Self::compute_norm(&self.coords)
    }

    /// Get the coordinates.
    #[must_use]
    pub fn coords(&self) -> &[f32] {
        &self.coords
    }

    fn compute_norm(coords: &[f32]) -> f32 {
        coords.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Compute the conformal factor λ_x = 2 / (1 - ||x||²).
    ///
    /// This scales the metric tensor at point x.
    #[must_use]
    pub fn conformal_factor(&self) -> f32 {
        let norm_sq = self.coords.iter().map(|x| x * x).sum::<f32>();
        2.0 / (1.0 - norm_sq + Self::EPS)
    }

    /// Project a point onto the Poincaré ball if it's outside.
    ///
    /// Ensures ||x|| < MAX_NORM for numerical stability.
    pub fn project_to_ball(coords: Vec<f32>) -> Self {
        let norm = Self::compute_norm(&coords);
        if norm < Self::MAX_NORM {
            Self { coords }
        } else {
            let scale = Self::MAX_NORM / (norm + Self::EPS);
            Self {
                coords: coords.iter().map(|x| x * scale).collect(),
            }
        }
    }
}

// ============================================================================
// Distance Computation
// ============================================================================

/// Trait for computing distances in hyperbolic space.
pub trait PoincareDistance {
    /// Compute the geodesic distance to another embedding.
    fn distance(&self, other: &Self) -> Result<f32, HyperbolicError>;

    /// Compute squared distance (avoids sqrt, useful for comparisons).
    fn distance_squared(&self, other: &Self) -> Result<f32, HyperbolicError>;
}

impl PoincareDistance for HyperbolicEmbedding {
    /// Compute the geodesic distance in the Poincaré ball.
    ///
    /// Formula: d(x, y) = arcosh(1 + 2 * ||x - y||² / ((1 - ||x||²)(1 - ||y||²)))
    fn distance(&self, other: &Self) -> Result<f32, HyperbolicError> {
        if self.dim() != other.dim() {
            return Err(HyperbolicError::DimensionMismatch {
                expected: self.dim(),
                got: other.dim(),
            });
        }

        let diff_norm_sq: f32 = self
            .coords
            .iter()
            .zip(other.coords.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();

        let self_norm_sq: f32 = self.coords.iter().map(|x| x * x).sum();
        let other_norm_sq: f32 = other.coords.iter().map(|x| x * x).sum();

        let denom = (1.0 - self_norm_sq) * (1.0 - other_norm_sq);
        if denom <= Self::EPS {
            return Err(HyperbolicError::NumericalInstability(
                "Denominator too small (points too close to boundary)".to_string(),
            ));
        }

        let arg = 1.0 + 2.0 * diff_norm_sq / denom;
        // arcosh(x) = ln(x + sqrt(x² - 1)) for x >= 1
        let dist = (arg + (arg * arg - 1.0).max(0.0).sqrt()).ln();

        Ok(dist)
    }

    fn distance_squared(&self, other: &Self) -> Result<f32, HyperbolicError> {
        let d = self.distance(other)?;
        Ok(d * d)
    }
}

// ============================================================================
// Hyperbolic Operations (Stubs)
// ============================================================================

/// Möbius addition in the Poincaré ball.
///
/// This is the hyperbolic analog of vector addition.
///
/// # Status: STUB
///
/// Full implementation requires careful numerical handling.
pub fn mobius_add(
    x: &HyperbolicEmbedding,
    y: &HyperbolicEmbedding,
) -> Result<HyperbolicEmbedding, HyperbolicError> {
    if x.dim() != y.dim() {
        return Err(HyperbolicError::DimensionMismatch {
            expected: x.dim(),
            got: y.dim(),
        });
    }

    // STUB: Proper Möbius addition formula
    // x ⊕ y = ((1 + 2⟨x,y⟩ + ||y||²)x + (1 - ||x||²)y) / (1 + 2⟨x,y⟩ + ||x||²||y||²)
    //
    // For now, return a placeholder that projects to the ball
    let sum: Vec<f32> = x
        .coords
        .iter()
        .zip(y.coords.iter())
        .map(|(a, b)| a + b)
        .collect();

    Ok(HyperbolicEmbedding::project_to_ball(sum))
}

/// Exponential map from tangent space to the Poincaré ball.
///
/// Maps a tangent vector at point x to a point on the manifold.
///
/// # Status: STUB
///
/// Required for Riemannian optimization.
pub fn exp_map(
    _base: &HyperbolicEmbedding,
    _tangent: &[f32],
) -> Result<HyperbolicEmbedding, HyperbolicError> {
    // TODO: Implement exp_x(v) = x ⊕ (tanh(λ_x ||v|| / 2) * v / ||v||)
    unimplemented!("Exponential map not yet implemented - see docs/GEOMETRIC_FOUNDATIONS.md")
}

/// Logarithmic map from the Poincaré ball to tangent space.
///
/// Maps a point on the manifold to a tangent vector at base.
///
/// # Status: STUB
///
/// Required for Riemannian optimization.
pub fn log_map(
    _base: &HyperbolicEmbedding,
    _point: &HyperbolicEmbedding,
) -> Result<Vec<f32>, HyperbolicError> {
    // TODO: Implement log_x(y) = (2/λ_x) * arctanh(||-x ⊕ y||) * (-x ⊕ y) / ||-x ⊕ y||
    unimplemented!("Logarithmic map not yet implemented - see docs/GEOMETRIC_FOUNDATIONS.md")
}

// ============================================================================
// Integration with Anno Types
// ============================================================================

/// Trait for types that can be embedded in hyperbolic space.
///
/// # Future Use
///
/// ```rust,ignore
/// impl HyperbolicEmbeddable for EntityType {
///     fn to_hyperbolic(&self, model: &HyperbolicModel) -> HyperbolicEmbedding {
///         model.embed_type(self)
///     }
/// }
/// ```
pub trait HyperbolicEmbeddable {
    /// Convert to a hyperbolic embedding.
    fn to_hyperbolic(&self) -> Result<HyperbolicEmbedding, HyperbolicError>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_creation() {
        let emb = HyperbolicEmbedding::new(vec![0.1, 0.2, 0.3]).unwrap();
        assert_eq!(emb.dim(), 3);
        assert!(emb.norm() < 1.0);
    }

    #[test]
    fn test_outside_ball_rejected() {
        let result = HyperbolicEmbedding::new(vec![0.7, 0.7, 0.7]);
        assert!(result.is_err());
    }

    #[test]
    fn test_origin() {
        let origin = HyperbolicEmbedding::origin(5);
        assert_eq!(origin.norm(), 0.0);
        assert_eq!(origin.dim(), 5);
    }

    #[test]
    fn test_distance_self_is_zero() {
        let emb = HyperbolicEmbedding::new(vec![0.1, 0.2]).unwrap();
        let dist = emb.distance(&emb).unwrap();
        assert!(
            dist.abs() < 1e-5,
            "Distance to self should be ~0, got {}",
            dist
        );
    }

    #[test]
    fn test_distance_symmetric() {
        let a = HyperbolicEmbedding::new(vec![0.1, 0.2]).unwrap();
        let b = HyperbolicEmbedding::new(vec![0.3, 0.1]).unwrap();

        let d_ab = a.distance(&b).unwrap();
        let d_ba = b.distance(&a).unwrap();

        assert!(
            (d_ab - d_ba).abs() < 1e-5,
            "Distance should be symmetric: {} vs {}",
            d_ab,
            d_ba
        );
    }

    #[test]
    fn test_conformal_factor() {
        let origin = HyperbolicEmbedding::origin(3);
        assert!((origin.conformal_factor() - 2.0).abs() < 1e-5);

        // Closer to boundary = higher conformal factor
        let boundary = HyperbolicEmbedding::new(vec![0.9, 0.0, 0.0]).unwrap();
        assert!(boundary.conformal_factor() > origin.conformal_factor());
    }

    #[test]
    fn test_project_to_ball() {
        let outside = vec![2.0, 2.0, 2.0];
        let projected = HyperbolicEmbedding::project_to_ball(outside);
        assert!(projected.norm() < 1.0);
    }
}
