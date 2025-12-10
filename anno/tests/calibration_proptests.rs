//! Property tests for calibration metrics.
//!
//! These tests verify invariants of calibration measures:
//! - ECE, MCE are in [0, 1]
//! - Brier score is in [0, 1]
//! - Cross-entropy is non-negative
//! - Perfect calibration has ECE = 0
//! - Perfectly wrong predictions have Brier = 1
//!
//! Based on Kehler (1997) and Guo et al. (2017) "On Calibration of Modern Neural Networks".

#![cfg(feature = "eval-advanced")]

use proptest::prelude::*;

// We need to test calibration without importing the module directly since it's
// feature-gated. These tests verify the mathematical properties.

/// Generate a calibration sample (confidence, correctness)
fn arb_sample() -> impl Strategy<Value = (f64, bool)> {
    (0.0f64..=1.0f64, proptest::bool::ANY)
}

/// Generate a batch of calibration samples
fn arb_samples(n: usize) -> impl Strategy<Value = Vec<(f64, bool)>> {
    proptest::collection::vec(arb_sample(), n)
}

// =============================================================================
// Brier Score Properties
// =============================================================================

/// Compute Brier score (mean squared error of probability estimates)
fn brier_score(samples: &[(f64, bool)]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples
        .iter()
        .map(|(conf, correct)| {
            let y = if *correct { 1.0 } else { 0.0 };
            (conf - y).powi(2)
        })
        .sum();

    sum_sq / samples.len() as f64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Brier score is always in [0, 1].
    #[test]
    fn prop_brier_bounded(samples in arb_samples(50)) {
        let brier = brier_score(&samples);
        prop_assert!(brier >= 0.0, "Brier should be >= 0: {}", brier);
        prop_assert!(brier <= 1.0, "Brier should be <= 1: {}", brier);
    }

    /// Perfect predictions (conf=1 when correct, conf=0 when wrong) have Brier=0.
    #[test]
    fn prop_perfect_predictions_brier_zero(correctness in proptest::collection::vec(proptest::bool::ANY, 10)) {
        let perfect: Vec<(f64, bool)> = correctness
            .iter()
            .map(|&c| (if c { 1.0 } else { 0.0 }, c))
            .collect();

        let brier = brier_score(&perfect);
        prop_assert!((brier - 0.0).abs() < 1e-10, "Perfect should have Brier=0: {}", brier);
    }

    /// Worst predictions (conf=1 when wrong, conf=0 when correct) have Brier=1.
    #[test]
    fn prop_worst_predictions_brier_one(correctness in proptest::collection::vec(proptest::bool::ANY, 10)) {
        let worst: Vec<(f64, bool)> = correctness
            .iter()
            .map(|&c| (if c { 0.0 } else { 1.0 }, c))
            .collect();

        let brier = brier_score(&worst);
        prop_assert!((brier - 1.0).abs() < 1e-10, "Worst should have Brier=1: {}", brier);
    }
}

// =============================================================================
// Cross-Entropy Properties
// =============================================================================

/// Compute cross-entropy loss
fn cross_entropy(samples: &[(f64, bool)], eps: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let eps = eps.max(1e-15);

    let sum: f64 = samples
        .iter()
        .map(|(conf, correct)| {
            let p = conf.clamp(eps, 1.0 - eps);
            if *correct {
                -p.ln()
            } else {
                -(1.0 - p).ln()
            }
        })
        .sum();

    sum / samples.len() as f64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Cross-entropy is always non-negative.
    #[test]
    fn prop_cross_entropy_non_negative(samples in arb_samples(50)) {
        let ce = cross_entropy(&samples, 1e-15);
        prop_assert!(ce >= 0.0, "Cross-entropy should be >= 0: {}", ce);
    }

    /// Higher confidence in correct predictions gives lower cross-entropy.
    /// (Kehler's key insight: P=0.9 correct is better than P=0.6 correct)
    #[test]
    fn prop_kehler_insight_high_conf_better(
        high_conf in 0.7f64..=0.99f64,
        low_conf in 0.5f64..=0.69f64
    ) {
        let high_sample = vec![(high_conf, true)];
        let low_sample = vec![(low_conf, true)];

        let ce_high = cross_entropy(&high_sample, 1e-15);
        let ce_low = cross_entropy(&low_sample, 1e-15);

        prop_assert!(ce_high < ce_low,
            "Higher confidence correct should have lower CE: {:.4} vs {:.4}",
            ce_high, ce_low);
    }

    /// Cross-entropy heavily penalizes confident wrong predictions.
    #[test]
    fn prop_confident_wrong_penalized(high_conf in 0.9f64..=0.99f64) {
        let confident_wrong = vec![(high_conf, false)];
        let uncertain_wrong = vec![(0.5, false)];

        let ce_confident = cross_entropy(&confident_wrong, 1e-15);
        let ce_uncertain = cross_entropy(&uncertain_wrong, 1e-15);

        prop_assert!(ce_confident > ce_uncertain,
            "Confident wrong should be penalized more: {:.4} vs {:.4}",
            ce_confident, ce_uncertain);
    }
}

// =============================================================================
// ECE Properties
// =============================================================================

