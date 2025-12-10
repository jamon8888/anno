//! Numerical edge case tests for similarity functions and thresholds.
//!
//! Tests correct handling of:
//! - Near-zero values
//! - Very large values
//! - Special floats (infinity, NaN, subnormals)
//! - Precision issues

use anno_coalesce::{
    embedding_similarity, AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient,
    Nameability,
};

// =============================================================================
// Embedding Similarity Edge Cases
// =============================================================================

#[test]
fn test_embedding_zero_vectors() {
    let zero = vec![0.0; 10];
    let unit = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    // Zero vector has no direction
    let sim = embedding_similarity(&zero, &unit);
    assert_eq!(sim, 0.0, "Zero vector similarity should be 0");

    let sim = embedding_similarity(&zero, &zero);
    assert_eq!(sim, 0.0, "Zero-zero similarity should be 0");
}

#[test]
fn test_embedding_near_zero_vectors() {
    // Very small but non-zero
    let small = vec![1e-10; 10];
    let unit = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    let sim = embedding_similarity(&small, &unit);
    // Should be valid, not NaN or infinite
    assert!(
        sim.is_finite(),
        "Near-zero embedding similarity should be finite: {}",
        sim
    );
    assert!(
        sim >= 0.0 && sim <= 1.0,
        "Similarity out of bounds: {}",
        sim
    );
}

#[test]
fn test_embedding_large_vectors() {
    // Large magnitude (tests overflow)
    let large = vec![1e30; 10];
    let unit = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    let sim = embedding_similarity(&large, &unit);
    // Should handle gracefully
    assert!(sim.is_finite() || sim == 0.0, "Large vector: {}", sim);
}

#[test]
fn test_embedding_mixed_magnitude() {
    // One very large, one normal
    let large = vec![1e10; 10];
    let small = vec![1e-10; 10];

    let sim = embedding_similarity(&large, &small);
    assert!(
        sim.is_finite(),
        "Mixed magnitude should be finite: {}",
        sim
    );
}

#[test]
fn test_embedding_negative_values() {
    let pos = vec![1.0, 0.0, 0.0];
    let neg = vec![-1.0, 0.0, 0.0];

    let sim = embedding_similarity(&pos, &neg);
    // Opposite vectors should have low similarity (0.0 after normalization)
    assert!(
        (sim - 0.0).abs() < 0.001,
        "Opposite vectors: {}",
        sim
    );
}

#[test]
fn test_embedding_single_element() {
    let a = vec![1.0];
    let b = vec![1.0];

    let sim = embedding_similarity(&a, &b);
    assert!((sim - 1.0).abs() < 0.001, "Single element: {}", sim);
}

#[test]
fn test_embedding_empty() {
    let sim = embedding_similarity(&[], &[]);
    assert_eq!(sim, 0.0, "Empty vectors should return 0");
}

#[test]
fn test_embedding_mismatched_dimensions() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![1.0, 2.0];

    let sim = embedding_similarity(&a, &b);
    assert_eq!(sim, 0.0, "Mismatched dimensions should return 0");
}

// =============================================================================
// Alignment Score Numerical Tests
// =============================================================================

#[test]
fn test_alignment_many_identical_matches() {
    // Previously caused variance to go slightly negative due to float precision
    let mut alignment = AlignmentScore::new();

    for _ in 0..1000 {
        alignment.record_match(0.85);
    }

    let var = alignment.variance();
    assert!(!var.is_nan(), "Variance should not be NaN");
    assert!(var >= 0.0, "Variance should be non-negative: {}", var);
    assert!(var < 0.001, "Variance of identical values should be ~0: {}", var);

    let conf = alignment.confidence();
    assert!(!conf.is_nan(), "Confidence should not be NaN");
    assert!(conf >= 0.0 && conf <= 1.0, "Confidence bounds: {}", conf);
}

#[test]
fn test_alignment_extreme_values() {
    let mut alignment = AlignmentScore::new();

    // Mix of extreme values
    alignment.record_match(0.0);
    alignment.record_match(1.0);
    alignment.record_match(0.0);
    alignment.record_match(1.0);

    let var = alignment.variance();
    assert!(!var.is_nan(), "Variance NaN for extreme values");
    assert!(var > 0.2, "High variance expected: {}", var);

    let conf = alignment.confidence();
    assert!(!conf.is_nan(), "Confidence NaN");
    // Lower confidence due to high variance
    assert!(conf >= 0.0 && conf <= 1.0, "Confidence bounds: {}", conf);
}

