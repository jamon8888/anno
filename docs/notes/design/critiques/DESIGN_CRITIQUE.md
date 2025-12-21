# Anno Design Critique: Comparison with Industry Systems

A critical analysis of `anno`'s architecture compared to state-of-the-art NER and entity resolution systems.

## Executive Summary

`anno` has a well-designed hierarchical abstraction (Signal → Track → Identity) that aligns with research best practices. However, there are several areas where the design could be improved based on patterns from production systems like SpaCy, Splink, and Dedupe.

---

## Strengths

### 1. Clean Hierarchical Abstraction

The Signal → Track → Identity hierarchy maps cleanly to the research literature:

| anno | Vision (MOT) | NLP Standard | Research Term |
|------|--------------|--------------|---------------|
| Signal | Detection | Mention | Span extraction |
| Track | Tracklet | CorefChain | Within-doc clustering |
| Identity | Re-ID | Entity | Cross-doc + KB linking |

**Verdict**: This is a strong design choice, aligning with Maverick (ACL 2024) and SpanBERT patterns.

### 2. Multimodal Location Support

The `Location` enum supports:
- Text spans
- Bounding boxes (visual)
- Temporal intervals (audio/video)
- 3D cuboids (LiDAR)
- Genomic intervals
- Discontinuous spans

**Verdict**: This is more comprehensive than most NER libraries (SpaCy, Flair, Stanza all assume text-only).

### 3. Provenance Tracking

Full pipeline lineage: source → ingest → preprocess → extract → mentions.

**Comparison**:
- Dedupe: No built-in provenance
- Splink: Minimal lineage tracking
- SpaCy: Component-level metadata only

**Verdict**: `anno`'s provenance is production-grade and exceeds most alternatives.

### 4. Backend Modularity

The trait-based backend system (`Model` trait) enables:
- Zero-dependency baselines (Regex, Heuristic)
- ML backends (BERT, GLiNER, Candle)
- Ensemble/stacked composition

**Comparison**: Similar to SpaCy's pipeline component model, but with cleaner feature gating.

---

## Weaknesses and Suggested Improvements

### 1. Type Duplication Across Crates

**Problem**: `coalesce` defines `EntityMention`/`EntityCluster` separately from `anno-core`'s `Track`/`Identity`, leading to:
- Maintenance burden
- Conversion overhead
- Semantic drift risk

**What others do**:
- Splink: Single `RecordPair` / `Cluster` type throughout
- Dedupe: Unified `Record` → `Cluster` flow
- SpaCy: `Doc` → `Span` → `Entity` with no duplicates

**Recommendation**: Merge `coalesce`'s types into `anno-core` or make `coalesce` depend on `anno-core` exclusively.

### 2. Missing Active Learning / Human-in-the-Loop

**Problem**: No built-in mechanism for:
- Uncertain pair labeling
- Annotation refinement
- Model retraining triggers

**What others do**:
- Dedupe: Core feature - active learning with `markPairs()`
- Splink: `linker.label_clusters_as_match_or_non_match()`
- SpaCy: Prodigy integration for annotation loops

**Recommendation**: Add `Resolver::request_human_labels(uncertain_pairs: &[(TrackRef, TrackRef)])` interface.

### 3. No Blocking Strategy API

**Problem**: Cross-document coreference does O(n²) comparisons. For large corpora, this is prohibitive.

**What others do**:
- Dedupe: Configurable blocking predicates (e.g., "first 3 chars of name")
- Splink: `blocking_rules` with SQL generation
- Zingg: ML-learned blocking keys

**Current anno approach**: LSH module exists but isn't integrated into the resolver pipeline.

**Recommendation**: 
```rust
pub trait BlockingStrategy {
    fn generate_blocking_keys(&self, track: &Track) -> Vec<String>;
}
```

### 4. Coreference Evaluation Metrics Incomplete

**Problem**: `anno/src/eval/coref.rs` implements basic metrics but lacks:
- LEA (Link-based Entity-Aware) - important for entity-weighted evaluation
- Singleton handling configuration
- Genre-specific evaluation

**What the field uses** (2024-2025):
- CoNLL score = avg(MUC, B³, CEAFφ4)
- LEA for entity-centric analysis (CorefUD shared tasks)
- Anaphor-specific breakdown

**Recommendation**: Implement LEA and add evaluation config for singleton handling.

### 5. No Span Pruning / Coarse-to-Fine

**Problem**: Current coreference doesn't implement the coarse-to-fine pattern that SOTA systems use to handle long documents efficiently.

**What SOTA does** (Maverick 2024):
1. Score all spans cheaply (MLP)
2. Prune to top-K per document
3. Expensive pairwise scoring only on survivors

**Recommendation**: Add `MentionProposer` trait with configurable pruning threshold.

### 6. Testing Gaps

**Problem**: Limited property-based and robustness testing compared to production systems.

**What others test**:
- **SpaCy**: Behavioral tests (invariance to irrelevant changes)
- **Splink**: Statistical consistency tests (v3 vs v4 identical results)
- **Flair**: Embedding ablation tests

**Missing in anno**:
- Metamorphic tests (paraphrase invariance)
- Cross-version regression tests
- Throughput/latency benchmarks
- OOM stress tests for long documents

