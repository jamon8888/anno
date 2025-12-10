//! Geometric and topological foundations for entity resolution.
//!
//! This module provides advanced geometric representations that complement
//! Anno's existing box embeddings with alternative mathematical frameworks:
//!
//! - [`hyperbolic`]: Poincaré ball embeddings for hierarchical entity types
//! - [`sheaf`]: Sheaf neural networks for gradient-level transitivity
//! - [`tda`]: Topological data analysis for structural analysis
//!
//! # Design Philosophy
//!
//! These are **complementary** approaches, not replacements:
//!
//! | Representation | Best For | Current Status |
//! |---------------|----------|----------------|
//! | Box embeddings | Temporal, uncertainty | Implemented |
//! | Hyperbolic | Deep type hierarchies | Stub |
//! | Sheaf NN | Transitivity enforcement | Stub |
//! | TDA | Diagnostic analysis | Stub |
//!
//! # Relationship to External Projects
//!
//! Anno's geometric module is **self-contained** — it doesn't depend on external projects.
//! However, it's designed to integrate with the broader ecosystem:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    PROJECT RELATIONSHIPS                        │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
//! │  anno (this crate)                                              │
//! │  ├── backends/box_embeddings.rs  ← Basic box operations        │
//! │  ├── geometric/                   ← THIS MODULE (stubs)        │
//! │  │   ├── hyperbolic.rs           ← Poincaré ball               │
//! │  │   ├── sheaf.rs                ← Sheaf neural networks       │
//! │  │   └── tda.rs                  ← Topological analysis        │
//! │  └── eval/coref_metrics.rs       ← Evaluation metrics          │
//! │                                                                 │
//! │  box-coref (separate repo, depends on anno + subsume)           │
//! │  └── Training infrastructure, could adopt sheaf losses          │
//! │                                                                 │
//! │  subsume (separate repo, pure geometry)                         │
//! │  └── Advanced box math (ndarray), no sheaf/hyperbolic           │
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Base Primitives (What We Actually Use)
//!
//! All geometric operations use **only standard library types**:
//!
//! | Type | Usage | Why Not Tensors? |
//! |------|-------|------------------|
//! | `Vec<f32>` | Embeddings, weights | Simple, no deps |
//! | `HashMap` | Graph structure | Flexible adjacency |
//! | `serde` | Persistence | Optional serialization |
//!
//! **Explicitly NOT imported**:
//! - `subsume` crate (pure geometry, separate project)
//! - `candle` / `ndarray` (GPU tensors — feature-gated stubs exist)
//! - `torch` / `tch` (Rust bindings to PyTorch)
//!
//! This keeps anno self-contained. Training with GPU happens in:
//! - **box-coref** (Python/PyTorch) for box embeddings
//! - Future Candle feature for in-process GPU (when implemented)
//!
//! # Research Background
//!
//! See `docs/GEOMETRIC_FOUNDATIONS.md` for:
//! - Mathematical details
//! - Implementation priorities
//! - Reference implementations (Apache 2.0 licensed)
//!
//! # When to Use What
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ Question: How to represent entity relationships?                │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
//! │  Need temporal evolution?                                       │
//! │    └─ Yes → BoxEmbedding (with velocity)                       │
//! │                                                                 │
//! │  Need uncertainty quantification?                               │
//! │    └─ Yes → BoxEmbedding (volume = confidence)                 │
//! │                                                                 │
//! │  Need deep type hierarchies?                                    │
//! │    └─ Yes → HyperbolicEmbedding (Poincaré ball)               │
//! │                                                                 │
//! │  Need gradient-level transitivity?                              │
//! │    └─ Yes → SheafDiffusion (Laplacian energy)                  │
//! │                                                                 │
//! │  Need structural diagnostics?                                   │
//! │    └─ Yes → PersistentHomology (attention topology)            │
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod hyperbolic;
pub mod sheaf;
pub mod tda;

// Re-exports for convenience
pub use hyperbolic::{HyperbolicEmbedding, PoincareDistance};
pub use sheaf::{RestrictionMap, SheafDiffusionConfig, SheafGraph};
pub use tda::{PersistenceDiagram, PersistencePair};

// ============================================================================
// Unified Geometric Coreference Trait
// ============================================================================

/// A geometric representation of an entity mention.
///
/// This trait unifies different geometric embeddings (boxes, hyperbolic, etc.)
/// by providing a common interface for coreference scoring.
pub trait GeometricMention: Sized {
    /// Compute a similarity/coreference score with another mention.
    ///
    /// Returns a value in [0, 1] where 1 = definitely coreferent.
    fn coref_score(&self, other: &Self) -> f32;

    /// Get the "specificity" of this mention (how specific vs. general).
    ///
    /// Higher values = more specific (e.g., "John" vs "person").
    /// Used for type hierarchy reasoning.
    fn specificity(&self) -> f32;
}

