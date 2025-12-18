# xCoRe Integration Design: Cross-Context Coreference Resolution

**Paper**: Martinelli, Gatti & Navigli. "xCoRe: Cross-context Coreference Resolution" (EMNLP 2025)
**Code**: https://github.com/sapienzanlp/xcore

## Executive Summary

xCoRe unifies long-document and cross-document coreference under a single "cross-context" formulation. The key insight: when documents exceed memory limits and must be processed in windows, the problem becomes structurally identical to cross-document coreference. Both settings require merging clusters formed in separate contexts.

**Connection to Anno**: The xCoRe authors (Sapienza NLP) also created Maverick, which anno already partially implements in `eval/maverick_coref.rs`. xCoRe extends Maverick with a learned cluster merging layer.

## The Cross-Context Formulation

### Core Insight

| Setting | Context Definition | Challenge |
|---------|-------------------|-----------|
| **Short document** | Single doc = 1 context | Standard coref |
| **Long document** | Each window = 1 context | Merge clusters across windows |
| **Cross-document** | Each doc = 1 context | Merge clusters across docs |

The unification enables **joint training** on both long-doc and cross-doc datasets, increasing data availability for the challenging cross-doc task.

### Three-Step Pipeline

```
┌────────────────────────────────────────────────────────────────────┐
│ Step 1: Within-Context Mention Extraction (NER-like)               │
│         For each context c_i: extract mentions M_i                 │
│         Uses start-to-end span detection (Maverick-style)          │
├────────────────────────────────────────────────────────────────────┤
│ Step 2: Within-Context Mention Clustering (Intra-doc coref)        │
│         For each context c_i: cluster mentions into W_i            │
│         Uses LingMess multi-expert scorer (category-specific)      │
├────────────────────────────────────────────────────────────────────┤
│ Step 3: Cross-Context Cluster Merging (THE KEY INNOVATION)         │
│         Input: All local clusters {W_c1, W_c2, ...}                │
│         Output: Merged cross-context clusters                      │
│         Uses learned cluster representations + pairwise scoring    │
└────────────────────────────────────────────────────────────────────┘
```

## The Cluster Merging Innovation

### Current Anno Approach (CDCR + Coalesce)

```rust
// From cdcr.rs: String similarity + Union-Find
fn mention_similarity(&self, a: &MentionRef, b: &MentionRef) -> f64 {
    crate::similarity::string_similarity(&a.text, &b.text)
}
```

**Limitations**:
- O(n²) pairwise comparisons (mitigated by LSH)
- No learned representations - relies on surface string matching
- Separate systems for long-doc (incremental_coref.rs) vs cross-doc (cdcr.rs)

### xCoRe's Approach: Learned Cluster Representations

```python
# From xCoRe: Single-layer Transformer encodes cluster members
def compute_cluster_hidden(self, cluster_mentions):
    # cluster_mentions: [num_mentions_in_cluster, hidden_dim]
    # Output: [hidden_dim] - single vector representing entire cluster
    return self.cluster_transformer(cluster_mentions)

def cluster_merge_probability(self, cluster_a, cluster_b):
    h_a = self.compute_cluster_hidden(cluster_a)
    h_b = self.compute_cluster_hidden(cluster_b)
    # Bilinear scoring
    return sigmoid(self.merge_scorer(concat(h_a, h_b)))
```

**Key properties**:
1. **Cluster-level reasoning**: Compares clusters directly, not individual mentions
2. **Learned representations**: Encodes semantic similarity, not just string overlap
3. **Single-pass merging**: No hierarchical multi-stage like Gupta et al. (2024)
4. **Order-invariant**: Works for both sequential windows and unordered documents

## Mapping to Anno's Architecture

### Level Alignment

| Anno Concept | xCoRe Concept | Notes |
|--------------|---------------|-------|
| `Signal<Location>` | Mention | Raw entity detection |
| `Track` | Within-context cluster | Intra-doc coreference chain |
| `Identity` | Cross-context cluster | Merged across contexts |
| `GroundedDocument` | Context | Single processing unit |
| `Corpus` | Context set | Multiple documents/windows |

### Where Cluster Merging Fits

