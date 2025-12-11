//! Topological data analysis for structural diagnostics.
//!
//! Persistent homology provides coordinate-free analysis of data structure,
//! identifying features that persist across scales. For NLP:
//!
//! - **Attention topology**: Detect structurally important connections in attention maps
//! - **Embedding structure**: Analyze the shape of embedding spaces
//! - **Long-distance dependencies**: Identify persistent coreference patterns
//!
//! # Mathematical Background
//!
//! **Persistent homology** tracks the birth and death of topological features
//! (connected components, loops, voids) as we vary a threshold parameter.
//!
//! For a distance matrix D between n points:
//!
//! 1. Start with n disconnected points (each is a 0-dimensional feature)
//! 2. Gradually lower the connection threshold
//! 3. When points connect, components merge (some "die")
//! 4. Loops may form (1-dimensional features "born")
//! 5. Track (birth, death) pairs = **persistence diagram**
//!
//! Features with large persistence (death - birth) are structurally important;
//! features with small persistence are likely noise.
//!
//! # Application to Coreference
//!
//! Given a matrix of pairwise coreference scores:
//!
//! ```text
//! Score Matrix:
//!       A     B     C     D
//! A   1.0   0.9   0.2   0.1
//! B   0.9   1.0   0.8   0.1
//! C   0.2   0.8   1.0   0.7
//! D   0.1   0.1   0.7   1.0
//! ```
//!
//! Persistent homology identifies:
//! - **High persistence 0-dim**: Strong clusters (A-B and C-D are clearly separate)
//! - **Low persistence 0-dim**: Weak connections (B-C is borderline)
//!
//! This can guide threshold selection and identify ambiguous links.
//!
//! # Integration Status: STUB
//!
//! This module provides trait definitions and placeholder implementations.
//! Full implementation requires:
//!
//! - Integration with TDA library (giotto-tda, ripser, or Rust equivalent)
//! - Distance matrix computation from coreference scores
//! - Persistence diagram analysis utilities
//!
//! # References
//!
//! - arXiv:2411.10298: "Unveiling Topological Structures in Text" (TDA4NLP survey)
//! - arXiv:2206.15195: "Topological BERT"
//! - EMNLP 2021: "Artificial text detection via examining the topology of attention maps"
//! - Awesome TDA4NLP: <https://github.com/AdaUchendu/AwesomeTDA4NLP>
//!
//! # Python Libraries for Prototyping
//!
//! Until native Rust TDA is available:
//!
//! ```bash
//! pip install giotto-tda ripser gudhi
//! ```
//!
//! Example Python script for coreference analysis:
//!
//! ```python,ignore
//! from ripser import ripser
//! import numpy as np
//!
//! # Convert scores to distances: d = 1 - score
//! distance_matrix = 1.0 - score_matrix
//!
//! # Compute persistent homology
//! result = ripser(distance_matrix, distance_matrix=True, maxdim=1)
//!
//! # result['dgms'][0] = 0-dimensional persistence (clusters)
//! # result['dgms'][1] = 1-dimensional persistence (loops)
//! ```

use serde::{Deserialize, Serialize};

// ============================================================================
// Core Types
// ============================================================================

/// A persistence pair representing a topological feature.
///
/// Features are born at one threshold and die at another.
/// Persistence = death - birth indicates importance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PersistencePair {
    /// Threshold at which the feature appears.
    pub birth: f32,

    /// Threshold at which the feature disappears.
    /// May be f32::INFINITY for features that never die.
    pub death: f32,

    /// Homological dimension (0 = components, 1 = loops, 2 = voids).
    pub dimension: u8,
}

impl PersistencePair {
    /// Create a new persistence pair.
    pub fn new(birth: f32, death: f32, dimension: u8) -> Self {
        Self {
            birth,
            death,
            dimension,
        }
    }

    /// Compute the persistence (lifespan) of the feature.
    ///
    /// Higher persistence = more important feature.
    #[must_use]
    pub fn persistence(&self) -> f32 {
        if self.death.is_infinite() {
            f32::INFINITY
        } else {
            self.death - self.birth
        }
    }

    /// Check if this is an "essential" feature (never dies).
    #[must_use]
    pub fn is_essential(&self) -> bool {
        self.death.is_infinite()
    }

    /// Get the midpoint of the feature's lifespan.
    #[must_use]
    pub fn midpoint(&self) -> f32 {
        if self.death.is_infinite() {
            self.birth
        } else {
            (self.birth + self.death) / 2.0
        }
    }
}

/// A persistence diagram containing all features at all dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistenceDiagram {
    /// All persistence pairs, sorted by dimension then by persistence.
    pairs: Vec<PersistencePair>,
}

impl PersistenceDiagram {
    /// Create an empty persistence diagram.
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Add a persistence pair.
    pub fn add(&mut self, pair: PersistencePair) {
        self.pairs.push(pair);
    }

    /// Get all pairs.
    pub fn pairs(&self) -> &[PersistencePair] {
        &self.pairs
    }

