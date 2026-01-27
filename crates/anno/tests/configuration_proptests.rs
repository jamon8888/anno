//! Property tests for coreference configuration distributions.
//!
//! These tests verify invariants derived from the type-theoretic perspective:
//! - Configurations are valid partitions (each mention in exactly one cell)
//! - Probability distributions are normalized
//! - Cross-entropy is non-negative
//! - Dempster combination properties hold
//!
//! Based on Kehler (1997) and Curry-Howard analysis in docs/notes/research/theory/TYPE_THEORY_AND_NER.md.

use anno::coalesce as anno_coalesce;

use anno_coalesce::configuration::{bell_number, ConfigurationBuilder, CorefConfiguration};
use proptest::prelude::*;

// =============================================================================
// Partition Validity Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// same_cell is symmetric: same_cell(i,j) == same_cell(j,i)
    #[test]
    fn prop_same_cell_symmetric(n in 2usize..=10, i in 0usize..10, j in 0usize..10) {
        let n = n.max(1);
        let i = i % n;
        let j = j % n;

        let config = CorefConfiguration::all_singletons(n);
        prop_assert_eq!(config.same_cell(i, j), config.same_cell(j, i));
    }

    /// same_cell is transitive: if same_cell(i,j) and same_cell(j,k), then same_cell(i,k)
    #[test]
    fn prop_same_cell_transitive(cells in proptest::collection::vec(1usize..=3, 1..=4)) {
        let n: usize = cells.iter().sum();
        if n == 0 {
            return Ok(());
        }

        // Build a configuration from the cell sizes
        let mut current = 0;
        let cell_contents: Vec<Vec<usize>> = cells.iter()
            .map(|&size| {
                let cell: Vec<usize> = (current..current + size).collect();
                current += size;
                cell
            })
            .collect();

        let config = CorefConfiguration::new(cell_contents);

        // Check transitivity for all triples
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    if config.same_cell(i, j) && config.same_cell(j, k) {
                        prop_assert!(config.same_cell(i, k),
                            "Transitivity violated: same_cell({},{}) and same_cell({},{}) but not same_cell({},{})",
                            i, j, j, k, i, k);
                    }
                }
            }
        }
    }
}

// =============================================================================
// Distribution Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// For n=2, distribution should be well-formed (probabilities sum to ~1).
    #[test]
    fn prop_distribution_two_mentions(prob in 0.0f64..=1.0f64) {
        let mut builder = ConfigurationBuilder::new(2);
        builder.set_pairwise(0, 1, prob);
        let dist = builder.build_evidential();

        // Check that mode exists and is valid
        let mode = dist.mode();
        prop_assert!(mode.is_some(), "Should have a mode");

        // Check probabilities for all configs
        let all_same = CorefConfiguration::all_same(2);
        let all_sep = CorefConfiguration::all_singletons(2);

        let p_same = dist.prob(&all_same);
        let p_sep = dist.prob(&all_sep);

        prop_assert!(p_same >= 0.0, "Negative probability: {}", p_same);
        prop_assert!(p_sep >= 0.0, "Negative probability: {}", p_sep);
        prop_assert!((0.0..=1.0).contains(&p_same), "Probability > 1: {}", p_same);
        prop_assert!((0.0..=1.0).contains(&p_sep), "Probability > 1: {}", p_sep);

        // Sum should be close to 1
        let total = p_same + p_sep;
        prop_assert!((total - 1.0).abs() < 0.1,
            "Distribution not normalized: sum = {}", total);
    }

    /// Cross-entropy with the mode should be relatively low.
    #[test]
    fn prop_cross_entropy_with_mode(n in 2usize..=4) {
        let builder = ConfigurationBuilder::new(n);
        let dist = builder.build_evidential();

        if let Some(mode) = dist.mode() {
            let ce = dist.cross_entropy(mode);
            prop_assert!(ce >= 0.0, "Negative cross-entropy: {}", ce);
            // Cross-entropy with mode should be bounded
            prop_assert!(ce < 10.0, "Cross-entropy too high: {}", ce);
        }
    }

    /// Entropy is non-negative.
    #[test]
    fn prop_entropy_non_negative(n in 2usize..=5) {
        let builder = ConfigurationBuilder::new(n);
        let dist = builder.build_evidential();

        let entropy = dist.entropy();
        prop_assert!(entropy >= 0.0, "Negative entropy: {}", entropy);
    }
}

// =============================================================================
// Bell Number Properties
// =============================================================================

proptest! {
    /// Bell number recurrence: B_{n+1} = sum_{k=0}^{n} C(n,k) * B_k
    #[test]
    fn prop_bell_recurrence(n in 0usize..=7) {
        if n == 0 {
            return Ok(());
        }

        let b_n_plus_1 = bell_number(n);

        // Compute via recurrence
        let mut sum = 0usize;
        for k in 0..n {
            let binomial = binomial_coeff(n - 1, k);
            sum += binomial * bell_number(k);
        }

        prop_assert_eq!(b_n_plus_1, sum,
            "Bell recurrence failed for n={}", n);
    }

    /// Bell numbers are positive
    #[test]
    fn prop_bell_positive(n in 0usize..=10) {
        prop_assert!(bell_number(n) >= 1, "Bell({}) should be >= 1", n);
    }

    /// Bell numbers are monotonically increasing for n >= 1
    #[test]
    fn prop_bell_monotonic(n in 1usize..=9) {
        prop_assert!(bell_number(n) <= bell_number(n + 1),
            "Bell({}) = {} > Bell({}) = {}",
            n, bell_number(n), n + 1, bell_number(n + 1));
    }
}

