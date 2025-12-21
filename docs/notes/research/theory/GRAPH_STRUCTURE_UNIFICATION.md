# Graph-Based Structure Discovery: A Unifying Framework

This document describes the unifying principle behind several modules in the anno workspace.

## The Pattern

All of these algorithms share a common structure:

```
Local Relationships → Graph Construction → Iterative Algorithm → Hierarchical Output
```

| Module | Input | Graph | Algorithm | Output |
|--------|-------|-------|-----------|--------|
| `anno::keywords` | Text | Word co-occurrence | PageRank/RAKE/YAKE | Important terms |
| `anno::salience` | Text + Entities | Entity co-occurrence | PageRank | Important entities |
| `anno::graph_coref` | Mentions | Coreference links | Iterative refinement | Coref chains |
| `anno-coalesce` | Entity mentions | Similarity matrix | HAC/Union-Find | Entity clusters |
| `anno-strata` | Knowledge graph | Entity-relation | Leiden/Louvain | Community hierarchy |

## Visual Representation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    GRAPH-BASED STRUCTURE DISCOVERY                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Text                                                                      │
│    │                                                                        │
│    ├──► anno::keywords ──► Word Graph ──► PageRank ──► Keyword Ranking      │
│    │                                                                        │
│    ├──► anno::salience ──► Entity Graph ──► PageRank ──► Entity Ranking     │
│    │                                                                        │
│    ▼                                                                        │
│   Mentions                                                                  │
│    │                                                                        │
│    ├──► graph_coref ──► Coref Graph ──► Iterative Refine ──► Coref Chains   │
│    │                                                                        │
│    ├──► coalesce ──► Similarity Graph ──► HAC/Correlation ──► Clusters      │
│    │                                                                        │
│    ▼                                                                        │
│   Knowledge Graph                                                           │
│    │                                                                        │
│    └──► strata ──► Entity-Relation Graph ──► Leiden ──► Communities         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Mathematical Foundation

### Common Theme: Finding Structure from Local Relationships

All these algorithms work by:

1. **Building a weighted graph** from pairwise relationships
2. **Running an iterative algorithm** that propagates information through the graph
3. **Extracting structure** (rankings, clusters, communities) from the converged state

### PageRank Family (Keywords, Salience)

The PageRank algorithm finds node importance by solving:

```
PR(u) = (1-d)/N + d × Σ PR(v)/deg(v)
        for all v linking to u
```

Where:
- `d` = damping factor (typically 0.85)
- `N` = number of nodes
- `deg(v)` = out-degree of node v

The intuition: a node is important if it's connected to other important nodes.

### Modularity Optimization (Strata)

The Leiden algorithm maximizes modularity:

```
Q = (1/2m) × Σ [A_ij - γ × k_i × k_j / 2m] × δ(c_i, c_j)
```

Where:
- `m` = total edge weight
- `A_ij` = adjacency matrix
- `k_i` = degree of node i
- `γ` = resolution parameter
- `δ` = 1 if same community

The intuition: good communities have more internal edges than expected by chance.

### Agglomerative Clustering (Coalesce)

HAC builds a dendrogram by iteratively merging closest clusters:

```
D(A∪B, C) = α_A × D(A,C) + α_B × D(B,C) + β × D(A,B) + γ × |D(A,C) - D(B,C)|
```

Different linkage methods set different parameters:
- Single: γ = -1/2 (minimum distance)
- Complete: γ = +1/2 (maximum distance)
- Average: γ = 0 (mean distance)

### Iterative Graph Refinement (Graph Coref)

The Graph-to-Graph Transformer approach (Miculicich & Henderson 2022) refines
coreference graphs iteratively:

```
Gₜ = f(D, Gₜ₋₁)  for t = 1, 2, ..., T
```

The key insight: condition each iteration on the previous graph structure,
enabling global consistency in decisions. The algorithm stops when the graph
converges (Gₜ = Gₜ₋₁) or after T iterations (empirically T=4 is optimal).

**Graph encoding in attention** (the core G2GT technique):

