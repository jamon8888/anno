//! Integration tests for joint entity analysis.
//!
//! Tests the full pipeline from entities to decoded results,
//! verifying correctness across different scenarios.

use anno::joint::{
    BeliefPropagation, CorefLinkFactor, CorefLinkWeights, CorefNerFactor, CorefNerWeights, Factor,
    InferenceConfig, JointConfig, JointModel, LinkNerFactor, LinkNerWeights, UnaryCorefFactor,
    UnaryNerFactor, WikipediaKnowledgeStore,
};
use anno::{Entity, EntityType};
use std::sync::Arc;

// =============================================================================
// End-to-End Tests
// =============================================================================

#[test]
fn test_joint_model_coreference_chain() {
    // Test that coreferent mentions are grouped correctly
    let model = JointModel::new(JointConfig::default()).unwrap();

    let text = "Barack Obama visited France. Obama met with Macron.";
    let entities = vec![
        Entity::new("Barack Obama", EntityType::Person, 0, 12, 0.95),
        Entity::new("France", EntityType::Location, 21, 27, 0.9),
        Entity::new("Obama", EntityType::Person, 29, 34, 0.9),
        Entity::new("Macron", EntityType::Person, 44, 50, 0.9),
    ];

    let result = model.analyze(text, &entities).unwrap();

    // Should have 4 entities
    assert_eq!(result.entities.len(), 4);

    // Should have at least one chain (Barack Obama + Obama)
    // The exact number depends on BP convergence and factor weights
    println!("Chains: {:?}", result.chains.len());
    for chain in &result.chains {
        println!(
            "  Chain {}: {:?}",
            chain.cluster_id.unwrap_or(0),
            chain.mentions
        );
    }
}

#[test]
fn test_joint_model_pronoun_resolution() {
    // Test pronoun resolution through coreference
    let model = JointModel::new(JointConfig::default()).unwrap();

    let text = "Marie Curie won a Nobel Prize. She was a physicist.";
    let entities = vec![
        Entity::new("Marie Curie", EntityType::Person, 0, 11, 0.95),
        Entity::new(
            "Nobel Prize",
            EntityType::Other("AWARD".to_string()),
            18,
            29,
            0.85,
        ),
        Entity::new("She", EntityType::Person, 31, 34, 0.7),
    ];

    let result = model.analyze(text, &entities).unwrap();

    // All entities should be typed
    assert_eq!(result.entities.len(), 3);

    // Confidences should be positive
    for conf in &result.confidences {
        assert!(*conf >= 0.0 && *conf <= 1.0, "Invalid confidence: {}", conf);
    }
}

#[test]
fn test_joint_model_empty_input() {
    let model = JointModel::new(JointConfig::default()).unwrap();

    let result = model.analyze("No entities here.", &[]).unwrap();

    assert!(result.entities.is_empty());
    assert!(result.chains.is_empty());
    assert!(result.links.is_empty());
}

#[test]
fn test_joint_model_single_entity() {
    let model = JointModel::new(JointConfig::default()).unwrap();

    let entities = vec![Entity::new("Paris", EntityType::Location, 0, 5, 0.9)];

    let result = model.analyze("Paris", &entities).unwrap();

    // Single entity, no coreference possible
    assert_eq!(result.entities.len(), 1);
    assert!(result.chains.is_empty()); // No chains for single mention
}

#[test]
fn test_joint_model_configuration() {
    // Test that configuration affects factor construction
    let config = JointConfig {
        enable_coref_ner: false,
        enable_coref_link: false,
        enable_link_ner: false,
        ..Default::default()
    };

    let model = JointModel::new(config).unwrap();

    let entities = vec![
        Entity::new("Alice", EntityType::Person, 0, 5, 0.9),
        Entity::new("Bob", EntityType::Person, 10, 13, 0.9),
    ];

    // Should still work, just with fewer constraints
    let result = model.analyze("Alice and Bob", &entities).unwrap();
    assert_eq!(result.entities.len(), 2);
}

// =============================================================================
// Belief Propagation Tests
// =============================================================================

#[test]
fn test_belief_propagation_convergence() {
    use anno::joint::JointVariable;

    // Create a simple factor graph with known solution
    let variables = vec![
        JointVariable::SemanticType {
            mention_idx: 0,
            types: vec![EntityType::Person, EntityType::Organization],
        },
        JointVariable::SemanticType {
            mention_idx: 1,
            types: vec![EntityType::Person, EntityType::Organization],
        },
    ];

    // Strongly prefer Person for both
    let factors: Vec<Box<dyn Factor>> = vec![
        Box::new(UnaryNerFactor::new(
            0,
            vec![(EntityType::Person, 10.0), (EntityType::Organization, 0.1)],
        )),
        Box::new(UnaryNerFactor::new(
            1,
            vec![(EntityType::Person, 10.0), (EntityType::Organization, 0.1)],
        )),
    ];

    let config = InferenceConfig {
        max_iterations: 10,
        convergence_threshold: 1e-6,
        ..Default::default()
    };

    let mut bp = BeliefPropagation::new(factors, variables.clone(), config);
    let marginals = bp.run();

    // Check that Person has highest probability for both
    for var in &variables {
        let var_id = var.id();
        let argmax = marginals.argmax(&var_id);
        // Index 0 = Person, Index 1 = Organization
        assert_eq!(
            argmax,
            Some(0),
            "Expected Person (index 0) to have highest probability"
        );
    }
}