#[test]
fn test_alignment_near_boundary_values() {
    let mut alignment = AlignmentScore::new();

    // Values very close to but not exactly 0 and 1
    alignment.record_match(1e-10);
    alignment.record_match(1.0 - 1e-10);

    let mean = alignment.mean();
    assert!(mean.is_finite(), "Mean should be finite: {}", mean);

    let var = alignment.variance();
    assert!(var.is_finite(), "Variance should be finite: {}", var);

    let conf = alignment.confidence();
    assert!(conf.is_finite(), "Confidence should be finite: {}", conf);
}

// =============================================================================
// Gradient Numerical Tests
// =============================================================================

#[test]
fn test_gradient_near_zero_similarity() {
    let gradients = [
        GeneralizationGradient::linear(),
        GeneralizationGradient::quadratic(),
        GeneralizationGradient::exponential(2.0),
    ];

    for gradient in &gradients {
        let adj = gradient.threshold_adjustment(1e-10, 1.0, 0.2);
        assert!(
            adj.is_finite(),
            "{:?} adjustment at near-zero similarity should be finite: {}",
            gradient,
            adj
        );
        assert!(
            adj.abs() <= 0.2 + 0.001,
            "Adjustment magnitude too large: {}",
            adj
        );
    }
}

#[test]
fn test_gradient_near_one_similarity() {
    let gradients = [
        GeneralizationGradient::linear(),
        GeneralizationGradient::quadratic(),
        GeneralizationGradient::exponential(2.0),
    ];

    for gradient in &gradients {
        let adj = gradient.threshold_adjustment(1.0 - 1e-10, 1.0, 0.2);
        assert!(
            adj.is_finite(),
            "{:?} adjustment at near-1 similarity: {}",
            gradient,
            adj
        );
    }
}

#[test]
fn test_gradient_extreme_decay_rate() {
    // Very high decay rate
    let fast = GeneralizationGradient::exponential(100.0);
    let adj = fast.threshold_adjustment(0.5, 1.0, 0.2);
    assert!(adj.is_finite(), "High decay rate: {}", adj);

    // Very low decay rate
    let slow = GeneralizationGradient::exponential(0.001);
    let adj = slow.threshold_adjustment(0.5, 1.0, 0.2);
    assert!(adj.is_finite(), "Low decay rate: {}", adj);
}

#[test]
fn test_gradient_zero_max_adjustment() {
    let gradient = GeneralizationGradient::quadratic();
    let adj = gradient.threshold_adjustment(0.5, 1.0, 0.0);
    assert_eq!(adj, 0.0, "Zero max_adjustment should give zero");
}

#[test]
fn test_gradient_zero_confidence() {
    let gradient = GeneralizationGradient::quadratic();
    let adj = gradient.threshold_adjustment(0.9, 0.0, 0.2);
    assert!(
        adj.abs() < 0.001,
        "Zero confidence should give ~0 adjustment: {}",
        adj
    );
}

// =============================================================================
// Config Threshold Numerical Tests
// =============================================================================

#[test]
fn test_config_threshold_edge_values() {
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.99999,
        min_threshold: 0.00001,
        max_adjustment: 0.5,
        gradient: GeneralizationGradient::quadratic(),
        use_nameability: true,
    };

    let alignment = AlignmentScore::new();
    let nameability = Nameability::new(0.5);

    let thresh = config.compute_threshold(&alignment, 0.5, Some(nameability));
    assert!(thresh.is_finite(), "Threshold finite: {}", thresh);
    assert!(
        thresh >= config.min_threshold,
        "Threshold below min: {}",
        thresh
    );
    assert!(thresh <= 1.0, "Threshold above 1: {}", thresh);
}

#[test]
fn test_config_extreme_base_thresholds() {
    // Very low base
    let low_config = AdaptiveResolutionConfig {
        base_threshold: 0.01,
        min_threshold: 0.001,
        max_adjustment: 0.5,
        gradient: GeneralizationGradient::linear(),
        use_nameability: false,
    };

    let alignment = AlignmentScore::new();
    let thresh = low_config.compute_threshold(&alignment, 0.5, None);
    assert!(thresh >= low_config.min_threshold);

    // Very high base
    let high_config = AdaptiveResolutionConfig {
        base_threshold: 0.99,
        min_threshold: 0.1,
        max_adjustment: 0.1,
        gradient: GeneralizationGradient::linear(),
        use_nameability: false,
    };

    let thresh = high_config.compute_threshold(&alignment, 0.5, None);
    assert!(thresh <= 1.0);
}

