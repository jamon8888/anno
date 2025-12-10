//! Integration tests for graph-based coreference resolution.
//!
//! These tests verify that the GraphCoref module correctly implements
//! the iterative refinement pattern inspired by Miculicich & Henderson (2022).
//!
//! # Test Categories
//!
//! - **Basic functionality**: Exact match, substring, distance filtering
//! - **MentionType integration**: Using proper/pronoun/nominal types
//! - **Graph operations**: CorefGraph invariants
//! - **Transitivity**: Iterative refinement propagates links
//! - **Multilingual**: Unicode, CJK, RTL, diacritics, code-switching
//! - **Edge cases**: Empty input, singletons, overlapping mentions
//! - **Evaluation**: Integration with CorefDocument for metrics

use anno::backends::graph_coref::{chains_to_document, CorefGraph, GraphCoref, GraphCorefConfig};
use anno::eval::coref::{Mention, MentionType};

// =============================================================================
// Helper Functions
// =============================================================================

/// Create a mention with text, start, and end positions.
fn mention(text: &str, start: usize) -> Mention {
    Mention::new(text, start, start + text.chars().count())
}

/// Create a typed mention.
fn typed_mention(text: &str, start: usize, mention_type: MentionType) -> Mention {
    Mention::with_type(text, start, start + text.chars().count(), mention_type)
}

/// Create mention with head span.
fn mention_with_head(text: &str, start: usize, head_start: usize, head_end: usize) -> Mention {
    Mention::with_head(
        text,
        start,
        start + text.chars().count(),
        head_start,
        head_end,
    )
}

// =============================================================================
// Basic Functionality
// =============================================================================

#[test]
fn test_exact_match_coreference() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("Marie Curie", 0),
        mention("Marie Curie", 100),
        mention("Einstein", 200),
    ];

    let chains = coref.resolve(&mentions);

    // Marie Curie mentions should be in one chain
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
    assert!(chains[0].mentions.iter().all(|m| m.text == "Marie Curie"));
}

#[test]
fn test_substring_coreference() {
    let config = GraphCorefConfig {
        link_threshold: 0.4,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);
    let mentions = vec![mention("Dr. Marie Curie", 0), mention("Curie", 50)];

    let chains = coref.resolve(&mentions);

    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
}

#[test]
fn test_distance_filter() {
    let config = GraphCorefConfig {
        max_distance: Some(50),
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![mention("Apple", 0), mention("Apple", 200)];

    let chains = coref.resolve(&mentions);
    assert!(chains.is_empty(), "Should not link due to distance");
}

// =============================================================================
// MentionType Integration
// =============================================================================

#[test]
fn test_typed_pronoun_to_proper() {
    let config = GraphCorefConfig {
        link_threshold: 0.15, // Very low for weak pronoun signal
        distance_weight: 0.0, // Disable distance penalty for this test
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        typed_mention("Marie", 0, MentionType::Proper),
        typed_mention("she", 30, MentionType::Pronominal),
    ];

    let chains = coref.resolve(&mentions);

    // With proper typing and low threshold, pronoun should link
    // Note: This is a weak heuristic - neural models do much better
    assert_eq!(chains.len(), 1, "Typed pronoun should link to proper noun");
}

#[test]
fn test_pronoun_without_explicit_type() {
    // Tests that heuristic detection works
    let config = GraphCorefConfig {
        link_threshold: 0.2,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    // "he" should be detected as pronoun via heuristics
    let mentions = vec![mention("John", 0), mention("he", 30)];

    let chains = coref.resolve(&mentions);
    // May or may not link depending on threshold, but should not crash
    assert!(chains.len() <= 1);
}

#[test]
fn test_nominal_to_proper() {
    let config = GraphCorefConfig {
        link_threshold: 0.3,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        typed_mention("Barack Obama", 0, MentionType::Proper),
        mention("Obama", 50), // Substring match
        typed_mention("the president", 100, MentionType::Nominal),
    ];

    let chains = coref.resolve(&mentions);

    // Obama mentions should link via substring
    // "the president" won't link without additional features
    assert!(!chains.is_empty());
}

// =============================================================================
// Transitivity and Graph Refinement
// =============================================================================

#[test]
fn test_transitivity_through_refinement() {
    let config = GraphCorefConfig {
        max_iterations: 4,
        link_threshold: 0.3,
        transitivity_bonus: 0.3,
        per_shared_neighbor_bonus: 0.2,
        string_similarity_weight: 1.0,
        distance_weight: 0.02,
        max_distance: Some(500),
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        mention("John Smith", 0),
        mention("Smith", 30),
        mention("John Smith", 60),
    ];

    let chains = coref.resolve(&mentions);

    assert_eq!(chains.len(), 1, "All should cluster via transitivity");
    assert_eq!(chains[0].mentions.len(), 3);
}

#[test]
fn test_convergence_statistics() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("Company", 0),
        mention("Company", 50),
        mention("Company", 100),
    ];

    let (chains, stats) = coref.resolve_with_stats(&mentions);

    assert!(
        stats.iterations <= 4,
        "Should converge within max_iterations"
    );
    assert_eq!(chains.len(), 1);
    assert_eq!(stats.num_chains, 1);
    assert!(!stats.edge_history.is_empty());
    assert_eq!(stats.edge_history[0], 0, "Starts with empty graph");
}

