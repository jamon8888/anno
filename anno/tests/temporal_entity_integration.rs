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
