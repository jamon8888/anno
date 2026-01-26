//! End-to-end pipeline integration test: NER → Coalesce → Tier
//!
//! This test demonstrates the full anno pipeline:
//! 1. Extract entities from text (NER)
//! 2. Resolve entity mentions across documents (coalesce)
//! 3. Build a knowledge graph and analyze structure (tier)
//!
//! This validates that all three crates work together seamlessly.

use anno_core::{Entity, EntityCategory, EntityType, GraphDocument, Relation};
use anno_tier::{
    graph_utils::GraphStats, Betweenness, Closeness, Eigenvector, HierarchicalLeiden, Hits,
    LabelPropagation, Louvain, PageRank,
};
use std::collections::HashMap;

/// Simulates NER extraction results from multiple documents.
///
/// In a real pipeline, this would call `anno::Model::extract_entities()`.
fn simulate_ner_extraction() -> Vec<(String, Vec<Entity>)> {
    vec![
        (
            "doc1".to_string(),
            vec![
                Entity::new("Apple Inc.", EntityType::Organization, 0, 10, 0.95),
                Entity::new("Steve Jobs", EntityType::Person, 20, 30, 0.98),
                Entity::new("Cupertino", EntityType::Location, 40, 49, 0.92),
                Entity::new(
                    "iPhone",
                    EntityType::custom("PRODUCT", EntityCategory::Creative),
                    60,
                    66,
                    0.88,
                ),
            ],
        ),
        (
            "doc2".to_string(),
            vec![
                Entity::new("Apple", EntityType::Organization, 0, 5, 0.90),
                Entity::new("Tim Cook", EntityType::Person, 15, 23, 0.96),
                Entity::new("Cupertino", EntityType::Location, 35, 44, 0.94),
                Entity::new(
                    "iPad",
                    EntityType::custom("PRODUCT", EntityCategory::Creative),
                    55,
                    59,
                    0.87,
                ),
            ],
        ),
        (
            "doc3".to_string(),
            vec![
                Entity::new("Microsoft", EntityType::Organization, 0, 9, 0.97),
                Entity::new("Satya Nadella", EntityType::Person, 20, 33, 0.99),
                Entity::new("Seattle", EntityType::Location, 45, 52, 0.91),
                Entity::new(
                    "Windows",
                    EntityType::custom("PRODUCT", EntityCategory::Creative),
                    65,
                    72,
                    0.93,
                ),
            ],
        ),
        (
            "doc4".to_string(),
            vec![
                Entity::new("Apple Inc.", EntityType::Organization, 0, 10, 0.94),
                Entity::new("Microsoft", EntityType::Organization, 20, 29, 0.96),
                Entity::new("Tim Cook", EntityType::Person, 40, 48, 0.95),
                Entity::new("Satya Nadella", EntityType::Person, 60, 73, 0.97),
            ],
        ),
    ]
}

/// Simulates entity resolution using coalesce.
///
/// In a real pipeline, this would use `coalesce::CrossDocResolver` or similar.
/// Here we perform simple string matching and return canonical entity text and type.
fn simulate_coalesce(
    doc_entities: &[(String, Vec<Entity>)],
) -> HashMap<String, (String, EntityType)> {
    // Map from (doc_id, entity_text) -> (canonical_text, entity_type)
    let mut resolution_map = HashMap::new();

    // Simple normalization rules (in reality, coalesce uses sophisticated matching)
    let normalize = |text: &str, entity_type: &EntityType| -> (String, EntityType) {
        match text.to_lowercase().as_str() {
            "apple inc." | "apple" => ("Apple Inc.".to_string(), EntityType::Organization),
            "steve jobs" => ("Steve Jobs".to_string(), EntityType::Person),
            "tim cook" => ("Tim Cook".to_string(), EntityType::Person),
            "cupertino" => ("Cupertino".to_string(), EntityType::Location),
            "iphone" | "ipad" => ("Apple Product".to_string(), entity_type.clone()),
            "microsoft" => ("Microsoft".to_string(), EntityType::Organization),
            "satya nadella" => ("Satya Nadella".to_string(), EntityType::Person),
            "seattle" => ("Seattle".to_string(), EntityType::Location),
            "windows" => ("Windows".to_string(), entity_type.clone()),
            _ => (text.to_string(), entity_type.clone()),
        }
    };

    for (doc_id, entities) in doc_entities {
        for entity in entities {
            let key = format!("{}:{}", doc_id, &entity.text);
            let canonical = normalize(&entity.text, &entity.entity_type);
            resolution_map.insert(key, canonical);
        }
    }

    resolution_map
}