    /// Get pairs at a specific dimension.
    pub fn dimension(&self, dim: u8) -> Vec<&PersistencePair> {
        self.pairs.iter().filter(|p| p.dimension == dim).collect()
    }

    /// Get 0-dimensional features (connected components).
    pub fn components(&self) -> Vec<&PersistencePair> {
        self.dimension(0)
    }

    /// Get 1-dimensional features (loops/cycles).
    pub fn loops(&self) -> Vec<&PersistencePair> {
        self.dimension(1)
    }

    /// Compute the total persistence (sum of all lifespans).
    ///
    /// Excludes infinite persistence features.
    #[must_use]
    pub fn total_persistence(&self) -> f32 {
        self.pairs
            .iter()
            .filter(|p| !p.is_essential())
            .map(|p| p.persistence())
            .sum()
    }

    /// Get the n most persistent features.
    pub fn top_persistent(&self, n: usize) -> Vec<&PersistencePair> {
        let mut sorted: Vec<_> = self.pairs.iter().collect();
        sorted.sort_by(|a, b| {
            b.persistence()
                .partial_cmp(&a.persistence())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.into_iter().take(n).collect()
    }

    /// Filter pairs by persistence threshold.
    ///
    /// Returns only features with persistence >= threshold.
    pub fn filter_by_persistence(&self, threshold: f32) -> Vec<&PersistencePair> {
        self.pairs
            .iter()
            .filter(|p| p.persistence() >= threshold)
            .collect()
    }

    /// Compute Betti numbers at a given threshold.
    ///
    /// Betti_k = number of k-dimensional features alive at threshold.
    pub fn betti_numbers(&self, threshold: f32) -> Vec<usize> {
        let max_dim = self.pairs.iter().map(|p| p.dimension).max().unwrap_or(0) as usize;
        let mut betti = vec![0; max_dim + 1];

        for pair in &self.pairs {
            if pair.birth <= threshold && (pair.death > threshold || pair.death.is_infinite()) {
                betti[pair.dimension as usize] += 1;
            }
        }

        betti
    }
}

// ============================================================================
// Distance Matrix Utilities
// ============================================================================

/// Configuration for TDA analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdaConfig {
    /// Maximum homological dimension to compute (0, 1, or 2).
    pub max_dimension: u8,

    /// Minimum persistence to report (filters noise).
    pub min_persistence: f32,

    /// Whether to compute representative cycles (slower but more informative).
    pub compute_representatives: bool,
}

impl Default for TdaConfig {
    fn default() -> Self {
        Self {
            max_dimension: 1,
            min_persistence: 0.01,
            compute_representatives: false,
        }
    }
}

/// Convert a similarity matrix to a distance matrix.
///
/// distance = 1.0 - similarity (clamped to [0, 1])
pub fn similarity_to_distance(similarity: &[Vec<f32>]) -> Vec<Vec<f32>> {
    similarity
        .iter()
        .map(|row| row.iter().map(|&s| (1.0 - s).clamp(0.0, 1.0)).collect())
        .collect()
}

/// Compute persistent homology from a distance matrix.
///
/// # Status: STUB
///
/// Full implementation requires integration with a TDA library.
/// Returns a placeholder diagram.
///
/// # Future Implementation
///
/// Options for Rust TDA:
/// 1. FFI to ripser (C++)
/// 2. Pure Rust implementation (complex)
/// 3. Subprocess call to Python giotto-tda (prototyping)
pub fn compute_persistence(
    _distance_matrix: &[Vec<f32>],
    _config: &TdaConfig,
) -> PersistenceDiagram {
    // TODO: Implement via FFI or pure Rust
    // For now, return empty diagram
    eprintln!("TDA compute_persistence is a stub - see docs/GEOMETRIC_FOUNDATIONS.md");
    PersistenceDiagram::new()
}

// ============================================================================
// Coreference Analysis
// ============================================================================

/// Analyze coreference scores using persistent homology.
///
/// # Status: STUB
///
/// This will identify:
/// - Strong clusters (high persistence 0-dim features)
/// - Ambiguous links (low persistence 0-dim features)
/// - Potential transitivity violations (1-dim features = loops in similarity)
pub struct CorefTopologyAnalysis {
    /// The persistence diagram.
    pub diagram: PersistenceDiagram,

    /// Suggested clustering threshold based on largest persistence gap.
    pub suggested_threshold: Option<f32>,

    /// Indices of ambiguous edges (low persistence).
    pub ambiguous_edges: Vec<(usize, usize)>,
}

impl CorefTopologyAnalysis {
    /// Analyze a coreference score matrix.
    ///
    /// # Status: STUB
    pub fn from_scores(_scores: &[Vec<f32>], _config: &TdaConfig) -> Self {
        // TODO: Implement full analysis
        Self {
            diagram: PersistenceDiagram::new(),
            suggested_threshold: None,
            ambiguous_edges: Vec::new(),
        }
    }

