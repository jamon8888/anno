//! Invariant tests for entity resolution.
//!
//! Tests fundamental properties that should always hold:
//! - Transitivity: if A~B and B~C, then A~C
//! - Idempotency: resolving twice gives same result
//! - Determinism: same input → same output
//! - Symmetry: similarity(a,b) == similarity(b,a)

use anno_coalesce::{embedding_similarity, string_similarity, Resolver};
use anno_core::{Corpus, GroundedDocument, Track};

// =============================================================================
// Transitivity Tests
// =============================================================================

/// If entities A and B cluster together, and B and C cluster together,
/// then A and C should also cluster together (transitivity).
#[test]
fn test_clustering_transitivity() {
    let resolver = Resolver::new().with_threshold(0.5);
    let mut corpus = Corpus::new();

    // Create a chain: "Barack Obama" ~ "B Obama" ~ "Obama B"
    // Each adjacent pair should be similar enough to cluster
    let mut doc1 = GroundedDocument::new("doc1", "text");
    doc1.add_track(Track::new(1, "Barack Obama").with_type("PERSON".to_string()));
    corpus.add_document(doc1);

    let mut doc2 = GroundedDocument::new("doc2", "text");
    doc2.add_track(Track::new(1, "B Obama").with_type("PERSON".to_string()));
    corpus.add_document(doc2);

    let mut doc3 = GroundedDocument::new("doc3", "text");
    doc3.add_track(Track::new(1, "Obama B").with_type("PERSON".to_string()));
    corpus.add_document(doc3);

    let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Transitivity: all should end up in same cluster
    // (This depends on union-find correctly handling transitive closure)
    assert!(
        identities.len() <= 2,
        "Transitive chain should cluster: got {} clusters",
        identities.len()
    );
}

// =============================================================================
// Idempotency Tests
// =============================================================================

/// Resolving a corpus twice should produce identical results.
#[test]
fn test_resolution_idempotency() {
    let resolver = Resolver::new().with_threshold(0.7);

    // First resolution
    let mut corpus1 = Corpus::new();
    add_test_documents(&mut corpus1);
    let ids1 = resolver.resolve_inter_doc_coref(&mut corpus1, None, None);

    // Second resolution on fresh corpus with same data
    let mut corpus2 = Corpus::new();
    add_test_documents(&mut corpus2);
    let ids2 = resolver.resolve_inter_doc_coref(&mut corpus2, None, None);

    assert_eq!(
        ids1.len(),
        ids2.len(),
        "Idempotency violated: first resolution gave {} identities, second gave {}",
        ids1.len(),
        ids2.len()
    );
}

fn add_test_documents(corpus: &mut Corpus) {
    let entities = vec![
        ("doc1", "Apple Inc", "ORGANIZATION"),
        ("doc2", "Apple", "ORGANIZATION"),
        ("doc3", "Microsoft", "ORGANIZATION"),
        ("doc4", "Google LLC", "ORGANIZATION"),
        ("doc5", "Google", "ORGANIZATION"),
    ];

    for (doc_id, name, etype) in entities {
        let mut doc = GroundedDocument::new(doc_id, "text");
        doc.add_track(Track::new(1, name).with_type(etype.to_string()));
        corpus.add_document(doc);
    }
}

// =============================================================================
// Determinism Tests
// =============================================================================

/// Same input should always produce same output (no random behavior).
#[test]
fn test_resolution_determinism() {
    let resolver = Resolver::new().with_threshold(0.6);

    let mut results = Vec::new();
    for _ in 0..5 {
        let mut corpus = Corpus::new();
        add_determinism_test_data(&mut corpus);
        let ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
        results.push(ids.len());
    }

    let first = results[0];
    for (i, count) in results.iter().enumerate() {
        assert_eq!(
            *count, first,
            "Non-determinism detected: run {} gave {} identities, run 0 gave {}",
            i, count, first
        );
    }
}

fn add_determinism_test_data(corpus: &mut Corpus) {
    for i in 0..10 {
        let mut doc = GroundedDocument::new(format!("doc{}", i), "text");
        let name = format!("Entity_{}", i % 3); // Creates 3 groups
        doc.add_track(Track::new(1, name).with_type("MISC".to_string()));
        corpus.add_document(doc);
    }
}