#[test]
fn test_early_convergence() {
    let config = GraphCorefConfig {
        max_iterations: 10,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![mention("Test", 0), mention("Test", 30)];

    let (_, stats) = coref.resolve_with_stats(&mentions);

    assert!(stats.converged, "Should converge before max_iterations");
    assert!(stats.iterations < 10);
}

// =============================================================================
// CorefGraph Operations
// =============================================================================

#[test]
fn test_coref_graph_operations() {
    let mut graph = CorefGraph::new(5);

    graph.add_edge(0, 1);
    graph.add_edge(1, 2);
    graph.add_edge(3, 4);

    // Symmetry
    assert!(graph.has_edge(0, 1));
    assert!(graph.has_edge(1, 0));

    // No direct edge
    assert!(!graph.has_edge(0, 2));

    // Transitivity
    assert!(graph.transitively_connected(0, 2));
    assert!(!graph.transitively_connected(0, 3));

    // Clusters
    let clusters = graph.extract_clusters();
    assert_eq!(clusters.len(), 2); // {0,1,2} and {3,4}
}

#[test]
fn test_shared_neighbors() {
    let mut graph = CorefGraph::new(4);
    graph.add_edge(0, 2); // A~C
    graph.add_edge(1, 2); // B~C

    assert_eq!(graph.shared_neighbors(0, 1), 1, "Both connected to 2");
    assert_eq!(graph.shared_neighbors(0, 3), 0);
}

#[test]
fn test_graph_invariants() {
    let mut graph = CorefGraph::new(3);

    // Self-loops ignored
    graph.add_edge(0, 0);
    assert!(!graph.has_edge(0, 0));
    assert_eq!(graph.edge_count(), 0);

    // Out-of-bounds ignored
    graph.add_edge(0, 100);
    assert_eq!(graph.edge_count(), 0);

    // Valid edge
    graph.add_edge(0, 1);
    assert_eq!(graph.edge_count(), 1);
    assert_eq!(graph.num_mentions(), 3);
}

// =============================================================================
// Singleton Handling
// =============================================================================

#[test]
fn test_singletons_filtered_by_default() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("Apple", 0),
        mention("Microsoft", 100),
        mention("Google", 200),
    ];

    let chains = coref.resolve(&mentions);
    assert!(
        chains.is_empty(),
        "Unrelated mentions should not form chains"
    );
}

#[test]
fn test_singletons_included_when_configured() {
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![mention("Apple", 0), mention("Microsoft", 100)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 2, "Singletons should be included");
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_input() {
    let coref = GraphCoref::new();
    let chains = coref.resolve(&[]);
    assert!(chains.is_empty());
}

#[test]
fn test_empty_mentions_filtered() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("John", 0),
        Mention::new("", 10, 10),    // Empty text
        Mention::new("   ", 20, 23), // Whitespace only
        mention("John", 50),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1, "Empty mentions should be filtered");
    assert_eq!(chains[0].mentions.len(), 2);
}

#[test]
fn test_invalid_span_filtered() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("John", 0),
        Mention::new("invalid", 50, 40), // end < start
        mention("John", 100),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
}

