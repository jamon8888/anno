//! Numerical stability and edge case tests.
//!
//! Tests ensure that similarity calculations, alignment scores, and
//! other numerical operations handle edge cases correctly.

use anno::coalesce as anno_coalesce;

use anno_coalesce::alignment::{
    AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient, Nameability,
};
use anno_coalesce::resolver::embedding_similarity;
use anno_coalesce::similarity::{jaro_winkler_similarity, levenshtein_distance, Similarity};

// =============================================================================
// Embedding Similarity Edge Cases
// =============================================================================

#[test]
fn test_embedding_zero_vectors() {
    let zero = vec![0.0, 0.0, 0.0];
    let normal = vec![1.0, 0.0, 0.0];

    // Zero vector should return 0 similarity
    let sim = embedding_similarity(&zero, &normal);
    assert_eq!(sim, 0.0);

    // Both zero should return 0 (can't compute cosine)
    let sim = embedding_similarity(&zero, &zero);
    assert_eq!(sim, 0.0);
}

#[test]
fn test_embedding_near_zero_vectors() {
    let near_zero = vec![1e-10, 1e-10, 1e-10];
    let normal = vec![1.0, 0.0, 0.0];

    // Should not panic or produce NaN
    let sim = embedding_similarity(&near_zero, &normal);
    assert!(sim.is_finite());
}

#[test]
fn test_embedding_large_vectors() {
    let large = vec![1e10, 1e10, 1e10];
    let normal = vec![1.0, 1.0, 1.0];

    // Cosine similarity should handle magnitude differences
    let sim = embedding_similarity(&large, &normal);
    assert!(sim.is_finite());
    assert!(sim > 0.9); // Same direction
}

#[test]
fn test_embedding_mixed_magnitude() {
    let small = vec![0.001, 0.001, 0.001];
    let large = vec![1000.0, 1000.0, 1000.0];

    // Same direction, different magnitudes
    let sim = embedding_similarity(&small, &large);
    assert!(sim.is_finite());
    assert!(sim > 0.9);
}

#[test]
fn test_embedding_negative_values() {
    let neg = vec![-1.0, -1.0, 0.0];
    let pos = vec![1.0, 1.0, 0.0];

    // Opposite directions
    let sim = embedding_similarity(&neg, &pos);
    assert!(sim.is_finite());
    assert!(sim < 0.5);
}

#[test]
fn test_embedding_single_element() {
    let a = vec![1.0];
    let b = vec![1.0];

    assert_eq!(embedding_similarity(&a, &b), 1.0);
}

#[test]
fn test_embedding_empty() {
    let empty: Vec<f32> = Vec::new();
    let normal = vec![1.0, 0.0];

    assert_eq!(embedding_similarity(&empty, &normal), 0.0);
    assert_eq!(embedding_similarity(&empty, &empty), 0.0);
}

#[test]
fn test_embedding_mismatched_dimensions() {
    let a = vec![1.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];

    // Should handle gracefully (return 0)
    assert_eq!(embedding_similarity(&a, &b), 0.0);
}

// =============================================================================
// Alignment Score Edge Cases
// =============================================================================

#[test]
fn test_alignment_many_identical_matches() {
    let mut alignment = AlignmentScore::new();

    // Many identical matches
    for _ in 0..1000 {
        alignment.record_match(0.85);
    }

    let conf = alignment.confidence();
    assert!(conf.is_finite());
    assert!(conf > 0.0 && (0.0..=1.0).contains(&conf));

    // Variance should be near zero for identical values
    assert!(alignment.variance() < 0.001);
}

#[test]
fn test_alignment_extreme_values() {
    let mut alignment = AlignmentScore::new();

    alignment.record_match(0.0);
    alignment.record_match(1.0);

    let mean = alignment.mean();
    assert!((mean - 0.5).abs() < 0.001);

    // Should not overflow or produce NaN
    assert!(alignment.variance().is_finite());
    assert!(alignment.confidence().is_finite());
}

#[test]
fn test_alignment_near_boundary_values() {
    let mut alignment = AlignmentScore::new();

    alignment.record_match(0.0001);
    alignment.record_match(0.9999);

    assert!(alignment.mean().is_finite());
    assert!(alignment.variance().is_finite());
}

// =============================================================================
// Generalization Gradient Edge Cases
// =============================================================================

#[test]
fn test_gradient_near_zero_similarity() {
    let gradient = GeneralizationGradient::quadratic();

    // Very low similarity should give minimal adjustment
    let adj = gradient.threshold_adjustment(0.001, 1.0, 0.2);
    assert!(adj.is_finite());
    assert!(adj.abs() < 0.01);
}

#[test]
fn test_gradient_near_one_similarity() {
    let gradient = GeneralizationGradient::quadratic();

    // Very high similarity should give maximum adjustment
    let adj = gradient.threshold_adjustment(0.999, 1.0, 0.2);
    assert!(adj.is_finite());
    assert!(adj < -0.15); // Should be near max
}

#[test]
fn test_gradient_extreme_decay_rate() {
    // Very high decay rate
    let gradient = GeneralizationGradient::exponential(100.0);
    let adj = gradient.threshold_adjustment(0.5, 1.0, 0.2);
    assert!(adj.is_finite());

    // Very low decay rate
    let gradient = GeneralizationGradient::exponential(0.01);
    let adj = gradient.threshold_adjustment(0.5, 1.0, 0.2);
    assert!(adj.is_finite());
}

#[test]
fn test_gradient_zero_max_adjustment() {
    let gradient = GeneralizationGradient::quadratic();

    // Zero max adjustment should give zero
    let adj = gradient.threshold_adjustment(0.9, 1.0, 0.0);
    assert_eq!(adj, 0.0);
}