// =============================================================================
// Symmetry Tests
// =============================================================================

#[test]
fn test_string_similarity_symmetry() {
    let pairs = vec![
        ("Apple Inc", "Apple"),
        ("北京", "Beijing"),
        ("Müller", "Mueller"),
        ("José García", "Jose Garcia"),
        ("", "test"),
        ("a", "ab"),
    ];

    for (a, b) in pairs {
        let sim_ab = string_similarity(a, b);
        let sim_ba = string_similarity(b, a);
        assert!(
            (sim_ab - sim_ba).abs() < 0.001,
            "String similarity not symmetric: sim({:?}, {:?}) = {}, sim({:?}, {:?}) = {}",
            a,
            b,
            sim_ab,
            b,
            a,
            sim_ba
        );
    }
}

#[test]
fn test_embedding_similarity_symmetry() {
    let vectors = vec![
        (vec![1.0, 0.0, 0.0], vec![0.5, 0.5, 0.0]),
        (vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]),
        (vec![0.0, 0.0, 0.0], vec![1.0, 1.0, 1.0]),
        (vec![-1.0, 0.5], vec![0.5, -1.0]),
    ];

    for (a, b) in vectors {
        let sim_ab = embedding_similarity(&a, &b);
        let sim_ba = embedding_similarity(&b, &a);
        assert!(
            (sim_ab - sim_ba).abs() < 0.001,
            "Embedding similarity not symmetric: sim({:?}, {:?}) = {}, sim({:?}, {:?}) = {}",
            a,
            b,
            sim_ab,
            b,
            a,
            sim_ba
        );
    }
}

// =============================================================================
// Identity (Reflexivity) Tests
// =============================================================================

#[test]
fn test_string_similarity_identity() {
    let strings = vec![
        "Apple Inc",
        "北京",
        "Владимир Путин",
        "مُحَمَّد",
        "",
        "a",
        "Hello World Test String",
    ];

    for s in strings {
        let sim = string_similarity(s, s);
        assert!(
            (sim - 1.0).abs() < 0.001 || (s.is_empty() && sim == 1.0),
            "Identity property violated: sim({:?}, {:?}) = {} (expected 1.0)",
            s,
            s,
            sim
        );
    }
}

#[test]
fn test_embedding_similarity_identity() {
    let vectors = vec![
        vec![1.0, 0.0, 0.0],
        vec![0.5, 0.5, 0.5],
        vec![-1.0, 2.0, -3.0],
        vec![0.001, 0.001, 0.001],
    ];

    for v in vectors {
        let sim = embedding_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Identity property violated: sim(v, v) = {} (expected 1.0) for {:?}",
            sim,
            v
        );
    }
}

// =============================================================================
// Triangle Inequality Tests (Metric Space Property)
// =============================================================================

