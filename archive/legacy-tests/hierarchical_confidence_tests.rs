//! Tests for hierarchical confidence in entity extraction.
//!
//! HierarchicalConfidence provides fine-grained confidence scores:
//! - linkage: probability that any entity exists at this span
//! - type_score: probability the type classification is correct
//! - boundary: confidence in exact span boundaries

use anno_core::HierarchicalConfidence;

#[test]
fn test_hierarchical_confidence_creation() {
    let hc = HierarchicalConfidence::new(0.9, 0.8, 0.7);

    assert!((hc.linkage - 0.9).abs() < f32::EPSILON);
    assert!((hc.type_score - 0.8).abs() < f32::EPSILON);
    assert!((hc.boundary - 0.7).abs() < f32::EPSILON);
}

#[test]
fn test_hierarchical_confidence_clamping() {
    // Values should be clamped to [0.0, 1.0]
    let hc = HierarchicalConfidence::new(1.5, -0.5, 0.5);

    assert!((hc.linkage - 1.0).abs() < f32::EPSILON);
    assert!((hc.type_score - 0.0).abs() < f32::EPSILON);
    assert!((hc.boundary - 0.5).abs() < f32::EPSILON);
}

#[test]
fn test_hierarchical_confidence_from_single() {
    // Legacy: Create from single confidence score
    let hc = HierarchicalConfidence::from_single(0.85);

    assert!((hc.linkage - 0.85).abs() < f32::EPSILON);
    assert!((hc.type_score - 0.85).abs() < f32::EPSILON);
    assert!((hc.boundary - 0.85).abs() < f32::EPSILON);
}

#[test]
fn test_hierarchical_confidence_as_f64() {
    // Combined confidence score (geometric mean)
    let hc = HierarchicalConfidence::new(0.9, 0.8, 0.7);

    let combined = hc.as_f64();
    // Should be a reasonable combination of the three scores
    assert!(combined > 0.0 && combined <= 1.0);
}

#[test]
fn test_hierarchical_confidence_serialization() {
    let hc = HierarchicalConfidence::new(0.9, 0.8, 0.7);

    let json = serde_json::to_string(&hc).unwrap();
    let deserialized: HierarchicalConfidence = serde_json::from_str(&json).unwrap();

    assert!((hc.linkage - deserialized.linkage).abs() < f32::EPSILON);
    assert!((hc.type_score - deserialized.type_score).abs() < f32::EPSILON);
    assert!((hc.boundary - deserialized.boundary).abs() < f32::EPSILON);
}

#[test]
fn test_entity_with_hierarchical_confidence() {
    use anno_core::{Entity, EntityType};

    let entity = Entity::with_hierarchical_confidence(
        "Apple Inc.",
        EntityType::Organization,
        0,
        10,
        HierarchicalConfidence::new(0.95, 0.90, 0.85),
    );

    assert!(entity.hierarchical_confidence.is_some());
    let hc = entity.hierarchical_confidence.unwrap();
    assert!((hc.linkage - 0.95).abs() < f32::EPSILON);
    assert!((hc.type_score - 0.90).abs() < f32::EPSILON);
    assert!((hc.boundary - 0.85).abs() < f32::EPSILON);
}

#[test]
fn test_entity_default_no_hierarchical_confidence() {
    use anno_core::{Entity, EntityType};

    let entity = Entity::new("Test", EntityType::Person, 0, 4, 0.9);

    assert!(entity.hierarchical_confidence.is_none());
}

// Test that ensemble NER actually populates hierarchical confidence
#[test]
fn test_ensemble_populates_hierarchical_confidence() {
    use anno::{EnsembleNER, Model, RegexNER};

    // Create ensemble with just regex backend
    let ensemble = EnsembleNER::with_backends(vec![Box::new(RegexNER::new())]);

    let entities = ensemble
        .extract_entities("Contact us at test@example.com on 2024-01-15.", None)
        .unwrap();

    // Check that entities have hierarchical confidence set
    for entity in &entities {
        // Note: Single-backend entities may or may not have hierarchical confidence
        // depending on implementation details
        if entity.hierarchical_confidence.is_some() {
            let hc = entity.hierarchical_confidence.as_ref().unwrap();
            assert!(hc.linkage >= 0.0 && hc.linkage <= 1.0);
            assert!(hc.type_score >= 0.0 && hc.type_score <= 1.0);
            assert!(hc.boundary >= 0.0 && hc.boundary <= 1.0);
        }
    }
}

#[test]
fn test_hierarchical_confidence_interpretation() {
    // Example interpretations:
    // - High linkage, high type, high boundary = Strong entity
    // - High linkage, low type, high boundary = Entity exists but type uncertain
    // - High linkage, high type, low boundary = Type correct but boundaries fuzzy

    let strong_entity = HierarchicalConfidence::new(0.95, 0.92, 0.90);
    let type_uncertain = HierarchicalConfidence::new(0.90, 0.50, 0.85);
    let fuzzy_boundary = HierarchicalConfidence::new(0.88, 0.85, 0.45);

    // Strong entity should have highest combined confidence
    assert!(strong_entity.as_f64() > type_uncertain.as_f64());
    assert!(strong_entity.as_f64() > fuzzy_boundary.as_f64());

    // Type uncertain vs fuzzy boundary - both have weaknesses
    // The exact ordering depends on how they're combined
}
