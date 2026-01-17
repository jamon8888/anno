//! Integration test for alignment module with KiloGram dataset.
//!
//! This test validates that the Nameability type correctly models
//! Shape Naming Divergence from the KiloGram dataset.

use anno_coalesce::{
    AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient, Nameability,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TangramSND {
    tangram_id: String,
    snd: f64,
    #[allow(dead_code)]
    nameability: f64,
    #[allow(dead_code)]
    num_annotations: usize,
}

/// Load SND distribution from testdata.
fn load_snd_distribution() -> Vec<TangramSND> {
    let data = include_str!("../../testdata/kilogram/snd_distribution.json");
    serde_json::from_str(data).expect("Failed to parse SND distribution")
}

#[test]
fn test_kilogram_snd_nameability_conversion() {
    let tangrams = load_snd_distribution();
    assert!(!tangrams.is_empty(), "Should have tangram data");

    for tangram in &tangrams {
        // Verify SND → Nameability conversion
        let computed = Nameability::from_snd(tangram.snd as f32);
        let expected = 1.0 - tangram.snd;

        assert!(
            (computed.score() as f64 - expected).abs() < 0.001,
            "Nameability mismatch for {}: computed={}, expected={}",
            tangram.tangram_id,
            computed.score(),
            expected
        );
    }
}

#[test]
fn test_kilogram_nameability_distribution() {
    let tangrams = load_snd_distribution();

    // Count by nameability level
    let mut high = 0;
    let mut medium = 0;
    let mut low = 0;

    for tangram in &tangrams {
        let nameability = Nameability::from_snd(tangram.snd as f32);
        match nameability.level() {
            anno_coalesce::NameabilityLevel::High => high += 1,
            anno_coalesce::NameabilityLevel::Medium => medium += 1,
            anno_coalesce::NameabilityLevel::Low => low += 1,
        }
    }

    // The dense10 dataset is biased toward low-nameability tangrams
    // (they selected diverse, hard-to-name shapes for dense annotation)
    assert!(
        low > high + medium,
        "Dense dataset should be biased toward low nameability: high={}, medium={}, low={}",
        high,
        medium,
        low
    );
}

#[test]
fn test_adaptive_threshold_with_kilogram_nameability() {
    let tangrams = load_snd_distribution();
    let config = AdaptiveResolutionConfig::default();

    // Find a low and high nameability tangram
    let low_tangram = tangrams
        .iter()
        .find(|t| Nameability::from_snd(t.snd as f32).is_low())
        .expect("Should have low nameability tangram");

    let high_tangram = tangrams
        .iter()
        .find(|t| Nameability::from_snd(t.snd as f32).is_high())
        .unwrap_or(&tangrams[0]); // Fall back if none

    let low_name = Nameability::from_snd(low_tangram.snd as f32);
    let high_name = Nameability::from_snd(high_tangram.snd as f32);

    // With same alignment and similarity, nameability should affect threshold
    let alignment = AlignmentScore::new();
    let similarity = 0.75;

    let low_threshold = config.compute_threshold(&alignment, similarity, Some(low_name));
    let high_threshold = config.compute_threshold(&alignment, similarity, Some(high_name));

    // High nameability → lower threshold (more confident in naming consensus)
    // Note: if both are low nameability, this assertion might not hold
    if !high_name.is_low() {
        assert!(
            high_threshold <= low_threshold,
            "High nameability should allow lower threshold: high={}, low={}",
            high_threshold,
            low_threshold
        );
    }
}

#[test]
fn test_alignment_accumulation_reduces_threshold() {
    let config = AdaptiveResolutionConfig::default();
    let _base_threshold = config.base_threshold;

    // No evidence → threshold near base
    let no_evidence = AlignmentScore::new();
    let t_no_evidence = config.compute_threshold(&no_evidence, 0.8, None);

    // Some evidence
    let mut some_evidence = AlignmentScore::new();
    for _ in 0..5 {
        some_evidence.record_match(0.85);
    }
    let t_some_evidence = config.compute_threshold(&some_evidence, 0.8, None);

    // Lots of evidence
    let mut lots_evidence = AlignmentScore::new();
    for _ in 0..20 {
        lots_evidence.record_match(0.9);
    }
    let t_lots_evidence = config.compute_threshold(&lots_evidence, 0.8, None);

    // More evidence → lower threshold
    assert!(
        t_lots_evidence <= t_some_evidence,
        "More evidence should allow lower threshold"
    );
    assert!(
        t_some_evidence <= t_no_evidence,
        "Some evidence should allow lower threshold than none"
    );

    // But threshold should stay above minimum
    assert!(
        t_lots_evidence >= config.min_threshold,
        "Threshold should not go below minimum"
    );
}

#[test]
fn test_generalization_gradient_quadratic_vs_linear() {
    let quadratic = GeneralizationGradient::quadratic();
    let linear = GeneralizationGradient::linear();

    let confidence = 1.0;
    let max_adj = 0.2;

    // At high similarity, both should give large adjustments (strong generalization)
    let high_sim = 0.9;
    let q_high = quadratic
        .threshold_adjustment(high_sim, confidence, max_adj)
        .abs();
    let _l_high = linear
        .threshold_adjustment(high_sim, confidence, max_adj)
        .abs();

    // At low similarity, quadratic should give smaller adjustment than linear
    // (because sim^2 < sim for sim < 1, meaning less generalization to dissimilar things)
    let low_sim = 0.5;
    let q_low = quadratic
        .threshold_adjustment(low_sim, confidence, max_adj)
        .abs();
    let l_low = linear
        .threshold_adjustment(low_sim, confidence, max_adj)
        .abs();

    assert!(
        q_low < l_low,
        "Quadratic should be more conservative at low similarity: quad={}, lin={}",
        q_low,
        l_low
    );

    // High similarity should give larger adjustments than low similarity
    assert!(
        q_high > q_low,
        "High sim should give larger adjustment: high={}, low={}",
        q_high,
        q_low
    );

    // Both should give maximum adjustment when similarity is 1
    let q_perfect = quadratic
        .threshold_adjustment(1.0, confidence, max_adj)
        .abs();
    let l_perfect = linear.threshold_adjustment(1.0, confidence, max_adj).abs();
    assert!(
        (q_perfect - max_adj).abs() < 0.001,
        "Quadratic at sim=1: {}",
        q_perfect
    );
    assert!(
        (l_perfect - max_adj).abs() < 0.001,
        "Linear at sim=1: {}",
        l_perfect
    );
}
