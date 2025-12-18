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
| design/TESTING_GAPS_ANALYSIS.md | Current gaps. Update as fixed. |
| design/CRATE_BOUNDARY_ANALYSIS.md | Type duplication analysis. Still relevant. |
| RESEARCH_SYNTHESIS.md | Research integration priorities. Active. |
| PIPELINE.md | End-to-end guide. |
| design/CLI_DESIGN.md | CLI architecture. |

## Reference

Historical context, theory, or external research. Not expected to match code.

| File | Notes |
|------|-------|
| research/HISTORICAL_SYSTEMS.md | Pre-transformer systems catalog. |
| RESEARCH.md | What's novel vs implementation. |
| design/clustering/CLUSTERING_FOUNDATIONS.md | Math background for coalesce. |
| research/DYNAMIC_SEMANTICS_THEORY.md | Theory. No code. |
| research/TYPE_THEORY_AND_NER.md | Curry-Howard perspective. Aspirational. |
| research/CENTERING_THEORY_GUIDE.md | Centering implementation. |
| research/PROBABILISTIC_COREF_RESEARCH.md | Research notes. |
| MATH_DOCUMENTATION_GUIDE.md | Writing guidelines. |

## Stale / Needs Review

May describe features not implemented or plans that changed.

| File | Issue |
|------|-------|
| research/HYPERGRAPH_EVIDENCE_DESIGN.md | Not implemented. Research note. |
| research/XCORE_INTEGRATION.md | xCoRe not integrated. Research note. |
| design/FEATURE_CACHE_DESIGN.md | Status unclear. |
| design/embeddings/BOX_COREF_INTEGRATION.md | box-coref is external project. |
| design/embeddings/BOX_EMBEDDINGS*.md | Research code. Unclear if used. |
| design/joint/JOINT_MODEL_DESIGN.md | Large design doc. Verify implementation status. |
| research/GRAPH_STRUCTURE_UNIFICATION.md | Unifying framework. Mostly conceptual. |

## Auto-Generated

| File | Source |
|------|--------|
| generated/DATASETS_GENERATED.md | dataset_registry.rs |
| generated/dataset_catalog.html | scripts |
| generated/datasets.csv | scripts |

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
