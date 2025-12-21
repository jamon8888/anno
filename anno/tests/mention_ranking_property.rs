//! Property-based tests for mention-ranking coreference resolution.
//!
//! These tests verify invariants that must hold for all inputs,
//! using proptest to generate diverse test cases including:
//! - Various string lengths and Unicode content
//! - Different configurations (clustering strategies, thresholds)
//! - Edge cases in mention positions and types
//!
//! # Invariants Tested
//!
//! 1. **Span validity**: Output mentions have valid spans (start <= end)
//! 2. **Span bounds**: All spans are within document character length
//! 3. **Determinism**: Same input produces same output
//! 4. **Config monotonicity**: Higher thresholds -> fewer/equal clusters
//! 5. **Unicode safety**: Works correctly with multilingual text

use anno::backends::mention_ranking::{
    ClusteringStrategy, MentionRankingConfig, MentionRankingCoref,
};
use proptest::prelude::*;

// =============================================================================
// Configuration for property tests
// =============================================================================

/// Configuration for slower, more thorough property tests.
fn slow_config() -> ProptestConfig {
    ProptestConfig {
        cases: 20,
        ..ProptestConfig::default()
    }
}

// =============================================================================
// Strategies for generating test data
// =============================================================================

/// Generate diverse multilingual text samples.
fn arb_multilingual_text() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple English sentences
        Just("John went to the store. He bought some milk.".to_string()),
        Just("Mary and John met at the park. She was happy to see him.".to_string()),
        Just("The cat sat on the mat. It was warm.".to_string()),
        // CJK text with pronouns
        Just("北京是中國的首都。它很美麗。".to_string()),
        Just("田中さんは東京に住んでいます。彼は教師です。".to_string()),
        // European languages with diacritics
        Just("François rencontra Marie. Il était content de la voir.".to_string()),
        Just("Müller ging nach Berlin. Er besuchte das Museum.".to_string()),
        Just("José fue a Madrid. Él compró un libro.".to_string()),
        // Cyrillic
        Just("Путин встретился с Си Цзиньпином. Он был доволен.".to_string()),
        // Mixed scripts
        Just("Dr. 田中 presented at MIT. He was nervous.".to_string()),
        Just("Angela Merkel met 習近平 in Berlin. She discussed trade.".to_string()),
        // Arabic (RTL)
        Just("محمد ذهب إلى السوق. هو اشترى خبزاً.".to_string()),
        // Empty and minimal
        Just("".to_string()),
        Just("Hello".to_string()),
        Just("A".to_string()),
        // Just pronouns
        Just("He saw her. She smiled.".to_string()),
        // No pronouns (proper nouns only)
        Just("John met Mary. Mary greeted John.".to_string()),
        // Long proper noun chains
        Just("President Biden met Chancellor Scholz. Biden thanked Scholz.".to_string()),
    ]
}

/// Generate a valid config with random parameters.
fn arb_config() -> impl Strategy<Value = MentionRankingConfig> {
    (
        0.1f64..0.9,   // link_threshold
        10usize..100,  // pronoun_max_antecedents
        50usize..500,  // proper_max_antecedents
        any::<bool>(), // enable_global_proper_coref
        0.5f64..0.95,  // global_proper_threshold
        prop::sample::select(vec![
            ClusteringStrategy::LeftToRight,
            ClusteringStrategy::EasyFirst,
        ]),
        any::<bool>(), // use_non_coref_constraints
    )
        .prop_map(
            |(
                link_threshold,
                pronoun_max,
                proper_max,
                global_proper,
                global_threshold,
                strategy,
                non_coref,
            )| {
                MentionRankingConfig {
                    link_threshold,
                    pronoun_max_antecedents: pronoun_max,
                    proper_max_antecedents: proper_max,
                    nominal_max_antecedents: proper_max,
                    enable_global_proper_coref: global_proper,
                    global_proper_threshold: global_threshold,
                    clustering_strategy: strategy,
                    use_non_coref_constraints: non_coref,
                    ..Default::default()
                }
            },
        )
}

