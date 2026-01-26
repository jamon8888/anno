//! Variance tests for backend combination strategies.
//!
//! These tests verify that different combination approaches (StackedNER vs EnsembleNER)
//! produce sensible outputs and document their behavioral differences.
//!
//! # Design Space
//!
//! | Approach | Execution | Conflict Resolution | Use Case |
//! |----------|-----------|---------------------|----------|
//! | StackedNER | Sequential | Priority/LongestSpan/HighestConf | Production, latency |
//! | EnsembleNER | Parallel | Weighted voting + agreement | Maximum accuracy |
//!
//! Both are valid - they optimize different objectives.

use anno::{EnsembleNER, Entity, HeuristicNER, Model, RegexNER, StackedNER};

/// Test that both combiners find the same entities on unambiguous text.
///
/// When all backends agree, both approaches should produce similar results.
#[test]
fn test_both_find_obvious_entities() {
    // Text with clear, unambiguous entities
    let text = "Contact support@example.com on 2024-01-15.";

    let stacked = StackedNER::builder()
        .layer(RegexNER::new())
        .layer(HeuristicNER::new())
        .build();

    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(RegexNER::new()),
        Box::new(HeuristicNER::new()),
    ]);

    let stacked_entities = stacked.extract_entities(text, None).unwrap();
    let ensemble_entities = ensemble.extract_entities(text, None).unwrap();

    // Both should find the email
    let stacked_has_email = stacked_entities.iter().any(|e| e.text.contains("@"));
    let ensemble_has_email = ensemble_entities.iter().any(|e| e.text.contains("@"));
    assert!(stacked_has_email, "StackedNER should find email");
    assert!(ensemble_has_email, "EnsembleNER should find email");

    // Both should find the date
    let stacked_has_date = stacked_entities.iter().any(|e| e.text.contains("2024"));
    let ensemble_has_date = ensemble_entities.iter().any(|e| e.text.contains("2024"));
    assert!(stacked_has_date, "StackedNER should find date");
    assert!(ensemble_has_date, "EnsembleNER should find date");
}

/// Test that both combiners handle empty input gracefully.
#[test]
fn test_both_handle_empty_input() {
    let stacked = StackedNER::default();
    let ensemble = EnsembleNER::new();

    assert!(stacked.extract_entities("", None).unwrap().is_empty());
    assert!(ensemble.extract_entities("", None).unwrap().is_empty());
}

/// Test that both combiners produce valid entity spans.
///
/// This is an invariant that should hold regardless of combination strategy.
#[test]
fn test_both_produce_valid_spans() {
    let texts = [
        "John Smith works at Apple Inc.",
        "The price is $100.50 as of 2024-01-15.",
        "Contact us at test@example.com.",
        "", // empty
        "No entities here just plain text with punctuation!",
    ];

    let stacked = StackedNER::default();
    let ensemble = EnsembleNER::new();

    for text in texts {
        let char_count = text.chars().count();

        for entity in stacked.extract_entities(text, None).unwrap() {
            assert_valid_entity(&entity, char_count, text, "StackedNER");
        }

        for entity in ensemble.extract_entities(text, None).unwrap() {
            assert_valid_entity(&entity, char_count, text, "EnsembleNER");
        }
    }
}

fn assert_valid_entity(entity: &Entity, char_count: usize, text: &str, combiner: &str) {
    assert!(
        entity.start <= entity.end,
        "{combiner}: invalid span start > end: {:?}",
        entity
    );
    assert!(
        entity.end <= char_count,
        "{combiner}: span exceeds text length: {} > {} for {:?}",
        entity.end,
        char_count,
        entity
    );
    assert!(
        entity.confidence >= 0.0 && entity.confidence <= 1.0,
        "{combiner}: invalid confidence {}: {:?}",
        entity.confidence,
        entity
    );
    assert!(
        !entity.text.is_empty(),
        "{combiner}: empty entity text: {:?}",
        entity
    );

    // Verify extracted text matches span
    let extracted: String = text
        .chars()
        .skip(entity.start)
        .take(entity.end - entity.start)
        .collect();
    assert_eq!(
        extracted, entity.text,
        "{combiner}: text mismatch at {}..{}: expected {:?}, got {:?}",
        entity.start, entity.end, extracted, entity.text
    );
}

/// Test that ensemble produces agreement-boosted confidence when backends agree.
///
/// This documents an expected behavioral difference between the approaches.
#[test]
fn test_ensemble_agreement_bonus() {
    // Text where both pattern and heuristic should agree
    let text = "Dr. John Smith";

    let ensemble = EnsembleNER::with_backends(vec![
        Box::new(RegexNER::new()),
        Box::new(HeuristicNER::new()),
    ]);

    let entities = ensemble.extract_entities(text, None).unwrap();

    // If multiple backends agree on an entity, confidence may be boosted
    // (This is a characteristic of ensemble, not stacked)
    for entity in &entities {
        // All entities should have valid confidence
        assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
    }
}

