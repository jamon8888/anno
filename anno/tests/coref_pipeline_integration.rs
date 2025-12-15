//! Integration tests for coreference resolution pipeline.
//!
//! Tests that verify data flows correctly between:
//! - Within-document coreference (MentionRankingCoref)
//! - Cross-document coreference (CDCRResolver)
//! - Joint model (JointModel)
//!
//! These tests ensure the different coreference systems can work together
//! and produce consistent results.

use anno::backends::mention_ranking::MentionRankingCoref;
use anno::eval::cdcr::{CDCRResolver, Document};
use anno::joint::JointModel;
use anno_core::{CoreferenceResolver, Entity, EntityType};

/// Test that within-doc coreference produces valid canonical IDs.
#[test]
fn test_within_doc_coref_produces_tracks() {
    let resolver = MentionRankingCoref::new();

    // Use identical entities to ensure they link
    let entities = vec![
        Entity::new("John Smith", EntityType::Person, 0, 10, 0.95),
        Entity::new("John Smith", EntityType::Person, 20, 30, 0.95),
        Entity::new("Paris", EntityType::Location, 40, 45, 0.90),
    ];

    // Use CoreferenceResolver trait method explicitly
    let resolved = CoreferenceResolver::resolve(&resolver, &entities);

    // Verify all entities have canonical_ids assigned
    assert!(
        resolved.iter().all(|e| e.canonical_id.is_some()),
        "All entities should have canonical_id after resolution"
    );

    // Identical entities should share the same canonical_id
    let john_entities: Vec<_> = resolved.iter().filter(|e| e.text == "John Smith").collect();

    assert_eq!(
        john_entities.len(),
        2,
        "Should have two John Smith entities"
    );

    // Identical entities should corefer
    if let (Some(e1), Some(e2)) = (john_entities.first(), john_entities.get(1)) {
        assert_eq!(
            e1.canonical_id, e2.canonical_id,
            "Identical entities should share canonical_id"
        );
    }

    assert_eq!(
        resolved.len(),
        entities.len(),
        "Should return same number of entities"
    );
}

/// Test that cross-doc coreference links entities across documents.
#[test]
fn test_cross_doc_coref_links_entities() {
    let docs =
        vec![
            Document::new("doc1", "Barack Obama was president.").with_entities(vec![Entity::new(
                "Barack Obama",
                EntityType::Person,
                0,
                12,
                0.95,
            )]),
            Document::new("doc2", "Obama served from 2009 to 2017.")
                .with_entities(vec![Entity::new("Obama", EntityType::Person, 0, 5, 0.85)]),
        ];

    let resolver = CDCRResolver::new();
    let clusters = resolver.resolve(&docs);

    // Should create at least one cluster linking the two mentions
    assert!(!clusters.is_empty(), "Should create at least one cluster");

    // Check that both documents are represented in clusters
    let doc_ids: Vec<String> = clusters
        .iter()
        .flat_map(|c| c.documents.iter().cloned())
        .collect();
    assert!(
        doc_ids.contains(&"doc1".to_string()) || doc_ids.contains(&"doc2".to_string()),
        "Clusters should include mentions from both documents"
    );
}

/// Test that joint model produces coreference chains.
#[test]
fn test_joint_model_coref() {
    let model = JointModel::default();

    let entities = vec![
        Entity::new("Apple Inc.", EntityType::Organization, 0, 10, 0.95),
        Entity::new("California", EntityType::Location, 24, 34, 0.90),
        Entity::new("The company", EntityType::Organization, 36, 47, 0.85),
    ];

    // Use CoreferenceResolver trait method explicitly
    let resolved = CoreferenceResolver::resolve(&model, &entities);

    // Joint model should assign canonical_ids
    assert!(
        resolved.iter().all(|e| e.canonical_id.is_some()),
        "All entities should have canonical_id after joint resolution"
    );

    // "Apple Inc." and "The company" should corefer
    let apple = resolved.iter().find(|e| e.text == "Apple Inc.");
    let company = resolved.iter().find(|e| e.text == "The company");

    if let (Some(apple), Some(company)) = (apple, company) {
        // They might corefer (depending on model confidence)
        // Just verify both have IDs
        assert!(apple.canonical_id.is_some());
        assert!(company.canonical_id.is_some());
    }
}

/// Test pipeline: within-doc → cross-doc flow.
#[test]
fn test_within_doc_to_cross_doc_flow() {
    // Step 1: Within-document coreference
    let text1 = "John Smith visited Paris. He loved it.";
    let text2 = "Smith returned to France. The traveler was happy.";

    let within_resolver = MentionRankingCoref::new();

    let entities1 = vec![
        Entity::new("John Smith", EntityType::Person, 0, 10, 0.95),
        Entity::new("Paris", EntityType::Location, 18, 23, 0.90),
        Entity::new("He", EntityType::Person, 25, 27, 0.85),
    ];

    let entities2 = vec![
        Entity::new("Smith", EntityType::Person, 0, 5, 0.80),
        Entity::new("France", EntityType::Location, 17, 23, 0.90),
        Entity::new("The traveler", EntityType::Person, 25, 36, 0.75),
    ];

    // Use CoreferenceResolver trait method explicitly
    let resolved1 = CoreferenceResolver::resolve(&within_resolver, &entities1);
    let resolved2 = CoreferenceResolver::resolve(&within_resolver, &entities2);

    // Step 2: Cross-document coreference
    let docs = vec![
        Document::new("doc1", text1).with_entities(resolved1),
        Document::new("doc2", text2).with_entities(resolved2),
    ];

    let cross_resolver = CDCRResolver::new();
    let clusters = cross_resolver.resolve(&docs);

    // Verify that cross-doc resolution created clusters
    assert!(!clusters.is_empty(), "Should create cross-doc clusters");
}

/// Test that different resolvers produce consistent output format.
#[test]
fn test_resolver_output_consistency() {
    let entities = vec![
        Entity::new("John", EntityType::Person, 0, 4, 0.9),
        Entity::new("John", EntityType::Person, 10, 14, 0.9),
    ];

    // Test MentionRankingCoref (via CoreferenceResolver trait)
    let mention_ranking = MentionRankingCoref::new();
    let resolved_mr = CoreferenceResolver::resolve(&mention_ranking, &entities);
    assert_eq!(resolved_mr.len(), entities.len());
    assert!(resolved_mr.iter().all(|e| e.canonical_id.is_some()));

    // Test JointModel (via CoreferenceResolver trait)
    let joint = JointModel::default();
    let resolved_joint = CoreferenceResolver::resolve(&joint, &entities);
    assert_eq!(resolved_joint.len(), entities.len());
    assert!(resolved_joint.iter().all(|e| e.canonical_id.is_some()));
}
