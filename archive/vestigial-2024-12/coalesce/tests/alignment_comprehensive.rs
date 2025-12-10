//! Comprehensive tests for adaptive alignment module.
//!
//! Tests covering:
//! - Unicode/multilingual entity names
//! - Edge cases and boundary conditions
//! - Property-based invariants
//! - Regression scenarios

use anno_coalesce::{
    entity_type_nameability, AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient,
    Nameability, NameabilityLevel,
};

// =============================================================================
// Unicode and Multilingual Tests
// =============================================================================

/// Test nameability handles Unicode entity types
#[test]
fn test_unicode_entity_type_lookup() {
    // Standard ASCII
    assert!(entity_type_nameability("PERSON").is_high());

    // Mixed case
    assert!(entity_type_nameability("Person").is_high());
    assert!(entity_type_nameability("pErSoN").is_high());

    // Unknown types default to medium
    let unknown_cn = entity_type_nameability("人物"); // Chinese for "person"
    assert!(!unknown_cn.is_high());
    assert!(!unknown_cn.is_low());

    let unknown_ar = entity_type_nameability("شخص"); // Arabic for "person"
    assert_eq!(unknown_ar.level(), NameabilityLevel::Medium);
}

/// Test alignment scores with Unicode similarity values
/// (Similarity values are floats, so this tests float handling)
#[test]
fn test_alignment_unicode_text_tracking() {
    let mut alignment = AlignmentScore::new();

    // Simulate matches for multilingual entities
    // These similarity scores could come from comparing:
    // "東京" vs "Tokyo" (0.0 string similarity, but maybe 0.9 embedding similarity)
    alignment.record_match(0.92);
    // "Москва" vs "Moscow" (similar case)
    alignment.record_match(0.88);
    // "الرياض" vs "Riyadh"
    alignment.record_match(0.85);

    assert_eq!(alignment.count(), 3);
    assert!(alignment.mean() > 0.85);
    assert!(alignment.confidence() > 0.0);
}

// =============================================================================
// Boundary and Edge Case Tests
// =============================================================================

#[test]
fn test_nameability_exact_boundaries() {
    // Test exact boundary values for level classification
    let at_07 = Nameability::new(0.7);
    assert_eq!(at_07.level(), NameabilityLevel::High);
    assert!(at_07.is_high());

    let just_below_07 = Nameability::new(0.6999);
    assert_eq!(just_below_07.level(), NameabilityLevel::Medium);
    assert!(!just_below_07.is_high());

    let at_04 = Nameability::new(0.4);
    assert_eq!(at_04.level(), NameabilityLevel::Medium);
    assert!(!at_04.is_low());

    let just_below_04 = Nameability::new(0.3999);
    assert_eq!(just_below_04.level(), NameabilityLevel::Low);
    assert!(just_below_04.is_low());
}

#[test]
fn test_alignment_single_match() {
    let mut alignment = AlignmentScore::new();
    alignment.record_match(0.75);

    assert_eq!(alignment.count(), 1);
    assert!((alignment.mean() - 0.75).abs() < 0.001);
    assert_eq!(alignment.variance(), 0.0); // Single value has no variance
    assert_eq!(alignment.std_dev(), 0.0);
    assert!(alignment.confidence() > 0.0); // Even 1 match gives some confidence
}

#[test]
fn test_alignment_identical_matches() {
    let mut alignment = AlignmentScore::new();
    for _ in 0..10 {
        alignment.record_match(0.85);
    }

    assert_eq!(alignment.count(), 10);
    assert!((alignment.mean() - 0.85).abs() < 0.001);
    // Variance should be essentially zero for identical values
    assert!(alignment.variance() < 0.0001);
}

#[test]
fn test_alignment_extreme_variance() {
    let mut alignment = AlignmentScore::new();
    // Mix of very high and very low similarities
    alignment.record_match(1.0);
    alignment.record_match(0.0);
    alignment.record_match(1.0);
    alignment.record_match(0.0);

    assert_eq!(alignment.count(), 4);
    assert!((alignment.mean() - 0.5).abs() < 0.001);
    // Variance should be high (0.25 for this distribution)
    assert!(alignment.variance() > 0.2);
}

#[test]
fn test_gradient_none_gives_zero_adjustment() {
    let gradient = GeneralizationGradient::none();

    for sim in [0.0, 0.5, 1.0] {
        for conf in [0.0, 0.5, 1.0] {
            let adj = gradient.threshold_adjustment(sim, conf, 0.2);
            assert_eq!(adj, 0.0, "None gradient should always give 0 adjustment");
        }
    }
}

