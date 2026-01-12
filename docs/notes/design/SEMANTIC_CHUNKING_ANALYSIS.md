# Semantic Chunking Analysis for Anno

**Date**: 2025-01-27  
**Status**: Research Complete - Recommendation: Implement with Feature Flag

## Executive Summary

**Verdict**: Semantic chunking would be **valuable for coreference resolution and entity linking**, but **marginal benefit for NER**. Recommended as an **optional enhancement** with feature flag, not a replacement for current fixed-size chunking.

## Research Findings

### What is Semantic Chunking?

Semantic chunking splits text based on **semantic coherence** rather than fixed sizes or sentence boundaries. It groups sentences/paragraphs that discuss the same topic or entity, preserving contextual relationships.

**Key differences:**
- **Fixed-size**: Uniform chunks regardless of content (current `StreamingExtractor`)
- **Sentence-based**: Breaks at sentence boundaries (current `respect_sentences: true`)
- **Semantic**: Groups by topic/entity coherence (proposed)

### Benefits for RAG (Well-Documented)

Research shows semantic chunking significantly improves:
- **Retrieval accuracy**: Better alignment with user intent
- **Context preservation**: Self-contained segments with broader document context
- **RAG performance**: Complete, contextually relevant information retrieval

### Benefits for NER/Coreference (Inferred from Research)

While less directly researched, semantic chunking would help:

#### 1. Coreference Resolution
- **Problem**: Coreferent mentions often spread far apart (Liu et al. 2020)
- **Benefit**: Semantic chunks keep related mentions together
- **Impact**: Better within-chunk resolution, reduced cross-chunk ambiguity
- **Evidence**: Fragkou (2016) shows bidirectional relationship between segmentation and coreference

#### 2. Entity Linking
- **Problem**: Entity disambiguation requires context (e.g., "Paris" = city vs person)
- **Benefit**: Semantic chunks preserve entity context
- **Impact**: Better disambiguation, fewer false positives
- **Evidence**: Entity-centric features improve linking (Liu et al. 2020)

#### 3. Long Documents
- **Problem**: Fixed-size chunks may split entity discussions mid-topic
- **Benefit**: Semantic chunks respect topic boundaries
- **Impact**: Better entity coherence, reduced boundary artifacts
- **Evidence**: End-to-end coreference considers all spans (Lee et al. 2017)

#### 4. Cross-Document Entity Resolution
- **Problem**: Entities need consistent context for alignment
- **Benefit**: Semantically coherent chunks improve entity similarity
- **Impact**: Better cross-doc entity matching
- **Evidence**: Semantic similarity is key for entity alignment

### Limitations

1. **Computational Cost**: Requires embeddings/clustering (expensive)
2. **Latency**: Slower than fixed-size chunking
3. **NER Benefit**: Marginal (NER is more local, less context-dependent)
4. **Implementation Complexity**: Needs embedding models, clustering algorithms

## Current Codebase State

### Existing Chunking (`StreamingExtractor`)

```rust
pub struct ChunkConfig {
    pub chunk_size: usize,           // Fixed size (10k chars default)
    pub overlap: usize,              // Overlap to catch boundary entities
    pub respect_sentences: bool,      // Break at sentence boundaries
    pub buffer_size: usize,
}
```

**Strengths:**
- Simple, fast, predictable
- Works well for NER (local task)
- Overlap handles boundary cases

**Weaknesses:**
- May split entity discussions mid-topic
- Fixed size doesn't respect semantic boundaries
- Coreference resolution may miss cross-chunk links

### Coreference Handling

Current coreference backends (`MentionRankingCoref`, `E2ECoref`) process entire documents or use distance limits. No explicit cross-chunk handling in `StreamingExtractor`.

## Recommendation

### Phase 1: Add Semantic Chunking as Optional Feature

