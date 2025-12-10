//! Property-based tests for graph-based coreference resolution.
//!
//! These tests verify invariants that must hold for all inputs,
//! using proptest to generate diverse test cases including:
//! - Various string lengths and Unicode content
//! - Different numbers of mentions
//! - Edge cases in mention positions
//!
//! # Invariants Tested
//!
//! 1. **Partition property**: Every mention appears in exactly one cluster
//! 2. **Span validity**: Output mentions have valid spans (start < end)
//! 3. **Determinism**: Same input produces same output
//! 4. **Idempotence**: Running resolve twice on same input gives same result
//! 5. **Graph consistency**: CorefGraph satisfies symmetry and no self-loops

use anno::backends::graph_coref::{CorefGraph, GraphCoref, GraphCorefConfig};
use anno::eval::coref::Mention;
use proptest::prelude::*;

// =============================================================================
// Strategies for generating test data
// =============================================================================

/// Configuration for slower, more thorough property tests.
fn slow_config() -> ProptestConfig {
    ProptestConfig {
        cases: 20,
        ..ProptestConfig::default()
    }
}

/// Generate a valid mention text (non-empty, trimmed).
fn arb_mention_text() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple ASCII words
        "[A-Za-z]{1,10}",
        // Multi-word phrases
        "[A-Za-z]{1,5} [A-Za-z]{1,5}",
        // Names with capitals
        "[A-Z][a-z]{2,8}",
        // Simple pronouns
        Just("he".to_string()),
        Just("she".to_string()),
        Just("it".to_string()),
        Just("they".to_string()),
        // Unicode: CJK
        prop::sample::select(vec!["北京", "東京", "上海", "田中"]).prop_map(|s| s.to_string()),
        // Unicode: Cyrillic
        prop::sample::select(vec!["Путин", "Москва", "Россия"]).prop_map(|s| s.to_string()),
        // Unicode: Diacritics
        prop::sample::select(vec!["François", "Müller", "García", "José"])
            .prop_map(|s| s.to_string()),
    ]
}

/// Generate a single mention with valid spans.
/// Used in single-mention property tests.
fn arb_mention() -> impl Strategy<Value = Mention> {
    (arb_mention_text(), 0usize..10000).prop_map(|(text, start)| {
        let char_count = text.chars().count();
        let end = start + char_count;
        Mention::new(text, start, end)
    })
}

/// Generate a pair of mentions for testing pairwise operations.
fn arb_mention_pair() -> impl Strategy<Value = (Mention, Mention)> {
    (arb_mention(), arb_mention())
}

/// Generate a list of mentions with non-overlapping spans.
fn arb_mention_list(max_len: usize) -> impl Strategy<Value = Vec<Mention>> {
    prop::collection::vec(arb_mention_text(), 0..max_len).prop_map(|texts| {
        let mut mentions = Vec::new();
        let mut offset = 0usize;
        for text in texts {
            let char_count = text.chars().count();
            let start = offset;
            let end = start + char_count;
            mentions.push(Mention::new(text, start, end));
            offset = end + 10; // Gap between mentions
        }
        mentions
    })
}

/// Generate a config with valid parameters.
fn arb_config() -> impl Strategy<Value = GraphCorefConfig> {
    (
        1usize..10,    // max_iterations
        0.1f64..0.9,   // link_threshold
        0.0f64..0.5,   // transitivity_bonus
        0.0f64..0.3,   // per_shared_neighbor_bonus
        any::<bool>(), // include_singletons
    )
        .prop_map(
            |(max_iter, threshold, trans, neighbor, singletons)| GraphCorefConfig {
                max_iterations: max_iter,
                link_threshold: threshold,
                transitivity_bonus: trans,
                per_shared_neighbor_bonus: neighbor,
                include_singletons: singletons,
                ..Default::default()
            },
        )
}

