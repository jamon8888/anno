# Testing Gaps Analysis

Generated from matrix testing experience. Last updated: December 2024.

## Session Summary

### Tests Added
- `env_integration.rs` - Environment variable and .env loading
- `backend_integration.rs` - Backend factory and builtin backends
- `dataset_loading.rs` - Dataset loader functionality
- `w2ner_integration.rs` - W2NER discontinuous NER backend

### Matrix Testing Improvements
- Added DiscontinuousNER task to matrix
- Added biomedical datasets (BC5CDR, NCBIDisease, BC2GM, BC4CHEMD)
- Added MultiNERD for multilingual coverage
- Added sampling strategies (Random, MlOnly, WorstFirst, MlAll)

### Infrastructure
- Created `anno/src/env.rs` for centralized .env loading
- Added S3 fallback for GLiNER-Candle safetensors
- Created `scripts/upload_safetensors_s3.sh` for model caching

## Backend Implementation Status

### Production Ready (10)

| Backend | Feature | Notes |
|---------|---------|-------|
| `pattern` / `RegexNER` | - | Fast pattern-based extraction |
| `heuristic` / `HeuristicNER` | - | Rule-based fallback |
| `crf` / `CrfNER` | - | Conditional random field |
| `stacked` / `StackedNER` | - | Multi-backend stacking |
| `ensemble` / `EnsembleNER` | - | Voting ensemble |
| `bert_onnx` / `BertNEROnnx` | `onnx` | BERT-based NER |
| `gliner_onnx` / `GLiNEROnnx` | `onnx` | Zero-shot NER |
| `nuner` / `NuNER` | `onnx` | Zero-shot NER |
| `gliner2` / `GLiNER2Onnx` | `onnx` | Multi-task extraction |
| `candle_ner` / `CandleNER` | `candle` | Pure Rust BERT |

### Working with Caveats (4)

| Backend | Feature | Issue |
|---------|---------|-------|
| `hmm` / `HmmNER` | - | Needs trained weights |
| `bilstm_crf` / `BiLstmCrfNER` | - | Falls back to heuristic |
| `w2ner` / `W2NER` | `onnx` | Requires HF auth |
| `gliner_candle` / `GLiNERCandle` | `candle` | Needs safetensors (S3 fallback added) |

### Placeholder Only (5)

**These backends exist but are NOT functional for production use:**

| Backend | Feature | Reality |
|---------|---------|---------|
| `burn` / `BurnPoweredNER` | `burn` | **Placeholder** - wraps HeuristicNER |
| `tplinker` / `TPLinker` | - | **Placeholder** - uses heuristics, no ONNX model |
| `gliner_poly` / `GLiNERPoly` | `onnx` | **Placeholder** - wraps bi-encoder, no fusion |
| `deberta_v3` / `DeBERTaV3NER` | `onnx` | **Placeholder** - model not integrated |
| `albert` / `ALBERTNER` | `onnx` | **Placeholder** - model not integrated |

### Meta/Wrapper Backends (4)

| Backend | Purpose |
|---------|---------|
| `router` / `AutoNER` | Selects best backend per entity type |
| `extractor` / `NERExtractor` | Streaming extraction wrapper |
| `universal_ner` / `UniversalNER` | LLM-based (requires API key) |
| `llm` / `LlmNER` | Direct LLM prompting (requires API key) |

### Missing from Backend Factory

These backends exist but aren't exposed via `BackendFactory::create()`:
- `hmm` 
- `bilstm_crf`
- `rule` (RuleBasedNER)
- `llm` (LlmNER)
- `router` (AutoNER)
- `extractor` (NERExtractor)
- `tplinker`

### Not Implemented

These backends are referenced but are placeholders:
- Actual Burn model (not just wrapper)
- TPLinker (relation extraction)
- DeBERTa-v3 NER
- ALBERT NER

## Datasets Summary

### 156 DatasetId Variants

**Well-tested in matrix:**
- WikiGold
- Wnut17
- MitMovie
- MitRestaurant
- CoNLL2003 (sample)
- OntoNotes5 (sample)