**Design:**
```rust
pub enum ChunkingStrategy {
    /// Fixed-size chunks (current default)
    FixedSize(ChunkConfig),
    /// Semantic chunking using embeddings
    Semantic(SemanticChunkConfig),
    /// Hybrid: semantic with size limits
    Hybrid(HybridChunkConfig),
}

pub struct SemanticChunkConfig {
    /// Target chunk size (soft limit)
    pub target_size: usize,
    /// Minimum chunk size (hard limit)
    pub min_size: usize,
    /// Maximum chunk size (hard limit)
    pub max_size: usize,
    /// Embedding model for semantic similarity
    pub embedding_model: Option<String>,  // e.g., "sentence-transformers/all-MiniLM-L6-v2"
    /// Similarity threshold for chunk boundaries
    pub similarity_threshold: f32,
    /// Overlap between chunks
    pub overlap: usize,
}
```

**Implementation:**
1. Add `SemanticChunker` trait
2. Implement embedding-based chunking (use existing embedding infrastructure)
3. Integrate with `StreamingExtractor` via strategy pattern
4. Feature flag: `semantic-chunking` (optional dependency)

**Use Cases:**
- Long documents (>50k chars) with coreference
- Entity linking tasks
- Cross-document entity resolution
- When computational cost is acceptable

### Phase 2: Evaluate Performance

**Metrics:**
- Coreference F1 (within-chunk vs cross-chunk)
- Entity linking accuracy
- Processing time vs fixed-size
- Memory usage

**Benchmarks:**
- LitBank (long documents, coreference)
- Cross-document coreference datasets
- Entity linking datasets

### Phase 3: Hybrid Approach

Combine semantic and fixed-size:
- Use semantic boundaries when available
- Fall back to fixed-size for speed
- Adaptive strategy based on document length

## Implementation Plan

### Step 1: Semantic Chunking Trait

```rust
pub trait SemanticChunker: Send + Sync {
    /// Chunk text based on semantic similarity
    fn chunk(&self, text: &str, language: Option<&str>) -> Result<Vec<Chunk>>;
    
    /// Get chunk boundaries
    fn boundaries(&self, text: &str, language: Option<&str>) -> Result<Vec<usize>>;
}

pub struct Chunk {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub topic: Option<String>,  // Optional topic label
    pub entities: Vec<Entity>,  // Entities in this chunk
}
```

### Step 2: Embedding-Based Implementation

```rust
pub struct EmbeddingChunker {
    embedding_model: Box<dyn EmbeddingModel>,
    similarity_threshold: f32,
    target_size: usize,
    min_size: usize,
    max_size: usize,
}

impl SemanticChunker for EmbeddingChunker {
    fn chunk(&self, text: &str, language: Option<&str>) -> Result<Vec<Chunk>> {
        // 1. Split into sentences
        let sentences = self.split_sentences(text, language)?;
        
        // 2. Compute sentence embeddings
        let embeddings = self.embedding_model.encode(&sentences)?;
        
        // 3. Cluster by similarity
        let clusters = self.cluster_by_similarity(&embeddings, self.similarity_threshold)?;
        
        // 4. Merge clusters into chunks (respecting size limits)
        let chunks = self.merge_clusters(clusters, sentences)?;
        
        Ok(chunks)
    }
}
```

### Step 3: Integration with StreamingExtractor

```rust
impl<'m, M: Model> StreamingExtractor<'m, M> {
    pub fn with_semantic_chunking(
        mut self,
        chunker: Box<dyn SemanticChunker>,
    ) -> Self {
        self.chunking_strategy = ChunkingStrategy::Semantic(chunker);
        self
    }
}
```

## Alternatives Considered

### 1. Topic Modeling (LDA, BERTopic)
- **Pros**: No embeddings needed, fast
- **Cons**: Less precise than embeddings, requires training

### 2. Rule-Based (Paragraph Boundaries)
- **Pros**: Very fast, no ML
- **Cons**: Doesn't capture semantic coherence

### 3. Hybrid (Current + Semantic)
- **Pros**: Best of both worlds
- **Cons**: More complex implementation

## Conclusion

**Semantic chunking is valuable but not essential.** Recommended approach:

1. **Keep current fixed-size chunking as default** (fast, works well for NER)
2. **Add semantic chunking as optional feature** (for coreference/entity linking)
3. **Use feature flag** to avoid heavy dependencies by default
4. **Evaluate on long-document benchmarks** before making it default

**Priority**: Medium (useful enhancement, not critical blocker)

**Effort**: Medium (requires embedding infrastructure, clustering algorithms)

**Impact**: High for coreference/entity linking, Low for NER
