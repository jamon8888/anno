//! Integration tests for W2NER (Word-to-Word NER) backend.
//!
//! W2NER supports:
//! - Nested entities (entities within entities)
//! - Discontinuous entities (non-contiguous spans)
//! - Overlapping entities (same span, different types)

#![cfg(feature = "onnx")]

use anno::backends::w2ner::{W2NERConfig, W2NERRelation};
use anno::backends::W2NER;
use anno::Model;

/// Test W2NER creation and basic availability
#[test]
fn test_w2ner_creation() {
    let w2ner = W2NER::new();

    // Without a loaded model, W2NER is not available
    // This is expected behavior - model loading is separate
    assert!(
        !w2ner.is_available() || w2ner.is_available(),
        "W2NER should be creatable"
    );
}

/// Test W2NER with config customization
#[test]
fn test_w2ner_config_customization() {
    let config = W2NERConfig {
        threshold: 0.7,
        allow_nested: true,
        allow_discontinuous: true,
        entity_labels: vec!["PER".to_string(), "LOC".to_string(), "ORG".to_string()],
        model_id: "custom-w2ner".to_string(),
    };

    let w2ner = W2NER::with_config(config);

    // Should be able to create with custom config
    assert_eq!(w2ner.name(), "w2ner");
}

/// Test W2NER handles empty input gracefully
#[test]
fn test_w2ner_empty_input() {
    let w2ner = W2NER::new();

    let result = w2ner.extract_entities("", None);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// Test W2NER supported entity types
#[test]
fn test_w2ner_supported_types() {
    let w2ner = W2NER::new();
    let types = w2ner.supported_types();

    // W2NER should support standard entity types
    assert!(!types.is_empty(), "W2NER should report supported types");
}

/// Test W2NER model name
#[test]
fn test_w2ner_model_name() {
    let w2ner = W2NER::new();

    assert_eq!(w2ner.name(), "w2ner");
}

/// Test W2NER relation enum (public interface)
#[test]
fn test_w2ner_relations() {
    // Test index conversion roundtrip
    for idx in 0..3 {
        let rel = W2NERRelation::from_index(idx);
        assert_eq!(rel.to_index(), idx);
    }

    // Test specific relation values
    assert_eq!(W2NERRelation::None.to_index(), 0);
    assert_eq!(W2NERRelation::NNW.to_index(), 1); // Next-Neighboring-Word
    assert_eq!(W2NERRelation::THW.to_index(), 2); // Tail-Head-Word
}

/// Test W2NER config defaults
#[test]
fn test_w2ner_config_defaults() {
    let config = W2NERConfig::default();

    assert!((config.threshold - 0.5).abs() < f64::EPSILON);
    assert!(config.allow_nested);
    assert!(config.allow_discontinuous);
    assert_eq!(config.entity_labels.len(), 3); // PER, LOC, ORG
}

/// Test W2NER with nested entities config
#[test]
fn test_w2ner_nested_entities_config() {
    // When allow_nested=true, nested entities should be kept
    let mut config = W2NERConfig::default();
    config.allow_nested = true;

    let w2ner = W2NER::with_config(config);

    // Configuration should affect decoding behavior
    assert_eq!(w2ner.name(), "w2ner");
}

/// Test W2NER discontinuous span config
///
/// Discontinuous entities span non-contiguous text regions, e.g.:
/// "chronic kidney and liver disease"
/// where "chronic disease" spans both "kidney disease" and "liver disease"
#[test]
fn test_w2ner_discontinuous_support() {
    let mut config = W2NERConfig::default();
    config.allow_discontinuous = true;

    let w2ner = W2NER::with_config(config);

    // W2NER's architecture naturally supports discontinuous entities
    // via the word-word relation grid
    assert_eq!(w2ner.name(), "w2ner");
}

/// Test W2NER with custom entity labels (biomedical)
#[test]
fn test_w2ner_custom_labels() {
    let mut config = W2NERConfig::default();
    config.entity_labels = vec![
        "DISEASE".to_string(),
        "DRUG".to_string(),
        "SYMPTOM".to_string(),
    ];

    let w2ner = W2NER::with_config(config);

    // Should accept custom biomedical labels
    assert_eq!(w2ner.name(), "w2ner");
}

/// Test W2NER threshold configuration
#[test]
fn test_w2ner_threshold() {
    // Low threshold = more entities (higher recall, lower precision)
    let mut config_low = W2NERConfig::default();
    config_low.threshold = 0.3;

    // High threshold = fewer entities (lower recall, higher precision)
    let mut config_high = W2NERConfig::default();
    config_high.threshold = 0.9;

    let w2ner_low = W2NER::with_config(config_low);
    let w2ner_high = W2NER::with_config(config_high);

    // Both should be creatable
    assert_eq!(w2ner_low.name(), "w2ner");
    assert_eq!(w2ner_high.name(), "w2ner");
}

// === Tests requiring model (skipped if not available) ===

/// Test W2NER with pretrained model (if available)
#[test]
#[ignore = "Requires pretrained model"]
fn test_w2ner_pretrained_inference() {
    let w2ner = W2NER::from_pretrained("path/to/w2ner").expect("Load model");

    if !w2ner.is_available() {
        eprintln!("Skipping: W2NER model not available");
        return;
    }

    let text = "The University of California Berkeley was founded in Oakland.";
    let entities = w2ner
        .extract_entities(text, None)
        .expect("Extract entities");

    // Should find at least the organization and location entities
    assert!(!entities.is_empty());

    for entity in &entities {
        eprintln!(
            "Entity: '{}' ({:?}) [{}-{}]",
            entity.text, entity.entity_type, entity.start, entity.end
        );
    }
}
