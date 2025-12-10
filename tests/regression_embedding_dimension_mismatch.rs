//! Regression test: Embedding dimension mismatch handling in crossdoc coreference
//!
//! This test ensures that when tracks have embeddings with different dimensions,
//! the similarity calculation handles it gracefully (should fall back to string similarity).

use anno_coalesce::embedding_similarity;

#[test]
fn test_embedding_dimension_mismatch() {
    // Different dimensions should return 0.0 (handled by embedding_similarity)
    let emb1 = vec![1.0, 0.0, 0.0]; // 3 dimensions
    let emb2 = vec![1.0, 0.0]; // 2 dimensions
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 0.0, "Different dimension embeddings should return 0.0 similarity");
}

#[test]
fn test_empty_embeddings() {
    // Empty embeddings should return 0.0
    let emb1: Vec<f32> = vec![];
    let emb2: Vec<f32> = vec![];
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 0.0, "Empty embeddings should return 0.0 similarity");
}

#[test]
fn test_zero_norm_embeddings() {
    // Zero-norm embeddings (all zeros) should return 0.0
    let emb1 = vec![0.0, 0.0, 0.0];
    let emb2 = vec![1.0, 0.0, 0.0];
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 0.0, "Zero-norm embedding should return 0.0 similarity");
}

#[test]
fn test_identical_embeddings() {
    // Identical embeddings should return 1.0 (after normalization)
    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![1.0, 0.0, 0.0];
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 1.0, "Identical embeddings should return 1.0 similarity");
}

#[test]
fn test_orthogonal_embeddings() {
    // Orthogonal embeddings (dot product = 0) should return 0.5 (normalized from -1 to [0,1])
    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![0.0, 1.0, 0.0];
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 0.5, "Orthogonal embeddings should return 0.5 similarity (normalized)");
}

#[test]
fn test_opposite_embeddings() {
    // Opposite embeddings (dot product = -1) should return 0.0 (normalized from -1 to [0,1])
    let emb1 = vec![1.0, 0.0, 0.0];
    let emb2 = vec![-1.0, 0.0, 0.0];
    
    let sim = embedding_similarity(&emb1, &emb2);
    assert_eq!(sim, 0.0, "Opposite embeddings should return 0.0 similarity (normalized)");
}