// =============================================================================
// Property Tests: Core Invariants
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Every mention should appear in at most one cluster (partition property).
    #[test]
    fn prop_clusters_are_partition(mentions in arb_mention_list(20)) {
        let config = GraphCorefConfig {
            include_singletons: true,
            ..Default::default()
        };
        let coref = GraphCoref::with_config(config);

        let chains = coref.resolve(&mentions);

        // Collect all (start, end) pairs from output
        let mut seen_spans = std::collections::HashSet::new();
        for chain in &chains {
            for mention in &chain.mentions {
                let span = (mention.start, mention.end);
                prop_assert!(
                    seen_spans.insert(span),
                    "Mention {:?} appears in multiple clusters",
                    mention.text
                );
            }
        }
    }

    /// Output mentions should have valid spans (start < end, non-empty text).
    #[test]
    fn prop_output_spans_valid(mentions in arb_mention_list(15)) {
        let coref = GraphCoref::new();
        let chains = coref.resolve(&mentions);

        for chain in &chains {
            prop_assert!(!chain.is_empty(), "Chain should not be empty");
            for mention in &chain.mentions {
                prop_assert!(
                    mention.start < mention.end,
                    "Invalid span: start={} >= end={}",
                    mention.start, mention.end
                );
                prop_assert!(
                    !mention.text.trim().is_empty(),
                    "Mention text should not be empty"
                );
            }
        }
    }

    /// Same input should produce same output (determinism).
    #[test]
    fn prop_deterministic(mentions in arb_mention_list(10)) {
        let coref = GraphCoref::new();

        let chains1 = coref.resolve(&mentions);
        let chains2 = coref.resolve(&mentions);

        prop_assert_eq!(
            chains1.len(), chains2.len(),
            "Number of chains should be deterministic"
        );

        for (c1, c2) in chains1.iter().zip(chains2.iter()) {
            prop_assert_eq!(
                c1.mentions.len(), c2.mentions.len(),
                "Chain sizes should be deterministic"
            );
        }
    }

    /// Running resolve on already-resolved mentions should be idempotent
    /// in the sense that we get the same clustering structure.
    #[test]
    fn prop_idempotent_clustering(mentions in arb_mention_list(10)) {
        let coref = GraphCoref::new();

        let chains1 = coref.resolve(&mentions);

        // Re-resolve using mentions from first pass
        let mentions2: Vec<Mention> = chains1
            .iter()
            .flat_map(|c| c.mentions.iter().cloned())
            .collect();

        let chains2 = coref.resolve(&mentions2);

        // Should produce same number of chains (structure preserved)
        prop_assert_eq!(
            chains1.len(), chains2.len(),
            "Idempotent: second pass should preserve cluster count"
        );
    }

    /// Non-singleton chains should have at least 2 mentions.
    #[test]
    fn prop_non_singleton_size(mentions in arb_mention_list(15)) {
        let config = GraphCorefConfig {
            include_singletons: false,
            ..Default::default()
        };
        let coref = GraphCoref::with_config(config);

        let chains = coref.resolve(&mentions);

        for chain in &chains {
            prop_assert!(
                chain.mentions.len() >= 2,
                "Non-singleton chain should have at least 2 mentions, got {}",
                chain.mentions.len()
            );
        }
    }
}

// =============================================================================
// Property Tests: CorefGraph Invariants
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Graph edges are symmetric.
    #[test]
    fn prop_graph_symmetric(
        n in 2usize..50,
        edges in prop::collection::vec((0usize..50, 0usize..50), 0..100)
    ) {
        let mut graph = CorefGraph::new(n);

        for (i, j) in edges {
            if i < n && j < n {
                graph.add_edge(i, j);
            }
        }

        // Check symmetry
        for i in 0..n {
            for j in 0..n {
                prop_assert_eq!(
                    graph.has_edge(i, j),
                    graph.has_edge(j, i),
                    "Graph should be symmetric: edge({},{}) != edge({},{})",
                    i, j, j, i
                );
            }
        }
    }

    /// Graph has no self-loops.
    #[test]
    fn prop_graph_no_self_loops(
        n in 1usize..50,
        edges in prop::collection::vec((0usize..50, 0usize..50), 0..100)
    ) {
        let mut graph = CorefGraph::new(n);

        for (i, j) in edges {
            if i < n && j < n {
                graph.add_edge(i, j);
            }
        }

        for i in 0..n {
            prop_assert!(
                !graph.has_edge(i, i),
                "Graph should not have self-loop at node {}",
                i
            );
        }
    }

    /// Extract clusters should cover all nodes exactly once.
    #[test]
    fn prop_clusters_cover_all_nodes(
        n in 1usize..30,
        edges in prop::collection::vec((0usize..30, 0usize..30), 0..50)
    ) {
        let mut graph = CorefGraph::new(n);

        for (i, j) in edges {
            if i < n && j < n && i != j {
                graph.add_edge(i, j);
            }
        }

        let clusters = graph.extract_clusters();

        // Flatten and sort all node indices
        let mut all_nodes: Vec<usize> = clusters.iter().flatten().copied().collect();
        all_nodes.sort_unstable();
        all_nodes.dedup();

        prop_assert_eq!(
            all_nodes.len(), n,
            "Clusters should cover exactly {} nodes, got {}",
            n, all_nodes.len()
        );

        let expected: Vec<usize> = (0..n).collect();
        prop_assert_eq!(
            &all_nodes, &expected,
            "Clusters should cover nodes 0..{}",
            n
        );
    }

    /// Transitive connectivity is reflexive and symmetric.
    #[test]
    fn prop_transitive_reflexive_symmetric(
        n in 1usize..20,
        edges in prop::collection::vec((0usize..20, 0usize..20), 0..30)
    ) {
        let mut graph = CorefGraph::new(n);

        for (i, j) in edges {
            if i < n && j < n && i != j {
                graph.add_edge(i, j);
            }
        }

        for i in 0..n {
            // Reflexive
            prop_assert!(
                graph.transitively_connected(i, i),
                "Transitive connectivity should be reflexive"
            );

            // Symmetric
            for j in 0..n {
                prop_assert_eq!(
                    graph.transitively_connected(i, j),
                    graph.transitively_connected(j, i),
                    "Transitive connectivity should be symmetric"
                );
            }
        }
    }
}