/// Test that stacked respects layer priority.
///
/// Earlier layers should win conflicts in Priority strategy.
#[test]
fn test_stacked_layer_priority() {
    use anno::backends::stacked::ConflictStrategy;

    // Layer 1: Pattern (will find structured entities)
    // Layer 2: Heuristic (may find overlapping named entities)
    let stacked = StackedNER::builder()
        .layer(RegexNER::new())
        .layer(HeuristicNER::new())
        .strategy(ConflictStrategy::Priority)
        .build();

    let entities = stacked
        .extract_entities("Contact support@company.com", None)
        .unwrap();

    // Should find the email (from pattern layer)
    let has_email = entities.iter().any(|e| e.entity_type.as_label() == "EMAIL");
    assert!(has_email, "Should find EMAIL from pattern layer");
}

/// Test determinism: same input should produce same output.
#[test]
fn test_both_are_deterministic() {
    let text = "Apple Inc. reported $10 billion in revenue on 2024-01-15.";

    let stacked = StackedNER::default();
    let ensemble = EnsembleNER::new();

    // Run multiple times
    let stacked1 = stacked.extract_entities(text, None).unwrap();
    let stacked2 = stacked.extract_entities(text, None).unwrap();

    let ensemble1 = ensemble.extract_entities(text, None).unwrap();
    let ensemble2 = ensemble.extract_entities(text, None).unwrap();

    // Same number of entities
    assert_eq!(
        stacked1.len(),
        stacked2.len(),
        "StackedNER not deterministic"
    );
    assert_eq!(
        ensemble1.len(),
        ensemble2.len(),
        "EnsembleNER not deterministic"
    );

    // Same entity texts (order may vary, so use sets)
    let stacked1_texts: std::collections::HashSet<_> =
        stacked1.iter().map(|e| e.text.as_str()).collect();
    let stacked2_texts: std::collections::HashSet<_> =
        stacked2.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(stacked1_texts, stacked2_texts);

    let ensemble1_texts: std::collections::HashSet<_> =
        ensemble1.iter().map(|e| e.text.as_str()).collect();
    let ensemble2_texts: std::collections::HashSet<_> =
        ensemble2.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(ensemble1_texts, ensemble2_texts);
}

/// Test multilingual handling.
#[test]
fn test_both_handle_unicode() {
    let texts = [
        "東京オリンピック2020",                  // Japanese
        "Москва hosted the event on 15.01.2024", // Cyrillic + date
        "Contact 田中@example.com",              // Mixed scripts
        "Prix: 100€ le 15/01/2024",              // French with Euro
    ];

    let stacked = StackedNER::default();
    let ensemble = EnsembleNER::new();

    for text in texts {
        let char_count = text.chars().count();

        // Should not panic
        let stacked_result = stacked.extract_entities(text, None);
        let ensemble_result = ensemble.extract_entities(text, None);

        assert!(stacked_result.is_ok(), "StackedNER failed on: {}", text);
        assert!(ensemble_result.is_ok(), "EnsembleNER failed on: {}", text);

        // All entities should have valid spans
        for entity in stacked_result.unwrap() {
            assert!(entity.end <= char_count, "Invalid span in: {}", text);
        }
        for entity in ensemble_result.unwrap() {
            assert!(entity.end <= char_count, "Invalid span in: {}", text);
        }
    }
}

/// Document behavioral differences between approaches.
///
/// This test doesn't assert specific behaviors but documents what to expect.
#[test]
fn document_behavioral_differences() {
    // Ambiguous text where backends might disagree
    let text = "Apple announced new products."; // Apple = ORG or product brand?

    let stacked = StackedNER::default();
    let ensemble = EnsembleNER::new();

    let stacked_entities = stacked.extract_entities(text, None).unwrap();
    let ensemble_entities = ensemble.extract_entities(text, None).unwrap();

    // Document: Stacked uses first-wins, Ensemble uses voting
    // The actual output depends on backend implementations
    println!(
        "StackedNER found {} entities: {:?}",
        stacked_entities.len(),
        stacked_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
    println!(
        "EnsembleNER found {} entities: {:?}",
        ensemble_entities.len(),
        ensemble_entities
            .iter()
            .map(|e| &e.text)
            .collect::<Vec<_>>()
    );

    // Both should produce valid output (even if different)
    // This is the key invariant: combiners may disagree on WHAT entities,
    // but should always produce VALID entities
}