// =============================================================================
// Nameability Numerical Tests
// =============================================================================

#[test]
fn test_nameability_boundary_scores() {
    // Exactly at boundaries
    let at_low = Nameability::new(0.4);
    let below_low = Nameability::new(0.39999);
    let at_high = Nameability::new(0.7);
    let below_high = Nameability::new(0.69999);

    // Should not panic
    let _ = at_low.level();
    let _ = below_low.level();
    let _ = at_high.level();
    let _ = below_high.level();
}

#[test]
fn test_nameability_extreme_snd() {
    // SND at extremes
    let low_snd = Nameability::from_snd(0.0);
    assert!((low_snd.score() - 1.0).abs() < 0.001);

    let high_snd = Nameability::from_snd(1.0);
    assert!((high_snd.score() - 0.0).abs() < 0.001);
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Embedding similarity should always be finite for finite inputs
        #[test]
        fn embedding_sim_finite(
            dim in 1usize..100,
            seed in any::<u64>()
        ) {
            let mut rng = seed;
            let a: Vec<f32> = (0..dim).map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                // Range [-1e6, 1e6]
                ((rng % 2_000_000) as f32 - 1_000_000.0) / 1000.0
            }).collect();
            let b: Vec<f32> = (0..dim).map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                ((rng % 2_000_000) as f32 - 1_000_000.0) / 1000.0
            }).collect();

            let sim = embedding_similarity(&a, &b);
            prop_assert!(sim.is_finite() || sim == 0.0,
                "Similarity not finite: {} for dim={}", sim, dim);
        }

        /// Alignment confidence should never be NaN
        #[test]
        fn alignment_confidence_not_nan(
            values in prop::collection::vec(0.0f32..1.0f32, 1..100)
        ) {
            let mut alignment = AlignmentScore::new();
            for v in values {
                alignment.record_match(v);
            }

            let conf = alignment.confidence();
            prop_assert!(!conf.is_nan(), "Confidence is NaN");
            prop_assert!(conf >= 0.0 && conf <= 1.0,
                "Confidence out of bounds: {}", conf);
        }

        /// Variance should never be negative
        #[test]
        fn alignment_variance_non_negative(
            values in prop::collection::vec(0.0f32..1.0f32, 1..100)
        ) {
            let mut alignment = AlignmentScore::new();
            for v in values {
                alignment.record_match(v);
            }

            let var = alignment.variance();
            prop_assert!(!var.is_nan(), "Variance is NaN");
            prop_assert!(var >= 0.0, "Variance is negative: {}", var);
        }

        /// Gradient adjustments should be bounded
        #[test]
        fn gradient_adjustment_bounded(
            sim in 0.0f32..1.0f32,
            conf in 0.0f32..1.0f32,
            max_adj in 0.0f32..1.0f32
        ) {
            let gradients = [
                GeneralizationGradient::none(),
                GeneralizationGradient::linear(),
                GeneralizationGradient::quadratic(),
                GeneralizationGradient::exponential(2.0),
            ];

            for gradient in &gradients {
                let adj = gradient.threshold_adjustment(sim, conf, max_adj);
                prop_assert!(adj.is_finite(),
                    "{:?} adjustment not finite: {}", gradient, adj);
                prop_assert!(adj.abs() <= max_adj + 0.001,
                    "{:?} adjustment {} exceeds max {}", gradient, adj.abs(), max_adj);
            }
        }

        /// Config threshold should always be in valid range
        #[test]
        fn config_threshold_valid_range(
            base in 0.1f32..0.9f32,
            min in 0.05f32..0.3f32,
            max_adj in 0.1f32..0.5f32,
            sim in 0.0f32..1.0f32,
            name_score in 0.0f32..1.0f32
        ) {
            let config = AdaptiveResolutionConfig {
                base_threshold: base,
                min_threshold: min,
                max_adjustment: max_adj,
                gradient: GeneralizationGradient::quadratic(),
                use_nameability: true,
            };

            let alignment = AlignmentScore::new();
            let nameability = Nameability::new(name_score);

            let thresh = config.compute_threshold(&alignment, sim, Some(nameability));
            prop_assert!(thresh.is_finite(), "Threshold not finite: {}", thresh);
            prop_assert!(thresh >= min, "Threshold {} below min {}", thresh, min);
            prop_assert!(thresh <= 1.0, "Threshold {} above 1.0", thresh);
        }
    }
}