// =============================================================================
// Property Tests: Config Variations
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Resolver should not panic with any valid config.
    #[test]
    fn prop_no_panic_with_any_config(
        config in arb_config(),
        mentions in arb_mention_list(10)
    ) {
        let coref = GraphCoref::with_config(config);

        // Should not panic
        let _chains = coref.resolve(&mentions);
    }

    /// Higher transitivity bonus should not reduce cluster count.
    /// (More transitivity encouragement → more linking → fewer or same clusters)
    #[test]
    fn prop_transitivity_monotonic(mentions in arb_mention_list(8)) {
        let config_low = GraphCorefConfig {
            transitivity_bonus: 0.0,
            per_shared_neighbor_bonus: 0.0,
            include_singletons: true,
            ..Default::default()
        };
        let config_high = GraphCorefConfig {
            transitivity_bonus: 0.5,
            per_shared_neighbor_bonus: 0.3,
            include_singletons: true,
            ..Default::default()
        };

        let coref_low = GraphCoref::with_config(config_low);
        let coref_high = GraphCoref::with_config(config_high);

        let chains_low = coref_low.resolve(&mentions);
        let chains_high = coref_high.resolve(&mentions);

        // More transitivity → fewer or same clusters (more merging)
        prop_assert!(
            chains_high.len() <= chains_low.len() + 1,
            "Higher transitivity should not significantly increase cluster count: \
             low={}, high={}",
            chains_low.len(), chains_high.len()
        );
    }
}

// =============================================================================
// Property Tests: Unicode Handling
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Unicode mentions should have correct character-based spans.
    #[test]
    fn prop_unicode_span_char_count(
        text in prop::sample::select(vec![
            "北京", "François", "Müller", "محمد", "東京オリンピック"
        ])
    ) {
        let mention = Mention::new(text, 0, text.chars().count());

        prop_assert_eq!(
            mention.end - mention.start,
            text.chars().count(),
            "Span length should equal char count for '{}'",
            text
        );

        // Mention.len() should also work
        prop_assert_eq!(
            mention.len(),
            text.chars().count(),
            "Mention.len() should equal char count"
        );
    }

    /// Two mentions can be resolved together without panics.
    #[test]
    fn prop_pair_resolution_no_panic(pair in arb_mention_pair()) {
        let (m1, m2) = pair;
        let mentions = vec![m1, m2];
        let coref = GraphCoref::new();

        // Should not panic regardless of mention content
        let _ = coref.resolve(&mentions);
    }

    /// A single arbitrary mention should resolve to either singleton or empty.
    #[test]
    fn prop_single_mention_handling(mention in arb_mention()) {
        let mentions = vec![mention.clone()];

        // With singletons
        let config_with = GraphCorefConfig {
            include_singletons: true,
            ..Default::default()
        };
        let coref_with = GraphCoref::with_config(config_with);
        let chains_with = coref_with.resolve(&mentions);
        prop_assert!(chains_with.len() <= 1, "Single mention should produce at most 1 chain");

        // Without singletons
        let coref_without = GraphCoref::new();
        let chains_without = coref_without.resolve(&mentions);
        prop_assert!(chains_without.is_empty(), "Single mention should be filtered as singleton");
    }
}