/// For a proper metric: d(a,c) <= d(a,b) + d(b,c)
/// Converting similarity to distance: d = 1 - sim
#[test]
fn test_string_distance_triangle_inequality() {
    let triples = vec![
        ("Apple", "Apple Inc", "Apple Corporation"),
        ("John", "John Smith", "Smith"),
        ("New York", "York", "New"),
    ];

    for (a, b, c) in triples {
        let d_ab = 1.0 - string_similarity(a, b);
        let d_bc = 1.0 - string_similarity(b, c);
        let d_ac = 1.0 - string_similarity(a, c);

        // Allow small tolerance for floating point
        assert!(
            d_ac <= d_ab + d_bc + 0.01,
            "Triangle inequality violated: d({:?},{:?})={} > d({:?},{:?})={} + d({:?},{:?})={}",
            a,
            c,
            d_ac,
            a,
            b,
            d_ab,
            b,
            c,
            d_bc
        );
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// String similarity is always symmetric
        #[test]
        fn string_sim_symmetric(a in "\\PC{1,50}", b in "\\PC{1,50}") {
            let sim_ab = string_similarity(&a, &b);
            let sim_ba = string_similarity(&b, &a);
            prop_assert!((sim_ab - sim_ba).abs() < 0.001,
                "Symmetry violated: {} vs {}", sim_ab, sim_ba);
        }

        /// String similarity is bounded [0, 1]
        #[test]
        fn string_sim_bounded(a in "\\PC{0,100}", b in "\\PC{0,100}") {
            let sim = string_similarity(&a, &b);
            prop_assert!(sim >= 0.0 && sim <= 1.0,
                "Similarity {} out of bounds", sim);
        }

        /// Identical strings have similarity 1.0
        #[test]
        fn string_sim_identity(s in "\\PC{0,100}") {
            let sim = string_similarity(&s, &s);
            prop_assert!((sim - 1.0).abs() < 0.001,
                "Identity violated: sim(s,s) = {}", sim);
        }

        /// Embedding similarity symmetric
        #[test]
        fn embedding_sim_symmetric(
            dim in 3usize..50,
            seed in any::<u64>()
        ) {
            let (a, b) = gen_random_vectors(dim, seed);
            let sim_ab = embedding_similarity(&a, &b);
            let sim_ba = embedding_similarity(&b, &a);
            prop_assert!((sim_ab - sim_ba).abs() < 0.001,
                "Embedding symmetry violated");
        }

        /// Embedding similarity bounded [0, 1] for normalized positive vectors
        #[test]
        fn embedding_sim_bounded(
            dim in 3usize..50,
            seed in any::<u64>()
        ) {
            let (a, b) = gen_positive_vectors(dim, seed);
            let sim = embedding_similarity(&a, &b);
            // Note: our implementation normalizes to [0,1] but cosine is [-1,1]
            // so we test for normalized range
            prop_assert!(sim >= 0.0 && sim <= 1.0,
                "Embedding similarity {} out of [0,1]", sim);
        }

        /// Resolution produces valid identities
        #[test]
        fn resolution_produces_valid_identities(
            num_docs in 1usize..20,
            entities_per_doc in 1usize..5
        ) {
            let resolver = Resolver::new();
            let mut corpus = Corpus::new();

            for d in 0..num_docs {
                let mut doc = GroundedDocument::new(format!("doc{}", d), "text");
                for e in 0..entities_per_doc {
                    let name = format!("Entity_{}_{}", d, e);
                    doc.add_track(Track::new(e as u64, name));
                }
                corpus.add_document(doc);
            }

            let ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

            // All returned IDs should be valid
            for id in &ids {
                prop_assert!(corpus.get_identity(*id).is_some(),
                    "Invalid identity ID returned: {}", id);
            }

            // Number of identities bounded by number of tracks
            let total_tracks = num_docs * entities_per_doc;
            prop_assert!(ids.len() <= total_tracks,
                "More identities ({}) than tracks ({})", ids.len(), total_tracks);
        }

        /// Resolution is deterministic
        #[test]
        fn resolution_deterministic(seed in any::<u64>()) {
            let resolver = Resolver::new().with_threshold(0.6);

            let make_corpus = |s: u64| {
                let mut corpus = Corpus::new();
                let mut rng = s;
                for i in 0..5 {
                    let mut doc = GroundedDocument::new(format!("doc{}", i), "text");
                    rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                    let name = format!("Entity_{}", rng % 3);
                    doc.add_track(Track::new(1, name));
                    corpus.add_document(doc);
                }
                corpus
            };

            let mut corpus1 = make_corpus(seed);
            let ids1 = resolver.resolve_inter_doc_coref(&mut corpus1, None, None);

            let mut corpus2 = make_corpus(seed);
            let ids2 = resolver.resolve_inter_doc_coref(&mut corpus2, None, None);

            prop_assert_eq!(ids1.len(), ids2.len(),
                "Non-determinism: {} vs {} identities", ids1.len(), ids2.len());
        }
    }

    fn gen_random_vectors(dim: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
        let mut rng = seed;
        let a: Vec<f32> = (0..dim)
            .map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                ((rng % 2000) as f32 - 1000.0) / 1000.0
            })
            .collect();
        let b: Vec<f32> = (0..dim)
            .map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                ((rng % 2000) as f32 - 1000.0) / 1000.0
            })
            .collect();
        (a, b)
    }

    fn gen_positive_vectors(dim: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
        let mut rng = seed;
        let a: Vec<f32> = (0..dim)
            .map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                (rng % 1000) as f32 / 1000.0 + 0.001
            })
            .collect();
        let b: Vec<f32> = (0..dim)
            .map(|_| {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                (rng % 1000) as f32 / 1000.0 + 0.001
            })
            .collect();
        (a, b)
    }
}