**Untested in matrix (but defined):**
- 150+ other datasets
- Biomedical: BC5CDR, NCBIDisease, GENIA, AnatEM, BC2GM, JNLPBA, etc.
- Multilingual: CoNLL2002*, GermEval, HAREM, WikiANN, etc.
- Social media: TweetNER7, BroadTwitterCorpus
- Domain-specific: FabNER, LegalNER, CrossNER

### Datasets Not Cached

Many datasets require download or authentication:
- OntoNotes (LDC license)
- ACE2004/ACE2005 (LDC license)
- Some biomedical datasets

## Tasks Summary

### 10 Task Types Defined

| Task | Implemented | Datasets | Backends |
|------|-------------|----------|----------|
| NER | Yes | 100+ | Most |
| NED | Partial | AIDA, TACKBP | None |
| RelationExtraction | Partial | ACE2005 | GLiNER2, TPLinker |
| IntraDocCoref | Yes | OntoNotes | E2ECoref |
| InterDocCoref | Partial | ECBPlus | None |
| AbstractAnaphora | No | - | - |
| DiscontinuousNER | Yes | CADEC, ShARe* | W2NER |
| EventExtraction | No | ACE2005 | None |
| TextClassification | No | - | GLiNER2 |
| HierarchicalExtraction | No | - | GLiNER2 |

### Gaps by Task

1. **NED (Named Entity Disambiguation)** - No working backend
2. **InterDocCoref** - No backend for cross-document coreference
3. **AbstractAnaphora** - No implementation
4. **EventExtraction** - Dataset exists (ACE2005), no backend
5. **TextClassification** - GLiNER2 supports it, no test harness
6. **HierarchicalExtraction** - GLiNER2 supports it, no test harness

## Environment Setup

### Required for Full Coverage

```bash
# .env file
HF_TOKEN=hf_xxxx  # For gated models (w2ner, some GLiNER)
OPENAI_API_KEY=sk-xxxx  # For LLM backend
```

### Features to Enable

```bash
# Full ML support
cargo test --features "eval-advanced,onnx,candle,burn"
```

## Completed Improvements (December 2024)

1. **Centralized .env loading** - Added `anno::env` module for HF_TOKEN
2. **S3 safetensors fallback** - GLiNER-Candle tries S3 before Python conversion
3. **Fixed GLiNER2 cache downcast** - Resolved parallel evaluation issue
4. **Sampling strategies** - Added `random`, `ml-only`, `worst-first`, `ml-all`
5. **Backend integration tests** - New test suite for backend factories
6. **Dataset loading tests** - Tests for DatasetLoader functionality
7. **Documented placeholders** - Clear warnings on non-functional backends

## Remaining Gaps

### High Priority

1. **Add burn model** - Implement actual Burn-based NER (currently placeholder)
2. **Test discontinuous NER** - Add W2NER integration test with HF_TOKEN
3. **Upload safetensors to S3** - Pre-convert and cache for faster CI

### Medium Priority

1. **Add more datasets to matrix** - Include biomedical, multilingual
2. **Test relation extraction** - GLiNER2 has the capability
3. **Test coreference** - E2ECoref exists but not in matrix
4. **Expose missing backends** - Add hmm, bilstm_crf to BackendFactory

### Low Priority

1. **Implement placeholder backends** - TPLinker, DeBERTa, ALBERT
2. **Add event extraction** - Requires new backend
3. **Add NED** - Requires entity linking implementation

## Addressed Gaps (December 2024)

### Research Synthesis Integration

1. **Min-Max Correlation Clustering** - Added 4-approximation variant to coalesce
   - `min_max_clustering()` minimizes worst-case per-cluster disagreements
   - New `MinMaxClusteringResult` with per-cluster metrics
   - Test: `anno-coalesce/tests/research_gaps_integration.rs`

2. **Chromatic Correlation Clustering** - Color-constrained clustering
   - `chromatic_clustering()` for exclusion constraints
   - Test: `test_chromatic_clustering_basic`, `test_chromatic_clustering_three_colors`

3. **Streaming Entity Resolution Tests** - Property tests for streaming resolver
   - Covers arbitrary inputs, type constraints, incremental updates
   - Test: `proptests::streaming_handles_arbitrary_inputs`