#[test]
fn test_gradient_zero_confidence() {
    let gradient = GeneralizationGradient::quadratic();

    // Zero confidence should give zero adjustment
    let adj = gradient.threshold_adjustment(0.9, 0.0, 0.2);
    assert_eq!(adj, 0.0);
}

// =============================================================================
// Adaptive Config Edge Cases
// =============================================================================

#[test]
fn test_config_threshold_edge_values() {
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.5,
        min_threshold: 0.0,  // Edge: can go to zero
        max_adjustment: 1.0, // Edge: large adjustment
        ..Default::default()
    };

    let alignment = AlignmentScore::new();
    let threshold = config.compute_threshold(&alignment, 0.9, None);

    assert!(threshold.is_finite());
    assert!(threshold >= 0.0);
}

#[test]
fn test_config_extreme_base_thresholds() {
    // Very low base
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.01,
        min_threshold: 0.0,
        ..Default::default()
    };

    let alignment = AlignmentScore::new();
    let threshold = config.compute_threshold(&alignment, 0.5, None);
    assert!(threshold.is_finite());

    // Very high base
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.99,
        min_threshold: 0.5,
        ..Default::default()
    };

    let threshold = config.compute_threshold(&alignment, 0.5, None);
    assert!(threshold.is_finite());
}

#[test]
fn test_nameability_boundary_scores() {
    // Exact boundaries
    let high = Nameability::new(0.7);
    assert!(high.is_high());

    let low = Nameability::new(0.4);
    assert!(!low.is_high());
    assert!(!low.is_low());

    let very_low = Nameability::new(0.39);
    assert!(very_low.is_low());
}

#[test]
fn test_nameability_extreme_snd() {
    // SND can be out of range, should be clamped
    let from_negative = Nameability::from_snd(-0.5);
    assert!(from_negative.score() <= 1.0);

    let from_over = Nameability::from_snd(1.5);
    assert!(from_over.score() >= 0.0);
}

// =============================================================================
// String Similarity Edge Cases
// =============================================================================

#[test]
fn test_similarity_single_char() {
    let sim = Similarity::new();

    assert_eq!(sim.compute("a", "a"), 1.0);
    assert!(sim.compute("a", "b").is_finite());
}

#[test]
fn test_similarity_very_long_strings() {
    let sim = Similarity::new();

    let long = "a".repeat(10000);
    let score = sim.compute(&long, &long);
    assert_eq!(score, 1.0);
}

#[test]
fn test_levenshtein_single_chars() {
    assert_eq!(levenshtein_distance("a", "a"), 0);
    assert_eq!(levenshtein_distance("a", "b"), 1);
}

#[test]
fn test_jaro_winkler_empty() {
    // Two empty strings are identical, so similarity = 1.0
    assert_eq!(jaro_winkler_similarity("", ""), 1.0);
    // One empty, one non-empty = 0.0
    assert_eq!(jaro_winkler_similarity("test", ""), 0.0);
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Embedding similarity produces finite values
        #[test]
        fn embedding_sim_finite(dim in 1usize..20, seed in any::<u64>()) {
            let mut rng = seed;
            let emb1: Vec<f32> = (0..dim).map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                (rng % 2000) as f32 / 1000.0 - 1.0
            }).collect();
            let emb2: Vec<f32> = (0..dim).map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                (rng % 2000) as f32 / 1000.0 - 1.0
            }).collect();

            let sim = embedding_similarity(&emb1, &emb2);
            prop_assert!(sim.is_finite(), "Embedding sim not finite: {}", sim);
        }

        /// Alignment confidence never produces NaN
        #[test]
        fn alignment_confidence_not_nan(
            scores in prop::collection::vec(0.0f32..1.0f32, 0..50)
        ) {
            let mut alignment = AlignmentScore::new();
            for score in scores {
                alignment.record_match(score);
            }

            let conf = alignment.confidence();
            prop_assert!(!conf.is_nan(), "Confidence is NaN");
            prop_assert!((0.0..=1.0).contains(&conf));
        }

        /// Alignment variance is always non-negative
        #[test]
        fn alignment_variance_non_negative(
            scores in prop::collection::vec(0.0f32..1.0f32, 0..50)
        ) {
            let mut alignment = AlignmentScore::new();
            for score in scores {
                alignment.record_match(score);
            }

            let var = alignment.variance();
            prop_assert!(var >= 0.0, "Variance is negative: {}", var);
        }

        /// Gradient adjustment is always bounded
        #[test]
        fn gradient_adjustment_bounded(
            similarity in 0.0f32..1.0f32,
            confidence in 0.0f32..1.0f32,
            max_adj in 0.0f32..1.0f32
        ) {
            let gradient = GeneralizationGradient::quadratic();
            let adj = gradient.threshold_adjustment(similarity, confidence, max_adj);

            prop_assert!(adj.is_finite());
            prop_assert!(adj <= 0.0); // Always non-positive
            prop_assert!(adj.abs() <= max_adj + 0.001);
        }

        /// Config threshold produces valid values
        #[test]
        fn config_threshold_valid_range(
            base in 0.3f32..0.9f32,
            min in 0.1f32..0.4f32,
            sim in 0.0f32..1.0f32
        ) {
            let config = AdaptiveResolutionConfig {
                base_threshold: base,
                min_threshold: min,
                ..Default::default()
            };

            let alignment = AlignmentScore::new();
            let threshold = config.compute_threshold(&alignment, sim, None);

            prop_assert!(threshold.is_finite());
            prop_assert!(threshold >= min);
            prop_assert!((0.0..=1.0).contains(&threshold));
        }
    }
}