```
Attention(Q, K, V, Lk, Lv) = softmax(Q·(K + Lk)/√d)·(V + Lv)
where Lk = E(Gₜ₋₁)·Wk, Lv = E(Gₜ₋₁)·Wv
```

The previous iteration's graph is encoded via relation embeddings added
directly to attention keys/values. This is more expressive than our
heuristic implementation, which uses explicit transitivity bonuses instead.

**Complexity**: O(N² × T) vs O(N⁴) for Lee et al. (2017) span enumeration.

## Relationship to RAPTOR

RAPTOR (Recursive Abstractive Processing for Tree-Organized Retrieval) follows the same pattern:

```
Chunks → Embedding Similarity → GMM Clustering → Recursive Summarization → Tree
```

| Step | RAPTOR | Anno Equivalent |
|------|--------|-----------------|
| Local relationship | Embedding similarity | Entity co-occurrence / Similarity |
| Graph structure | Similarity matrix | Entity graph / KG |
| Iterative algorithm | GMM + recursion | Leiden / HAC |
| Output | Summary tree | Community hierarchy / Clusters |

The key insight: **all are ways of discovering hierarchical structure from local relationships**.

## Practical Implications

### When to Use What

| Task | Module | Reason |
|------|--------|--------|
| "What terms are important?" | `anno::keywords` | Term-level ranking |
| "What entities matter?" | `anno::salience` | Entity-level ranking |
| "Which mentions refer to the same entity?" | `anno::graph_coref` | Within-document coreference |
| "Which mentions are the same entity (across docs)?" | `anno-coalesce` | Cross-document identity resolution |
| "What communities exist?" | `anno-strata` | Graph structure |
| "What are the key points?" | Summarization* | Sentence-level selection |

*Summarization not yet implemented but would follow the same pattern.

### Coref vs Coalesce: Key Differences

Both resolve "which things are the same," but at different scopes:

| Aspect | `graph_coref` | `coalesce` |
|--------|--------------|------------|
| Scope | Single document | Multiple documents |
| Input | Mentions (text spans) | Entities (typed, canonical) |
| Algorithm | Iterative refinement | HAC / Union-Find |
| Output | Coref chains | Entity clusters |
| Transitivity | Built into refinement | Post-hoc via clustering |

**Pipeline integration**: NER → `graph_coref` → `coalesce` → `strata`

```rust
// Full pipeline
let mentions = ner.extract_mentions(doc)?;
let chains = graph_coref.resolve(&mentions);           // Within-doc
let entities = chains_to_entities(chains);
let clusters = coalesce.cluster(&all_entities);        // Cross-doc
let communities = strata.leiden(&knowledge_graph);     // Graph structure
```

### Chaining Modules

These modules compose naturally:

```rust
// Extract → Rank → Coalesce → Stratify
let entities = model.extract_entities(text, None)?;
let salient = salience::TextRankSalience::default().rank(text, &entities);
let clusters = coalesce::cluster_entities(&salient_entities, similarity_fn);
let communities = strata::leiden(&knowledge_graph);
```

## Future Directions

1. **Unified Graph Representation**: Share graph construction code between modules
2. **Composable Rankers**: Pipeline keyword → entity → cluster ranking
3. **Extractive Summarization**: Apply same pattern at sentence level
4. **Cross-Modal**: Images → visual entities → graph → structure

## References

- Page, L., et al. (1999). "The PageRank Citation Ranking"
- Mihalcea, R., & Tarau, P. (2004). "TextRank: Bringing Order into Text"
- Traag, V., et al. (2019). "From Louvain to Leiden"
- Rose, S., et al. (2010). "RAKE: Rapid Automatic Keyword Extraction"
- Sarthi, P., et al. (2024). "RAPTOR: Recursive Abstractive Processing"
- Lee, K., et al. (2017). "End-to-end Neural Coreference Resolution"
- Miculicich, L. & Henderson, J. (2022). "Graph Refinement for Coreference Resolution"
  [arXiv:2203.16574](https://arxiv.org/abs/2203.16574)

