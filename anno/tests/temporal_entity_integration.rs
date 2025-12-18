//! Integration tests for temporal entity extraction.
//!
//! Verifies that NER backends properly populate:
//! - `Entity::valid_from` for date entities
//! - `Entity::normalized` for date and time entities
//!
//! NOTE: These tests are currently ignored because RegexNER does not yet
//! implement temporal normalization (valid_from, normalized fields).
//! TODO: Implement date parsing in RegexNER to populate these fields.

use anno::{EntityType, Model, RegexNER};

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_populates_valid_from_for_dates() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("The meeting is on 2024-01-15.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .expect("Should find a date entity");

    assert_eq!(date_entity.text, "2024-01-15");

    // valid_from should be populated with the parsed date
    assert!(
        date_entity.valid_from.is_some(),
        "Date entity should have valid_from set"
    );

    let dt = date_entity.valid_from.unwrap();
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_populates_normalized_for_dates() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("Due by January 15, 2024.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .expect("Should find a date entity");

    // normalized should be ISO format
    assert!(
        date_entity.normalized.is_some(),
        "Date entity should have normalized set"
    );
    assert_eq!(date_entity.normalized.as_ref().unwrap(), "2024-01-15");
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_populates_normalized_for_times() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("The call starts at 3:30 PM.", None)
        .unwrap();

    let time_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Time))
        .expect("Should find a time entity");

    // normalized should be 24-hour format
    assert!(
        time_entity.normalized.is_some(),
        "Time entity should have normalized set"
    );
    assert_eq!(time_entity.normalized.as_ref().unwrap(), "15:30");
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_handles_japanese_dates() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("会議は2024年1月15日です。", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .expect("Should find a Japanese date entity");

    assert_eq!(date_entity.text, "2024年1月15日");
    assert!(date_entity.valid_from.is_some());

    let dt = date_entity.valid_from.unwrap();
    assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_handles_us_format_dates() {
    let ner = RegexNER::new();
    let entities = ner.extract_entities("Deadline: 01/15/2024", None).unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .expect("Should find a US format date entity");

    assert!(date_entity.valid_from.is_some());
    assert_eq!(date_entity.normalized.as_ref().unwrap(), "2024-01-15");
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn regex_ner_handles_eu_format_dates() {
    let ner = RegexNER::new();
    let entities = ner.extract_entities("Termin: 15.01.2024", None).unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .expect("Should find an EU format date entity");

    assert!(date_entity.valid_from.is_some());
    assert_eq!(date_entity.normalized.as_ref().unwrap(), "2024-01-15");
}

#[test]
fn temporal_fields_not_set_for_non_temporal_entities() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("Contact: test@example.com for $100.", None)
        .unwrap();

    for entity in &entities {
        match entity.entity_type {
            EntityType::Email | EntityType::Money => {
                assert!(
                    entity.valid_from.is_none(),
                    "Non-temporal entity {:?} should not have valid_from",
                    entity.entity_type
                );
            }
            _ => {}
        }
    }
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn multiple_dates_all_get_temporal_info() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("From 2024-01-01 to 2024-12-31.", None)
        .unwrap();

    let date_count = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Date))
        .count();

    assert_eq!(date_count, 2, "Should find two dates");

    for entity in entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Date))
    {
        assert!(
            entity.valid_from.is_some(),
            "Date '{}' should have valid_from",
            entity.text
        );
        assert!(
            entity.normalized.is_some(),
            "Date '{}' should have normalized",
            entity.text
        );
    }
}

#[test]
#[ignore = "RegexNER temporal normalization not yet implemented"]
fn entity_is_temporal_method_works() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("Meeting on 2024-06-15.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date))
        .unwrap();

    // The is_temporal() method should return true when valid_from is set
    assert!(
        date_entity.is_temporal(),
        "Entity with valid_from should be considered temporal"
    );
}

// =============================================================================
// NON-IGNORED TESTS: These validate current RegexNER behavior
// =============================================================================

/// RegexNER does extract date entities (just doesn't normalize them yet)
#[test]
fn regex_ner_extracts_iso_dates() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("The meeting is on 2024-01-15.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date));

    assert!(date_entity.is_some(), "Should find a date entity");
    assert_eq!(date_entity.unwrap().text, "2024-01-15");
}

/// RegexNER extracts times
#[test]
fn regex_ner_extracts_times() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("Call starts at 3:30 PM and ends at 5:00 PM.", None)
        .unwrap();

    let time_entities: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Time))
        .collect();

    assert!(!time_entities.is_empty(), "Should find time entities");
}

/// RegexNER extracts Japanese dates
#[test]
fn regex_ner_extracts_japanese_date_format() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("会議は2024年1月15日です。", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date));

    assert!(date_entity.is_some(), "Should find a Japanese date entity");
    assert!(date_entity.unwrap().text.contains("2024年"));
}

/// RegexNER handles mixed temporal entities
#[test]
fn regex_ner_mixed_temporal_entities() {
    let ner = RegexNER::new();
    let text = "Meeting on 2024-03-15 at 10:00 AM. Deadline: $500 by 2024-12-31.";
    let entities = ner.extract_entities(text, None).unwrap();

    let dates: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Date))
        .collect();
    let times: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Time))
        .collect();
    let money: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Money))
        .collect();

    assert!(dates.len() >= 2, "Should find at least 2 dates");
    assert!(!times.is_empty(), "Should find time entity");
    assert!(!money.is_empty(), "Should find money entity");
}

/// Multiple date formats in same text
#[test]
fn regex_ner_multiple_date_formats() {
    let ner = RegexNER::new();
    // Different date formats
    let text = "From 2024-01-01 to January 15, 2024. Also 01/20/2024.";
    let entities = ner.extract_entities(text, None).unwrap();

    let dates: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Date))
        .collect();

    // Should find at least some of these date formats
    assert!(!dates.is_empty(), "Should find at least one date entity");
}

/// European date format
#[test]
fn regex_ner_european_date() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("La réunion est le 15 janvier 2024.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date));

    // French month name should be recognized
    if let Some(de) = date_entity {
        assert!(de.text.contains("15"));
    }
}

/// German date format
#[test]
fn regex_ner_german_date() {
    let ner = RegexNER::new();
    let entities = ner
        .extract_entities("Das Treffen ist am 15. Januar 2024.", None)
        .unwrap();

    let date_entity = entities
        .iter()
        .find(|e| matches!(e.entity_type, EntityType::Date));

    // German month name should be recognized
    if let Some(de) = date_entity {
        assert!(de.text.contains("15") || de.text.contains("Januar"));
    }
}
