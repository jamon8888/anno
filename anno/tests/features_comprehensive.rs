//! Comprehensive tests for the entity features module.
//!
//! Tests cover:
//! - MentionType classification
//! - MentionContext extraction
//! - ChainFeatures computation
//! - CooccurrenceFeatures
//! - PairwiseFeatures
//! - Embedding aggregation
//! - Integration with salience
//! - Multilingual support
//! - Edge cases

use anno::features::{
    aggregate_embeddings, AggregationMethod, ChainFeatures, CooccurrenceFeatures, DocumentFeatures,
    EntityFeatureExtractor, ExtractorConfig, MentionContext, MentionType, PairwiseFeatures,
};
use anno::salience::{features_to_salience_scores, ChainFeatureSalience, EntityRanker};
use anno::{Entity, EntityType};

// =============================================================================
// Test Fixtures
// =============================================================================

fn sample_entities() -> Vec<Entity> {
    vec![
        Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.95),
        Entity::new("Angela Merkel", EntityType::Person, 17, 30, 0.92),
        Entity::new("Berlin", EntityType::Location, 34, 40, 0.88),
        Entity::new("He", EntityType::Person, 42, 44, 0.85),
        Entity::new("Obama", EntityType::Person, 60, 65, 0.90),
    ]
}

fn multilingual_entities() -> Vec<Entity> {
    vec![
        // Chinese
        Entity::new("習近平", EntityType::Person, 0, 3, 0.9),
        Entity::new("北京", EntityType::Location, 4, 6, 0.9),
        // Japanese
        Entity::new("東京", EntityType::Location, 10, 12, 0.9),
        // Arabic
        Entity::new("محمد", EntityType::Person, 15, 19, 0.9),
        // Cyrillic
        Entity::new("Москва", EntityType::Location, 25, 31, 0.9),
    ]
}

// =============================================================================
// MentionType Classification Tests
// =============================================================================

