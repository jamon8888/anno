use super::*;

fn make_mention(text: &str, start: usize) -> Mention {
    Mention::new(text, start, start + text.chars().count())
}

fn make_typed_mention(text: &str, start: usize, mention_type: MentionType) -> Mention {
    Mention::with_type(text, start, start + text.chars().count(), mention_type)
}

// -------------------------------------------------------------------------
// Basic functionality
// -------------------------------------------------------------------------

#[test]
fn test_empty_input() {
    let coref = GraphCoref::new();
    let chains = coref.resolve(&[]);
    assert!(chains.is_empty());
}

#[test]
fn test_single_mention() {
    let coref = GraphCoref::new();
    let mentions = vec![make_mention("John", 0)];
    let chains = coref.resolve(&mentions);
    assert!(
        chains.is_empty(),
        "Single mention should be filtered as singleton"
    );
}

#[test]
fn test_single_mention_with_singletons() {
    let config = GraphCorefConfig {
        include_singletons: true,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);
    let mentions = vec![make_mention("John", 0)];
    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1, "Should include singleton when configured");
}

#[test]
fn test_exact_match_linking() {
    let coref = GraphCoref::new();
    let mentions = vec![make_mention("John", 0), make_mention("John", 50)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
}

#[test]
fn test_substring_linking() {
    let config = GraphCorefConfig {
        link_threshold: 0.4,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);
    let mentions = vec![make_mention("Marie Curie", 0), make_mention("Curie", 50)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
}

// -------------------------------------------------------------------------
// MentionType usage
// -------------------------------------------------------------------------

#[test]
fn test_typed_pronoun_linking() {
    let config = GraphCorefConfig {
        link_threshold: 0.2,
        distance_weight: 0.0, // Disable distance penalty to isolate type signal
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        make_typed_mention("Marie", 0, MentionType::Proper),
        make_typed_mention("she", 20, MentionType::Pronominal),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1, "Typed pronoun should link to proper noun");
}

#[test]
fn test_inferred_pronoun_detection() {
    let coref = GraphCoref::new();

    // Create mention without type - should be inferred
    let he = make_mention("he", 0);
    assert_eq!(
        coref.infer_mention_type(&he),
        MentionType::Pronominal,
        "Should detect 'he' as pronoun"
    );

    let john = make_mention("John", 0);
    assert_eq!(
        coref.infer_mention_type(&john),
        MentionType::Proper,
        "Should detect 'John' as proper"
    );

    let dog = make_mention("the dog", 0);
    assert_eq!(
        coref.infer_mention_type(&dog),
        MentionType::Nominal,
        "Should detect 'the dog' as nominal"
    );
}

// -------------------------------------------------------------------------
// Transitivity and graph refinement
// -------------------------------------------------------------------------

#[test]
fn test_transitivity() {
    let config = GraphCorefConfig {
        max_iterations: 4,
        link_threshold: 0.3,
        transitivity_bonus: 0.3,
        per_shared_neighbor_bonus: 0.2,
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![
        make_mention("John Smith", 0),
        make_mention("Smith", 30),
        make_mention("John Smith", 60),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 3);
}

#[test]
fn test_convergence() {
    let coref = GraphCoref::new();
    let mentions = vec![
        make_mention("Apple", 0),
        make_mention("Apple", 50),
        make_mention("Microsoft", 100),
    ];

    let (chains, stats) = coref.resolve_with_stats(&mentions);

    assert!(stats.iterations <= 4);
    assert!(stats.converged || stats.iterations == 4);
    assert_eq!(chains.len(), 1);
    assert_eq!(stats.num_chains, 1);
}

// -------------------------------------------------------------------------
// CorefGraph tests
// -------------------------------------------------------------------------

#[test]
fn test_coref_graph_basics() {
    let mut graph = CorefGraph::new(5);

    graph.add_edge(0, 1);
    graph.add_edge(1, 2);

    assert!(graph.has_edge(0, 1));
    assert!(graph.has_edge(1, 0)); // Symmetric
    assert!(graph.has_edge(1, 2));
    assert!(!graph.has_edge(0, 2));

    assert!(graph.transitively_connected(0, 2));

    let clusters = graph.extract_clusters();
    assert_eq!(clusters.len(), 3); // {0,1,2}, {3}, {4}

    let main_cluster = clusters.iter().find(|c| c.len() == 3).unwrap();
    assert!(main_cluster.contains(&0));
    assert!(main_cluster.contains(&1));
    assert!(main_cluster.contains(&2));
}

#[test]
fn test_shared_neighbors() {
    let mut graph = CorefGraph::new(4);
    graph.add_edge(0, 2);
    graph.add_edge(1, 2);

    assert_eq!(graph.shared_neighbors(0, 1), 1);
    assert_eq!(graph.shared_neighbors(0, 3), 0);
}

#[test]
fn test_graph_self_loop_ignored() {
    let mut graph = CorefGraph::new(3);
    graph.add_edge(0, 0); // Self-loop
    assert!(!graph.has_edge(0, 0));
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_graph_out_of_bounds_ignored() {
    let mut graph = CorefGraph::new(3);
    graph.add_edge(0, 10); // Out of bounds
    assert_eq!(graph.edge_count(), 0);
}

// -------------------------------------------------------------------------
// Edge cases
// -------------------------------------------------------------------------

#[test]
fn test_empty_mention_filtered() {
    let coref = GraphCoref::new();
    let mentions = vec![
        make_mention("John", 0),
        Mention::new("", 10, 10),    // Empty
        Mention::new("   ", 20, 23), // Whitespace only
        make_mention("John", 50),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].mentions.len(), 2);
}

#[test]
fn test_distance_filter() {
    let config = GraphCorefConfig {
        max_distance: Some(100),
        ..Default::default()
    };
    let coref = GraphCoref::with_config(config);

    let mentions = vec![make_mention("John", 0), make_mention("John", 200)];

    let chains = coref.resolve(&mentions);
    assert!(chains.is_empty());
}

#[test]
fn test_stats_edge_history() {
    let coref = GraphCoref::new();
    let mentions = vec![
        make_mention("A", 0),
        make_mention("A", 10),
        make_mention("A", 20),
    ];

    let (_, stats) = coref.resolve_with_stats(&mentions);

    assert!(!stats.edge_history.is_empty());
    assert_eq!(stats.edge_history[0], 0);
}

// -------------------------------------------------------------------------
// Unicode / multilingual
// -------------------------------------------------------------------------

#[test]
fn test_unicode_cjk() {
    let coref = GraphCoref::new();
    let mentions = vec![
        make_mention("北京", 0),
        make_mention("北京", 20),
        make_mention("東京", 40),
    ];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
    assert!(chains[0].mentions.iter().all(|m| m.text == "北京"));
}

#[test]
fn test_unicode_diacritics() {
    let coref = GraphCoref::new();
    let mentions = vec![make_mention("François", 0), make_mention("François", 50)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
}

#[test]
fn test_unicode_arabic_rtl() {
    let coref = GraphCoref::new();
    // Arabic: "Muhammad" repeated
    let mentions = vec![make_mention("محمد", 0), make_mention("محمد", 20)];

    let chains = coref.resolve(&mentions);
    assert_eq!(chains.len(), 1);
}

// -------------------------------------------------------------------------
// Evaluation helper
// -------------------------------------------------------------------------

#[test]
fn test_chains_to_document() {
    let chain = CorefChain::new(vec![make_mention("John", 0), make_mention("he", 20)]);

    let doc = chains_to_document("John went home. He slept.", vec![chain]);

    assert_eq!(doc.chain_count(), 1);
    assert_eq!(doc.mention_count(), 2);
}

// -------------------------------------------------------------------------
// Co-occurrence seeding (SpanEIT-inspired)
// -------------------------------------------------------------------------

#[test]
fn test_cooccurrence_seeding_basic() {
    let mut graph = CorefGraph::new(3);
    let positions = vec![0, 50, 200]; // Character offsets

    // Window of 100: should connect 0-1 (distance 50) but not 0-2 (distance 200)
    graph.seed_cooccurrence_edges(&positions, 100, None::<fn(usize, usize) -> bool>);

    assert!(graph.has_edge(0, 1), "Close mentions should be connected");
    assert!(
        !graph.has_edge(0, 2),
        "Distant mentions should not be connected"
    );
    assert!(
        !graph.has_edge(1, 2),
        "Distant mentions should not be connected"
    );
}

#[test]
fn test_cooccurrence_seeding_with_scorer() {
    let mut graph = CorefGraph::new(3);
    let positions = vec![0, 50, 80];

    // Custom scorer: only connect if both indices are even
    let scorer = |i: usize, j: usize| i.is_multiple_of(2) && j.is_multiple_of(2);
    graph.seed_cooccurrence_edges(&positions, 100, Some(scorer));

    assert!(graph.has_edge(0, 2), "0 and 2 are both even");
    assert!(!graph.has_edge(0, 1), "1 is odd");
    assert!(!graph.has_edge(1, 2), "1 is odd");
}

#[test]
fn test_cooccurrence_seeding_empty() {
    let mut graph = CorefGraph::new(3);
    let positions: Vec<usize> = vec![];

    graph.seed_cooccurrence_edges(&positions, 100, None::<fn(usize, usize) -> bool>);

    assert!(graph.is_empty(), "Empty positions should create no edges");
}