// =============================================================================
// Property Tests: Core Invariants
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Output mentions should have valid spans (start <= end).
    #[test]
    fn prop_output_spans_valid(text in arb_multilingual_text()) {
        let coref = MentionRankingCoref::new();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    prop_assert!(
                        mention.start <= mention.end,
                        "Invalid span: start={} > end={} for mention '{}'",
                        mention.start, mention.end, mention.text
                    );
                }
            }
        }
    }

    /// All spans should be within document character length.
    #[test]
    fn prop_spans_within_bounds(text in arb_multilingual_text()) {
        let coref = MentionRankingCoref::new();
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    prop_assert!(
                        mention.end <= char_count,
                        "Span end {} exceeds doc length {} (chars) for mention '{}'",
                        mention.end, char_count, mention.text
                    );
                }
            }
        }
    }

    /// Same input should produce same output (determinism).
    #[test]
    fn prop_deterministic(text in arb_multilingual_text()) {
        let coref = MentionRankingCoref::new();

        let result1 = coref.resolve(&text);
        let result2 = coref.resolve(&text);

        match (result1, result2) {
            (Ok(clusters1), Ok(clusters2)) => {
                prop_assert_eq!(
                    clusters1.len(), clusters2.len(),
                    "Number of clusters should be deterministic"
                );

                // Check cluster sizes match
                for (c1, c2) in clusters1.iter().zip(clusters2.iter()) {
                    prop_assert_eq!(
                        c1.mentions.len(), c2.mentions.len(),
                        "Cluster sizes should be deterministic"
                    );
                }
            }
            (Err(_), Err(_)) => {
                // Both failed consistently - that's deterministic
            }
            _ => {
                prop_assert!(false, "Results should be consistently Ok or Err");
            }
        }
    }

    /// Resolver should not panic with any valid config.
    #[test]
    fn prop_no_panic_with_any_config(
        config in arb_config(),
        text in arb_multilingual_text()
    ) {
        let coref = MentionRankingCoref::with_config(config);

        // Should not panic regardless of config/text combination
        let _ = coref.resolve(&text);
    }

    /// Unicode character positions should be consistent.
    /// The mention text should match what's at the span position.
    #[test]
    fn prop_mention_text_matches_span(text in arb_multilingual_text()) {
        let coref = MentionRankingCoref::new();
        let text_chars: Vec<char> = text.chars().collect();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    if mention.start < text_chars.len() && mention.end <= text_chars.len() {
                        let extracted: String = text_chars[mention.start..mention.end].iter().collect();

                        // Allow case differences (pronouns get lowercased)
                        prop_assert!(
                            mention.text.to_lowercase() == extracted.to_lowercase()
                            || mention.text == extracted,
                            "Mention text '{}' doesn't match extracted '{}' at span {}..{}",
                            mention.text, extracted, mention.start, mention.end
                        );
                    }
                }
            }
        }
    }
}

// =============================================================================
// Property Tests: Configuration Effects
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Higher link thresholds should produce fewer or equal clusters.
    /// (Harder to link -> more singletons -> roughly same cluster count
    /// but this is a soft property due to cascading effects)
    #[test]
    fn prop_threshold_effect(text in arb_multilingual_text()) {
        let config_low = MentionRankingConfig {
            link_threshold: 0.1,
            ..Default::default()
        };
        let config_high = MentionRankingConfig {
            link_threshold: 0.9,
            ..Default::default()
        };

        let coref_low = MentionRankingCoref::with_config(config_low);
        let coref_high = MentionRankingCoref::with_config(config_high);

        if let (Ok(clusters_low), Ok(clusters_high)) = (coref_low.resolve(&text), coref_high.resolve(&text)) {
            // Count total mentions in non-singleton clusters
            let linked_low: usize = clusters_low.iter()
                .filter(|c| c.mentions.len() > 1)
                .map(|c| c.mentions.len())
                .sum();
            let linked_high: usize = clusters_high.iter()
                .filter(|c| c.mentions.len() > 1)
                .map(|c| c.mentions.len())
                .sum();

            // Higher threshold should link fewer or equal mentions
            // (Allow some tolerance due to cascading effects)
            prop_assert!(
                linked_high <= linked_low + 2,
                "Higher threshold should not link significantly more: low={}, high={}",
                linked_low, linked_high
            );
        }
    }

    /// EasyFirst and LeftToRight should both produce valid output.
    #[test]
    fn prop_both_strategies_valid(text in arb_multilingual_text()) {
        let config_ltr = MentionRankingConfig {
            clustering_strategy: ClusteringStrategy::LeftToRight,
            ..Default::default()
        };
        let config_ef = MentionRankingConfig {
            clustering_strategy: ClusteringStrategy::EasyFirst,
            ..Default::default()
        };

        let coref_ltr = MentionRankingCoref::with_config(config_ltr);
        let coref_ef = MentionRankingCoref::with_config(config_ef);

        // Both should produce valid results (or both fail)
        let result_ltr = coref_ltr.resolve(&text);
        let result_ef = coref_ef.resolve(&text);

        // If one succeeds, both should succeed
        if result_ltr.is_ok() {
            prop_assert!(result_ef.is_ok(), "EasyFirst should also succeed if LeftToRight succeeds");
        }
    }
}

