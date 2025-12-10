//! Integration tests for edit distance in entity linking.
//!
//! Tests the complete workflow from fuzzy mention matching through
//! candidate generation using different similarity metrics.

use anno::edit_distance::{
    damerau_levenshtein, edit_distance_wildcards, edit_similarity, levenshtein,
    normalized_edit_distance,
};
use anno::linking::{
    Candidate, CandidateGenerator, CandidateSource, DictionaryCandidateGenerator, SimilarityMetric,
};

// =============================================================================
// Edit Distance Unit Integration
// =============================================================================

#[test]
fn test_edit_distance_basic_workflow() {
    // Simulate OCR error: "Einstien" for "Einstein"
    let mention = "Einstien";
    let candidate = "Einstein";

    let dist = levenshtein(mention, candidate);
    let similarity = edit_similarity(mention, candidate);

    assert_eq!(dist, 2); // Two character swaps
    assert!(similarity > 0.7, "Similarity should be high for typo: {}", similarity);
}

#[test]
fn test_wildcard_for_damaged_text() {
    // Simulating damaged inscription where some characters are illegible
    let damaged_patterns = vec![
        ("R?ma", "Roma", 0),      // Single char unknown
        ("???TOR", "CASTOR", 0),  // Multiple chars unknown
        ("Ein*", "Einstein", 0), // Suffix unknown
        ("*stein", "Einstein", 0), // Prefix unknown
        ("M?r?e C*", "Marie Curie", 0), // Complex pattern
    ];

    for (pattern, text, expected_dist) in damaged_patterns {
        let dist = edit_distance_wildcards(pattern, text);
        assert_eq!(
            dist, expected_dist,
            "Pattern '{}' should match '{}' with distance {}",
            pattern, text, expected_dist
        );
    }
}

#[test]
fn test_damerau_for_typos() {
    // Adjacent transpositions are common typos
    let typo_pairs = vec![
        ("teh", "the", 1),       // Common typo
        ("recieve", "receive", 1), // ei/ie swap
        ("adn", "and", 1),      // Adjacent swap
    ];

    for (typo, correct, expected) in typo_pairs {
        let dl_dist = damerau_levenshtein(typo, correct);
        let lev_dist = levenshtein(typo, correct);

        assert_eq!(dl_dist, expected, "Damerau distance for {} -> {}", typo, correct);
        // Damerau should be <= Levenshtein for transpositions
        assert!(dl_dist <= lev_dist);
    }
}

// =============================================================================
// Multilingual Edit Distance
// =============================================================================

#[test]
fn test_multilingual_edit_distance() {
    // CJK: Character-level edits
    assert_eq!(levenshtein("北京", "北平"), 1);
    assert_eq!(levenshtein("東京", "東京都"), 1);

    // Arabic
    assert_eq!(levenshtein("محمد", "أحمد"), 1);

    // Cyrillic
    assert_eq!(levenshtein("Москва", "Москве"), 1);

    // Diacritics
    assert_eq!(levenshtein("François", "Francois"), 1);
    assert_eq!(levenshtein("José", "Jose"), 1);
}

#[test]
fn test_normalized_distance_bounds() {
    // Normalized distance should always be in [0, 1]
    let test_pairs = vec![
        ("hello", "hello"),
        ("hello", "world"),
        ("abc", "xyz"),
        ("北京", "東京"),
        ("", "test"),
    ];

    for (a, b) in test_pairs {
        let dist = normalized_edit_distance(a, b);
        assert!(
            dist >= 0.0 && dist <= 1.0,
            "Normalized distance out of bounds for ({}, {}): {}",
            a,
            b,
            dist
        );
    }
}

#[test]
fn test_normalized_distance_symmetry() {
    let pairs = vec![
        ("Einstein", "Einstien"),
        ("hello", "hallo"),
        ("北京", "北平"),
        ("Marie Curie", "mary curie"),
    ];

    for (a, b) in pairs {
        let d1 = normalized_edit_distance(a, b);
        let d2 = normalized_edit_distance(b, a);
        assert!(
            (d1 - d2).abs() < 0.0001,
            "Asymmetric distance for ({}, {}): {} vs {}",
            a,
            b,
            d1,
            d2
        );
    }
}

// =============================================================================
// Candidate Generator Integration
// =============================================================================

#[test]
fn test_generator_with_jaccard_metric() {
    let gen = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::Jaccard)
        .with_well_known();

    // Exact match should work
    let candidates = gen.generate("albert einstein", "", None, 5);
    assert!(!candidates.is_empty());
    assert!(candidates.iter().any(|c| c.kb_id == "Q937"));
}

#[test]
fn test_generator_with_edit_distance_metric() {
    let gen = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::EditDistance)
        .with_well_known();

    // Typo should still find Einstein
    let candidates = gen.generate("albert einstien", "", None, 10);
    
    // Check that we found some candidates
    assert!(!candidates.is_empty(), "Should find candidates for typo");
    
    // Einstein should be in the results with decent similarity
    let einstein = candidates.iter().find(|c| c.kb_id == "Q937");
    assert!(einstein.is_some(), "Einstein should be in candidates");
    
    if let Some(e) = einstein {
        assert!(e.string_sim > 0.7, "String similarity should be high: {}", e.string_sim);
    }
}