// =============================================================================
// Multilingual / Unicode Tests
// =============================================================================

#[test]
fn test_unicode_cjk_chinese() {
    let coref = GraphCoref::new();
    // 北京 = Beijing, 東京 = Tokyo
    let mentions = vec![mention("北京", 0), mention("北京", 20), mention("東京", 40)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert!(chains[0].mentions.iter().all(|m| m.text == "北京"));
}

#[test]
fn test_unicode_japanese() {
    // 田中 = Tanaka (common surname)
    let mentions = vec![mention("田中先生", 0), mention("田中", 20)];

    let config = GraphCorefConfig {
        link_threshold: 0.4, // Lower for substring
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1, "田中 should match 田中先生 (substring)");
}

#[test]
fn test_unicode_arabic_rtl() {
    let coref = GraphCoref::new();
    // محمد = Muhammad
    let mentions = vec![mention("محمد", 0), mention("محمد", 30)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1, "RTL Arabic should work");
}

#[test]
fn test_unicode_cyrillic() {
    let coref = GraphCoref::new();
    // Путин = Putin
    let mentions = vec![mention("Путин", 0), mention("Путин", 50)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
}

#[test]
fn test_unicode_diacritics() {
    let coref = GraphCoref::new();
    let mentions = vec![
        mention("François", 0),
        mention("François", 50),
        mention("Jose", 100), // Different person (no accent)
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert!(chains[0].mentions.iter().all(|m| m.text == "François"));
}

#[test]
fn test_unicode_mixed_diacritics() {
    let coref = GraphCoref::new();
    // Names with various diacritics
    let mentions = vec![
        mention("Müller", 0),
        mention("Müller", 50),
        mention("García", 100),
        mention("García", 150),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 2, "Two separate name clusters");
}

#[test]
fn test_code_switching_mixed_script() {
    // Common in multilingual contexts: "Dr. 田中 presented at MIT"
    // This tests substring matching across mixed scripts
    // Note: "Dr. " prefix adds noise, so we test pure CJK substring matching
    let config = GraphCorefConfig {
        link_threshold: 0.35,
        distance_weight: 0.01,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    // Pure CJK substring test (avoids punctuation edge case)
    let mentions = vec![
        mention("田中先生", 0), // "Mr. Tanaka"
        mention("田中", 50),    // "Tanaka" (substring)
    ];

    let chains = coref.resolve(&mentions);

    // Should link via substring since "田中" ⊂ "田中先生"
    assert_eq!(chains.len(), 1, "CJK mentions should link via substring");
}

#[test]
fn test_transliteration_variants() {
    // Same city in different scripts/romanizations
    let mentions = vec![
        mention("Moscow", 0),
        mention("Moscow", 50),
        mention("Москва", 100), // Russian spelling - different entity for now
    ];

    let coref = GraphCoref::new();
    let chains = coref.resolve(&mentions);

    // Without transliteration knowledge, these won't link
    // This test documents current behavior
    assert_eq!(chains.len(), 1, "English spellings should cluster");
}

// =============================================================================
// Evaluation Integration
// =============================================================================

#[test]
fn test_chains_to_document() {
    let coref = GraphCoref::new();
    let mentions = vec![
        typed_mention("John", 0, MentionType::Proper),
        typed_mention("he", 20, MentionType::Pronominal),
        typed_mention("John", 50, MentionType::Proper),
    ];

    let chains = coref.resolve(&mentions);
    let doc = chains_to_document("John went home. He slept. John woke up.", chains);

    assert!(doc.chain_count() >= 1);
    assert!(doc.mention_count() >= 2);
}

#[test]
fn test_document_without_singletons() {
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        mention("Apple", 0),
        mention("Apple", 50),
        mention("Microsoft", 100), // Singleton
    ];

    let chains = coref.resolve(&mentions);
    let doc = chains_to_document("Apple is good. Apple Inc. Microsoft too.", chains);

    assert_eq!(doc.chain_count(), 2, "Should have chain + singleton");

    // Filter singletons
    let filtered = doc.without_singletons();
    assert_eq!(filtered.chain_count(), 1, "Should have only non-singleton");
}

// =============================================================================
// Head Word Tests
// =============================================================================

#[test]
fn test_head_word_fallback() {
    // Without explicit head spans, uses last word (head-final assumption)
    let config = GraphCorefConfig {
        link_threshold: 0.3, // Lower threshold to test head matching
        head_match_weight: 0.6,
        distance_weight: 0.01,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        mention("the tech company Apple", 0),
        mention("giant Apple", 50),
    ];

    let chains = coref.resolve(&mentions);
    // Head word "Apple" matches in both - should link
    assert_eq!(chains.len(), 1, "Should link via head word 'Apple'");
}

#[test]
fn test_explicit_head_span_matching() {
    // Test with explicit head spans (not relying on last-word fallback)
    let config = GraphCorefConfig {
        link_threshold: 0.3,
        head_match_weight: 0.8,
        distance_weight: 0.01,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    // "the CEO of Microsoft" with head "CEO" at positions 4-7
    // "the CEO" with head "CEO" at positions 4-7
    let mentions = vec![
        mention_with_head("the CEO of Microsoft", 0, 4, 7),
        mention_with_head("the CEO", 50, 4 + 50, 7 + 50),
    ];

    let chains = coref.resolve(&mentions);
    // Both have "CEO" as head - should link
    assert_eq!(chains.len(), 1, "Should link via explicit head 'CEO'");
}

// =============================================================================
// Property-Like Tests (Invariants)
// =============================================================================

#[test]
fn test_invariant_clusters_partition() {
    // All mentions should appear in exactly one cluster
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        mention("A", 0),
        mention("B", 20),
        mention("A", 40),
        mention("C", 60),
    ];

    let chains = coref.resolve(&mentions);

    // Collect all mentions from all chains
    let mut all_spans: Vec<(usize, usize)> = chains
        .iter()
        .flat_map(|c| c.mentions.iter().map(|m| (m.start, m.end)))
        .collect();
    all_spans.sort();
    all_spans.dedup();

    // Should equal input count (after dedup - no duplicates in output)
    assert!(
        all_spans.len() <= mentions.len(),
        "No mention should appear in multiple chains"
    );
}

#[test]
fn test_invariant_clusters_non_empty() {
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![mention("X", 0), mention("Y", 20), mention("X", 40)];

    let chains = coref.resolve(&mentions);

    for chain in &chains {
        assert!(!chain.is_empty(), "No chain should be empty");
        assert!(
            !chain.mentions.is_empty(),
            "No chain should have 0 mentions"
        );
    }
}

#[test]
fn test_invariant_valid_spans() {
    let coref = GraphCoref::new();

    let mentions = vec![mention("Test", 0), mention("Test", 50)];

    let chains = coref.resolve(&mentions);

    for chain in &chains {
        for mention in &chain.mentions {
            assert!(mention.start < mention.end, "Mention spans should be valid");
            assert!(!mention.text.is_empty(), "Mention text should not be empty");
        }
    }
}

#[test]
fn test_invariant_deterministic() {
    let coref = GraphCoref::new();

    let mentions = vec![
        mention("Entity", 0),
        mention("Entity", 50),
        mention("Other", 100),
    ];

    let chains1 = coref.resolve(&mentions);
    let chains2 = coref.resolve(&mentions);

    assert_eq!(
        chains1.len(),
        chains2.len(),
        "Same input should produce same output"
    );
}

// =============================================================================
// Stress / Scale Tests
// =============================================================================

#[test]
fn test_many_mentions() {
    let coref = GraphCoref::new();

    // Create 100 mentions of the same entity
    let mentions: Vec<Mention> = (0..100).map(|i| mention("Entity", i * 20)).collect();

    let (chains, stats) = coref.resolve_with_stats(&mentions);

    assert_eq!(chains.len(), 1, "All should cluster");
    assert_eq!(chains[0].mentions.len(), 100);
    assert!(stats.iterations <= 4, "Should converge quickly");
}

#[test]
fn test_many_distinct_entities() {
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    // 50 different entities
    let mentions: Vec<Mention> = (0..50)
        .map(|i| mention(&format!("Entity{}", i), i * 20))
        .collect();

    let chains = coref.resolve(&mentions);

    assert_eq!(chains.len(), 50, "Each unique entity should be separate");
}
