# Cross-Document Coreference Architecture

**Related**: See [INTER_INTRA_DOC_ABSTRACTIONS.md](INTER_INTRA_DOC_ABSTRACTIONS.md) for coreference abstractions.

## How Crossdoc Coref Works via Tracks

### Overview

Cross-document coreference resolution in `anno-coalesce` operates on **tracks** (Level 2), not raw signals (Level 1). This design choice enables efficient clustering across documents.

### Algorithm: Online Merging (Not Query-Time)

**Merging happens during `resolve_inter_doc_coref()` execution**, not at query time. This is an **online** (eager) approach, not lazy.

### Step-by-Step Process

1. **Track Collection** (Lines 75-92 in `coalesce/src/resolver.rs`):
   - Iterate through all documents in `Corpus`
   - Extract all `Track` instances (within-document coreference chains)
   - Collect track metadata: `canonical_surface`, `entity_type`, `embedding`, `cluster_confidence`
   - Create `TrackRef` for each track (doc_id + track_id)

2. **Similarity Computation** (Lines 117-142):
   - Compare all track pairs using union-find
   - **Similarity metric**: Prefer embeddings if available, fallback to string similarity (Jaccard)
   - **Type matching**: If `require_type_match=true`, only compare tracks with same `entity_type`
   - **Threshold**: Only merge if `similarity >= threshold` (default 0.7)

3. **Clustering** (Lines 144-149):
   - Build clusters using union-find data structure
   - Each cluster contains tracks from potentially multiple documents

4. **Identity Creation** (Lines 170-188):
   - For each cluster, create an `Identity` (Level 3)
   - Use first track's `canonical_surface` as identity name
   - Store all `TrackRef`s in `IdentitySource::CrossDocCoref`
   - **Singleton clusters** (one track) still create identities

5. **Track Linking** (Lines 190-213):
   - Link each track to its identity via `doc.link_track_to_identity(track_id, identity_id)`
   - This updates the `track_to_identity` mapping in each `GroundedDocument`
   - **This is the merging step** - tracks are now linked to global identities

### Why Tracks, Not Signals?

**Tracks are the right abstraction** because:
- Tracks already represent within-document coreference (multiple signals → one track)
- Cross-doc coref is "tracks → identity" (multiple tracks → one identity)
- This avoids O(n²) signal comparisons across documents
- Tracks have canonical surface forms and embeddings (better for similarity)

### Online vs Query-Time Trade-offs

**Current: Online (Eager) Merging**
- ✅ All identities created upfront
- ✅ Fast queries (identities already exist)
- ✅ Consistent state (no lazy evaluation surprises)
- ❌ Slower initial processing (must process all documents)
- ❌ Memory overhead (all identities in memory)

**Alternative: Query-Time (Lazy) Merging**
- ✅ Faster initial processing
- ✅ Lower memory (only compute when needed)
- ❌ Slower queries (must compute on-demand)
- ❌ Inconsistent results (depends on query order)
- ❌ Harder to cache

**Current choice is correct** for batch processing workflows (most common use case).

### Example Flow

```rust
// 1. Documents with tracks
doc1: Track(id=1, canonical="barack obama", signals=[Signal("Barack Obama"), Signal("He")])
doc2: Track(id=1, canonical="obama", signals=[Signal("Obama")])

// 2. resolve_inter_doc_coref() runs
//    - Compares track1 from doc1 vs track1 from doc2
//    - Similarity("barack obama", "obama") = 0.67 (Jaccard)
//    - If threshold=0.6, they merge

// 3. Creates Identity
Identity(id=1, canonical_name="barack obama", 
         source=CrossDocCoref { track_refs: [
           TrackRef(doc_id="doc1", track_id=1),
           TrackRef(doc_id="doc2", track_id=1)
         ]})

// 4. Links tracks to identity
doc1.link_track_to_identity(track_id=1, identity_id=1)
doc2.link_track_to_identity(track_id=1, identity_id=1)

// 5. Now queries can use identity_id to find all mentions
doc1.get_track(1).identity_id == Some(1)
doc2.get_track(1).identity_id == Some(1)
```

### Performance Characteristics

- **Time Complexity**: O(n²) track comparisons, where n = total tracks across all documents
- **Space Complexity**: O(n) for union-find + O(k) for identities, where k = number of clusters
- **Optimization**: Could use LSH (Locality-Sensitive Hashing) for large corpora (not yet implemented)

### Future Improvements

1. **Incremental Updates**: Add new documents to existing corpus without recomputing all identities
2. **LSH for Scalability**: Use approximate similarity for O(n log n) clustering
3. **Query-Time Refinement**: Allow re-clustering with different thresholds without full recompute
4. **Streaming**: Process documents one-by-one, maintaining identity graph incrementally