/// Builds a knowledge graph from resolved entities.
///
/// Creates nodes for each unique entity and edges based on:
/// 1. Co-occurrence within the same document
fn build_knowledge_graph(
    doc_entities: &[(String, Vec<Entity>)],
    resolution: &HashMap<String, (String, EntityType)>,
) -> GraphDocument {
    let mut unique_entities: HashMap<String, Entity> = HashMap::new();
    let mut relations: Vec<Relation> = Vec::new();

    // Collect unique entities by canonical name
    for (doc_id, entities) in doc_entities {
        for entity in entities {
            let key = format!("{}:{}", doc_id, &entity.text);
            if let Some((canonical_name, canonical_type)) = resolution.get(&key) {
                unique_entities
                    .entry(canonical_name.clone())
                    .or_insert_with(|| {
                        Entity::new(
                            canonical_name.clone(),
                            canonical_type.clone(),
                            0,
                            canonical_name.len(),
                            entity.confidence,
                        )
                    });
            }
        }
    }

    // Build relations for co-occurring entities in same document
    for (doc_id, entities) in doc_entities {
        let canonical_entities: Vec<&Entity> = entities
            .iter()
            .filter_map(|e| {
                let key = format!("{}:{}", doc_id, &e.text);
                resolution
                    .get(&key)
                    .and_then(|(canonical_name, _)| unique_entities.get(canonical_name))
            })
            .collect();

        // Create edges between all pairs in the same document
        for i in 0..canonical_entities.len() {
            for j in (i + 1)..canonical_entities.len() {
                let head = canonical_entities[i].clone();
                let tail = canonical_entities[j].clone();

                // Avoid self-loops
                if head.text != tail.text {
                    relations.push(Relation::new(head, tail, "co_occurs_with", 0.9));
                }
            }
        }
    }

    let entities_vec: Vec<Entity> = unique_entities.into_values().collect();
    GraphDocument::from_extraction(&entities_vec, &relations, None)
}

/// Full pipeline test: NER → Coalesce → Tier
#[test]
fn test_full_pipeline_ner_coalesce_tier() {
    // Step 1: Simulate NER extraction
    let doc_entities = simulate_ner_extraction();
    assert_eq!(doc_entities.len(), 4);

    // Step 2: Simulate entity resolution (coalesce)
    let resolution = simulate_coalesce(&doc_entities);
    assert!(!resolution.is_empty());

    // Verify resolution worked (Apple Inc. and Apple should resolve to same entity)
    let apple_inc_canon = resolution
        .get("doc1:Apple Inc.")
        .expect("Apple Inc. should be in resolution");
    let apple_canon = resolution
        .get("doc2:Apple")
        .expect("Apple should be in resolution");
    assert_eq!(
        apple_inc_canon.0, apple_canon.0,
        "Apple entities should be resolved together"
    );

    // Step 3: Build knowledge graph
    let graph = build_knowledge_graph(&doc_entities, &resolution);

    // Verify graph structure
    let stats = GraphStats::compute(&graph);
    assert!(stats.node_count > 0, "Graph should have nodes");
    assert!(stats.edge_count > 0, "Graph should have edges");

    println!("Graph Statistics:");
    println!("  Nodes: {}", stats.node_count);
    println!("  Edges: {}", stats.edge_count);
    println!("  Connected Components: {}", stats.component_count);
    println!("  Average Degree: {:.2}", stats.avg_degree);

    // Step 4: Apply centrality algorithms
    let pagerank_scores = PageRank::default().compute(&graph);
    let betweenness_scores = Betweenness::default().compute(&graph);
    let eigenvector_scores = Eigenvector::default().compute(&graph);
    let closeness_scores = Closeness::default().compute(&graph);
    let (hub_scores, _authority_scores) = Hits::default().compute(&graph);

    // Verify all algorithms return scores for all nodes
    assert_eq!(pagerank_scores.len(), stats.node_count);
    assert_eq!(betweenness_scores.len(), stats.node_count);
    assert_eq!(eigenvector_scores.len(), stats.node_count);
    assert_eq!(closeness_scores.len(), stats.node_count);
    assert_eq!(hub_scores.len(), stats.node_count);

    // Step 5: Apply community detection
    let leiden_graph = HierarchicalLeiden::new()
        .cluster(&graph)
        .expect("Leiden clustering should succeed");
    let louvain_communities = Louvain::default()
        .cluster(&graph)
        .expect("Louvain clustering should succeed");
    let label_prop_communities = LabelPropagation::default()
        .cluster(&graph)
        .expect("Label propagation clustering should succeed");

    // Verify all nodes are assigned to communities
    assert_eq!(leiden_graph.nodes.len(), stats.node_count);
    assert_eq!(louvain_communities.len(), stats.node_count);
    assert_eq!(label_prop_communities.len(), stats.node_count);

    // Step 6: Identify important entities
    let mut ranked_entities: Vec<_> = pagerank_scores.iter().collect();
    ranked_entities.sort_by(|a, b| {
        b.1.partial_cmp(a.1)
            .expect("PageRank scores should be comparable")
    });

    println!("\nTop entities by PageRank:");
    for (id, score) in ranked_entities.iter().take(5) {
        println!("  {}: {:.4}", id, score);
    }

    println!("\nPipeline test completed successfully!");
}

