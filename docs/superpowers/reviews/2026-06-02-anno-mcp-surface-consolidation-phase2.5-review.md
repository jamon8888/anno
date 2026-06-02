# Phase 2.5 Plan Review — MCP Surface Consolidation

**Date:** 2026-06-02
**Verdict:** Validated with amendments. Do not execute the old
`origin/codex/knowledge-plans-phase25-phase3` branch directly.

## Current Repo / Fork State

- `jamon8888/anno` remains the source repository, but GitHub Actions there are
  blocked before job startup by billing/spending-limit failures.
- `candy-hacienda/anno` is the build fork and is writable by the current user.
  Use the local remote name `build-fork`.
- Before this review, `build-fork/main` was at `33184a7b` and local `main`
  was ahead with `0978454e` (`fix: harden knowledge source privacy and forget`).

## Validated

- The Phase 2.5 goal is sound: 5 verb tools (`index`, `search`, `sources`,
  `status`, `forget`) make Phase 3 semantic search easier to expose.
- Existing tool prerequisites are present in `crates/anno-rag-mcp/src/lib.rs`:
  `search`, `legal_ingest`, `legal_search`, `knowledge_*`, and `vault_stats`.
- GitNexus impact checks on `AnnoRagServer`, `Store`, and `Pipeline` are LOW.
- `Pipeline::embedder_loaded()` and `Pipeline::detector_loaded()` already exist,
  so status model telemetry does not require new model-loading logic.

## Required Amendments

- `sources()` must not expose raw local paths. Return pseudonymous ids/labels
  for both knowledge and legal sources.
- Legal deletion must operate by `folder_path`, not `source_path`.
  `Store::delete_doc_rows()` is not suitable for deleting a legal corpus.
- `forget(path)` for knowledge must use an exact provider-key lookup helper,
  not label matching.
- Reusing the `search` name is an intentional surface takeover. Preserve legacy
  requests that carry `rerank`; document migration to `legacy_search` for old
  calls without `rerank`.
- Implementation must start from current `main`, not the stale plans branch.

## Execution Recommendation

1. Push updated `main` to `build-fork/main`.
2. Create a fresh implementation branch from that `main`.
3. Execute the amended Phase 2.5 plan with targeted tests only; avoid local
   workspace-wide Rust builds.
4. Use Actions on `candy-hacienda/anno` as the CI signal until the original
   repository billing issue is resolved.