```rust
// Current: anno-coalesce/src/resolver.rs
pub fn resolve_inter_doc_coref(corpus: &mut Corpus, ...) -> Vec<Identity> {
    // Collects tracks from all documents
    // Uses string similarity + union-find
    // O(n²) with LSH mitigation
}

// Proposed: Add learned cluster merging
pub fn resolve_cross_context_coref(
    contexts: &[Context],
    cluster_encoder: &ClusterEncoder,
    merge_scorer: &MergeScorer,
) -> Vec<Identity> {
    // 1. Within-context: Use existing Track formation
    let local_clusters: Vec<Vec<Track>> = contexts
        .iter()
        .map(|c| resolve_intra_doc_coref(c))
        .collect();
    
    // 2. Cross-context: NEW - learned cluster merging
    let cluster_embeddings: Vec<Tensor> = local_clusters
        .iter()
        .map(|tracks| cluster_encoder.encode(tracks))
        .collect();
    
    // 3. Pairwise scoring with threshold
    let merged = merge_clusters_by_score(
        &cluster_embeddings,
        merge_scorer,
        threshold: 0.5,
    );
    
    // 4. Convert to Identity
    merged.into_iter().map(Identity::from_cluster).collect()
}
```

## Implementation Path

### Phase 1: Cluster Encoder Infrastructure

```rust
// anno/src/backends/cluster_encoder.rs

/// Encodes a cluster of mentions into a single representation.
/// 
/// Architecture: Single-layer Transformer over mention hidden states
/// Input: Vec<Mention> with hidden states from encoder
/// Output: Fixed-size cluster embedding
pub struct ClusterEncoder {
    /// Single-layer self-attention
    attention: TransformerLayer,
    /// Pooling strategy (mean, [CLS], attention-weighted)
    pooling: PoolingStrategy,
    /// Output dimension
    hidden_dim: usize,
}

impl ClusterEncoder {
    /// Encode a cluster's mentions into a single vector.
    pub fn encode(&self, mentions: &[&Mention], hidden_states: &Tensor) -> Tensor {
        // 1. Gather mention hidden states (start concat end)
        let mention_embeds = self.gather_mention_states(mentions, hidden_states);
        
        // 2. Self-attention over mentions
        let attended = self.attention.forward(&mention_embeds);
        
        // 3. Pool to single vector
        self.pooling.apply(&attended)
    }
}
```

### Phase 2: Merge Scorer

```rust
// anno/src/backends/merge_scorer.rs

/// Scores the probability that two clusters should be merged.
/// 
/// Architecture: Bilinear scoring with feedforward
/// Input: Two cluster embeddings
/// Output: Merge probability in [0, 1]
pub struct MergeScorer {
    /// First projection
    proj1: Linear,
    /// Second projection  
    proj2: Linear,
    /// Final classification
    classifier: Linear,
}

impl MergeScorer {
    /// Compute merge probability for two clusters.
    pub fn score(&self, cluster_a: &Tensor, cluster_b: &Tensor) -> f32 {
        let h_a = relu(self.proj1.forward(cluster_a));
        let h_b = relu(self.proj2.forward(cluster_b));
        let combined = concat(&[h_a, h_b], -1);
        sigmoid(self.classifier.forward(&combined))
    }
}
```

### Phase 3: Cross-Context Resolver Trait

```rust
// anno/src/eval/cross_context_coref.rs

/// Configuration for cross-context coreference resolution.
pub struct CrossContextConfig {
    /// Maximum context size (tokens)
    pub max_context_size: usize,
    /// Merge probability threshold
    pub merge_threshold: f32,
    /// Whether to compare clusters from same context (usually false)
    pub compare_same_context: bool,
}

/// Unified resolver for long-doc and cross-doc coreference.
pub trait CrossContextResolver {
    /// Resolve coreference across multiple contexts.
    /// 
    /// For long documents: contexts are windows
    /// For cross-document: contexts are separate documents
    fn resolve_cross_context(
        &self,
        contexts: &[impl AsContext],
        config: &CrossContextConfig,
    ) -> Vec<CorefChain>;
}
```

### Phase 4: Training Infrastructure

The xCoRe paper describes a **dynamic batching** strategy that's critical for training:

```rust
// Training sample construction
struct CrossContextSample {
    /// Contexts in this sample
    contexts: Vec<Context>,
    /// Gold clusters (spans cross contexts)
    gold_clusters: Vec<GoldCluster>,
}

impl CrossContextSample {
    /// Dynamic batching: variable context count and size
    fn sample_from_document(doc: &Document, max_tokens: usize) -> Self {
        // 1. Sample number of contexts n ∈ [1, max_tokens/avg_sentence_len]
        let n = sample_context_count(max_tokens, doc.avg_sentence_len());
        
        // 2. Split document into n contiguous contexts
        let context_size = doc.token_count() / n;
        let contexts = doc.split_into_windows(context_size);
        
        // 3. Derive gold clusters that span contexts
        let gold_clusters = derive_cross_context_gold(doc.gold_chains(), &contexts);
        
        Self { contexts, gold_clusters }
    }
}
```

