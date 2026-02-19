# Changelog

## [Unreleased]

## [0.3.0] - 2026-02-19

### Changed
- Workspace refactor: split into `anno-core`, `anno`, `anno-eval`, `anno-cli`, `anno-metrics`, `anno-lattix`
- `PatternNER` → `RegexNER`
- Lib target renamed to `anno` (package name remains `anno-lib`); fixes `use anno::` in doctests and integration tests

### Added
- **GLiNER2** (`backends::gliner2`): Multi-task information extraction — NER, classification, structure extraction, and task composition via `TaskSchema`; ONNX and Candle backends
- **Coreference resolution**: T5-based seq2seq scaffold (`T5Coref`), graph-based iterative refinement (`GraphCoref`), mention-ranking (`MentionRankingCoref`), and rule-based fallback
- **Graph RAG** (`anno-core::graph`): `GraphDocument` with Neo4j Cypher and NetworkX/JSON-LD export; unified fractured-graph resolution via coref
- **Grounded entity representation** (`anno-core::grounded`): three-level Signal → Track → Identity hierarchy; `GroundedDocument` with dual spatial/chain indexes, HTML rendering, eval comparison
- **Task evaluation system** (`anno-eval`): dataset loaders, multi-objective LinUCB routing, regression detection, quality matrix, git-tagged scoring
- **Discourse analysis** (`anno::discourse`): centering theory, uncertain reference (ε-terms), abstract anaphora, shell nouns, event extraction
- `anno-metrics` crate: shared CorefChainStats and cluster evaluation primitives
- `anno-lattix` crate: adapters between `anno-core` and `lattix` graph substrates
- Contextual backend routing via muxer 0.1.2 (LinUCB + objective manifold)

### Notes
- `T5Coref::resolve()` currently uses a rule-based heuristic fallback; full encoder/decoder ONNX loop is scaffolded but not yet wired
- Publishing to crates.io is paused pending resolution of internal path deps; see `docs/PUBLISH_STATUS.md`

## [0.2.0] - 2025-11-27

### Added
- StackedNER: Composable layered extraction
- HeuristicNER: Zero-dependency NER
- Coreference metrics: MUC, B³, CEAF, LEA, BLANC
- NuNER, W2NER, CandleNER, GLiNERCandle
- 887 tests

### Changed
- `GLiNERv2` → `GLiNER`, `GLiNERNER` → `GLiNEROnnx`
- `LayeredNER`/`TieredNER` → `StackedNER`
- Entity uses character offsets consistently

## [0.1.0] - 2025-11-26

Initial release.
