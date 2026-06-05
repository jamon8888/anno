# MCP Folder Auto-Sync And Legal Output Isolation Design

## Summary

Anno MCP must let a user index a client folder once, come back later after adding documents, and get targeted search results for the selected corpus without accidentally indexing generated artifacts. The current knowledge path already behaves like an incremental rescan when `knowledge_sync` is called explicitly: local files are discovered again and unchanged revisions are skipped by provider version. The current legal path is less safe: `legal_ingest_impl` writes generated anonymized files to `folder/anon`, while `index(profile="legal" | "all")` ingests the source folder recursively. A later recursive run can therefore see Anno's own output as input.

This design keeps client folders as pure source roots, moves generated legal outputs out of the source tree by default, adds defensive generated-file exclusions, and introduces bounded corpus sync behavior that can refresh newly added documents on the next MCP use without requiring a permanent filesystem watcher.

## Goals

- Prevent `legal` and `profile="all"` from re-ingesting `anon/` outputs or `.anon.*` chains.
- Keep indexed client folders isolated by `corpus_id`.
- Allow documents added after the first index to be picked up by a later MCP interaction.
- Keep sync incremental and bounded so normal search does not become an unbounded full re-index.
- Make freshness visible in MCP responses.
- Provide an explicit `sync_corpus(corpus_id, ...)` tool for deterministic catch-up.
- Avoid hidden background work that consumes CPU or disk without a user-visible state.

## Non-Goals

- No permanent filesystem watcher in the first implementation.
- No automatic deletion of files that already exist in a user's `anon/` folder.
- No UI work in Claude Desktop.
- No cross-corpus auto-selection based only on Claude session state.
- No attempt to make legal semantic ingestion model-free.
- No broad rewrite of the existing knowledge or legal storage engines.

## Current Behavior

### Knowledge

`index(profile="general" | "all")` registers a local folder as a knowledge source, then calls `knowledge_sync`. `sync_local_scope` discovers files for that local folder each time it runs. For each discovered file, it computes an object id and provider version. If the same revision is already FTS-ready, the file is counted as `skipped_unchanged`; otherwise it is extracted, pseudonymized, and committed.

This means the knowledge side is already suitable for incremental catch-up, but only when a sync tool is called. There is no current watcher or automatic sync trigger.

### Legal

`legal_ingest_impl` creates its output directory as `folder.join("anon")`. `index(profile="legal" | "all")` calls legal ingest with `recursive: true`. On a real client folder, this can cause generated files under `anon/` to participate in later recursive scans, especially after repeated smoke tests or repeated `profile="all"` runs.

The observed consequences are:

- `.anon.anon.*` output chains can appear.
- Generated pseudonymized content can be reprocessed as source material.
- Indexing time grows with artifacts created by previous runs.
- Search quality can degrade because generated outputs look like first-class client documents.

## Architecture Decision

Adopt two invariants:

1. A client folder is a source root only.
2. Generated artifacts are output resources owned by Anno and addressed through `corpus_id`, not recursive inputs under the client root.

The recommended implementation is:

- Move default legal outputs to an internal data directory such as:

```text
<anno_data_dir>/corpora/<corpus_id>/outputs/legal-anon/
```

- Keep raw client paths internal. MCP responses expose `corpus_id`, pseudonymous labels, sync state, and export handles, not absolute output paths unless a user explicitly asks for an export destination.
- Add defensive generated-file exclusions to all local recursive discovery paths, including legal ingest and knowledge local-folder discovery.
- Add an explicit export operation for users who need generated anonymized files in a chosen location.

## Generated File Exclusion Policy

The source discovery layer should skip generated Anno artifacts before extraction:

- Directories named `anon`, `outputs`, `.anno`, `.anno-rag`, `.git`, `node_modules`, `target`.
- Files matching `.anon.*` or `*.anon.*`.
- Files carrying a future Anno-generated metadata marker, when available.
- The configured internal output root, even if a user points `index(path=...)` near it.

This exclusion must be applied in shared discovery code where possible. If knowledge and legal discovery currently use separate walkers, both paths need tests.

The exclusion should be reported in sync summaries:

```json
{
  "seen": 21,
  "skipped_unchanged": 20,
  "skipped_generated": 3,
  "extracted": 1,
  "failed": 0,
  "truncated": false
}
```

## Corpus Sync Model

Add an explicit corpus sync API:

```text
sync_corpus(corpus_id, profile?, max_files?, max_millis?, include_legal?)
```

Default behavior:

- `profile` defaults to the corpus profile registered by `index`.
- `include_legal` defaults to `false` for opportunistic sync and `true` for explicit user-requested full sync when legal was part of the profile.
- `max_files` and `max_millis` apply per call.
- The response reports knowledge and legal summaries separately.