// =============================================================================
// Property Tests: Unicode Edge Cases
// =============================================================================

/// Strategy for CJK prefix/suffix combinations.
fn arb_cjk_affixes() -> impl Strategy<Value = (String, String)> {
    (
        prop::sample::select(vec![
            "".to_string(),
            "前缀".to_string(),
            "プレフィックス".to_string(),
        ]),
        prop::sample::select(vec![
            "".to_string(),
            "后缀".to_string(),
            "サフィックス".to_string(),
        ]),
    )
}

proptest! {
    #![proptest_config(slow_config())]

    /// CJK text should not cause panics or invalid spans.
    #[test]
    fn prop_cjk_safe(affixes in arb_cjk_affixes()) {
        let (prefix, suffix) = affixes;
        let text = format!("{}北京是中國的首都{}", prefix, suffix);
        let coref = MentionRankingCoref::new();
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    prop_assert!(mention.start <= mention.end);
                    prop_assert!(mention.end <= char_count);
                }
            }
        }
    }
}

/// Emoji and special Unicode should not cause issues.
#[test]
fn test_emoji_safe() {
    let texts = vec![
        "John 🎉 met Mary 🎊. He was happy.",
        "The 北京 Olympics 🏅 were great. It was historic.",
        "👨‍👩‍👧‍👦 went to the park. They played.",
    ];

    for text in texts {
        let coref = MentionRankingCoref::new();
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    assert!(mention.start <= mention.end, "Invalid span in emoji text");
                    assert!(
                        mention.end <= char_count,
                        "Span exceeds length in emoji text"
                    );
                }
            }
        }
    }
}

/// RTL text (Arabic) should be handled correctly.
#[test]
fn test_rtl_safe() {
    let texts = vec![
        "محمد ذهب إلى السوق",
        "مرحبا بالعالم",
        "Mixed English and العربية text with محمد",
    ];

    for text in texts {
        let coref = MentionRankingCoref::new();
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    assert!(mention.start <= mention.end);
                    assert!(mention.end <= char_count);
                }
            }
        }
    }
}

// =============================================================================
// Property Tests: Book-Scale Features
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]

    /// Book-scale config should be valid.
    #[test]
    fn prop_book_scale_config_valid(text in arb_multilingual_text()) {
        let config = MentionRankingConfig::book_scale();
        let coref = MentionRankingCoref::with_config(config);

        // Should not panic
        let _ = coref.resolve(&text);
    }

    /// Global proper coref should not produce invalid clusters.
    #[test]
    fn prop_global_proper_coref_valid(text in arb_multilingual_text()) {
        let config = MentionRankingConfig {
            enable_global_proper_coref: true,
            global_proper_threshold: 0.7,
            ..Default::default()
        };
        let coref = MentionRankingCoref::with_config(config);
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                // All mentions in cluster should be valid
                for mention in &cluster.mentions {
                    prop_assert!(mention.start <= mention.end);
                    prop_assert!(mention.end <= char_count);
                }
            }
        }
    }
}

// =============================================================================
// Unit-style tests for specific scenarios
// =============================================================================