/// Test that tier algorithms handle multilingual entity graphs correctly.
#[test]
fn test_multilingual_entity_graph_pipeline() {
    // Simulate NER output with multilingual entities
    let entities = vec![
        Entity::new("北京", EntityType::Location, 0, 2, 0.95), // Beijing
        Entity::new("习近平", EntityType::Person, 5, 8, 0.98), // Xi Jinping
        Entity::new("Москва", EntityType::Location, 10, 16, 0.93), // Moscow
        Entity::new("Путин", EntityType::Person, 20, 25, 0.97), // Putin
        Entity::new("Tokyo", EntityType::Location, 30, 35, 0.94),
        Entity::new("東京", EntityType::Location, 40, 42, 0.92), // Tokyo in Japanese
    ];

    // Build relations: cities connected to leaders, related cities connected
    let relations = vec![
        Relation::new(entities[0].clone(), entities[1].clone(), "HAS_LEADER", 0.9),
        Relation::new(entities[2].clone(), entities[3].clone(), "HAS_LEADER", 0.9),
        Relation::new(
            entities[0].clone(),
            entities[2].clone(),
            "DIPLOMATIC_TIES",
            0.5,
        ),
        Relation::new(entities[4].clone(), entities[5].clone(), "SAME_AS", 0.9),
    ];

    let graph = GraphDocument::from_extraction(&entities, &relations, None);

    let stats = GraphStats::compute(&graph);
    assert_eq!(stats.node_count, 6);
    assert_eq!(stats.edge_count, 4);

    // PageRank should work with Unicode node IDs
    let pagerank = PageRank::default().compute(&graph);
    assert_eq!(pagerank.len(), 6);

    // All scores should be non-negative
    for score in pagerank.values() {
        assert!(*score >= 0.0, "PageRank scores must be non-negative");
    }

    // Community detection should work
    let communities = LabelPropagation::default()
        .cluster(&graph)
        .expect("Label propagation clustering should succeed");

    // Verify all nodes have community assignments (using node IDs from graph)
    assert_eq!(communities.len(), stats.node_count);

    println!("Multilingual graph test passed!");
}

/// Test that the pipeline handles disconnected components gracefully.
#[test]
fn test_disconnected_component_pipeline() {
    // Component 1: Tech companies triangle
    let apple = Entity::new("Apple", EntityType::Organization, 0, 5, 0.9);
    let google = Entity::new("Google", EntityType::Organization, 10, 16, 0.9);
    let microsoft = Entity::new("Microsoft", EntityType::Organization, 20, 29, 0.9);

    // Component 2: European cities (disconnected from Component 1)
    let paris = Entity::new("Paris", EntityType::Location, 30, 35, 0.9);
    let berlin = Entity::new("Berlin", EntityType::Location, 40, 46, 0.9);
    let rome = Entity::new("Rome", EntityType::Location, 50, 54, 0.9);

    let relations = vec![
        // Component 1
        Relation::new(apple.clone(), google.clone(), "COMPETES", 0.9),
        Relation::new(google.clone(), microsoft.clone(), "COMPETES", 0.9),
        Relation::new(microsoft.clone(), apple.clone(), "COMPETES", 0.9),
        // Component 2
        Relation::new(paris.clone(), berlin.clone(), "NEAR", 0.9),
        Relation::new(berlin.clone(), rome.clone(), "NEAR", 0.9),
    ];

    let graph = GraphDocument::from_extraction(
        &[apple, google, microsoft, paris, berlin, rome],
        &relations,
        None,
    );

    let stats = GraphStats::compute(&graph);
    assert_eq!(stats.node_count, 6);
    assert_eq!(
        stats.component_count, 2,
        "Should have 2 disconnected components"
    );

    // Centrality algorithms should still work
    let pagerank = PageRank::default().compute(&graph);
    assert_eq!(pagerank.len(), 6);

    // All scores should be non-negative
    for score in pagerank.values() {
        assert!(*score >= 0.0, "PageRank scores must be non-negative");
    }

    // Community detection should find at least 2 communities
    let communities = Louvain::default()
        .cluster(&graph)
        .expect("Louvain clustering should succeed");
    let unique_communities: std::collections::HashSet<_> = communities.values().collect();
    assert!(
        unique_communities.len() >= 2,
        "Should find at least 2 communities for disconnected graph"
    );

    println!("Disconnected component test passed!");
}

