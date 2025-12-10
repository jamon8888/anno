//! Tests for the AutoNER (automatic backend selection)
//!
//! AutoNER routes to the default model (StackedNER) for consistent behavior.

use anno::backends::router::AutoNER;
use anno::Model;

#[test]
fn test_auto_ner_creation() {
    let auto = AutoNER::new();
    assert!(auto.is_available());
}

#[test]
fn test_auto_ner_default() {
    let auto = AutoNER::default();
    assert_eq!(auto.name(), "auto");
}

#[test]
fn test_auto_ner_description() {
    let auto = AutoNER::new();
    let desc = auto.description();
    assert!(desc.contains("Automatic") || desc.contains("auto"));
}

#[test]
fn test_auto_ner_supported_types() {
    let auto = AutoNER::new();
    let types = auto.supported_types();

    // Should support standard NER types
    assert!(!types.is_empty());
}

#[test]
fn test_auto_ner_extract() {
    let auto = AutoNER::new();
    let text = "Apple Inc. is based in Cupertino.";

    let result = auto.extract_entities(text, None);
    assert!(result.is_ok());

    let entities = result.unwrap();
    // Should find at least Apple (ORG) and Cupertino (LOC)
    assert!(!entities.is_empty());
}

#[test]
fn test_auto_ner_consistency() {
    let auto1 = AutoNER::new();
    let auto2 = AutoNER::new();
    let text = "Tim Cook is the CEO.";

    let result1 = auto1.extract_entities(text, None).unwrap();
    let result2 = auto2.extract_entities(text, None).unwrap();

    // Same model should produce same results
    assert_eq!(result1.len(), result2.len());
}