/// Compute reliability diagram buckets
fn reliability_buckets(samples: &[(f64, bool)], n_bins: usize) -> Vec<(f64, f64, usize)> {
    let bin_width = 1.0 / n_bins as f64;

    let mut bucket_sum_conf: Vec<f64> = vec![0.0; n_bins];
    let mut bucket_correct: Vec<usize> = vec![0; n_bins];
    let mut bucket_count: Vec<usize> = vec![0; n_bins];

    for &(conf, correct) in samples {
        let bin = ((conf * n_bins as f64).floor() as usize).min(n_bins - 1);
        bucket_sum_conf[bin] += conf;
        bucket_count[bin] += 1;
        if correct {
            bucket_correct[bin] += 1;
        }
    }

    (0..n_bins)
        .map(|i| {
            let count = bucket_count[i];
            let mean_conf = if count > 0 {
                bucket_sum_conf[i] / count as f64
            } else {
                (i as f64 + 0.5) * bin_width
            };
            let accuracy = if count > 0 {
                bucket_correct[i] as f64 / count as f64
            } else {
                0.0
            };
            (mean_conf, accuracy, count)
        })
        .collect()
}

/// Compute Expected Calibration Error
fn ece(samples: &[(f64, bool)], n_bins: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let buckets = reliability_buckets(samples, n_bins);
    let n = samples.len() as f64;

    buckets
        .iter()
        .map(|(mean_conf, accuracy, count)| (*count as f64 / n) * (mean_conf - accuracy).abs())
        .sum()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// ECE is always in [0, 1].
    #[test]
    fn prop_ece_bounded(samples in arb_samples(100)) {
        let e = ece(&samples, 10);
        prop_assert!(e >= 0.0, "ECE should be >= 0: {}", e);
        prop_assert!(e <= 1.0, "ECE should be <= 1: {}", e);
    }

    /// Perfectly calibrated predictions have ECE ≈ 0.
    /// (Confidence matches accuracy in each bin)
    #[test]
    fn prop_perfect_calibration_low_ece(_seed in 0u64..100) {
        // Generate calibrated samples: if confidence is p, be correct with probability p
        // Approximate this by binning
        let mut samples = Vec::new();
        for conf_bucket in 0..10 {
            let conf = (conf_bucket as f64 + 0.5) / 10.0;
            let n_correct = (conf_bucket + 1) as usize; // Roughly conf * 10
            let n_wrong = 10 - n_correct;

            for _ in 0..n_correct {
                samples.push((conf, true));
            }
            for _ in 0..n_wrong {
                samples.push((conf, false));
            }
        }

        let e = ece(&samples, 10);
        // Should be relatively low (not perfectly 0 due to discretization)
        prop_assert!(e < 0.2, "Well-calibrated should have low ECE: {}", e);
    }

    /// Overconfident predictions (always conf=0.9, 50% correct) have high ECE.
    #[test]
    fn prop_overconfident_high_ece(n_samples in 20usize..100) {
        let samples: Vec<(f64, bool)> = (0..n_samples)
            .map(|i| (0.9, i % 2 == 0))  // 50% correct
            .collect();

        let e = ece(&samples, 10);
        // Gap between 0.9 confidence and 0.5 accuracy = 0.4
        prop_assert!(e > 0.3, "Overconfident should have high ECE: {}", e);
    }
}

// =============================================================================
// E2E Test: Full Calibration Pipeline
// =============================================================================

/// Test that calibration metrics behave correctly in an end-to-end scenario.
#[test]
fn e2e_calibration_metrics_consistency() {
    // Scenario: Model makes predictions with varying confidence
    let samples = vec![
        // High confidence, correct
        (0.95, true),
        (0.92, true),
        (0.88, true),
        // High confidence, wrong (should be penalized)
        (0.90, false),
        // Medium confidence, mixed
        (0.65, true),
        (0.60, false),
        (0.55, true),
        // Low confidence, correct (underconfident)
        (0.30, true),
        (0.25, true),
        // Low confidence, wrong (good calibration)
        (0.20, false),
        (0.15, false),
    ];

    let brier = brier_score(&samples);
    let ce = cross_entropy(&samples, 1e-15);
    let e = ece(&samples, 10);

    // Basic bounds
    assert!(brier >= 0.0 && brier <= 1.0);
    assert!(ce >= 0.0);
    assert!(e >= 0.0 && e <= 1.0);

    // This model is somewhat calibrated but not perfect
    // Should have moderate scores
    assert!(
        brier > 0.1 && brier < 0.5,
        "Brier should be moderate: {}",
        brier
    );
    assert!(e > 0.05 && e < 0.5, "ECE should be moderate: {}", e);

    println!("E2E Calibration Results:");
    println!("  Brier Score: {:.4}", brier);
    println!("  Cross-Entropy: {:.4}", ce);
    println!("  ECE: {:.4}", e);
}

/// Test: Kehler's calibration insight - downstream systems need reliable confidence.
#[test]
fn e2e_kehler_calibration_for_fusion() {
    // Simulate two NER systems for data fusion

    // System A: Well-calibrated (confidence matches accuracy)
    let system_a = vec![
        (0.9, true),
        (0.9, true),
        (0.9, true),
        (0.9, true),
        (0.9, false), // 80% acc at 0.9 conf
        (0.7, true),
        (0.7, true),
        (0.7, false), // 67% acc at 0.7 conf
        (0.5, true),
        (0.5, false), // 50% acc at 0.5 conf
    ];

    // System B: Overconfident (always says 0.95, but only 60% accurate)
    let system_b: Vec<(f64, bool)> = (0..10).map(|i| (0.95, i < 6)).collect();

    let ece_a = ece(&system_a, 10);
    let ece_b = ece(&system_b, 10);

    // System A should have lower ECE (better calibrated)
    assert!(
        ece_a < ece_b,
        "System A (calibrated) should have lower ECE than System B (overconfident): {} vs {}",
        ece_a,
        ece_b
    );

    // Kehler's point: For data fusion, we need to trust confidence scores.
    // A well-calibrated system is more useful even if raw accuracy is similar.
    println!("Data Fusion Calibration Comparison:");
    println!("  System A (calibrated) ECE: {:.4}", ece_a);
    println!("  System B (overconfident) ECE: {:.4}", ece_b);
}