#[test]
fn test_generator_with_wildcard_metric() {
    let gen = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::EditDistanceWildcard)
        .with_well_known();

    // Wildcard pattern should match
    let candidates = gen.generate("marie c*", "", None, 10);
    
    // Should find Marie Curie
    let curie = candidates.iter().find(|c| c.label.to_lowercase().contains("curie"));
    assert!(curie.is_some(), "Should find Marie Curie with wildcard");
}

#[test]
fn test_metric_comparison() {
    let gen_jaccard = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::Jaccard)
        .with_well_known();
    
    let gen_edit = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::EditDistance)
        .with_well_known();

    // Query with a typo
    let query = "albert einstien";
    
    let jaccard_results = gen_jaccard.generate(query, "", None, 10);
    let edit_results = gen_edit.generate(query, "", None, 10);

    // Both should return results
    assert!(!jaccard_results.is_empty());
    assert!(!edit_results.is_empty());

    // Edit distance may rank Einstein higher for character-level typos
    // This documents the behavioral difference
}

#[test]
fn test_similarity_metric_from_str() {
    assert_eq!(
        SimilarityMetric::from_str("jaccard"),
        Some(SimilarityMetric::Jaccard)
    );
    assert_eq!(
        SimilarityMetric::from_str("edit-distance"),
        Some(SimilarityMetric::EditDistance)
    );
    assert_eq!(
        SimilarityMetric::from_str("lev"),
        Some(SimilarityMetric::EditDistance)
    );
    assert_eq!(
        SimilarityMetric::from_str("wildcard"),
        Some(SimilarityMetric::EditDistanceWildcard)
    );
    assert_eq!(SimilarityMetric::from_str("invalid"), None);
}

#[test]
fn test_similarity_metric_compute() {
    let metric = SimilarityMetric::EditDistance;
    
    // Identical strings
    assert!(metric.compute("hello", "hello") > 0.99);
    
    // Similar strings
    let sim = metric.compute("Einstein", "Einstien");
    assert!(sim > 0.7 && sim < 1.0);
    
    // Very different strings
    assert!(metric.compute("abc", "xyz") < 0.5);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_string_handling() {
    assert_eq!(levenshtein("", ""), 0);
    assert_eq!(levenshtein("", "hello"), 5);
    assert_eq!(levenshtein("hello", ""), 5);
    
    // Normalized should handle empty gracefully
    assert!((normalized_edit_distance("", "") - 0.0).abs() < 0.001);
}

#[test]
fn test_unicode_edge_cases() {
    // Emoji (multi-codepoint)
    assert_eq!(levenshtein("🎉", "🎉"), 0);
    assert_eq!(levenshtein("🎉🎊", "🎉"), 1);

    // Mixed scripts
    let dist = levenshtein("Dr. 田中", "Dr. Tanaka");
    assert!(dist > 0);

    // Combining characters
    let _composed = "café";
    let _decomposed = "cafe\u{0301}";
    // These may or may not be equal depending on normalization
    // Document current behavior rather than assert equality
}

#[test]
fn test_wildcard_edge_cases() {
    // Multiple wildcards
    assert_eq!(edit_distance_wildcards("*", "anything"), 0);
    assert_eq!(edit_distance_wildcards("*", ""), 0);
    
    // Consecutive wildcards
    assert_eq!(edit_distance_wildcards("**", "hello"), 0);
    assert_eq!(edit_distance_wildcards("a**b", "ab"), 0);
    assert_eq!(edit_distance_wildcards("a**b", "aXXXb"), 0);
    
    // ? vs * behavior
    assert_eq!(edit_distance_wildcards("a?c", "abc"), 0);
    assert!(edit_distance_wildcards("a?c", "abbc") > 0); // ? matches exactly one
    assert_eq!(edit_distance_wildcards("a*c", "abbc"), 0); // * matches multiple
}

// =============================================================================
// Performance Smoke Tests
// =============================================================================

#[test]
fn test_long_string_performance() {
    // Should complete in reasonable time
    let long_a: String = "a".repeat(1000);
    let long_b: String = "b".repeat(1000);
    
    let dist = levenshtein(&long_a, &long_b);
    assert_eq!(dist, 1000); // All substitutions
    
    // Also test normalized
    let norm = normalized_edit_distance(&long_a, &long_b);
    assert!(norm > 0.5 && norm <= 1.0);
}

#[test]
fn test_candidate_generation_performance() {
    let gen = DictionaryCandidateGenerator::new()
        .with_metric(SimilarityMetric::EditDistance)
        .with_well_known();

    // Generate candidates for multiple queries
    let queries = vec![
        "einstein",
        "curie",
        "newton",
        "darwin",
        "tesla",
    ];

    for query in queries {
        let candidates = gen.generate(query, "", None, 5);
        // Should complete quickly and return results
        assert!(candidates.len() <= 5);
    }
}