---

## Feature Comparison Matrix

| Feature | anno | SpaCy | Dedupe | Splink | Stanza |
|---------|------|-------|--------|--------|--------|
| Zero-shot NER | ✅ GLiNER | ❌ | N/A | N/A | ❌ |
| Nested NER | ✅ W2NER | ⚠️ Experimental | N/A | N/A | ❌ |
| Cross-doc coref | ✅ coalesce | ❌ | ✅ | ✅ | ❌ |
| Active learning | ❌ | ⚠️ Prodigy | ✅ | ✅ | ❌ |
| Blocking | ⚠️ LSH only | N/A | ✅ | ✅ | N/A |
| Provenance | ✅ | ⚠️ | ❌ | ❌ | ❌ |
| Multimodal | ✅ | ⚠️ | ❌ | ❌ | ❌ |
| Config-driven | ⚠️ | ✅ | ⚠️ | ✅ | ❌ |
| Rule+ML hybrid | ✅ | ✅ | ❌ | ⚠️ | ❌ |

---

## Actionable Recommendations

### High Priority

1. **Unify types**: Eliminate `EntityMention`/`EntityCluster` duplication
2. **Add LEA metric**: Critical for modern coref evaluation
3. **Blocking API**: Make LSH/blocking first-class in resolver
4. **More tests**: Property tests, robustness tests, benchmarks

### Medium Priority

5. **Active learning hooks**: Enable human-in-the-loop refinement
6. **Coarse-to-fine**: Span pruning for efficiency

---

## Design Decision: Code-First Configuration

### The Problem

`anno` previously maintained BOTH:
1. `enum DatasetId` in Rust (hardcoded variants with ~50 methods)
2. `datasets.toml` with `rust_variant` field mapping back to enum

This is the **worst of both worlds**:
- Manual sync required between two files
- Changes need to be made in two places
- Sync bugs possible despite tests

### The Decision

**Code is primary. TOML is derived.**

```
┌─────────────────────────────────────────────────┐
│  dataset_registry.rs (PRIMARY)                  │
│  - define_datasets! macro                       │
│  - All metadata in code                         │
│  - Compile-time validation                      │
│  - IDE support (autocomplete, go-to-def)        │
└─────────────────────────────────────────────────┘
              │ cargo test --ignored
              ▼
┌─────────────────────────────────────────────────┐
│  datasets_generated.toml (DERIVED)              │
│  - Auto-generated, read-only                    │
│  - For external tooling/documentation           │
│  - NOT edited by hand                           │
└─────────────────────────────────────────────────┘
```

### Rationale

| Factor | Code-First | TOML-First |
|--------|------------|------------|
| Type safety | ✅ Compile-time | ❌ Runtime parsing |
| IDE support | ✅ Full autocomplete | ⚠️ Limited |
| Single source of truth | ✅ One file | ❌ Two files |
| External scripts | ⚠️ Needs generation | ✅ Native |
| Non-programmer editing | ❌ Need Rust | ✅ Text file |

**Since `anno` is a Rust library, not a CLI tool**, the benefits of code-first outweigh TOML-first:

1. **Users are Rust developers** - they can read/modify code
2. **Typos caught at compile time** - no runtime "unknown dataset" errors
3. **Exhaustive pattern matching** - compiler enforces handling all cases
4. **Single maintenance point** - change once, done

### Counter-Arguments Considered

**"But SpaCy uses config files!"**

SpaCy's `config.cfg` is for:
- Training configuration (hyperparameters)
- Pipeline composition (which components)
- NOT dataset enumeration

SpaCy's datasets are still defined in code.

**"What about non-programmers?"**

`anno` is a library, not an end-user application. Non-programmers won't be adding datasets - they'll use the CLI or Python bindings (future).

### Implementation

See `anno/src/eval/dataset_registry.rs`:

```rust
define_datasets! {
    WikiGold {
        name: "WikiGold",
        description: "Wikipedia-based NER...",
        url: "https://...",
        entity_types: ["PER", "LOC", "ORG", "MISC"],
        language: "en",
        domain: "wikipedia",
        categories: [ner],
    },
    // ... more datasets
}
```

The macro generates:
- `enum DatasetId { WikiGold, ... }`
- `impl DatasetId { fn name(), fn url(), fn is_ner(), ... }`
- `DatasetId::all_ner()`, `DatasetId::quick()`, etc.

To regenerate TOML for documentation:
```bash
cargo test -p anno --features eval generate_datasets_toml -- --ignored
```

### Low Priority (Future)

8. **Streaming coreference**: True online processing (partially done)
9. **Explainability**: Why did two mentions cluster together?
10. **Multi-task learning**: Joint NER+RE like GLiNER-multitask

---

## Conclusion

`anno` has a solid architectural foundation that compares favorably to academic NER systems. The Signal → Track → Identity hierarchy is well-motivated by research. The main gaps are in:

1. **Engineering polish**: Type unification, blocking API
2. **Evaluation completeness**: LEA, genre-specific metrics
3. **Production readiness**: Active learning, explainability
4. **Testing rigor**: More property tests, robustness checks

The codebase is well-positioned to address these gaps incrementally without major architectural changes.

