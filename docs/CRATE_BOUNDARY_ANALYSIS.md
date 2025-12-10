# Crate Boundary Analysis: Type Duplication in Entity Resolution

*Reviewing backwards to find what we missed*

---

## The Problem: Three Type Hierarchies

On close inspection, we have **three parallel type hierarchies** representing the same conceptual entities:

### 1. `anno-core::grounded` (The Canonical Types)

```rust
Signal<Location> → Track → Identity
    │                │         │
    │                │         └─ IdentitySource::CrossDocCoref { track_refs }
    │                └─ TrackRef { doc_id, track_id }
    └─ SignalRef
```

This is the **correct** abstraction. `Corpus` holds `GroundedDocument`s, each with Signals and Tracks. Identities span documents.

### 2. `coalesce::streaming` (Orphaned Types)

```rust
EntityMention → EntityCluster
    │               │
    │               └─ mentions: Vec<EntityMention>
    └─ doc_id, canonical_surface, entity_type, embedding
```

These types are **duplicates** of `anno-core` types. They have no conversion to/from `Track`/`Identity`. The streaming resolver operates in isolation.

### 3. `anno::eval::cdcr` (Evaluation Types)

```rust
Document → MentionRef → CrossDocCluster
    │           │              │
    │           │              └─ canonical_name, entity_type, doc_mentions
    │           └─ doc_id, entity_idx, within_doc_cluster
    └─ entities: Vec<Entity>, coref_chains: Vec<CorefChain>
```

Yet another set of types for evaluation. `CrossDocCluster` is conceptually the same as `Identity`, but there's no direct conversion.

---

## Why This Matters

### 1. No End-to-End Pipeline

You can't currently do:

```rust
// This should work but doesn't
let entities = ner.extract(&text)?;           // Vec<Entity>
let corpus = build_corpus(documents)?;        // Corpus with Tracks
let identities = streaming.resolve(corpus)?;  // Vec<Identity>
let metrics = evaluate(identities, gold)?;    // CorefEvaluation
```

Instead, you have to manually convert between type systems.

### 2. Duplicated Logic (Resolved)

- ~~`coalesce::streaming::string_similarity` duplicates `coalesce::resolver::string_similarity`~~
  - **Fixed**: Renamed to `trigram_similarity` (character n-grams) vs `string_similarity` (word Jaccard)
  - Both are valid with different use cases; now clearly distinguished
- `EntityCluster::canonical_name` duplicates `Identity::canonical_name`
- `EntityMention::doc_id` duplicates `TrackRef::doc_id`

### 3. Untestable Invariants

The `anno-core` hierarchy has important invariants:
- All `SignalRef`s in a `Track` must point to valid signals in the same document
- All `TrackRef`s in an `IdentitySource::CrossDocCoref` must point to valid tracks
- `Identity.canonical_name` should be derived from constituent tracks

But these invariants aren't enforced or tested because the streaming types bypass them entirely.

---

## The Interesting Crate Boundaries (Second Look)

### `anno-core` ↔ `coalesce`

**Current state:** `resolver.rs` correctly uses `anno-core` types (`Corpus`, `Track`, `Identity`).

**Issue:** `streaming.rs` defines its own types, breaking the abstraction.

**Fix needed:**

```rust
// streaming.rs should have:
impl StreamingResolver {
    /// Add a track from a document to the resolver.
    pub fn add_track(&mut self, doc_id: &str, track: &Track) -> u64 {
        // Convert Track to internal representation
        // Return cluster ID
    }

    /// Export clusters as Identity objects.
    pub fn to_identities(&self) -> Vec<Identity> {
        // Convert internal clusters to Identity
    }
}
```

### `coalesce` ↔ `anno::eval`

**Current state:** No direct connection. `cdcr.rs` has `CDCRResolver` that doesn't use `coalesce` at all.

**Issue:** We can't evaluate coalesce algorithms against coreference benchmarks.

**Fix needed:**