#[test]
fn test_mention_type_pronouns_comprehensive() {
    // Personal pronouns - subject
    assert_eq!(MentionType::classify("he"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("she"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("it"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("they"), MentionType::Pronominal);

    // Personal pronouns - object
    assert_eq!(MentionType::classify("him"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("her"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("them"), MentionType::Pronominal);

    // Possessive pronouns
    assert_eq!(MentionType::classify("his"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("hers"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("its"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("their"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("theirs"), MentionType::Pronominal);

    // First person
    assert_eq!(MentionType::classify("i"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("me"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("my"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("mine"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("we"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("us"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("our"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("ours"), MentionType::Pronominal);

    // Second person
    assert_eq!(MentionType::classify("you"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("your"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("yours"), MentionType::Pronominal);

    // Demonstrative
    assert_eq!(MentionType::classify("this"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("that"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("these"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("those"), MentionType::Pronominal);

    // Reflexive
    assert_eq!(MentionType::classify("himself"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("herself"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("itself"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("themselves"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("myself"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("yourself"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("ourselves"), MentionType::Pronominal);

    // Interrogative/relative
    assert_eq!(MentionType::classify("who"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("whom"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("whose"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("which"), MentionType::Pronominal);
    assert_eq!(MentionType::classify("what"), MentionType::Pronominal);
}

#[test]
fn test_mention_type_named_entities() {
    assert_eq!(MentionType::classify("Barack Obama"), MentionType::Proper);
    assert_eq!(MentionType::classify("Apple Inc."), MentionType::Proper);
    assert_eq!(MentionType::classify("New York City"), MentionType::Proper);
    assert_eq!(
        MentionType::classify("Microsoft Corporation"),
        MentionType::Proper
    );
    assert_eq!(MentionType::classify("Dr. John Smith"), MentionType::Proper);
    assert_eq!(MentionType::classify("United Nations"), MentionType::Proper);
    assert_eq!(MentionType::classify("European Union"), MentionType::Proper);
}

#[test]
fn test_mention_type_nominals() {
    assert_eq!(MentionType::classify("the president"), MentionType::Nominal);
    assert_eq!(MentionType::classify("a company"), MentionType::Nominal);
    assert_eq!(
        MentionType::classify("an organization"),
        MentionType::Nominal
    );
    assert_eq!(MentionType::classify("this person"), MentionType::Nominal);
    assert_eq!(MentionType::classify("that building"), MentionType::Nominal);
    assert_eq!(
        MentionType::classify("these countries"),
        MentionType::Nominal
    );
    assert_eq!(MentionType::classify("those leaders"), MentionType::Nominal);
}

#[test]
fn test_mention_type_edge_cases() {
    // Empty string
    assert_eq!(MentionType::classify(""), MentionType::Nominal);

    // Whitespace only
    assert_eq!(MentionType::classify("   "), MentionType::Nominal);

    // Single lowercase word (not pronoun)
    assert_eq!(MentionType::classify("president"), MentionType::Nominal);

    // Capitalized single word at sentence start would typically be Named
    // but without sentence context, we classify by pattern
    assert_eq!(MentionType::classify("President"), MentionType::Proper);
}

// =============================================================================
// MentionContext Tests
// =============================================================================

#[test]
fn test_mention_context_basic() {
    let text = "In Paris, Barack Obama met Angela Merkel. He discussed policy.";
    let entity = Entity::new("Barack Obama", EntityType::Person, 10, 22, 0.95);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert_eq!(ctx.entity.text, "Barack Obama");
    assert!(ctx.left_context.contains("Paris"));
    assert!(ctx.right_context.contains("met"));
    assert!(ctx.relative_position < 0.5);
    assert!(ctx.is_capitalized);
    assert!(!ctx.is_all_caps);
    assert!(!ctx.contains_digits);
    assert_eq!(ctx.word_count, 2);
}

#[test]
fn test_mention_context_at_document_start() {
    let text = "Obama met Merkel in Berlin.";
    let entity = Entity::new("Obama", EntityType::Person, 0, 5, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert!(ctx.left_context.is_empty());
    assert!(ctx.right_context.contains("met"));
    assert!(ctx.relative_position < 0.1);
    assert!(ctx.likely_subject);
}

#[test]
fn test_mention_context_at_document_end() {
    let text = "She met Obama in Berlin";
    let entity = Entity::new("Berlin", EntityType::Location, 18, 24, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert!(ctx.left_context.contains("Obama"));
    assert!(ctx.relative_position > 0.5);
}

#[test]
fn test_mention_context_all_caps() {
    let text = "NASA launched a rocket.";
    let entity = Entity::new("NASA", EntityType::Organization, 0, 4, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert!(ctx.is_capitalized);
    assert!(ctx.is_all_caps);
}

#[test]
fn test_mention_context_with_digits() {
    let text = "The F-35 fighter jet is expensive.";
    let entity = Entity::new("F-35", EntityType::Other("Product".to_string()), 4, 8, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert!(ctx.contains_digits);
}

#[test]
fn test_mention_context_full_context_string() {
    let text = "In Paris, Obama met Merkel.";
    let entity = Entity::new("Obama", EntityType::Person, 10, 15, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());
    let full = ctx.full_context();

    assert!(full.contains("[Obama]"));
    assert!(full.contains("Paris"));
    assert!(full.contains("met"));
}

#[test]
fn test_mention_context_custom_window() {
    let text = "A very long prefix text here. Obama. A very long suffix text here.";
    let entity = Entity::new("Obama", EntityType::Person, 30, 35, 0.9);

    let small_config = ExtractorConfig {
        context_window: 10,
        ..Default::default()
    };

    let ctx = MentionContext::extract(text, &entity, &small_config);

    // With small window, shouldn't capture all text
    assert!(ctx.left_context.len() <= 10);
    assert!(ctx.right_context.len() <= 10);
}

// =============================================================================
// ChainFeatures Tests
// =============================================================================

#[test]
fn test_chain_features_basic() {
    let entities = vec![
        Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.95),
        Entity::new("He", EntityType::Person, 50, 52, 0.85),
        Entity::new("Obama", EntityType::Person, 100, 105, 0.90),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 200);

    assert_eq!(features.chain_length, 3);
    assert!(features.named_count >= 1);
    assert!(features.pronominal_count >= 1);
    assert!(!features.is_singleton());
}

#[test]
fn test_chain_features_singleton() {
    let entities = vec![Entity::new("Berlin", EntityType::Location, 0, 6, 0.9)];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100);

    assert!(features.is_singleton());
    assert_eq!(features.chain_length, 1);
    // Spread is last_position - first_position, which is 6 - 0 = 6 for a single mention
    // (end position - start position of that mention)
    assert!(features.mention_spread <= 6); // Single mention spread based on span
}

#[test]
fn test_chain_features_spread_calculation() {
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("Obama", EntityType::Person, 100, 105, 0.9),
        Entity::new("Obama", EntityType::Person, 200, 205, 0.9),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 300);

    assert_eq!(features.first_mention_position, 0);
    assert_eq!(features.last_mention_position, 205);
    assert_eq!(features.mention_spread, 205);
    assert!((features.relative_spread - 205.0 / 300.0).abs() < 0.01);
}

#[test]
fn test_chain_features_mostly_pronominal() {
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("he", EntityType::Person, 10, 12, 0.85),
        Entity::new("him", EntityType::Person, 20, 23, 0.85),
        Entity::new("his", EntityType::Person, 30, 33, 0.85),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100);

    assert!(features.is_mostly_pronominal());
    assert_eq!(features.pronominal_count, 3);
    assert_eq!(features.named_count, 1);
    assert!(features.pronoun_ratio > 0.5);
}

#[test]
fn test_chain_features_confidence_statistics() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("A", EntityType::Person, 5, 6, 0.8),
        Entity::new("A", EntityType::Person, 10, 11, 0.7),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100);

    assert!((features.mean_confidence - 0.8).abs() < 0.01);
    assert!((features.min_confidence - 0.7).abs() < 0.01);
    assert!((features.max_confidence - 0.9).abs() < 0.01);
}

#[test]
fn test_chain_features_canonical_form_selection() {
    // Canonical should be longest named mention
    let entities = vec![
        Entity::new("he", EntityType::Person, 0, 2, 0.85),
        Entity::new("Barack Obama", EntityType::Person, 10, 22, 0.95),
        Entity::new("Obama", EntityType::Person, 30, 35, 0.9),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100);

    assert_eq!(features.canonical_form, "Barack Obama");
}

#[test]
fn test_chain_features_variations() {
    let entities = vec![
        Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.95),
        Entity::new("Obama", EntityType::Person, 20, 25, 0.9),
        Entity::new("President Obama", EntityType::Person, 40, 55, 0.92),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100);

    assert_eq!(features.variation_count(), 3);
    assert!(features.variations.contains(&"Barack Obama".to_string()));
    assert!(features.variations.contains(&"Obama".to_string()));
    assert!(features.variations.contains(&"President Obama".to_string()));
}

#[test]
fn test_chain_features_empty_input() {
    let features = ChainFeatures::from_mentions(&[], 100);

    assert_eq!(features.chain_length, 0);
    assert!(features.variations.is_empty());
    assert_eq!(features.mean_confidence, 0.0);
    assert!(features.canonical_form.is_empty());
}

#[test]
fn test_chain_features_with_embedding() {
    let entities = vec![Entity::new("A", EntityType::Person, 0, 1, 0.9)];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 100).with_centroid(vec![1.0, 2.0, 3.0]);

    assert!(features.centroid_embedding.is_some());
    assert_eq!(features.centroid_embedding.unwrap(), vec![1.0, 2.0, 3.0]);
}

// =============================================================================
// CooccurrenceFeatures Tests
// =============================================================================

#[test]
fn test_cooccurrence_basic() {
    let entities = sample_entities();

    let extractor = EntityFeatureExtractor::default();
    let cooc = extractor.extract_cooccurrence(&entities);

    let obama_cooc = cooc.get("barack obama").unwrap();
    assert!(obama_cooc
        .cooccurring_entities
        .contains(&"angela merkel".to_string()));
    assert!(obama_cooc
        .cooccurring_entities
        .contains(&"berlin".to_string()));
}

#[test]
fn test_cooccurrence_symmetry() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("B", EntityType::Person, 5, 6, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        cooccurrence_window: 100,
        ..Default::default()
    });
    let cooc = extractor.extract_cooccurrence(&entities);

    let a_cooc = cooc.get("a").unwrap();
    let b_cooc = cooc.get("b").unwrap();

    assert!(a_cooc.cooccurring_entities.contains(&"b".to_string()));
    assert!(b_cooc.cooccurring_entities.contains(&"a".to_string()));
}

#[test]
fn test_cooccurrence_outside_window() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("B", EntityType::Person, 1000, 1001, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        cooccurrence_window: 50,
        ..Default::default()
    });
    let cooc = extractor.extract_cooccurrence(&entities);

    let a_cooc = cooc.get("a").unwrap();
    assert!(!a_cooc.cooccurring_entities.contains(&"b".to_string()));
}

#[test]
fn test_cooccurrence_no_self_cooccurrence() {
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("Obama", EntityType::Person, 10, 15, 0.9),
    ];

    let extractor = EntityFeatureExtractor::default();
    let cooc = extractor.extract_cooccurrence(&entities);

    let obama_cooc = cooc.get("obama").unwrap();
    assert!(!obama_cooc
        .cooccurring_entities
        .contains(&"obama".to_string()));
}

#[test]
fn test_cooccurrence_counts() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("B", EntityType::Person, 5, 6, 0.9),
        Entity::new("B", EntityType::Person, 10, 11, 0.9),
        Entity::new("B", EntityType::Person, 15, 16, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        cooccurrence_window: 100,
        ..Default::default()
    });
    let cooc = extractor.extract_cooccurrence(&entities);

    let a_cooc = cooc.get("a").unwrap();
    // A should have multiple cooccurrences with B (3 B mentions)
    assert!(*a_cooc.cooccurrence_counts.get("b").unwrap() >= 1);
}

#[test]
fn test_cooccurrence_top_k() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("B", EntityType::Person, 5, 6, 0.9),
        Entity::new("B", EntityType::Person, 10, 11, 0.9),
        Entity::new("C", EntityType::Person, 15, 16, 0.9),
        Entity::new("D", EntityType::Person, 20, 21, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        cooccurrence_window: 100,
        ..Default::default()
    });
    let cooc = extractor.extract_cooccurrence(&entities);

    let a_cooc = cooc.get("a").unwrap();
    let top = a_cooc.top_k(2);

    assert!(top.len() <= 2);
}

// =============================================================================
// PairwiseFeatures Tests
// =============================================================================

#[test]
fn test_pairwise_exact_match() {
    let a = Entity::new("Obama", EntityType::Person, 0, 5, 0.9);
    let b = Entity::new("Obama", EntityType::Person, 10, 15, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert!(features.exact_match);
    assert!(features.case_insensitive_match);
    assert_eq!(features.string_similarity, 1.0);
    assert!(features.type_match);
}

#[test]
fn test_pairwise_case_insensitive_match() {
    let a = Entity::new("Obama", EntityType::Person, 0, 5, 0.9);
    let b = Entity::new("OBAMA", EntityType::Person, 10, 15, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert!(!features.exact_match);
    assert!(features.case_insensitive_match);
}

#[test]
fn test_pairwise_string_similarity() {
    let a = Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.9);
    let b = Entity::new("Obama", EntityType::Person, 20, 25, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    // "Barack Obama" and "Obama" share "Obama"
    assert!(features.string_similarity > 0.0);
    assert!(features.string_similarity < 1.0);
}

#[test]
fn test_pairwise_type_mismatch() {
    let a = Entity::new("Apple", EntityType::Organization, 0, 5, 0.9);
    let b = Entity::new(
        "Apple",
        EntityType::Other("Product".to_string()),
        10,
        15,
        0.9,
    );

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert!(!features.type_match);
}

#[test]
fn test_pairwise_distance_calculation() {
    let a = Entity::new("A", EntityType::Person, 0, 1, 0.9);
    let b = Entity::new("B", EntityType::Person, 100, 101, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 5);

    assert_eq!(features.char_distance, 99);
    assert_eq!(features.mention_distance, 5);
}

#[test]
fn test_pairwise_pronominal_anaphora_detection() {
    let a = Entity::new("John", EntityType::Person, 0, 4, 0.9);
    let b = Entity::new("he", EntityType::Person, 10, 12, 0.85);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert!(features.is_pronominal_anaphora);
    assert_eq!(features.mention_type_a, MentionType::Proper);
    assert_eq!(features.mention_type_b, MentionType::Pronominal);
}

#[test]
fn test_pairwise_not_anaphora_wrong_order() {
    // Pronoun comes first, so not an anaphora pattern
    let a = Entity::new("he", EntityType::Person, 0, 2, 0.85);
    let b = Entity::new("John", EntityType::Person, 10, 14, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert!(!features.is_pronominal_anaphora);
}

#[test]
fn test_pairwise_all_pairs() {
    let entities = vec![
        Entity::new("A", EntityType::Person, 0, 1, 0.9),
        Entity::new("B", EntityType::Person, 5, 6, 0.9),
        Entity::new("C", EntityType::Person, 10, 11, 0.9),
    ];

    let pairs = PairwiseFeatures::compute_all_pairs(&entities);

    // 3 entities = 3 pairs: (0,1), (0,2), (1,2)
    assert_eq!(pairs.len(), 3);

    // Verify indices
    let indices: Vec<(usize, usize)> = pairs.iter().map(|(i, j, _)| (*i, *j)).collect();
    assert!(indices.contains(&(0, 1)));
    assert!(indices.contains(&(0, 2)));
    assert!(indices.contains(&(1, 2)));
}

#[test]
fn test_pairwise_overlapping_entities() {
    let a = Entity::new("New York", EntityType::Location, 0, 8, 0.9);
    let b = Entity::new("New York City", EntityType::Location, 0, 13, 0.9);

    let features = PairwiseFeatures::compute(&a, &b, 1);

    assert_eq!(features.char_distance, 0); // Overlapping
}

// =============================================================================
// Full Extraction Tests
// =============================================================================

#[test]
fn test_full_extraction() {
    let text = "Barack Obama met Angela Merkel in Berlin. He discussed policy with her.";
    let entities = sample_entities();

    let extractor = EntityFeatureExtractor::default();
    let features = extractor.extract_all(text, &entities);

    assert_eq!(features.mention_contexts.len(), entities.len());
    assert!(!features.chain_features.is_empty());
    assert!(!features.cooccurrence.is_empty());
    assert_eq!(features.document_stats.mention_count, entities.len());
}

#[test]
fn test_full_extraction_document_stats() {
    let text = "Obama met Merkel. Obama and Merkel discussed policy.";
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("Merkel", EntityType::Person, 10, 16, 0.9),
        Entity::new("Obama", EntityType::Person, 18, 23, 0.9),
        Entity::new("Merkel", EntityType::Person, 28, 34, 0.9),
    ];

    let extractor = EntityFeatureExtractor::default();
    let features = extractor.extract_all(text, &entities);

    assert_eq!(features.document_stats.mention_count, 4);
    assert_eq!(features.document_stats.unique_entity_count, 2);
    assert!(features.document_stats.entity_density > 0.0);
    // The type distribution uses EntityType::as_label() which returns "PER" for Person
    assert!(
        features
            .document_stats
            .type_distribution
            .contains_key("PER")
            || features
                .document_stats
                .type_distribution
                .contains_key("Person")
    );
}

#[test]
fn test_full_extraction_empty_input() {
    let extractor = EntityFeatureExtractor::default();
    let features = extractor.extract_all("Some text here", &[]);

    assert!(features.mention_contexts.is_empty());
    assert!(features.chain_features.is_empty());
    assert!(features.cooccurrence.is_empty());
    assert_eq!(features.document_stats.mention_count, 0);
}

// =============================================================================
// Embedding Aggregation Tests
// =============================================================================

#[test]
fn test_aggregate_embeddings_mean() {
    let emb1 = vec![1.0, 2.0, 3.0];
    let emb2 = vec![2.0, 4.0, 6.0];
    let embeddings = vec![emb1, emb2];

    let mean = aggregate_embeddings(&embeddings, AggregationMethod::Mean).unwrap();
    assert_eq!(mean, vec![1.5, 3.0, 4.5]);
}

#[test]
fn test_aggregate_embeddings_max() {
    let emb1 = vec![1.0, 5.0, 3.0];
    let emb2 = vec![2.0, 4.0, 6.0];
    let embeddings = vec![emb1, emb2];

    let max = aggregate_embeddings(&embeddings, AggregationMethod::Max).unwrap();
    assert_eq!(max, vec![2.0, 5.0, 6.0]);
}

#[test]
fn test_aggregate_embeddings_first() {
    let emb1 = vec![1.0, 2.0, 3.0];
    let emb2 = vec![4.0, 5.0, 6.0];
    let embeddings = vec![emb1.clone(), emb2];

    let first = aggregate_embeddings(&embeddings, AggregationMethod::First).unwrap();
    assert_eq!(first, emb1);
}

#[test]
fn test_aggregate_embeddings_weighted() {
    let emb1 = vec![1.0, 0.0];
    let emb2 = vec![0.0, 1.0];
    let embeddings = vec![emb1, emb2];

    let weighted = aggregate_embeddings(
        &embeddings,
        AggregationMethod::WeightedMean {
            weights: vec![0.75, 0.25],
        },
    )
    .unwrap();

    assert!((weighted[0] - 0.75).abs() < 0.01);
    assert!((weighted[1] - 0.25).abs() < 0.01);
}

#[test]
fn test_aggregate_embeddings_empty() {
    let result = aggregate_embeddings(&[], AggregationMethod::Mean);
    assert!(result.is_none());
}

#[test]
fn test_aggregate_embeddings_mismatched_dimensions() {
    let emb1 = vec![1.0, 2.0];
    let emb2 = vec![1.0, 2.0, 3.0];
    let embeddings = vec![emb1, emb2];

    let result = aggregate_embeddings(&embeddings, AggregationMethod::Mean);
    assert!(result.is_none());
}

#[test]
fn test_aggregate_embeddings_single() {
    let emb = vec![1.0, 2.0, 3.0];
    let embeddings = vec![emb.clone()];

    let mean = aggregate_embeddings(&embeddings, AggregationMethod::Mean).unwrap();
    assert_eq!(mean, emb);
}

// =============================================================================
// Configuration Tests
// =============================================================================

#[test]
fn test_extractor_config_default() {
    let config = ExtractorConfig::default();

    assert!(config.context_window > 0);
    assert!(config.cooccurrence_window > 0);
    assert!(config.normalize_text);
    assert_eq!(config.min_cooccurrence_freq, 1);
}

#[test]
fn test_extractor_config_builder() {
    let config = ExtractorConfig::default()
        .with_context_window(200)
        .with_cooccurrence_window(300);

    assert_eq!(config.context_window, 200);
    assert_eq!(config.cooccurrence_window, 300);
}

// =============================================================================
// Multilingual Tests
// =============================================================================

#[test]
fn test_features_chinese() {
    let text = "習近平在北京會見了普京。他們討論了政策。";
    let entity = Entity::new("習近平", EntityType::Person, 0, 3, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert_eq!(ctx.char_count, 3);
    assert!(ctx.relative_position < 0.2);
}

#[test]
fn test_features_japanese() {
    let text = "東京は日本の首都です。";
    let entity = Entity::new("東京", EntityType::Location, 0, 2, 0.9);

    let ctx = MentionContext::extract(text, &entity, &ExtractorConfig::default());

    assert_eq!(ctx.char_count, 2);
}

#[test]
fn test_chain_features_unicode() {
    let entities = vec![
        Entity::new("北京", EntityType::Location, 0, 2, 0.9),
        Entity::new("北京", EntityType::Location, 10, 12, 0.9),
    ];
    let refs: Vec<&Entity> = entities.iter().collect();

    let features = ChainFeatures::from_mentions(&refs, 50);

    assert_eq!(features.chain_length, 2);
    assert!(features.variations.contains(&"北京".to_string()));
}

#[test]
fn test_cooccurrence_unicode() {
    let entities = vec![
        Entity::new("習近平", EntityType::Person, 0, 3, 0.9),
        Entity::new("北京", EntityType::Location, 4, 6, 0.9),
    ];

    let extractor = EntityFeatureExtractor::default();
    let cooc = extractor.extract_cooccurrence(&entities);

    assert!(!cooc.is_empty());
}

// =============================================================================
// Integration with Salience Tests
// =============================================================================

#[test]
fn test_chain_feature_salience_basic() {
    let text = "Obama met Merkel. Obama discussed policy. Obama waved.";
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("Merkel", EntityType::Person, 10, 16, 0.9),
        Entity::new("Obama", EntityType::Person, 18, 23, 0.9),
        Entity::new("Obama", EntityType::Person, 42, 47, 0.9),
    ];

    let ranker = ChainFeatureSalience::default();
    let ranked = ranker.rank(text, &entities);

    // Obama mentioned 3x should rank higher than Merkel (1x)
    assert!(!ranked.is_empty());
    assert_eq!(ranked[0].0.text.to_lowercase(), "obama");
}

#[test]
fn test_features_to_salience_scores() {
    let text = "Obama met Merkel. Obama discussed policy.";
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("Merkel", EntityType::Person, 10, 16, 0.9),
        Entity::new("Obama", EntityType::Person, 18, 23, 0.9),
    ];

    let scores = features_to_salience_scores(text, &entities);

    assert!(!scores.is_empty());
    assert!(scores.contains_key("obama"));
    assert!(scores.contains_key("merkel"));

    // Scores should be normalized to [0, 1]
    for (_, score) in &scores {
        assert!(*score >= 0.0 && *score <= 1.0);
    }

    // Obama should have higher score (more mentions)
    assert!(scores.get("obama").unwrap() >= scores.get("merkel").unwrap());
}

#[test]
fn test_chain_feature_salience_empty() {
    let ranker = ChainFeatureSalience::default();
    let ranked = ranker.rank("Some text", &[]);

    assert!(ranked.is_empty());
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_single_character_entity() {
    let entity = Entity::new("X", EntityType::Person, 0, 1, 0.9);
    let ctx = MentionContext::extract("X marks the spot", &entity, &ExtractorConfig::default());

    assert_eq!(ctx.word_count, 1);
    assert_eq!(ctx.char_count, 1);
}

#[test]
fn test_very_long_entity() {
    let long_name = "The Very Long Name Of A Company That Has Many Words In Its Title";
    let entity = Entity::new(
        long_name,
        EntityType::Organization,
        0,
        long_name.chars().count(),
        0.9,
    );

    let ctx = MentionContext::extract(
        &format!("{} is headquartered in NYC.", long_name),
        &entity,
        &ExtractorConfig::default(),
    );

    // Count words in the long name
    let actual_word_count = long_name.split_whitespace().count();
    assert_eq!(ctx.word_count, actual_word_count);
}

#[test]
fn test_entity_with_special_characters() {
    let entity = Entity::new("AT&T", EntityType::Organization, 0, 4, 0.9);
    let ctx = MentionContext::extract("AT&T is a company.", &entity, &ExtractorConfig::default());

    assert_eq!(ctx.entity.text, "AT&T");
}

#[test]
fn test_many_mentions_performance() {
    let mut entities = Vec::new();
    for i in 0..100 {
        entities.push(Entity::new(
            "Obama",
            EntityType::Person,
            i * 10,
            i * 10 + 5,
            0.9,
        ));
    }

    let text = "Obama ".repeat(100);
    let extractor = EntityFeatureExtractor::default();
    let features = extractor.extract_chains(&text, &entities);

    let chain = features.get("obama").unwrap();
    assert_eq!(chain.chain_length, 100);
}

#[test]
fn test_normalization_disabled() {
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("OBAMA", EntityType::Person, 10, 15, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        normalize_text: false,
        ..Default::default()
    });

    let chains = extractor.extract_chains("Obama OBAMA test", &entities);

    // Without normalization, "Obama" and "OBAMA" are separate chains
    assert_eq!(chains.len(), 2);
}

#[test]
fn test_normalization_enabled() {
    let entities = vec![
        Entity::new("Obama", EntityType::Person, 0, 5, 0.9),
        Entity::new("OBAMA", EntityType::Person, 10, 15, 0.9),
    ];

    let extractor = EntityFeatureExtractor::new(ExtractorConfig {
        normalize_text: true,
        ..Default::default()
    });

    let chains = extractor.extract_chains("Obama OBAMA test", &entities);

    // With normalization, "Obama" and "OBAMA" are in the same chain
    assert_eq!(chains.len(), 1);
    assert_eq!(chains.get("obama").unwrap().chain_length, 2);
}