    /// Find the optimal threshold using the persistence gap heuristic.
    ///
    /// The largest gap in the persistence diagram often corresponds
    /// to the natural clustering threshold.
    pub fn find_threshold_by_gap(diagram: &PersistenceDiagram) -> Option<f32> {
        let components = diagram.components();
        if components.len() < 2 {
            return None;
        }

        // Sort by death time (when components merge)
        let mut deaths: Vec<f32> = components
            .iter()
            .filter(|p| !p.is_essential())
            .map(|p| p.death)
            .collect();
        deaths.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Find largest gap
        let mut max_gap = 0.0;
        let mut threshold = None;

        for window in deaths.windows(2) {
            let gap = window[1] - window[0];
            if gap > max_gap {
                max_gap = gap;
                threshold = Some((window[0] + window[1]) / 2.0);
            }
        }

        threshold
    }
}

// ============================================================================
// Attention Topology (for future LLM integration)
// ============================================================================

/// Analyze the topology of attention maps.
///
/// Based on: "Artificial text detection via examining the topology of attention maps"
///
/// # Status: STUB
///
/// This is relevant for:
/// - Detecting hallucinated coreference links
/// - Analyzing attention patterns in transformer-based NER
pub struct AttentionTopology {
    /// Persistence diagrams for each attention head.
    pub head_diagrams: Vec<PersistenceDiagram>,

    /// Average Betti numbers across heads.
    pub avg_betti: Vec<f32>,
}

impl AttentionTopology {
    /// Analyze attention matrices from a transformer model.
    ///
    /// # Status: STUB
    ///
    /// Full implementation requires:
    /// - Attention matrix extraction from model
    /// - Per-head TDA computation
    /// - Aggregation statistics
    pub fn from_attention(_attention_matrices: &[Vec<Vec<f32>>], _config: &TdaConfig) -> Self {
        Self {
            head_diagrams: Vec::new(),
            avg_betti: Vec::new(),
        }
    }
}

// ============================================================================
// Integration with Anno
// ============================================================================

/// Trait for types that can be analyzed with TDA.
pub trait TopologicallyAnalyzable {
    /// Convert to a distance matrix suitable for TDA.
    fn to_distance_matrix(&self) -> Vec<Vec<f32>>;

    /// Compute persistent homology.
    fn compute_persistence(&self, config: &TdaConfig) -> PersistenceDiagram {
        let dm = self.to_distance_matrix();
        compute_persistence(&dm, config)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistence_pair() {
        let pair = PersistencePair::new(0.1, 0.5, 0);
        assert!((pair.persistence() - 0.4).abs() < 1e-5);
        assert!(!pair.is_essential());
        assert!((pair.midpoint() - 0.3).abs() < 1e-5);
    }

    #[test]
    fn test_essential_feature() {
        let pair = PersistencePair::new(0.0, f32::INFINITY, 0);
        assert!(pair.is_essential());
        assert!(pair.persistence().is_infinite());
    }

    #[test]
    fn test_persistence_diagram() {
        let mut diagram = PersistenceDiagram::new();
        diagram.add(PersistencePair::new(0.0, 0.5, 0));
        diagram.add(PersistencePair::new(0.1, 0.3, 0));
        diagram.add(PersistencePair::new(0.2, 0.8, 1));

        assert_eq!(diagram.components().len(), 2);
        assert_eq!(diagram.loops().len(), 1);
    }

    #[test]
    fn test_betti_numbers() {
        let mut diagram = PersistenceDiagram::new();
        // Two components that merge at different times
        diagram.add(PersistencePair::new(0.0, 0.3, 0));
        diagram.add(PersistencePair::new(0.0, 0.6, 0));
        diagram.add(PersistencePair::new(0.0, f32::INFINITY, 0)); // Essential

        // At threshold 0.2: all 3 alive
        let betti = diagram.betti_numbers(0.2);
        assert_eq!(betti[0], 3);

        // At threshold 0.4: first died, 2 alive
        let betti = diagram.betti_numbers(0.4);
        assert_eq!(betti[0], 2);

        // At threshold 0.7: only essential alive
        let betti = diagram.betti_numbers(0.7);
        assert_eq!(betti[0], 1);
    }

    #[test]
    fn test_similarity_to_distance() {
        let sim = vec![vec![1.0, 0.8], vec![0.8, 1.0]];
        let dist = similarity_to_distance(&sim);

        assert!((dist[0][0] - 0.0).abs() < 1e-5);
        assert!((dist[0][1] - 0.2).abs() < 1e-5);
    }

    #[test]
    fn test_top_persistent() {
        let mut diagram = PersistenceDiagram::new();
        diagram.add(PersistencePair::new(0.0, 0.1, 0)); // persistence 0.1
        diagram.add(PersistencePair::new(0.0, 0.5, 0)); // persistence 0.5
        diagram.add(PersistencePair::new(0.1, 0.4, 0)); // persistence 0.3

        let top = diagram.top_persistent(2);
        assert_eq!(top.len(), 2);
        assert!((top[0].persistence() - 0.5).abs() < 1e-5);
        assert!((top[1].persistence() - 0.3).abs() < 1e-5);
    }
}
