# Byte-Level Entity Embeddings Design

## Motivation

Current string similarity approaches (Jaccard, Levenshtein, Jaro-Winkler) work well for many cases but have fundamental limitations:

1. **No semantic understanding**: "NYC" vs "New York City" have low string similarity
2. **Language-specific tuning**: Different algorithms work better for different scripts
3. **Abbreviations/variants**: "Apple Inc." vs "Apple" require domain knowledge

A learned embedding approach can capture semantic similarity while remaining fast.

## Why Byte-Level?

| Approach | Pros | Cons |
|----------|------|------|
| **Subword (BPE/WordPiece)** | Good for major languages | Requires language-specific tokenizers, fails on OOV |
| **Character-level** | Script-agnostic | Still needs Unicode handling, longer sequences |
| **Byte-level** | Truly universal, no tokenizer | Longer sequences, needs efficient architecture |

**Byte-level is the right choice for entity matching** because:
- Entities span many languages and scripts
- Proper nouns are often OOV for subword tokenizers
- No preprocessing needed (raw bytes go in)
- Naturally handles mixed scripts (e.g., "iPhone 15 Pro Max")

## Architecture Options

### Option 1: ByT5-style (Full Transformer)

```
[bytes] -> Transformer Encoder -> [embedding]
```

- Pros: Most accurate
- Cons: Slow inference, large model

### Option 2: CANINE-style (Downsampling + Transformer)

```
[bytes] -> Conv1D downsample -> Transformer -> pooling -> [embedding]
```

- Pros: Handles long sequences, proven on multilingual NER
- Cons: Still needs transformer layers

### Option 3: CharFormer-style (Light Convolutions)

```
[bytes] -> Multiple Conv1D layers -> pooling -> [embedding]
```

- Pros: Fast inference, small model
- Cons: Less expressive

### Option 4: HashEmbed (No Training)

```
[bytes] -> hash -> lookup in fixed embedding table -> pooling -> [embedding]
```

- Pros: Zero training, instant, tiny
- Cons: No learning, just fingerprinting

## Recommended: Hybrid Approach

Start simple, add complexity only if needed:

### Phase 1: Fast Hash-Based Baseline

```rust
/// Fast byte-level fingerprinting (no training required)
pub fn byte_hash_embed(s: &str, dim: usize) -> Vec<f32> {
    let mut embedding = vec![0.0f32; dim];
    
    // Hash each byte trigram to embedding dimensions
    for window in s.as_bytes().windows(3) {
        let hash = hash_trigram(window);
        let idx = hash % dim;
        embedding[idx] += 1.0;
    }
    
    // L2 normalize
    normalize_l2(&mut embedding);
    embedding
}
```

This gives a fast baseline with zero training.

### Phase 2: Learned Convolutions (if Phase 1 insufficient)

Train a small model on entity pairs:

```rust
struct ByteEmbedder {
    conv1: Conv1D,  // 256 -> 128, kernel=3
    conv2: Conv1D,  // 128 -> 64, kernel=3
    pool: GlobalMaxPool,
    proj: Linear,   // 64 -> embedding_dim
}

impl ByteEmbedder {
    fn forward(&self, bytes: &[u8]) -> Vec<f32> {
        let x = self.conv1.forward(bytes);
        let x = relu(x);
        let x = self.conv2.forward(x);
        let x = relu(x);
        let x = self.pool.forward(x);
        self.proj.forward(x)
    }
}
```

### Phase 3: Contrastive Fine-tuning

Once we have entity pairs (same entity, different surface forms):

```python
# Training loop
for anchor, positive, negatives in dataset:
    anchor_emb = model(anchor.encode())
    pos_emb = model(positive.encode())
    neg_embs = [model(n.encode()) for n in negatives]
    
    # InfoNCE loss
    loss = info_nce_loss(anchor_emb, pos_emb, neg_embs)
    loss.backward()
```

## Data Sources for Training

1. **Wikipedia redirects**: "NYC" -> "New York City"
2. **Wikidata aliases**: Multiple names for same entity
3. **Cross-lingual links**: 北京 <-> Beijing <-> Пекин
4. **Anno coref clusters**: Our own coreference data
5. **Synthetic augmentation**: Case variations, typos, abbreviations

## Inference Optimization

For production deployment:

1. **Quantization**: INT8 or even INT4 for embeddings
2. **SIMD**: Vectorized convolutions (use `packed_simd` or `simdeez`)
3. **Batching**: Process multiple strings at once
4. **Caching**: LRU cache for common entities
5. **ANN index**: Use HNSW or IVF for fast similarity search

## Integration with anno-coalesce

```rust
pub struct Resolver {
    // Existing
    similarity_threshold: f32,
    adaptive_config: Option<AdaptiveResolutionConfig>,
    
    // New: optional byte embedder
    byte_embedder: Option<ByteEmbedder>,
}

impl Resolver {
    fn compute_similarity(&self, a: &str, b: &str) -> f32 {
        if let (Some(embedder), Some(emb_a), Some(emb_b)) = 
            (&self.byte_embedder, self.embed(a), self.embed(b)) 
        {
            // Use learned embeddings
            cosine_similarity(&emb_a, &emb_b)
        } else {
            // Fall back to string similarity
            string_similarity(a, b)
        }
    }
}
```

## Evaluation

Before committing to any approach, evaluate on:

1. **Accuracy**: Precision/recall on entity matching benchmark
2. **Latency**: Time per comparison (target: <100μs)
3. **Memory**: Model size (target: <10MB)
4. **Generalization**: Performance on unseen scripts/languages

## Timeline

| Phase | Work | Duration |
|-------|------|----------|
| 1 | Hash-based baseline | 1-2 days |
| 2 | Collect entity pair dataset | 1 week |
| 3 | Train convolution model | 1-2 weeks |
| 4 | Integrate and evaluate | 1 week |

## Open Questions

1. **Embedding dimension**: 64? 128? 256?
2. **N-gram size**: Byte trigrams? 4-grams?
3. **Pooling**: Max? Mean? Attention?
4. **Architecture**: Pure conv? Transformer? Hybrid?

These should be determined empirically on a held-out validation set.

## References

- [CANINE: Pre-training an Efficient Tokenization-Free Encoder](https://arxiv.org/abs/2103.06874)
- [ByT5: Towards a token-free future with pre-trained byte-to-byte models](https://arxiv.org/abs/2105.13626)
- [Charformer: Fast Character Transformers](https://arxiv.org/abs/2106.12672)
- [HashEmbed: Non-learned embedding lookup](https://github.com/explosion/thinc)