/// A geometric space that can embed entity mentions.
///
/// This trait allows different geometric representations to be used
/// interchangeably in coreference pipelines.
pub trait GeometricSpace {
    /// The type of embedding produced by this space.
    type Embedding: GeometricMention;

    /// The error type for embedding operations.
    type Error: std::error::Error;

    /// Embed a mention text into the geometric space.
    fn embed(&self, text: &str) -> Result<Self::Embedding, Self::Error>;

    /// Get the dimensionality of the embedding space.
    fn dim(&self) -> usize;

    /// Get the name of this geometric representation.
    fn name(&self) -> &'static str;
}

// ============================================================================
// Integration with Box Embeddings
// ============================================================================

use crate::backends::box_embeddings::BoxEmbedding;

impl GeometricMention for BoxEmbedding {
    fn coref_score(&self, other: &Self) -> f32 {
        self.coreference_score(other)
    }

    fn specificity(&self) -> f32 {
        // Smaller volume = more specific (tighter bounds)
        // Invert and normalize: specificity ∈ [0, 1]
        let vol = self.volume();
        if vol <= 0.0 {
            return 1.0; // Point = maximally specific
        }
        // Exponential decay: small volumes → high specificity
        (-vol.ln().max(0.0)).exp()
    }
}

impl GeometricMention for HyperbolicEmbedding {
    fn coref_score(&self, other: &Self) -> f32 {
        // In hyperbolic space, closer points = more similar
        // Convert distance to score: score = exp(-distance)
        match self.distance(other) {
            Ok(d) => (-d).exp(),
            Err(_) => 0.0,
        }
    }

    fn specificity(&self) -> f32 {
        // In Poincaré ball, higher norm = more specific (closer to boundary)
        self.norm()
    }
}

// ============================================================================
// Diagnostic Utilities
// ============================================================================

/// Compute consistency metrics for a set of coreference scores.
///
/// Returns violations of transitivity: (A~B, B~C) should imply A~C.
pub fn transitivity_violations(scores: &[Vec<f32>], threshold: f32) -> Vec<(usize, usize, usize)> {
    let n = scores.len();
    let mut violations = Vec::new();

    for a in 0..n {
        for b in (a + 1)..n {
            if scores[a][b] < threshold {
                continue;
            }
            for c in (b + 1)..n {
                if scores[b][c] < threshold {
                    continue;
                }
                // A~B and B~C, check A~C
                if scores[a][c] < threshold {
                    violations.push((a, b, c));
                }
            }
        }
    }

    violations
}

/// Compute the transitivity consistency score for a coreference matrix.
///
/// Returns a value in [0, 1] where 1 = perfectly transitive.
pub fn transitivity_consistency(scores: &[Vec<f32>], threshold: f32) -> f32 {
    let n = scores.len();
    if n < 3 {
        return 1.0;
    }

    let mut transitive_triples = 0;
    let mut violated_triples = 0;

    for a in 0..n {
        for b in (a + 1)..n {
            if scores[a][b] < threshold {
                continue;
            }
            for c in (b + 1)..n {
                if scores[b][c] < threshold {
                    continue;
                }
                transitive_triples += 1;
                if scores[a][c] < threshold {
                    violated_triples += 1;
                }
            }
        }
    }

    if transitive_triples == 0 {
        return 1.0;
    }

    1.0 - (violated_triples as f32 / transitive_triples as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transitivity_violations() {
        // Perfect transitivity: no violations
        let scores = vec![
            vec![1.0, 0.9, 0.9],
            vec![0.9, 1.0, 0.9],
            vec![0.9, 0.9, 1.0],
        ];
        let violations = transitivity_violations(&scores, 0.5);
        assert!(violations.is_empty());

        // Violation: A~B, B~C, but not A~C
        let scores = vec![
            vec![1.0, 0.9, 0.1],
            vec![0.9, 1.0, 0.9],
            vec![0.1, 0.9, 1.0],
        ];
        let violations = transitivity_violations(&scores, 0.5);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0], (0, 1, 2));
    }

    #[test]
    fn test_transitivity_consistency() {
        // Perfect
        let scores = vec![
            vec![1.0, 0.9, 0.9],
            vec![0.9, 1.0, 0.9],
            vec![0.9, 0.9, 1.0],
        ];
        assert!((transitivity_consistency(&scores, 0.5) - 1.0).abs() < 1e-5);

        // One violation out of one transitive triple
        let scores = vec![
            vec![1.0, 0.9, 0.1],
            vec![0.9, 1.0, 0.9],
            vec![0.1, 0.9, 1.0],
        ];
        assert!((transitivity_consistency(&scores, 0.5) - 0.0).abs() < 1e-5);
    }
}