#[test]
fn test_gradient_zero_confidence_gives_zero_adjustment() {
    let gradients = [
        GeneralizationGradient::linear(),
        GeneralizationGradient::quadratic(),
        GeneralizationGradient::exponential(2.0),
    ];

    for gradient in &gradients {
        for sim in [0.0, 0.5, 1.0] {
            let adj = gradient.threshold_adjustment(sim, 0.0, 0.2);
            assert!(
                adj.abs() < 0.001,
                "{:?} with 0 confidence should give ~0 adjustment, got {}",
                gradient,
                adj
            );
        }
    }
}

#[test]
fn test_exponential_decay_rates() {
    let slow = GeneralizationGradient::exponential(0.5);
    let fast = GeneralizationGradient::exponential(5.0);

    // At low similarity (high distance), fast decay should give smaller adjustment
    let sim = 0.3;
    let adj_slow = slow.threshold_adjustment(sim, 1.0, 0.2);
    let adj_fast = fast.threshold_adjustment(sim, 1.0, 0.2);

    // Both should be negative
    assert!(adj_slow < 0.0);
    assert!(adj_fast < 0.0);

    // Fast decay gives smaller (less negative) adjustment at high distance
    // because exp(-k*d) decays faster for larger k
    assert!(
        adj_slow.abs() > adj_fast.abs(),
        "Slow decay {} should exceed fast decay {} at low similarity",
        adj_slow.abs(),
        adj_fast.abs()
    );
}

// =============================================================================
// Adaptive Config Tests
// =============================================================================

#[test]
fn test_config_strict_vs_loose() {
    let strict = AdaptiveResolutionConfig::strict();
    let loose = AdaptiveResolutionConfig::loose();

    let alignment = AlignmentScore::new();

    let strict_thresh = strict.compute_threshold(&alignment, 0.8, None);
    let loose_thresh = loose.compute_threshold(&alignment, 0.8, None);

    assert!(
        strict_thresh > loose_thresh,
        "Strict config ({}) should have higher threshold than loose ({})",
        strict_thresh,
        loose_thresh
    );
}

#[test]
fn test_config_nameability_toggle() {
    let mut with_name = AdaptiveResolutionConfig::default();
    with_name.use_nameability = true;

    let mut without_name = AdaptiveResolutionConfig::default();
    without_name.use_nameability = false;

    let alignment = AlignmentScore::new();
    let high_name = Nameability::high(0.9);
    let low_name = Nameability::low(0.2);

    // With nameability enabled, PERSON should get lower threshold
    let thresh_high = with_name.compute_threshold(&alignment, 0.8, Some(high_name));
    let thresh_low = with_name.compute_threshold(&alignment, 0.8, Some(low_name));
    assert!(thresh_high < thresh_low, "High nameability should lower threshold");

    // Without nameability, both should be the same
    let thresh_high_off = without_name.compute_threshold(&alignment, 0.8, Some(high_name));
    let thresh_low_off = without_name.compute_threshold(&alignment, 0.8, Some(low_name));
    assert!(
        (thresh_high_off - thresh_low_off).abs() < 0.001,
        "With nameability off, both should be equal"
    );
}

#[test]
fn test_config_gradient_effect() {
    let mut config_linear = AdaptiveResolutionConfig::default();
    config_linear.gradient = GeneralizationGradient::linear();

    let mut config_quad = AdaptiveResolutionConfig::default();
    config_quad.gradient = GeneralizationGradient::quadratic();

    // Build alignment with evidence
    let mut alignment = AlignmentScore::new();
    for _ in 0..5 {
        alignment.record_match(0.85);
    }

    // At low similarity, quadratic should be more conservative (higher threshold)
    let sim = 0.5;
    let linear_thresh = config_linear.compute_threshold(&alignment, sim, None);
    let quad_thresh = config_quad.compute_threshold(&alignment, sim, None);

    assert!(
        quad_thresh > linear_thresh,
        "Quadratic ({}) should be more conservative than linear ({}) at low similarity",
        quad_thresh,
        linear_thresh
    );
}

// =============================================================================
// Regression Tests
// =============================================================================

/// Regression: variance calculation should never produce NaN
#[test]
fn test_variance_never_nan() {
    let mut alignment = AlignmentScore::new();

    // Add many identical values (which previously could cause negative variance)
    for _ in 0..100 {
        alignment.record_match(0.85);
    }

    let var = alignment.variance();
    assert!(!var.is_nan(), "Variance should not be NaN");
    assert!(var >= 0.0, "Variance should be non-negative");

    let std = alignment.std_dev();
    assert!(!std.is_nan(), "Std dev should not be NaN");
    assert!(std >= 0.0, "Std dev should be non-negative");

    let conf = alignment.confidence();
    assert!(!conf.is_nan(), "Confidence should not be NaN");
    assert!(conf >= 0.0 && conf <= 1.0, "Confidence should be in [0,1]");
}