```rust
// anno/src/eval/cdcr.rs should have:
impl From<Identity> for CrossDocCluster {
    fn from(identity: Identity) -> Self {
        // Convert for evaluation
    }
}

// Or better: use Identity directly in evaluation
fn evaluate_cdcr(
    predicted: &[Identity],
    gold: &[CrossDocCluster],
) -> CorefEvaluation
```

### `anno-core` ↔ `anno::eval`

**Current state:** `eval/coref.rs` uses `CorefChain` which is similar to but not the same as `Track`.

**Issue:** `Mention` (eval) ≠ `Signal` (core). `CorefChain` (eval) ≠ `Track` (core).

**This is actually okay** because evaluation types need to match dataset formats (CoNLL-2012, ECB+, etc.), which differ from the internal representation. The key is having clean conversion functions.

---

## Property Tests We Need

### 1. Hierarchy Consistency

```rust
proptest! {
    #[test]
    fn track_signals_all_exist(signals: Vec<Signal>, track_indices: Vec<usize>) {
        // All SignalRefs in a Track point to valid signals
    }

    #[test]
    fn identity_tracks_from_multiple_docs(tracks: Vec<(DocId, Track)>) {
        // IdentitySource::CrossDocCoref can have tracks from N documents (N >= 1)
    }
}
```

### 2. Clustering Invariants

```rust
proptest! {
    #[test]
    fn clusters_partition_all_items(items: Vec<String>) {
        // Every item in exactly one cluster
        // Total items in clusters == input count
    }

    #[test]
    fn cluster_count_bounded(n: usize) {
        // num_clusters <= num_items
    }

    #[test]
    fn identical_items_same_cluster(item: String, copies: usize) {
        // N copies of same string → 1 cluster
    }
}
```

### 3. Conversion Roundtrips

```rust
proptest! {
    #[test]
    fn track_to_mention_roundtrip(track: Track) {
        // Track → EntityMention → (equivalent Track info)
    }

    #[test]
    fn identity_to_cluster_roundtrip(identity: Identity) {
        // Identity → CrossDocCluster → (equivalent Identity info)
    }
}
```

### 4. Similarity Function Properties

```rust
proptest! {
    #[test]
    fn string_similarity_symmetric(a: String, b: String) {
        // sim(a, b) == sim(b, a)
    }

    #[test]
    fn string_similarity_reflexive(a: String) {
        // sim(a, a) == 1.0
    }

    #[test]
    fn string_similarity_bounded(a: String, b: String) {
        // 0.0 <= sim(a, b) <= 1.0
    }
}
```

---

## Evaluation Against Datasets

### What's Available

From `datasets.toml` and `loader.rs`:

| Dataset | Task | Has Gold Clusters? |
|---------|------|-------------------|
| ECBPlus | Cross-doc event coref | Yes |
| GVC | Gun violence coref | Yes |
| OntoNotes | Within-doc coref | Yes (chains) |
| CoNLL2012 | Within-doc coref | Yes (chains) |
| ARRAU | Within-doc coref | Yes |

### What's Missing

No integration test that:
1. Loads a coreference dataset
2. Runs coalesce algorithms on it
3. Evaluates with MUC/B³/CEAF metrics

### Proposed Integration Test

```rust
#[test]
#[ignore] // Requires dataset download
fn test_coalesce_on_ecbplus() {
    // 1. Load ECB+ dataset
    let loader = DatasetLoader::new();
    let docs = loader.load_coref(DatasetId::ECBPlus)?;

    // 2. Convert to Corpus with Tracks
    let corpus = docs_to_corpus(docs);

    // 3. Run coalesce
    let resolver = Resolver::new().with_threshold(0.7);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // 4. Convert to evaluation format
    let predicted = identities_to_clusters(&corpus, &identity_ids);
    let gold = docs_to_gold_clusters(docs);

    // 5. Evaluate
    let eval = CorefEvaluation::compute(&predicted, &gold);
    assert!(eval.conll_f1 > 0.5); // Sanity check
}
```

---

## Recommendations

### Short-term (Minimal Disruption)

