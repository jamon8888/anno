# Semantic Chunking Next Steps

**Date**: 2025-01-27  
**Status**: Phase 1 Complete, Phase 2 Pending

## Current Implementation

### Phase 1: Rule-Based Chunker ✅

- `RuleBasedSemanticChunker` - Lightweight, always available
- Uses paragraph breaks and sentence length heuristics
- No external dependencies
- Suitable for basic semantic segmentation

### Phase 2: Embedding-Based Chunker ⏳

- `EmbeddingSemanticChunker` - Placeholder implementation
- Requires embedding model integration
- Needs sentence encoder infrastructure

## Integration Points

### Existing Embedding Infrastructure

Located in `anno/src/eval/cluster_encoder.rs`:
- `ClusterEncoder` trait for text embeddings
- BERT-based encoders (ModernBERT, sentence-transformers)
- Similarity computation utilities

### Required Changes

1. **Update `EmbeddingSemanticChunker`**:
   ```rust
   pub struct EmbeddingSemanticChunker {
       encoder: Arc<dyn ClusterEncoder>,  // Use existing encoder infrastructure
       config: SemanticChunkConfig,
   }
   ```

2. **Implement semantic segmentation algorithm**:
   - TextTiling (Hearst 1997): Compute cosine similarity between adjacent sentences
   - C99 (Choi 2000): Hierarchical clustering of sentences
   - Custom similarity-based: Group sentences with similarity > threshold

3. **Integration with `StreamingExtractor`**:
   - Add `use_semantic_chunking: bool` flag
   - When enabled, use `SemanticChunker` instead of fixed-size chunking
   - Preserve overlap and boundary handling

## Algorithm: TextTiling for Semantic Chunking

```rust
fn texttiling_chunk(
    sentences: &[(&str, usize, usize)],
    embeddings: &[Vec<f32>],
    threshold: f32,
) -> Vec<(usize, usize)> {
    // 1. Compute similarity between adjacent sentences
    let similarities: Vec<f32> = (0..sentences.len() - 1)
        .map(|i| cosine_similarity(&embeddings[i], &embeddings[i + 1]))
        .collect();
    
    // 2. Find local minima (boundary points)
    let mut boundaries = vec![0];
    for i in 1..similarities.len() {
        if similarities[i] < threshold && 
           similarities[i] < similarities[i-1] && 
           similarities[i] < similarities.get(i+1).copied().unwrap_or(1.0) {
            boundaries.push(i + 1);
        }
    }
    boundaries.push(sentences.len());
    
    // 3. Group sentences into chunks
    boundaries.windows(2)
        .map(|w| (w[0], w[1]))
        .collect()
}
```

## Dependencies

- `anno/src/eval/cluster_encoder.rs` - Already exists
- `anno/src/eval/similarity.rs` - Already exists (cosine similarity)
- No new dependencies required

## Testing Strategy

1. **Unit tests**: Test TextTiling algorithm on synthetic data
2. **Integration tests**: Test with real documents (news articles, Wikipedia)
3. **Comparison**: Compare semantic chunks vs fixed-size chunks for:
   - Coreference resolution accuracy
   - Entity linking precision
   - Cross-document alignment

## Performance Considerations

- **Embedding computation**: Most expensive step (O(n) where n = sentences)
- **Similarity computation**: O(n) for adjacent pairs
- **Boundary detection**: O(n) for local minima
- **Total complexity**: O(n) - linear in number of sentences

## Future Enhancements

1. **Hierarchical chunking**: Multi-level semantic segmentation
2. **Domain-specific chunkers**: Biomedical, legal, scientific text
3. **Adaptive thresholds**: Learn threshold from document characteristics
4. **Cross-lingual chunking**: Language-aware semantic boundaries