#[test]
fn test_empty_text() {
    let coref = MentionRankingCoref::new();
    let result = coref.resolve("");
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_whitespace_only() {
    let coref = MentionRankingCoref::new();
    let result = coref.resolve("   \n\t  ");
    assert!(result.is_ok());
}

#[test]
#[ignore = "times out in quick profile - run with --profile ml"]
fn test_long_document_simulation() {
    // Simulate a longer document with repeated patterns
    let base = "John went to the store. He bought milk. Mary was there. She greeted him. ";
    let text = base.repeat(100);

    let config = MentionRankingConfig::book_scale();
    let coref = MentionRankingCoref::with_config(config);

    let result = coref.resolve(&text);
    assert!(result.is_ok(), "Should handle longer documents");

    let clusters = result.unwrap();
    // Should find some clusters (John/he, Mary/she)
    assert!(!clusters.is_empty() || text.is_empty());
}

#[test]
fn test_type_specific_limits() {
    // Verify type-specific antecedent limits from the paper
    let config = MentionRankingConfig::default();

    // Paper says: pronouns 30, proper/nominal 300
    assert_eq!(config.pronoun_max_antecedents, 30);
    assert_eq!(config.proper_max_antecedents, 300);
    assert_eq!(config.nominal_max_antecedents, 300);
}

// =============================================================================
// Salience Integration Property Tests
// =============================================================================

use std::collections::HashMap;

/// Strategy for generating salience scores
fn arb_salience_scores() -> impl Strategy<Value = HashMap<String, f64>> {
    prop::collection::hash_map(
        prop::sample::select(vec![
            "john".to_string(),
            "mary".to_string(),
            "bob".to_string(),
            "president".to_string(),
            "doctor".to_string(),
            "北京".to_string(),
            "東京".to_string(),
        ]),
        0.0f64..1.0,
        0..5,
    )
}

proptest! {
    #![proptest_config(slow_config())]

    /// Salience-weighted resolution should produce valid clusters.
    #[test]
    fn prop_salience_valid_clusters(
        text in arb_multilingual_text(),
        scores in arb_salience_scores(),
        salience_weight in 0.0f64..0.5,
    ) {
        let config = MentionRankingConfig {
            salience_weight,
            ..Default::default()
        };
        let coref = MentionRankingCoref::with_config(config).with_salience(scores);
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    prop_assert!(mention.start <= mention.end, "Invalid span");
                    prop_assert!(mention.end <= char_count, "Span exceeds text");
                }
            }
        }
    }

    /// Salience weight=0 should produce same results as no salience.
    #[test]
    fn prop_salience_zero_weight_no_effect(text in arb_multilingual_text()) {
        let config_no_salience = MentionRankingConfig {
            salience_weight: 0.0,
            ..Default::default()
        };

        let mut scores = HashMap::new();
        scores.insert("john".to_string(), 1.0);
        scores.insert("mary".to_string(), 0.5);

        let coref_no_salience = MentionRankingCoref::with_config(config_no_salience.clone());
        let coref_with_salience = MentionRankingCoref::with_config(config_no_salience)
            .with_salience(scores);

        let result1 = coref_no_salience.resolve(&text);
        let result2 = coref_with_salience.resolve(&text);

        // Both should succeed
        prop_assert!(result1.is_ok() == result2.is_ok());

        // With weight=0, cluster counts should be identical
        if let (Ok(c1), Ok(c2)) = (result1, result2) {
            prop_assert_eq!(c1.len(), c2.len(), "Cluster counts should match with weight=0");
        }
    }

    /// Salience with book-scale config should be valid.
    #[test]
    fn prop_salience_book_scale_valid(
        text in arb_multilingual_text(),
        scores in arb_salience_scores(),
    ) {
        let config = MentionRankingConfig::book_scale();
        // Book-scale already has salience_weight > 0
        let coref = MentionRankingCoref::with_config(config).with_salience(scores);
        let char_count = text.chars().count();

        if let Ok(clusters) = coref.resolve(&text) {
            for cluster in &clusters {
                for mention in &cluster.mentions {
                    prop_assert!(mention.start <= mention.end);
                    prop_assert!(mention.end <= char_count);
                }
            }
        }
    }
}

#[test]
fn test_salience_builder_pattern() {
    // Test the builder pattern for config
    let config = MentionRankingConfig::default().with_salience(0.3);

    assert!((config.salience_weight - 0.3).abs() < 0.001);
}

#[test]
fn test_salience_clamping() {
    // Values > 1.0 should be clamped
    let config = MentionRankingConfig::default().with_salience(2.0);
    assert!((config.salience_weight - 1.0).abs() < 0.001);

    // Negative values should be clamped to 0
    let config2 = MentionRankingConfig::default().with_salience(-0.5);
    assert!((config2.salience_weight - 0.0).abs() < 0.001);
}