/// Regression: confidence should be monotonic with evidence
#[test]
fn test_confidence_monotonic_with_evidence() {
    let mut alignment = AlignmentScore::new();
    let mut prev_confidence = 0.0;

    // Adding consistent evidence should monotonically increase confidence
    for i in 0..20 {
        alignment.record_match(0.85);
        let conf = alignment.confidence();

        assert!(
            conf >= prev_confidence - 0.001, // Small tolerance for float precision
            "Confidence decreased from {} to {} after match {}",
            prev_confidence,
            conf,
            i + 1
        );
        prev_confidence = conf;
    }
}

/// Regression: threshold should be monotonic with similarity (when nameability is disabled)
///
/// Higher similarity → stronger generalization → more threshold reduction → LOWER threshold.
/// So when iterating from low to high similarity, thresholds should decrease.
#[test]
fn test_threshold_monotonic_with_similarity() {
    let mut config = AdaptiveResolutionConfig::default();
    config.use_nameability = false; // Disable to isolate gradient effect

    // Build well-evidenced alignment
    let mut alignment = AlignmentScore::new();
    for _ in 0..10 {
        alignment.record_match(0.9);
    }

    // Iterate from LOW to HIGH similarity
    let mut prev_threshold = 1.0;
    for sim_int in 0..=100 {
        let sim = sim_int as f32 / 100.0;
        let thresh = config.compute_threshold(&alignment, sim, None);

        // Higher similarity should give LOWER or equal threshold
        assert!(
            thresh <= prev_threshold + 0.001,
            "Higher similarity should give lower threshold: sim {} gave {}, prev {} (sim {})",
            sim,
            thresh,
            prev_threshold,
            (sim_int as f32 - 1.0) / 100.0
        );
        prev_threshold = thresh;
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// AlignmentScore merge preserves total count
        #[test]
        fn alignment_merge_preserves_count(
            scores_a in prop::collection::vec(0.0f32..1.0f32, 0..20),
            scores_b in prop::collection::vec(0.0f32..1.0f32, 0..20)
        ) {
            let mut a = AlignmentScore::new();
            for s in &scores_a {
                a.record_match(*s);
            }

            let mut b = AlignmentScore::new();
            for s in &scores_b {
                b.record_match(*s);
            }

            let total_before = a.count() + b.count();
            a.merge(&b);

            prop_assert_eq!(a.count(), total_before,
                "Merge should preserve total count");
        }

        /// Nameability level classification is consistent
        #[test]
        fn nameability_level_consistent(score in 0.0f32..1.0f32) {
            let n = Nameability::new(score);
            let level = n.level();

            match level {
                NameabilityLevel::High => {
                    prop_assert!(n.score() >= 0.7);
                    prop_assert!(n.is_high());
                    prop_assert!(!n.is_low());
                }
                NameabilityLevel::Medium => {
                    prop_assert!(n.score() >= 0.4 && n.score() < 0.7);
                    prop_assert!(!n.is_high());
                    prop_assert!(!n.is_low());
                }
                NameabilityLevel::Low => {
                    prop_assert!(n.score() < 0.4);
                    prop_assert!(!n.is_high());
                    prop_assert!(n.is_low());
                }
            }
        }

        /// SND conversion is inverse of nameability score
        #[test]
        fn snd_inverse_of_nameability(snd in 0.0f32..1.0f32) {
            let n = Nameability::from_snd(snd);
            let expected = 1.0 - snd;
            prop_assert!((n.score() - expected).abs() < 0.001,
                "from_snd({}) should give {}, got {}", snd, expected, n.score());
        }

        /// Config threshold never exceeds 1.0 or goes below min
        #[test]
        fn config_threshold_bounded(
            base in 0.3f32..0.95f32,
            min in 0.1f32..0.5f32,
            max_adj in 0.1f32..0.4f32,
            similarity in 0.0f32..1.0f32,
            match_count in 0usize..20,
            nameability_score in 0.0f32..1.0f32
        ) {
            let config = AdaptiveResolutionConfig {
                base_threshold: base,
                min_threshold: min,
                max_adjustment: max_adj,
                gradient: GeneralizationGradient::quadratic(),
                use_nameability: true,
            };

            let mut alignment = AlignmentScore::new();
            for _ in 0..match_count {
                alignment.record_match(0.85);
            }

            let nameability = Nameability::new(nameability_score);
            let threshold = config.compute_threshold(&alignment, similarity, Some(nameability));

            prop_assert!(threshold >= min, "Threshold {} below min {}", threshold, min);
            prop_assert!(threshold <= 1.0, "Threshold {} exceeds 1.0", threshold);
        }

        /// All entity types return valid nameability
        #[test]
        fn entity_type_nameability_valid(entity_type in "[A-Z_]{1,20}") {
            let n = entity_type_nameability(&entity_type);
            prop_assert!(n.score() >= 0.0 && n.score() <= 1.0);
        }
    }
}