4. **Calibration Metrics** - Built into anno's box_embeddings.rs
   - `expected_calibration_error()` - ECE metric
   - `brier_score()` - Proper scoring rule
   - `reliability_diagram()` - Visualization data
   - `CalibrationReport` - Aggregate metrics
   - Tests: `test_ece_*`, `test_brier_*`, `test_calibration_report`

5. **Extended Algorithm Comparison**
   - `compare_algorithms_extended()` includes min-max with metrics
   - Reports per-algorithm max_disagreements where applicable

## Graph Algorithms (strata) - ARCHITECTURE

Graph importance and clustering are computed using **graph algorithms** in `strata`.
The key insight: **nodes can be anything** - entities, documents, sentences, chunks.
The algorithms only see graph structure.

### Available Algorithms

| Algorithm | Question | Complexity |
|-----------|----------|------------|
| `PageRank` | "Which nodes are connected to important nodes?" | O(V × iterations) |
| `Betweenness` | "Which nodes bridge communities?" | O(V × E) |
| `Hits` | "Is this a hub or authority?" | O(V × iterations) |
| `Leiden` | "How do nodes cluster?" | O(V log V) |

### Node Types strata Can Handle

| Node Type | Edge Type | Use Case |
|-----------|-----------|----------|
| Entities | Relations | Entity importance in KGs |
| Documents | Citations | Influential paper detection |
| Sentences | Similarity | Extractive summarization |
| Chunks | Embedding sim | RAPTOR-style RAG |
| Concepts | Ontology | Taxonomy analysis |
| Events | Causal | Pivotal event detection |

### Entity Salience (Legacy)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          GRAPH CENTRALITY ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   PREFERRED: Use actual relations (requires relation extraction)            │
│   ════════════════════════════════════════════════════════════              │
│   NER + RelationExtraction → GraphDocument → strata::PageRank               │
│                                                                              │
│   "Obama" ─[PRESIDENT_OF]→ "USA"     ← semantic edge                        │
│   "Obama" ─[BORN_IN]→ "Hawaii"       ← semantic edge                        │
│   → PageRank reveals structurally important entities                        │
│                                                                              │
│   FALLBACK: Use co-occurrence (when no relations available)                 │
│   ═══════════════════════════════════════════════════════════               │
│   NER only → entities → anno::salience::TextRankSalience                    │
│                                                                              │
│   "Obama" ─[NEAR]→ "USA"  (appeared within 50 chars)  ← weak signal         │
│   → PageRank on proximity is noisy approximation                            │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Implementation Locations

| Module | Algorithm | Input | Use When |
|--------|-----------|-------|----------|
| `anno_strata::PageRank` | PageRank | `GraphDocument` (with relations) | You have relation extraction |
| `anno_strata::Leiden` | Community detection | `GraphDocument` | You need clustering |
| `anno::salience::TextRankSalience` | PageRank on co-occurrence | Entities only | No relations available |
| `anno_coalesce::canonical` | Heuristic scoring | Mentions | Selecting canonical mention |

### Recommended Flow

```rust
// IF you have relations (from GLiNER2, TPLinker, etc.):
use anno_strata::PageRank;
let graph = GraphDocument::from_extraction(&entities, &relations, None);
let scores = PageRank::default().compute(&graph);  // Uses actual semantic edges

// IF you only have entities:
use anno::salience::{EntityRanker, TextRankSalience};
let ranker = TextRankSalience::default();
let ranked = ranker.rank(text, &entities);  // Falls back to co-occurrence
```

### Key Insight: ML Backends Are Multilingual

The co-occurrence fallback (`TextRankSalience`) and keyword extraction (`anno::keywords`)
use statistical heuristics with English stopwords. **For production multilingual use,
prefer:**

1. **GLiNER/Candle NER** - Transformer-based, works on any language
2. **Relation extraction** - Get actual edges, not proximity
3. **`strata::PageRank`** - Run on semantic graph, not co-occurrence

### Tests

- `strata/src/pagerank.rs` - 5 unit tests
- `anno/tests/salience_integration.rs` - 10 tests
- `coalesce/tests/canonical_integration.rs` - 19 tests