1. Add conversion functions between `coalesce` and `anno-core` types
2. Add property tests for similarity functions (already partially done)
3. Create integration test for coalesce → eval pipeline

### Medium-term (Refactor)

1. Remove `EntityMention`/`EntityCluster` from `streaming.rs`
2. Have `StreamingResolver` operate on `Track` / produce `Identity`
3. Unify `CrossDocCluster` and `Identity` (use `Identity` in eval)

### Long-term (Design)

1. Consider whether `coalesce` should be merged into `anno`
2. The crate boundary makes sense for independent use, but the type duplication is a maintenance burden
3. Alternative: Keep separate crates but have `coalesce` depend on `anno-core` for all types

---

## What Really Matters

**The `Signal → Track → Identity` hierarchy is correct and valuable.** The problem is that we haven't consistently used it across crates.

The algorithms in `coalesce` are solid:
- Union-Find batch resolution
- LSH blocking
- Streaming with Doubling Algorithm
- Correlation clustering
- Hierarchical clustering

The gap is in the **integration layer**: connecting these algorithms to `anno-core` types and the evaluation framework.

---

## Current Test Status (Updated)

```
anno-coalesce: 115 tests pass
├── Unit tests:              68 (module-level tests)
├── Property tests:          25 (integration_proptests.rs)
├── Eval integration tests:   9 (eval_integration.rs)
├── Doc tests:               10 (2 ignored)
└── Fuzz-like tests:         12 (adversarial inputs)
```

### Test Distribution by Module

| Module | Unit Tests | Property Tests | Doc Tests |
|--------|------------|----------------|-----------|
| `resolver` | 17 | 5 | 3 |
| `correlation` | 6 | 4 | 1 |
| `hierarchical` | 9 | 5 | 1 |
| `lsh` | 6 | 5 | 1 |
| `streaming` | 8 | 7 | 1 (ignored) |

### Test Coverage Focus

1. **Adversarial inputs**: Empty strings, very long strings, Unicode (emojis, CJK), mixed scripts
2. **Edge cases**: Zero similarity matrices, identical items, rapid streaming
3. **Conversion roundtrips**: Track ↔ EntityMention, Identity ↔ CrossDocCluster
4. **Invariants**: Symmetry, reflexivity, bounded values, partition completeness

### Conversions Implemented

| From | To | Location |
|------|-----|----------|
| `Track` | `EntityMention` | `coalesce/src/streaming.rs` |
| `EntityCluster` | `Identity` | `coalesce/src/streaming.rs` |
| `Identity` | `CrossDocCluster` | `anno/src/eval/cdcr.rs` |
| `CrossDocCluster` | `Identity` | `anno/src/eval/cdcr.rs` |
| `MentionCluster` | `Track` + `Vec<Signal>` | `anno/src/backends/mention_ranking.rs` |
| `RankedMention` | `Signal<Location>` | `anno/src/backends/mention_ranking.rs` |

### End-to-End Tests

- `test_e2e_cross_document_coreference`: Multi-doc NER → streaming → identity validation
- `test_e2e_hierarchical_entity_clustering`: Similarity matrix → HAC → cluster cut
- `test_streaming_vs_batch_consistency`: Verify streaming/batch produce comparable results

### Mention Ranking → Grounded Integration (NEW)

The `MentionRankingCoref` resolver now integrates with the canonical `Signal → Track → Identity` hierarchy:

```rust
use anno::backends::mention_ranking::MentionRankingCoref;
use anno_core::GroundedDocument;

let coref = MentionRankingCoref::new();
let text = "John saw Mary. He waved to her.";

// Option 1: Get signals and tracks separately
let (signals, tracks) = coref.resolve_to_grounded(text)?;

// Option 2: Add directly to a GroundedDocument
let mut doc = GroundedDocument::new("doc1", text);
let track_ids = coref.resolve_into_document(text, &mut doc)?;
```

This closes the gap identified in the crate boundary analysis - mention ranking output now correctly flows into the canonical type hierarchy.