## Loss Functions

xCoRe uses a multi-task loss combining all three pipeline stages:

```
L_total = L_mention_extraction + L_mention_clustering + L_cluster_merging
```

Where:
- `L_mention_extraction`: BCE on start/end token classification
- `L_mention_clustering`: BCE on mention-antecedent pairs (within context)
- `L_cluster_merging`: BCE on cluster-cluster pairs (cross context)

## Datasets for Evaluation

### Already in Anno Registry

| Dataset | Setting | Notes |
|---------|---------|-------|
| `OntoNotes` | Short-doc | Standard baseline |
| `PreCo` | Medium-doc | With singletons |
| `LitBank` | Long-doc | Literary texts, 2k tokens |
| `ECBPlus` | Cross-doc | Event coreference |
| `SciCo` | Cross-doc | Scientific concepts |

### Needed for Full xCoRe Evaluation

| Dataset | Setting | Status |
|---------|---------|--------|
| `BookCoref` | Very long-doc (full books) | Add to registry |
| `Animal Farm` | Long-doc (single book, gold) | Add to registry |

## Expected Results (from Paper)

| Setting | Dataset | xCoRe | Previous SOTA | Improvement |
|---------|---------|-------|---------------|-------------|
| Cross-doc | ECB+ | 42.4 | 35.7 (PMCoref) | +6.7 |
| Cross-doc | SciCo | 30.5 | 23.3 (PMCoref) | +7.2 |
| Long-doc | Animal Farm | 42.5 | 36.3 (Dual-cache) | +6.2 |
| Long-doc | LitBank | 78.2 | 78.0 (Maverick) | +0.2 |
| Medium | OntoNotes | 83.2 | 83.6 (Maverick) | -0.4 |

**Key observation**: xCoRe excels at cross-doc (+18-31% relative) and long-doc (+17% relative) while matching medium-doc performance.

## Step-wise Error Analysis (from Paper)

| Setting | Predicted Mentions | Gold Mentions | Gold Clusters |
|---------|-------------------|---------------|---------------|
| ECB+ | 40.3 | 73.8 | 77.4 |
| SciCo | 27.8 | 62.3 | 68.8 |
| Animal Farm | 42.2 | 58.9 | 62.7 |

**Takeaway**: Mention extraction is the main bottleneck for cross-doc. Cluster merging itself adds only ~5-6 points when starting from gold clusters, but mention errors compound significantly.

## Integration Priority

1. **High**: Add BookCoref and Animal Farm to dataset registry
2. **High**: Implement ClusterEncoder for learned cluster representations
3. **Medium**: Implement MergeScorer with bilinear classification
4. **Medium**: Add cross-context training infrastructure
5. **Low**: Unify `incremental_coref.rs` and `cdcr.rs` under common interface

## Relation to Existing Anno Components

### Box Embeddings (box_embeddings.rs)

xCoRe uses DeBERTa embeddings projected through linear layers. Box embeddings offer a geometric alternative where:
- Cluster containment = box containment
- Cluster merging = box intersection/union

**Potential synergy**: Use box embeddings as cluster representations for geometric merging decisions.

### Maverick (maverick_coref.rs)

xCoRe builds directly on Maverick's architecture:
- Same multi-expert scorer for within-context clustering
- Same start-to-end mention extraction
- Adds cluster-level Transformer + merge scorer

Anno's Maverick implementation provides the foundation for xCoRe integration.

### Coalesce (anno-coalesce)

The hierarchical clustering in coalesce could be enhanced with xCoRe's learned merge scores instead of simple similarity thresholds.

## Open Questions

1. **Model size**: xCoRe uses DeBERTa-v3-large (300M params). Can we achieve similar gains with smaller models?

2. **Incremental inference**: xCoRe requires all contexts at merge time. How to adapt for streaming documents?

3. **Event coreference**: Paper focuses on entity coref. Does the approach transfer to event coref (ECB+ annotates both)?

4. **Multilingual**: All experiments are English. How does cross-context generalize to other languages?

## References

1. Martinelli, Gatti & Navigli (2025). xCoRe: Cross-context Coreference Resolution. EMNLP 2025.
2. Martinelli, Barba & Navigli (2024). Maverick: Efficient and accurate coreference resolution. ACL 2024.
3. Cattan et al. (2021). Cross-document coreference resolution over predicted mentions. ACL Findings.
4. Guo et al. (2023). Dual cache for long document neural coreference resolution. ACL.

