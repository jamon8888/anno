# Clustering Methods in Anno: coalesce vs tier

*Understanding when to use which clustering approach*

---

## Overview

Anno provides two clustering crates for different purposes:

| Crate | Purpose | Input | Output | Algorithms |
|-------|---------|-------|--------|------------|
| **coalesce** | Entity resolution | Mentions from NER | Entity clusters | Union-Find, HAC, Correlation, Streaming |
| **tier** | Community detection | Knowledge graphs | Community hierarchy | Leiden, (Louvain) |

---

## coalesce: Entity Resolution

**Problem:** Given mentions extracted by NER (e.g., "Barack Obama", "Obama", "the president"), determine which refer to the same entity.

**Input format:** `Track` objects from `anno-core`, each representing an entity mention with:
- Surface form ("Barack Obama")
- Entity type (Person, Organization, etc.)
- Optional embedding vector
- Document context

**Output:** `Identity` objects linking related tracks across documents.

### Algorithms

#### 1. Union-Find (Batch Resolution)
```rust
let resolver = Resolver::new().with_threshold(0.7);
let identities = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
```

- **Complexity:** O(n²) pairwise comparisons
- **Use when:** Small corpus (<10K entities), need exact clustering
- **Strength:** Simple, deterministic, easy to tune threshold

#### 2. LSH + Union-Find (Scalable Resolution)
```rust
let mut lsh = MinHashLSH::new(LSHConfig::default());
for track in tracks {
    lsh.insert_text(&track.id, &track.canonical_surface);
}
// Only compare candidates
for (i, j) in lsh.candidate_pairs() {
    // ...
}
```

- **Complexity:** O(n log n) expected
- **Use when:** Large corpus (10K-1M entities)
- **Strength:** Scales to millions of entities

#### 3. Hierarchical Agglomerative Clustering (HAC)
```rust
let sims = compute_similarity_matrix(&tracks);
let dendrogram = hierarchical_from_similarity(&sims, Linkage::Ward);
let clusters = dendrogram.cut_to_k_clusters(k);
```

- **Complexity:** O(n² log n)
- **Use when:** Need interpretable hierarchy, exploratory analysis
- **Strength:** Produces dendrogram, can cut at any level

#### 4. Correlation Clustering
```rust
let mut graph = LabeledGraph::new(n);
graph.add_edge(i, j, EdgeLabel::Positive);  // Should cluster
graph.add_edge(i, k, EdgeLabel::Negative);  // Should not cluster
let result = pivot_clustering(&graph, &mut rng);
```

- **Complexity:** O(n + m)
- **Use when:** Have explicit match/non-match labels from a matcher
- **Strength:** 3-approximation guarantee, handles noisy labels

**Correlation Clustering Variants (2024-2025):**

| Algorithm | Approximation | When to Use |
|-----------|--------------|-------------|
| Pivot (ACN 2008) | 3 | Default, fast |
| Modified Pivot (2025) | ~2.5 | ~23% fewer errors |
| **Min-Max** (2024) | 4 | Avoid "bad" clusters |
| **Chromatic** | varies | Color-constrained clustering |

**Min-Max Correlation Clustering:**
```rust
// Minimizes the MAXIMUM disagreements per cluster, not total
let result = min_max_clustering(&graph, &mut rng);
println!("Max per-cluster disagreements: {}", result.max_disagreements);
```

**Chromatic Clustering:**
```rust
// Ensures no two same-colored nodes are in the same cluster
// Useful when entities have exclusion constraints (e.g., roles, teams)
let config = ChromaticClusteringConfig { k: 3, colors: vec![0, 0, 1, 1, 2, 2] };
let result = chromatic_clustering(&graph, &config, &mut rng);
```

#### 5. Streaming Resolution
```rust
let mut resolver = StreamingResolver::new(StreamingConfig::default());
for doc in documents {
    for track in doc.tracks {
        resolver.add_track(&doc.id, &track);
    }
}
let identities = resolver.to_identities();
```

- **Complexity:** O(1) amortized per entity
- **Use when:** Documents arrive continuously, can't batch
- **Strength:** Low latency, bounded memory

---

## tier: Community Detection

**Problem:** Given a knowledge graph with entities and relations, discover communities—groups of densely connected nodes.

**Input format:** `GraphDocument` from `anno-core`:
- Nodes (entities from coalesce)
- Edges (relations between entities)

**Output:** Community assignments at multiple granularities.

### Algorithms

#### Leiden (Hierarchical Community Detection)
```rust
let clusterer = HierarchicalLeiden::new()
    .with_resolution(1.0)
    .with_levels(3);
let annotated = clusterer.cluster(&graph)?;
```

- **Complexity:** O(n log n) expected
- **Use when:** Have a knowledge graph, want to find latent structure
- **Strength:** Guarantees well-connected communities

---

## When to Use Which

### Use coalesce when:
- You have entity mentions from NER output
- You need to determine which mentions are the same entity
- Your input is text-based (strings, embeddings)
- You're doing entity resolution / deduplication

### Use tier when:
- You have a constructed knowledge graph
- You want to find communities of related entities
- Your input is graph-structured (nodes + edges)
- You're doing graph summarization / clustering

---

## The Pipeline

```
Text → [anno NER] → Entities → [coalesce] → Identities → [Relations] → Graph → [tier] → Communities
```

1. **Extract:** NER produces entity mentions (anno)
2. **Coalesce:** Cluster mentions into entities (coalesce)
3. **Relate:** Extract relations between entities (anno)
4. **Stratify:** Find community structure (tier)

---

## Algorithm Selection Guide

```
START
  │
  ├─ Do you have a graph?
  │    │
  │    YES → Use tier (Leiden)
  │    │
  │    NO ↓
  │
  ├─ Do you have explicit +/- labels?
  │    │
  │    YES → Use coalesce::correlation (Pivot)
  │    │
  │    NO ↓
  │
  ├─ Is real-time processing required?
  │    │
  │    YES → Use coalesce::streaming
  │    │
  │    NO ↓
  │
  ├─ Do you need a dendrogram?
  │    │
  │    YES → Use coalesce::hierarchical (HAC)
  │    │
  │    NO ↓
  │
  ├─ Is n > 10,000?
  │    │
  │    YES → Use coalesce::lsh + resolver
  │    │
  │    NO → Use coalesce::resolver (Union-Find)
```

---

## Mathematical Comparison

| Aspect | coalesce HAC | tier Leiden |
|--------|--------------|---------------|
| **Objective** | Minimize linkage distance | Maximize modularity |
| **Input** | Similarity matrix | Adjacency matrix |
| **Output** | Dendrogram | Partition |
| **Hierarchy** | Explicit (cut dendrogram) | Implicit (multi-resolution) |
| **Guarantee** | Exact | Well-connected communities |

### HAC Objective (Ward's method)
```
Δ(Cᵢ, Cⱼ) = (nᵢ · nⱼ)/(nᵢ + nⱼ) · ||μᵢ - μⱼ||²
```

### Leiden Objective (Modularity)
```
Q = (1/2m) Σᵢⱼ [Aᵢⱼ - γ(kᵢkⱼ/2m)] δ(cᵢ, cⱼ)
```

---

## References

- Traag et al. (2019). "From Louvain to Leiden: guaranteeing well-connected communities"
- Ward (1963). "Hierarchical Grouping to Optimize an Objective Function"
- Bansal, Blum, Chawla (2004). "Correlation Clustering"
- Charikar et al. (1997). "Incremental clustering and dynamic information retrieval"