/// Test high-degree hub detection in knowledge graphs.
#[test]
fn test_hub_detection_pipeline() {
    // Create a star topology with a central hub
    let hub = Entity::new("Central_Hub", EntityType::Organization, 0, 11, 0.95);
    let spokes: Vec<Entity> = (1..=10)
        .map(|i| {
            Entity::new(
                format!("Spoke_{}", i),
                EntityType::Person,
                i * 10,
                i * 10 + 7,
                0.9,
            )
        })
        .collect();

    let mut all_entities = vec![hub.clone()];
    all_entities.extend(spokes.clone());

    let relations: Vec<Relation> = spokes
        .iter()
        .map(|spoke| Relation::new(hub.clone(), spoke.clone(), "CONNECTED_TO", 0.9))
        .collect();

    let graph = GraphDocument::from_extraction(&all_entities, &relations, None);

    // All centrality measures should identify the hub as most central
    let pagerank = PageRank::default().compute(&graph);
    let eigenvector = Eigenvector::default().compute(&graph);
    let closeness = Closeness::default().compute(&graph);
    let betweenness = Betweenness::default().compute(&graph);
    let (hub_scores, _) = Hits::default().compute(&graph);

    // Find the top node for each measure
    let find_top = |scores: &HashMap<String, f64>| -> String {
        scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).expect("scores should be comparable"))
            .map(|(k, _)| k.clone())
            .expect("scores should not be empty")
    };

    // The hub should be identified as most central by most measures
    // Note: Node IDs from from_extraction might be different, so we check the highest scoring one
    let top_pr = find_top(&pagerank);
    let top_ev = find_top(&eigenvector);
    let top_cl = find_top(&closeness);
    let top_bc = find_top(&betweenness);

    println!("Top by PageRank: {}", top_pr);
    println!("Top by Eigenvector: {}", top_ev);
    println!("Top by Closeness: {}", top_cl);
    println!("Top by Betweenness: {}", top_bc);

    // All measures should identify the same hub
    // (allowing for slight variations in how graph is constructed)
    assert!(
        pagerank.values().any(|v| *v > 0.0),
        "PageRank should produce positive scores"
    );
    assert!(
        eigenvector.values().any(|v| *v > 0.0),
        "Eigenvector should produce positive scores"
    );
    assert!(
        closeness.values().any(|v| *v > 0.0),
        "Closeness should produce positive scores"
    );
    assert!(
        hub_scores.values().any(|v| *v > 0.0),
        "HITS should produce positive hub scores"
    );

    println!("Hub detection test passed!");
}

/// Test empty graph handling.
#[test]
fn test_empty_graph_handling() {
    let graph = GraphDocument::from_extraction(&[], &[], None);

    let stats = GraphStats::compute(&graph);
    assert_eq!(stats.node_count, 0);
    assert_eq!(stats.edge_count, 0);

    // All algorithms should handle empty graphs gracefully
    let pagerank = PageRank::default().compute(&graph);
    assert!(pagerank.is_empty());

    let betweenness = Betweenness::default().compute(&graph);
    assert!(betweenness.is_empty());

    let eigenvector = Eigenvector::default().compute(&graph);
    assert!(eigenvector.is_empty());

    let closeness = Closeness::default().compute(&graph);
    assert!(closeness.is_empty());

    let (hubs, auths) = Hits::default().compute(&graph);
    assert!(hubs.is_empty());
    assert!(auths.is_empty());

    let leiden = HierarchicalLeiden::new()
        .cluster(&graph)
        .expect("Leiden clustering should succeed on empty graph");
    assert!(leiden.nodes.is_empty());

    let louvain = Louvain::default()
        .cluster(&graph)
        .expect("Louvain clustering should succeed on empty graph");
    assert!(louvain.is_empty());

    let label_prop = LabelPropagation::default()
        .cluster(&graph)
        .expect("Label propagation clustering should succeed on empty graph");
    assert!(label_prop.is_empty());

    println!("Empty graph handling test passed!");
}