Example response:

```json
{
  "ok": true,
  "corpus_id": "...",
  "freshness": "fresh",
  "knowledge": {
    "seen": 22,
    "skipped_unchanged": 21,
    "extracted": 1,
    "failed": 0,
    "truncated": false
  },
  "legal": {
    "ran": false,
    "reason": "opportunistic sync excludes legal by default"
  }
}
```

## Opportunistic Auto-Sync

Do not add a permanent watcher in v1. Instead, run a lightweight corpus freshness check before user-facing corpus operations:

- `search`
- `corpus_health`
- `sources`
- future selected-folder tools

If the selected corpus appears stale, run a bounded sync only when it will not surprise the user with heavy model startup.

Recommended default:

- For `search(scope="knowledge")` or mixed `scope="all"`, attempt a small knowledge sync only if models and vault dependencies are already ready or the sync can be performed within a configured budget.
- For `search(scope="legal")`, do not silently perform full legal ingest before search. Return freshness metadata and let the caller invoke `sync_corpus(..., include_legal=true)` or `index(profile="legal" | "all")`.
- If freshness cannot be resolved cheaply, search the existing index and return `index_fresh=false`.

Freshness metadata should be returned alongside search results:

```json
{
  "ok": true,
  "corpus_id": "...",
  "index_fresh": false,
  "sync": {
    "attempted": true,
    "truncated": true,
    "reason": "time_budget_exceeded"
  },
  "hits": []
}
```

This preserves usability: Claude Desktop can answer from the current index, but it can also see that a sync is needed.

## Staleness Detection

Use a cheap, conservative signal first:

- Track `last_sync_started_at`, `last_sync_finished_at`, `last_seen_file_count`, and `last_seen_root_mtime` per corpus/source.
- If the source root mtime or file count changes, mark the corpus as `maybe_stale`.
- A `maybe_stale` corpus becomes `fresh` only after a sync completes without truncation and without failed files.

The signal does not need to prove exact freshness. It only decides whether to attempt a bounded rescan or report that the index may be stale. The actual sync remains content-hash based and idempotent.

## Legal Output API

Replace implicit `folder/anon` output with explicit output ownership:

- Legal ingest stores generated files internally under the corpus output root.
- MCP returns output counts and opaque output ids.
- Add or extend a tool:

```text
export_anonymized(corpus_id, destination, overwrite?)
```

Export rules:

- Destination must be outside the indexed source root by default.
- If destination is inside the source root, require an explicit override and continue to exclude it from future indexing.
- Export should never make generated files part of the corpus source set.

## Error Handling

- If generated output isolation fails, `legal_ingest` returns `ok=false`; it must not fall back to writing into the source root.
- If opportunistic sync fails per file, return search results from the previous index plus `index_fresh=false` and sync errors.
- If multiple corpora exist and no `corpus_id` is provided, keep the existing corpus guard behavior: refuse sensitive unscoped operations unless `allow_cross_corpus=true`.
- If sync is truncated, do not mark the corpus fresh.

## Test Plan

Add focused tests before implementation:

- `legal_ingest` writes outputs outside the source root when called with a corpus id.
- Recursive legal ingest ignores an existing `source/anon/generated.anon.md`.
- `index(profile="all")` run twice on the same folder does not increase source document counts due to generated outputs.
- Knowledge sync indexes a file added after first index and skips unchanged files.
- Opportunistic search returns `index_fresh=false` when a corpus is stale but sync is skipped or truncated.
- `sync_corpus` catches up a newly added file and returns `index_fresh=true` only after a complete run.
- Exporting anonymized files into a user destination does not make those files searchable as source documents.

Add one MCP smoke fixture containing:

- normal text/markdown files;
- an `anon/` folder with generated-looking files;
- a file added after first index;
- two corpora to verify search does not cross corpus boundaries.

## Rollout

1. Add generated-file exclusion tests and exclusion helpers.
2. Move legal output root for corpus-scoped ingest.
3. Add `sync_corpus` with budgeted knowledge sync first.
4. Add freshness metadata to corpus/search responses.
5. Add optional bounded opportunistic sync before selected corpus operations.
6. Add legal sync support to `sync_corpus` with explicit `include_legal=true`.
7. Add `export_anonymized`.

This order fixes the `anon/` feedback loop before making any automatic sync behavior more active.

## Acceptance Criteria

- Re-running `index(profile="all")` on the same folder does not ingest Anno-generated files.
- Adding one document after first index and running `sync_corpus` indexes only the new or changed document.
- Search responses disclose whether the selected corpus index is fresh.
- Automatic sync never performs an unbounded recursive re-index.
- Legal generated outputs are not written into the client source root by default.
- Multi-corpus search remains constrained by `corpus_id`.
