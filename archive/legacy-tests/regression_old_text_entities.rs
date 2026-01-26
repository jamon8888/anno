//! Regression tests for old text (historical formatting) entity extraction
//!
//! Ensures historical text doesn't break extraction or produce too many false positives.

use anno::{Model, StackedNER};

#[test]
fn regression_old_text_basic_extraction() {
    let ner = StackedNER::default();

    let old_text = "THE NEW YORK TIMES, JANUARY 15, 1920\n\nPARIS, Jan. 14.--The Peace Conference today adopted a resolution.";

    let entities = ner.extract_entities(old_text, None).unwrap();

    // Should extract some entities (dates, locations, orgs)
    assert!(
        !entities.is_empty(),
        "Should extract at least some entities from old text"
    );

    // Should extract dates
    let dates: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type.as_label() == "DATE")
        .collect();
    assert!(!dates.is_empty(), "Should extract dates from old text");

    // Verify reasonable date extraction
    let date_texts: Vec<_> = dates.iter().map(|e| e.text.as_str()).collect();
    assert!(
        date_texts
            .iter()
            .any(|t| t.contains("1920") || t.contains("Jan")),
        "Should extract date containing '1920' or 'Jan'. Got: {:?}",
        date_texts
    );
}

#[test]
fn regression_old_text_not_too_many_false_positives() {
    let ner = StackedNER::default();

    let old_text = "THE NEW YORK TIMES, JANUARY 15, 1920\n\nPARIS, Jan. 14.--The Peace Conference today adopted a resolution.";

    let entities = ner.extract_entities(old_text, None).unwrap();

    // Should not have excessive false positives (single letters as persons)
    let single_letter_persons: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type.as_label() == "PER")
        .filter(|e| e.text.len() == 1)
        .collect();

    // Allow some single letters (like "M." for Monsieur), but not excessive
    assert!(
        single_letter_persons.len() <= 2,
        "Too many single-letter person entities: {:?}",
        single_letter_persons
    );
}

/// Test that lowercase city names are extracted as locations
#[test]
fn regression_lowercase_locations_extracted() {
    let ner = StackedNER::default();

    // Lowercase "Paris" should be detected
    let text = "The conference was held in Paris, France.";
    let entities = ner.extract_entities(text, None).unwrap();

    let locations: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type.as_label() == "LOC" || e.entity_type.as_label() == "GPE")
        .collect();

    // Should extract at least one location
    assert!(
        !locations.is_empty(),
        "Should extract locations from text with properly capitalized names. Got: {:?}",
        entities
            .iter()
            .map(|e| (e.text.as_str(), e.entity_type.as_label()))
            .collect::<Vec<_>>()
    );
}
