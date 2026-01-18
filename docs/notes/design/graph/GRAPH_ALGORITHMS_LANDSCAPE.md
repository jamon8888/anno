# Graph Algorithms Landscape (December 2025)

This document maps the landscape of graph algorithms relevant to anno-tier,
distinguishing between what we have implemented, what's classical, and what's
emerging in 2025.

## Design Principle

**tier is node-type agnostic.** Nodes can be entities, documents, sentences,
chunks, concepts, or events. The algorithms only see graph structure.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          NODE TYPES × ALGORITHMS                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Signal Extraction (anno)        Graph Analysis (tier)                   │
│   ════════════════════════        ═══════════════════════                   │
│                                                                              │
│   NER → entities ──────────┐                                                │
│   RE → relations           │      ┌──────────────────────────┐              │
│   Coref → clusters         │      │  CENTRALITY              │              │
│                            │      │  ├─ PageRank (✓)         │              │
│   Chunking → chunks ───────┼────► │  ├─ Betweenness (✓)      │              │
│   (for RAG)                │      │  ├─ HITS (✓)             │              │
│                            │      │  └─ GNN-based (future)   │              │
│   Summarization → sents ───┤      │                          │              │
│   (for LexRank)            │      │  COMMUNITY DETECTION     │              │
│                            │      │  ├─ Leiden (✓)           │              │
│   Events → temporal ───────┘      │  ├─ Louvain (todo)       │              │
│                                   │  ├─ Label Propagation    │              │
│                                   │  └─ GNN-based (future)   │              │
│                                   └──────────────────────────┘              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Implemented in tier (December 2025)

### Centrality

| Algorithm | Complexity | Use Case | Status |
|-----------|------------|----------|--------|
| `PageRank` | O(V × iter) | General importance | ✓ Implemented |
| `Eigenvector` | O(V × iter) | Simpler PageRank | ✓ Implemented |
| `Betweenness` | O(V × E) | Bridge detection | ✓ Implemented |
| `Closeness` | O(V × E) | Information spread | ✓ Implemented |
| `Hits` | O(V × iter) | Hub/authority distinction | ✓ Implemented |

### Community Detection

| Algorithm | Type | Use Case | Status |
|-----------|------|----------|--------|
| `Leiden` | Modularity | Best quality | ✓ Implemented |
| `Louvain` | Modularity | Comparison baseline | ✓ Implemented |
| `LabelPropagation` | Fast approx | Very large graphs | ✓ Implemented |
| `HierarchicalLeiden` | Multi-resolution | Hierarchical communities | ✓ Implemented |

## Classical Methods (Not Yet Implemented)

### Centrality

| Algorithm | When to Use | Complexity | Status |
|-----------|-------------|------------|--------|
| Katz | Weighted paths, directed | O(V²) or iterative | Not implemented |
| Harmonic | Disconnected graphs | O(V × E) | Closeness has harmonic mode |

### Community Detection

| Algorithm | When to Use | Complexity | Status |
|-----------|-------------|------------|--------|
| Infomap | Flow/navigation-based graphs | O(E log E) | Not implemented |
| Spectral | Small graphs, known k | O(V³) | Not implemented |
| Girvan-Newman | Small graphs, interpretable | O(V × E²) | Not implemented |

## 2025 State-of-the-Art (Future Direction)

### GNN-Based Community Detection

The 2025 frontier combines classical methods with learned representations:

| Method | Key Innovation | Reference |
|--------|----------------|-----------|
| DGCluster, MAGI | Soft modularity + GNN | Nature 2025 |
| CommDGI, CPGCL | Contrastive self-supervised | Frontiers AI 2025 |
| Bimodularity | Directed sender/receiver | PNAS 2025 |
| GNN + local centrality | Local features → global via GCN | arXiv 2025 |

**Key insight**: Local centrality measures (degree, egonet conductance) as GNN
node features can predict global importance without computing expensive global
centralities.

### GNN-Based Node Importance

| Method | Key Innovation | Reference |
|--------|----------------|-----------|
| FNGCN | Feature network over local centralities | arXiv 2508 |
| GATv2-FN | Attention + centrality features | Nature Sci Rep 2025 |
| GENI | Transformer-style importance | ACM KDD |
| Centrality-guided pretraining | Importance in embeddings | ICLR 2025 |

### Hierarchical Graph RAG (Beyond RAPTOR)

| Method | Key Innovation | Reference |
|--------|----------------|-----------|
| HiRAG | Multi-layer KG with cluster summaries | ACL Findings 2025 |
| KG-Retriever | Hierarchical Index Graph (HIG) | Shichuan 2025 |
| RAG4GFM | Task-aware (node/edge/graph) | NeurIPS 2025 |
| GraphRAG | Community hierarchy, global/local search | Microsoft 2024 |

**Pattern**: Build multi-layer graph index → recursive summarization →
coarse-to-fine retrieval.

## Implementation Roadmap

### Phase 1: Classical Additions (Low Effort) ✓ COMPLETE
- [x] `Louvain` - predecessor to Leiden, useful for comparison
- [x] `LabelPropagation` - O(E) fast approximation
- [x] `Eigenvector` centrality - simpler PageRank variant
- [x] `Closeness` centrality - distance-based importance

### Phase 2: Infomap and Flow (Medium Effort)
- [ ] `Infomap` - for navigation/click graphs, multi-scale
- [ ] `RandomWalkCentrality` - personalized PageRank variants

### Phase 3: GNN Integration (High Effort, Requires ML)
- [ ] `LocalCentralityGCN` - local features → GCN → global importance
- [ ] `CommunityGNN` - end-to-end community + embedding learning
- [ ] Integration with Candle/Burn for inference

### Phase 4: Hierarchical RAG (Requires Full Pipeline)
- [ ] `HierarchicalIndex` - multi-layer graph construction
- [ ] `CoarseToFineRetrieval` - adaptive layer navigation
- [ ] Integration with anno's RAG pipeline (if exists)

## Which Algorithm to Choose?

### For Node Importance

```
Is graph directed with clear hub/authority structure?
  └─ Yes → HITS
  └─ No → 
      Need to find bridges between clusters?
        └─ Yes → Betweenness
        └─ No → PageRank (default)
```

### For Community Detection

```
Is graph very large (>1M edges)?
  └─ Yes → Label Propagation (then refine with Leiden)
  └─ No →
      Need guaranteed well-connected communities?
        └─ Yes → Leiden (default)
        └─ No → Louvain (faster, slightly lower quality)
```

### For Hierarchical Structure

```
Need multi-scale communities?
  └─ Yes → HierarchicalLeiden at multiple resolutions
  └─ No →
      Graph represents navigation/flow?
        └─ Yes → Infomap (when implemented)
        └─ No → Single-level Leiden
```

## References

1. Traag et al. (2019). "From Louvain to Leiden" - Scientific Reports
2. Page et al. (1999). "PageRank Citation Ranking" - Stanford
3. Freeman (1977). "Betweenness centrality" - Sociometry
4. Kleinberg (1999). "HITS" - JACM
5. Rosvall & Bergstrom (2008). "Infomap" - PNAS
6. Nature (2025). "DGCluster: GNN community detection"
7. ACL Findings (2025). "HiRAG: Hierarchical Knowledge RAG"
8. arXiv 2508.01278 (2025). "FNGCN: Feature Network GCN for centrality"

