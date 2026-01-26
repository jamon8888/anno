//! Regression test: Single letters should not be detected as Person entities
//!
//! This test ensures that the fix for heuristic NER false positives works correctly.
//! Single letters like "A", "B", "C" should not be classified as Person entities.

use anno::{HeuristicNER, Model};
use anno_core::EntityType;

#[test]
fn test_single_letters_not_persons() {
    let ner = HeuristicNER::new();

    // Test cases that previously caused false positives
    let test_cases = [
        "A, B, C are variables",     // Should extract nothing
        "The letter A is important", // Should extract nothing
        "Options: A, B, or C",       // Should extract nothing
        "Choose A or B",             // Should extract nothing
    ];

    for text in test_cases {
        let entities = ner.extract_entities(text, None).unwrap();

        // Filter to only Person entities
        let person_entities: Vec<_> = entities
            .iter()
            .filter(|e| matches!(e.entity_type, EntityType::Person))
            .collect();

        assert_eq!(
            person_entities.len(),
            0,
            "Text: '{}' should not extract single letters as Person entities. Found: {:?}",
            text,
            person_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_single_letters_with_context() {
    let ner = HeuristicNER::new();

    // Single letters in context that might be legitimate (but still shouldn't be Person)
    let text = "The variables A, B, and C represent different values. A is the first variable.";
    let entities = ner.extract_entities(text, None).unwrap();

    let person_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, anno_core::EntityType::Person))
        .collect();

    // Should not extract "A", "B", "C" as Person entities
    assert_eq!(
        person_entities.len(),
        0,
        "Single letters in variable context should not be Person entities. Found: {:?}",
        person_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
}

#[test]
fn test_legitimate_single_letter_names() {
    let ner = HeuristicNER::new();

    // Edge case: Some legitimate names might be single letters (e.g., "I" as a name)
    // But these are extremely rare and should be filtered out by the heuristic
    let text = "The character I in the novel represents the narrator.";
    let entities = ner.extract_entities(text, None).unwrap();

    let person_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, anno_core::EntityType::Person))
        .collect();

    // Should not extract "I" as Person (it's a pronoun or character reference, not a name)
    assert_eq!(
        person_entities.len(),
        0,
        "Single letter 'I' should not be extracted as Person. Found: {:?}",
        person_entities.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
}
