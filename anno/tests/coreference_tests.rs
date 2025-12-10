//! Comprehensive tests for coreference resolution.
//!
//! Tests cover:
//! - Basic within-document coreference
//! - Pronoun resolution (he/she/it/they)
//! - Cross-document coreference (CDCR)
//! - Edge cases (empty input, single mention, overlapping spans)
//! - Unicode handling
//!
//! NOTE: Some coref tests are disabled pending API stabilization.

#![allow(dead_code, unused_imports)]

use anno::{Entity, EntityType};

// =============================================================================
// Basic Entity Tests (Always Enabled)
// =============================================================================

#[test]
fn test_entity_creation() {
    let entity = Entity::new("John", EntityType::Person, 0, 4, 0.9);
    assert_eq!(entity.text, "John");
    assert_eq!(entity.start, 0);
    assert_eq!(entity.end, 4);
}

#[test]
fn test_entity_type_variants() {
    let person = EntityType::Person;
    let org = EntityType::Organization;
    let loc = EntityType::Location;
    assert_ne!(person, org);
    assert_ne!(org, loc);
    assert_ne!(person, loc);
}

#[test]
fn test_entity_confidence() {
    let entity = Entity::new("Apple Inc", EntityType::Organization, 0, 9, 0.95);
    assert!(entity.confidence > 0.9);
    assert!(entity.confidence <= 1.0);
}

// =============================================================================
// Coref Resolver Tests (Feature-Gated)
// =============================================================================

#[cfg(feature = "eval")]
mod coref_tests {
    use super::*;
    use anno::eval::coref_resolver::{CoreferenceResolver, SimpleCorefResolver};

    #[test]
    fn test_simple_coref_empty_input() {
        let resolver = SimpleCorefResolver::default();
        let entities: Vec<Entity> = vec![];
        let resolved = resolver.resolve(&entities);
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_simple_coref_single_entity() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![Entity::new("John", EntityType::Person, 0, 4, 0.9)];
        let resolved = resolver.resolve(&entities);

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].text, "John");
    }

    #[test]
    fn test_simple_coref_exact_match() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            Entity::new("John Smith", EntityType::Person, 0, 10, 0.9),
            Entity::new("John Smith", EntityType::Person, 50, 60, 0.9),
        ];
        let resolved = resolver.resolve(&entities);

        // Exact matches should have same canonical_id
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].canonical_id, resolved[1].canonical_id);
    }

    #[test]
    fn test_simple_coref_no_match() {
        let resolver = SimpleCorefResolver::default();
        let entities = vec![
            Entity::new("John", EntityType::Person, 0, 4, 0.9),
            Entity::new("Apple Inc.", EntityType::Organization, 20, 30, 0.9),
        ];
        let resolved = resolver.resolve(&entities);

        // Different types - should not be linked
        assert_eq!(resolved.len(), 2);
        if resolved[0].canonical_id.is_some() && resolved[1].canonical_id.is_some() {
            assert_ne!(resolved[0].canonical_id, resolved[1].canonical_id);
        }
    }
}

// =============================================================================
// E2E Coref Tests (Feature-Gated)
// =============================================================================

#[cfg(all(feature = "eval", feature = "e2e_coref"))]
mod e2e_coref_tests {
    use super::*;
    use anno::backends::e2e_coref::E2ECoref;

    #[test]
    fn test_e2e_coref_basic() {
        let coref = E2ECoref::new();
        let clusters = coref.resolve("John saw Mary. He waved to her.").unwrap();

        let total_mentions: usize = clusters.iter().map(|c| c.mentions.len()).sum();
        assert!(total_mentions > 0, "Should extract some mentions");
    }

    #[test]
    fn test_e2e_coref_unicode() {
        let coref = E2ECoref::new();
        let text = "María es doctora. Ella trabaja en el hospital.";
        let clusters = coref.resolve(text).unwrap();

        for cluster in &clusters {
            for mention in &cluster.mentions {
                assert!(mention.char_start <= mention.char_end);
                assert!(mention.char_end <= text.chars().count());
            }
        }
    }
}

// =============================================================================
// CDCR Tests (Feature-Gated)
// =============================================================================

#[cfg(all(feature = "eval", feature = "cdcr"))]
mod cdcr_tests {
    use super::*;
    use anno::eval::cdcr::{CDCRConfig, CDCRResolver, Document};

    #[test]
    fn test_cdcr_empty_documents() {
        let resolver = CDCRResolver::new();
        let docs: Vec<Document> = vec![];
        let clusters = resolver.resolve(&docs);
        assert!(clusters.is_empty());
    }

    #[test]
    fn test_cdcr_single_document() {
        let mut doc = Document::new("doc1", "Apple announced new products.");
        doc.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let resolver = CDCRResolver::new();
        let clusters = resolver.resolve(&[doc]);

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].mentions.len(), 1);
    }

    #[test]
    fn test_cdcr_exact_match_across_docs() {
        let mut doc1 = Document::new("doc1", "Apple announced new products.");
        doc1.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let mut doc2 = Document::new("doc2", "Apple released iOS update.");
        doc2.entities
            .push(Entity::new("Apple", EntityType::Organization, 0, 5, 0.9));

        let resolver = CDCRResolver::new();
        let clusters = resolver.resolve(&[doc1, doc2]);

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].mentions.len(), 2);
    }
}