#[test]
fn test_salience_integration_e2e() {
    // End-to-end test: salience should bias toward salient entities
    let text = "John met Bob. He greeted him warmly.";

    // Make John highly salient
    let mut scores = HashMap::new();
    scores.insert("john".to_string(), 1.0);
    scores.insert("bob".to_string(), 0.1);

    let config = MentionRankingConfig {
        salience_weight: 0.3,
        ..Default::default()
    };

    let coref = MentionRankingCoref::with_config(config).with_salience(scores);
    let result = coref.resolve(text);

    assert!(result.is_ok());
    // Just verify it produces valid output
    for cluster in result.unwrap() {
        for mention in cluster.mentions {
            assert!(mention.start <= mention.end);
        }
    }
}

// =============================================================================
// MentionCluster ↔ CorefChain ↔ Track Integration Tests
// =============================================================================
//
// Terminology mapping:
//   - MentionCluster: Output of MentionRankingCoref::resolve() (this module)
//   - CorefChain: Evaluation format (anno::eval::coref)
//   - Track: Production format (anno_core::grounded::Track)
//   - Signal: Raw NER detection (Level 1)
//
// The hierarchy:
//   Signal → Track → Identity
//   (NER)   (Within-doc coref)   (Cross-doc resolution)
//
// MentionCluster is an intermediate representation that can convert to:
//   - CorefChain (for evaluation)
//   - Track (for production via GroundedDocument)

use anno::eval::coref::{CorefChain, Mention};
use anno::eval::coref_resolver::CoreferenceResolver;
use anno::{Entity, EntityType};

#[test]
fn test_mention_cluster_to_coref_chain_conversion() {
    // MentionCluster can be converted to CorefChain for evaluation
    let coref = MentionRankingCoref::new();
    let text = "John went to the store. He bought milk.";

    let clusters = coref.resolve(text).unwrap();

    // Convert each MentionCluster to CorefChain
    let chains: Vec<CorefChain> = clusters
        .iter()
        .map(|cluster| {
            let mentions: Vec<Mention> = cluster
                .mentions
                .iter()
                .map(|m| Mention::new(&m.text, m.start, m.end))
                .collect();
            CorefChain::new(mentions)
        })
        .collect();

    // Verify the conversion preserves structure
    for (cluster, chain) in clusters.iter().zip(chains.iter()) {
        assert_eq!(cluster.mentions.len(), chain.len());
        for (m, c) in cluster.mentions.iter().zip(chain.mentions.iter()) {
            assert_eq!(m.text, c.text);
            assert_eq!(m.start, c.start);
            assert_eq!(m.end, c.end);
        }
    }
}

#[test]
fn test_coreference_resolver_trait_integration() {
    // Test the CoreferenceResolver trait implementation
    // CoreferenceResolver::resolve takes &[Entity] and returns Vec<Entity>
    let coref = MentionRankingCoref::new();

    // Create entities (simulating NER output)
    let entities = vec![
        Entity::new("John", EntityType::Person, 0, 4, 0.9),
        Entity::new("He", EntityType::Person, 25, 27, 0.85),
    ];

    // Use the CoreferenceResolver trait
    let resolved: Vec<Entity> = CoreferenceResolver::resolve(&coref, &entities);

    // Verify structure
    assert_eq!(resolved.len(), entities.len());

    // Entities should preserve their spans
    for entity in &resolved {
        assert!(entity.start <= entity.end);
    }
}

#[test]
fn test_coref_chain_properties() {
    // Test CorefChain properties that should hold for all valid chains
    let john = Mention::new("John", 0, 4);
    let he = Mention::new("He", 25, 27);

    let chain = CorefChain::new(vec![john.clone(), he.clone()]);

    // Non-singleton
    assert!(!chain.is_singleton());
    assert_eq!(chain.len(), 2);

    // First mention is canonical
    let first = chain.first().unwrap();
    assert_eq!(first.text, "John");

    // Contains works
    assert!(chain.mentions.iter().any(|m| m.text == "John"));
    assert!(chain.mentions.iter().any(|m| m.text == "He"));
}

#[test]
fn test_singleton_handling() {
    // Test singleton mentions (mentioned only once)
    let coref = MentionRankingCoref::new();

    // Three different entities, no coreference
    let text = "John met Mary at Google.";
    let clusters = coref.resolve(text).unwrap();

    // Each entity that doesn't corefer is effectively a singleton
    // (though mention_ranking may not create explicit singletons)
    for cluster in &clusters {
        assert!(!cluster.mentions.is_empty());
    }
}