/// Helper: compute binomial coefficient C(n, k)
fn binomial_coeff(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let mut result = 1;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

// =============================================================================
// Incompatibility Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Incompatible mentions should have zero probability of being in same cell.
    #[test]
    fn prop_incompatible_zero_prob(n in 3usize..=4) {
        let mut builder = ConfigurationBuilder::new(n);
        // Make 0 and 1 incompatible
        builder.set_incompatible(0, 1);

        let dist = builder.build_evidential();

        // Any configuration with 0 and 1 in same cell should have zero probability
        let mut cells: Vec<Vec<usize>> = vec![vec![0, 1]];
        for i in 2..n {
            cells.push(vec![i]);
        }
        let invalid = CorefConfiguration::new(cells);
        let p = dist.prob(&invalid);
        prop_assert!(p == 0.0 || p < 0.001,
            "Incompatible config should have ~zero probability: {}", p);
    }
}

// =============================================================================
// Dempster Combination Properties
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// With uniform pairwise probabilities (0.5), entropy should be positive.
    #[test]
    fn prop_uniform_priors_positive_entropy(n in 2usize..=4) {
        let builder = ConfigurationBuilder::new(n);
        // All pairwise default to 0.5
        let dist = builder.build_evidential();

        let entropy = dist.entropy();
        prop_assert!(entropy > 0.0, "Entropy should be positive: {}", entropy);
    }

    /// High pairwise probability increases probability of merged configuration.
    #[test]
    fn prop_high_pairwise_favors_merge(prob in 0.7f64..=0.99f64) {
        let mut builder = ConfigurationBuilder::new(2);
        builder.set_pairwise(0, 1, prob);

        let dist = builder.build_evidential();

        let merged = CorefConfiguration::all_same(2);
        let separate = CorefConfiguration::all_singletons(2);

        let p_merged = dist.prob(&merged);
        let p_separate = dist.prob(&separate);

        prop_assert!(p_merged > p_separate,
            "High pairwise ({}) should favor merge: P(merged)={} vs P(sep)={}",
            prob, p_merged, p_separate);
    }
}

// =============================================================================
// E2E Test: Kehler's Example Consistency
// =============================================================================

/// Test that Kehler's example from the paper produces consistent results.
/// This verifies our implementation matches the paper's analysis.
#[test]
fn e2e_kehler_example_properties() {
    // Setup from Kehler (1997) Section 2
    // Templates A, B, C, D (indices 0, 1, 2, 3)
    // A-C and B-C incompatible (type mismatch: Rail vs Ammunition)
    // Pairwise: P(A~B)=0.671, P(A~D)=0.505, P(B~D)=0.752, P(C~D)=0.504

    let mut builder = ConfigurationBuilder::new(4);
    builder
        .set_pairwise(0, 1, 0.671)
        .set_pairwise(0, 3, 0.505)
        .set_pairwise(1, 3, 0.752)
        .set_pairwise(2, 3, 0.504)
        .set_incompatible(0, 2)
        .set_incompatible(1, 2);

    let dist = builder.build_evidential();

    // Property 1: Mode should be (A B D)(C)
    let mode = dist.mode().expect("should have mode");
    let expected_mode = CorefConfiguration::new(vec![vec![0, 1, 3], vec![2]]);
    assert_eq!(*mode, expected_mode, "Mode should be (ABD)(C)");

    // Property 2: Invalid configs have zero probability
    let invalid = CorefConfiguration::new(vec![vec![0, 2], vec![1], vec![3]]);
    let p_invalid = dist.prob(&invalid);
    assert!(
        p_invalid < 0.001,
        "A~C config should have ~zero prob: {}",
        p_invalid
    );

    // Property 3: Cross-entropy with correct answer should be low
    let ce = dist.cross_entropy(&expected_mode);
    assert!(
        ce < 1.5,
        "Cross-entropy should be low for correct config: {}",
        ce
    );

    // Property 4: Kehler's paper result - (ABD)(C) should get high probability
    let mode_prob = dist.prob(&expected_mode);
    assert!(
        mode_prob > 0.2,
        "Mode should have substantial probability: {}",
        mode_prob
    );
}

/// Test transitivity enforcement via triad-style reasoning.
/// If P(A~B) high and P(B~C) high, evidential combination should increase P(A~C).
#[test]
fn e2e_transitivity_boost() {
    let mut builder = ConfigurationBuilder::new(3);
    builder
        .set_pairwise(0, 1, 0.9) // A~B very likely
        .set_pairwise(1, 2, 0.9) // B~C very likely
        .set_pairwise(0, 2, 0.5); // A~C uncertain

    let dist = builder.build_evidential();

    // The all-same configuration (A B C) should be most likely
    let all_same = CorefConfiguration::all_same(3);
    let mode = dist.mode().expect("should have mode");

    // Due to transitivity in evidential combination, A~C should be boosted
    // because both A~B and B~C are high
    assert_eq!(
        *mode, all_same,
        "Transitive evidence should favor all-same clustering"
    );
}
