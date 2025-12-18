# Documentation Status

Last reviewed: 2025-12-18

## Legend

- **ACTIVE**: Current, matches code
- **STALE**: Describes code that changed or plans that didn't happen
- **REFERENCE**: Background info, not tied to specific code
- **AUTO**: Generated, don't edit

## Active

| File | Notes |
|------|-------|
| SCOPE.md | Canonical. What's in/out of scope. |
| BACKENDS.md | Backend selection guide. Keep updated. |
| EVALUATION.md | Eval framework overview. |
| UNICODE_OFFSETS.md | Character offset rationale. |
| TESTING.md | Test strategy. |
| TESTING_GAPS_ANALYSIS.md | Current gaps. Update as fixed. |
| CRATE_BOUNDARY_ANALYSIS.md | Type duplication analysis. Still relevant. |
| RESEARCH_SYNTHESIS.md | Research integration priorities. Active. |
| PIPELINE.md | End-to-end guide. |
| CLI_DESIGN.md | CLI architecture. |

## Reference

Historical context, theory, or external research. Not expected to match code.

| File | Notes |
|------|-------|
| HISTORICAL_SYSTEMS.md | Pre-transformer systems catalog. |
| RESEARCH.md | What's novel vs implementation. |
| CLUSTERING_FOUNDATIONS.md | Math background for coalesce. |
| DYNAMIC_SEMANTICS_THEORY.md | Theory. No code. |
| TYPE_THEORY_AND_NER.md | Curry-Howard perspective. Aspirational. |
| CENTERING_THEORY_GUIDE.md | Centering implementation. |
| PROBABILISTIC_COREF_RESEARCH.md | Research notes. |
| MATH_DOCUMENTATION_GUIDE.md | Writing guidelines. |

## Stale / Needs Review

May describe features not implemented or plans that changed.

| File | Issue |
|------|-------|
| HYPERGRAPH_EVIDENCE_DESIGN.md | Not implemented. Archive or implement. |
| XCORE_INTEGRATION.md | xCoRe not integrated. |
| FEATURE_CACHE_DESIGN.md | Status unclear. |
| BOX_COREF_INTEGRATION.md | box-coref is external project. |
| BOX_EMBEDDINGS*.md | Research code. Unclear if used. |
| JOINT_MODEL_DESIGN.md | Large design doc. Verify implementation status. |
| GRAPH_STRUCTURE_UNIFICATION.md | Unifying framework. Check if done. |

## Auto-Generated

| File | Source |
|------|--------|
| DATASETS_GENERATED.md | dataset_registry.rs |
| dataset_catalog.html | scripts |
| datasets.csv | scripts |

## Duplicative / Consider Merging

| Files | Issue |
|-------|-------|
| DATASET_*.md (7 files) | Too many. Consolidate into DATASETS.md + DATASETS_GENERATED.md |
| CLI_*.md (3 files) | Merge into CLI_DESIGN.md |
| BOX_EMBEDDINGS*.md (4 files) | Consolidate or move to separate repo |
| GRAPH_*.md (4 files) | Merge into one |

## Missing

Should exist but doesn't:

- QUICKSTART.md — 5-minute getting started
- CHANGELOG.md — Version history (exists at root)
- CONTRIBUTING.md — How to contribute
- MIGRATION.md — Upgrading between versions

## Recommendation

1. Archive files marked STALE to `docs/archive/`
2. Merge duplicative files
3. Add status headers to remaining docs
4. Create QUICKSTART.md