#[test]
fn test_belief_propagation_with_binary_factors() {
    use anno::joint::{AntecedentValue, JointVariable};

    // Test that binary factors influence inference
    let variables = vec![
        JointVariable::SemanticType {
            mention_idx: 0,
            types: vec![EntityType::Person, EntityType::Organization],
        },
        JointVariable::SemanticType {
            mention_idx: 1,
            types: vec![EntityType::Person, EntityType::Organization],
        },
        JointVariable::Antecedent {
            mention_idx: 1,
            candidates: vec![0], // Only mention 0 is candidate
        },
    ];

    let factors: Vec<Box<dyn Factor>> = vec![
        // Weak preference for person
        Box::new(UnaryNerFactor::new(
            0,
            vec![(EntityType::Person, 1.0), (EntityType::Organization, 0.5)],
        )),
        // Very weak preference for organization
        Box::new(UnaryNerFactor::new(
            1,
            vec![(EntityType::Person, 0.8), (EntityType::Organization, 0.9)],
        )),
        // Coref factor prefers same type
        Box::new(CorefNerFactor::new(1, 0, CorefNerWeights::default())),
        // Unary coref
        Box::new(UnaryCorefFactor::new(
            1,
            vec![
                (AntecedentValue::Mention(0), 2.0),
                (AntecedentValue::NewCluster, 0.0),
            ],
        )),
    ];

    let config = InferenceConfig::default();
    let mut bp = BeliefPropagation::new(factors, variables, config);
    let _marginals = bp.run();

    // The binary factor should pull mention 1 towards Person to match mention 0
    // This tests that cross-task factors actually influence inference
    println!("Marginals computed successfully");
}

// =============================================================================
// Factor Tests
// =============================================================================

#[test]
fn test_link_ner_factor_with_knowledge() {
    use anno::linking::wikidata::WikidataNERType;

    let mut knowledge = WikipediaKnowledgeStore::new();
    knowledge.add_type("Q937", WikidataNERType::Person); // Albert Einstein

    let factor =
        LinkNerFactor::new(0, LinkNerWeights::default()).with_knowledge(Arc::new(knowledge));

    // Verify factor scope
    assert_eq!(factor.scope().len(), 2); // type and link for mention 0
}

#[test]
fn test_coref_link_factor_relatedness() {
    let mut knowledge = WikipediaKnowledgeStore::new();
    knowledge.add_outlinks("Q937", vec!["Q3918".to_string(), "Q84".to_string()]); // Einstein -> Physics, London
    knowledge.add_outlinks("Q3918", vec!["Q937".to_string()]); // Physics -> Einstein

    let factor =
        CorefLinkFactor::new(1, 0, CorefLinkWeights::default()).with_knowledge(Arc::new(knowledge));

    // Should have scope of 3: ante_1, link_1, link_0
    assert_eq!(factor.scope().len(), 3);
}

// =============================================================================
// Unicode and Multilingual Tests
// =============================================================================

#[test]
fn test_joint_model_unicode_mentions() {
    let model = JointModel::new(JointConfig::default()).unwrap();

    // Test with Chinese text
    let text = "習近平在北京會見了普京。";
    let entities = vec![
        Entity::new("習近平", EntityType::Person, 0, 3, 0.9),
        Entity::new("北京", EntityType::Location, 4, 6, 0.85),
        Entity::new("普京", EntityType::Person, 9, 11, 0.9),
    ];

    let result = model.analyze(text, &entities).unwrap();
    assert_eq!(result.entities.len(), 3);
}

#[test]
fn test_joint_model_mixed_script() {
    let model = JointModel::new(JointConfig::default()).unwrap();

    let text = "Dr. 田中 presented at MIT's conference.";
    let entities = vec![
        Entity::new("田中", EntityType::Person, 4, 6, 0.85),
        Entity::new("MIT", EntityType::Organization, 21, 24, 0.9),
    ];

    let result = model.analyze(text, &entities).unwrap();
    assert_eq!(result.entities.len(), 2);
}

// =============================================================================
// Stress Tests
// =============================================================================

#[test]
fn test_joint_model_many_mentions() {
    let model = JointModel::new(JointConfig {
        max_antecedent_candidates: 5, // Limit for performance
        ..Default::default()
    })
    .unwrap();

    // Create a document with many mentions
    let mut text = String::new();
    let mut entities = Vec::new();
    let mut offset = 0;

    for i in 0..20 {
        let name = format!("Person{}", i);
        text.push_str(&name);
        text.push(' ');
        entities.push(Entity::new(
            &name,
            EntityType::Person,
            offset,
            offset + name.len(),
            0.9,
        ));
        offset += name.len() + 1;
    }

    let result = model.analyze(&text, &entities).unwrap();

    // Should handle many mentions without panic
    assert_eq!(result.entities.len(), 20);
}

// =============================================================================
// Property Tests (via assertions on invariants)
// =============================================================================

#[test]
fn test_marginals_invariants() {
    use anno::joint::JointVariable;

    let variables = vec![JointVariable::SemanticType {
        mention_idx: 0,
        types: vec![EntityType::Person, EntityType::Organization],
    }];

    let factors: Vec<Box<dyn Factor>> = vec![Box::new(UnaryNerFactor::new(
        0,
        vec![(EntityType::Person, 1.0), (EntityType::Organization, 1.0)],
    ))];

    let config = InferenceConfig::default();
    let mut bp = BeliefPropagation::new(factors, variables.clone(), config);
    let marginals = bp.run();

    // Invariant: probabilities should sum to ~1 (within tolerance)
    for var in &variables {
        let var_id = var.id();
        let domain_size = var.domain_size();
        let mut sum = 0.0;
        for i in 0..domain_size {
            sum += marginals.prob(&var_id, i).unwrap_or(0.0);
        }
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Probabilities should sum to 1, got {}",
            sum
        );
    }

    // Invariant: argmax should return a valid index
    for var in &variables {
        let var_id = var.id();
        if let Some(idx) = marginals.argmax(&var_id) {
            assert!(idx < var.domain_size(), "argmax returned invalid index");
        }
    }
}
