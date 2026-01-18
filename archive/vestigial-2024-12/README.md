# Archived Vestigial Crates (December 2024)

These crates were earlier implementations that were superseded by more
comprehensive versions but weren't cleaned up. Both pairs had the same
`name` field in Cargo.toml, causing confusion.

## What Was Archived

### `tier/` -> replaced by `anno-tier/`

- **tier/**: ~395 lines, simple Leiden + HierarchicalLeiden only
- **anno-tier/**: ~3169 lines, full suite:
  - centrality (PageRank, Betweenness, Closeness, Eigenvector, HITS)
  - graph_utils (connectivity, shortest paths, diameter)
  - label_propagation
  - leiden
  - louvain
  - pagerank

### `coalesce/` -> replaced by `anno-coalesce/`

- **coalesce/**: ~2182 lines, basic alignment/resolver/similarity
- **anno-coalesce/**: ~7043 lines, full suite:
  - canonical (mention selection strategies)
  - configuration (Dempster-Shafer probability distributions)
  - correlation (Pivot, Modified Pivot, Min-Max, Chromatic clustering)
  - evidence (multi-source aggregation)
  - hierarchical (dendrogram, Lance-Williams linkage)
  - lsh (MinHash, SimHash for blocking)
  - resolver (Union-Find batch resolution)
  - streaming (Doubling Algorithm for incremental resolution)

## Why Archived Instead of Deleted

- Preserves git history without polluting active workspace
- `alignment.rs` from `coalesce/` was needed by `anno-coalesce/` (copied over)
- Reference for any code that might have been unique to these versions

## Date

Archived: 2024-12-10
