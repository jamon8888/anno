//! Adaptive Entity Resolution Example
//!
//! Demonstrates dynamic threshold adjustment based on:
//! - Entity type nameability (prior consensus)
//! - Accumulated alignment evidence
//! - Generalization gradients (Shepard's Universal Law)
//!
//! Research basis: "Ad hoc conventions generalize to new referents" (Ji et al., 2025)
//!
//! Run with: cargo run --example adaptive_resolution

use anno_coalesce::{
    entity_type_nameability, AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient,
    Nameability, Resolver,
};

fn main() {
    println!("=== Adaptive Entity Resolution ===\n");

    // 1. Demonstrate nameability priors
    println!("1. Nameability Priors by Entity Type\n");
    println!("   High-nameability types (lower thresholds):");
    for t in ["PERSON", "LOCATION", "DATE"] {
        let n = entity_type_nameability(t);
        println!("     {}: {:.2}", t, n.score());
    }
    println!("\n   Low-nameability types (higher thresholds):");
    for t in ["MISC", "WORK_OF_ART"] {
        let n = entity_type_nameability(t);
        println!("     {}: {:.2}", t, n.score());
    }

    // 2. Demonstrate alignment score accumulation
    println!("\n2. Alignment Score Accumulation\n");
    let mut alignment = AlignmentScore::new();
    println!("   Initial confidence: {:.3}", alignment.confidence());

    for i in 1..=5 {
        alignment.record_match(0.85);
        println!(
            "   After {} match(es): confidence = {:.3}, mean = {:.3}",
            i,
            alignment.confidence(),
            alignment.mean()
        );
    }

    // 3. Demonstrate threshold adjustment
    println!("\n3. Adaptive Threshold Computation\n");
    let config = AdaptiveResolutionConfig {
        base_threshold: 0.7,
        min_threshold: 0.4,
        max_adjustment: 0.2,
        gradient: GeneralizationGradient::quadratic(),
        use_nameability: true,
    };

    // Empty alignment (no prior evidence)
    let empty = AlignmentScore::new();
    let person_name = entity_type_nameability("PERSON");
    let misc_name = entity_type_nameability("MISC");

    let t_person_empty = config.compute_threshold(&empty, 0.8, Some(person_name));
    let t_misc_empty = config.compute_threshold(&empty, 0.8, Some(misc_name));

    println!("   No prior evidence (similarity=0.8):");
    println!("     PERSON threshold: {:.3}", t_person_empty);
    println!("     MISC threshold:   {:.3}", t_misc_empty);

    // With accumulated evidence
    let mut evidenced = AlignmentScore::new();
    for _ in 0..10 {
        evidenced.record_match(0.85);
    }

    let t_person_evidenced = config.compute_threshold(&evidenced, 0.8, Some(person_name));
    let t_misc_evidenced = config.compute_threshold(&evidenced, 0.8, Some(misc_name));

    println!("\n   After 10 successful matches (similarity=0.8):");
    println!("     PERSON threshold: {:.3}", t_person_evidenced);
    println!("     MISC threshold:   {:.3}", t_misc_evidenced);

    // 4. Demonstrate gradient effects
    println!("\n4. Generalization Gradient Effects\n");
    println!("   Threshold adjustment at different similarity levels:");
    println!("   (confidence=0.8, max_adjustment=0.2)\n");

    let gradients = [
        ("None", GeneralizationGradient::none()),
        ("Linear", GeneralizationGradient::linear()),
        ("Quadratic", GeneralizationGradient::quadratic()),
        ("Exponential(2.0)", GeneralizationGradient::exponential(2.0)),
    ];

    println!("   {:20} {:>10} {:>10} {:>10}", "Gradient", "sim=0.5", "sim=0.7", "sim=0.9");
    for (name, gradient) in &gradients {
        let adj_05 = gradient.threshold_adjustment(0.5, 0.8, 0.2);
        let adj_07 = gradient.threshold_adjustment(0.7, 0.8, 0.2);
        let adj_09 = gradient.threshold_adjustment(0.9, 0.8, 0.2);
        println!(
            "   {:20} {:>10.3} {:>10.3} {:>10.3}",
            name, adj_05, adj_07, adj_09
        );
    }

    // 5. Practical usage with Resolver
    println!("\n5. Resolver Integration\n");
    let resolver = Resolver::new()
        .with_threshold(0.7)
        .with_adaptive(AdaptiveResolutionConfig::default());

    println!("   Resolver created with adaptive thresholds enabled.");
    println!("   The resolver will now:");
    println!("     - Lower thresholds for high-nameability types (PERSON, LOCATION)");
    println!("     - Raise thresholds for low-nameability types (MISC)");
    println!("     - Progressively lower thresholds as clusters accumulate evidence");

    // Show the resolver exists (we can't easily demo without a corpus)
    let _ = resolver;

    println!("\n=== End of Example ===");
}