#[test]
fn test_cluster_to_entity_canonical_id() {
    // Verify that resolved entities get canonical_ids via CoreferenceResolver trait
    let coref = MentionRankingCoref::new();

    let entities = vec![
        Entity::new("Mary", EntityType::Person, 0, 4, 0.9),
        Entity::new("She", EntityType::Person, 20, 23, 0.8),
        Entity::new("her", EntityType::Person, 40, 43, 0.75),
    ];

    // Use CoreferenceResolver trait
    let resolved: Vec<Entity> = CoreferenceResolver::resolve(&coref, &entities);

    // All entities should preserve their spans
    for (original, resolved_e) in entities.iter().zip(resolved.iter()) {
        assert_eq!(original.start, resolved_e.start);
        assert_eq!(original.end, resolved_e.end);
        assert_eq!(original.text, resolved_e.text);
    }

    // Coreferent entities should share canonical_id
    let canonical_ids: Vec<_> = resolved.iter().filter_map(|e| e.canonical_id).collect();

    // If Mary/She/her corefer, they should share an ID
    if canonical_ids.len() >= 2 {
        // IDs should be reasonable cluster indices
        assert!(
            canonical_ids.iter().all(|&id| id.get() < 1000),
            "IDs should be reasonable"
        );
    }
}

#[test]
fn test_multilingual_coref_chain_formation() {
    // Test that coref works across scripts
    let coref = MentionRankingCoref::new();

    let test_cases = [
        "北京是首都。它很大。",          // Chinese: Beijing is capital. It is big.
        "Путин встретился. Он был рад.", // Russian: Putin met. He was happy.
        "Jean est parti. Il reviendra.", // French: Jean left. He will return.
    ];

    for text in &test_cases {
        let result = coref.resolve(text);
        assert!(result.is_ok(), "Should handle: {}", text);

        let clusters = result.unwrap();
        for cluster in &clusters {
            for mention in &cluster.mentions {
                assert!(mention.start <= mention.end);
                let char_count = text.chars().count();
                assert!(mention.end <= char_count, "Span exceeds text in: {}", text);
            }
        }
    }
}

#[test]
fn test_coref_chain_span_ordering() {
    // CorefChain mentions should be orderable by position
    let coref = MentionRankingCoref::new();
    let text = "John saw Mary. He greeted her.";

    let clusters = coref.resolve(text).unwrap();

    for cluster in &clusters {
        // Convert to sorted mentions
        let mut sorted = cluster.mentions.clone();
        sorted.sort_by_key(|m| (m.start, m.end));

        // Check ordering is consistent
        for window in sorted.windows(2) {
            assert!(
                window[0].start <= window[1].start,
                "Mentions should be sortable by position"
            );
        }
    }
}

#[test]
fn test_coref_resolver_name() {
    let coref = MentionRankingCoref::new();
    assert_eq!(CoreferenceResolver::name(&coref), "MentionRankingCoref");
}

#[test]
fn test_mention_cluster_structure() {
    // Test MentionCluster basic structure
    let coref = MentionRankingCoref::new();
    let text = "Alice saw Bob. She waved to him.";

    let clusters = coref.resolve(text).unwrap();

    // Verify cluster IDs are assigned
    for cluster in &clusters {
        // Each cluster has an ID
        assert!(cluster.id < 1000, "Cluster ID should be reasonable");

        // Mentions have valid structure
        for mention in &cluster.mentions {
            assert!(!mention.text.is_empty(), "Mention text should not be empty");
            assert!(mention.start < mention.end || mention.text.is_empty());
        }
    }
}

#[test]
fn test_coref_chain_to_cluster_id_mapping() {
    // Test that we can map from chains back to cluster IDs
    let coref = MentionRankingCoref::new();
    let text = "Peter met Paul. He greeted him warmly.";

    let clusters = coref.resolve(text).unwrap();

    // Build a mention -> cluster_id map
    let mut mention_to_cluster: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();

    for cluster in &clusters {
        for mention in &cluster.mentions {
            mention_to_cluster.insert((mention.start, mention.end), cluster.id);
        }
    }

    // Verify mapping is consistent
    for cluster in &clusters {
        for mention in &cluster.mentions {
            let mapped_id = mention_to_cluster.get(&(mention.start, mention.end));
            assert_eq!(mapped_id, Some(&cluster.id), "Mapping should be consistent");
        }
    }
}
