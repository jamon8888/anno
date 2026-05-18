# Design — Memory FTS-Index Auto-Build

**Date**: 2026-05-18
**Status**: Draft for review
**Scope**: `crates/anno-rag` only. Recall-path wiring + tests. No new Store capability.

## 1. Problem

`Pipeline::recall_memory` and `Pipeline::recall_memory_reranked` hard-fail
through the public API with:

```
Store("memories hybrid: lance error: Invalid user input: Cannot perform
full text search unless an INVERTED index has been created on at least
one column")
```

Root cause is a wiring gap: nothing on any public `Pipeline` path ever
creates the memories FTS (INVERTED) index.

- `Pipeline::save_memory` (pipeline.rs:529) calls only
  `store.memory_insert(&m)`.
- `Store::build_memories_fts_index()` (store.rs:639) — a correct
  structural mirror of the chunks `maybe_build_fts_index`: `count == 0`
  guard, idempotent (skips if a `text`-column index exists), locked
  French tokenizer — exists but is never called.
- `Store::optimize_memories()` (store.rs:495) →
  `memories_tbl.optimize(OptimizeAction::All)` exists; it is the
  LanceDB mechanism that folds newly appended rows into an *existing*
  index. It is only driven by `Pipeline::spawn_compaction_task`
  (pipeline.rs:1034) on a **24h** interval (`compaction_interval_secs`,
  default 24h) — and only if that task is spawned by the entrypoint.

Surfaced while executing the cross-encoder rerank plan
(`reranked_memory_recall_returns_topk` is `#[ignore]`'d for this
reason; `recall_memory_reranked` is a faithful wrapper and its
structurally-identical sibling `search_reranked` is fully proven).

## 2. Why eager-in-`save_memory` is the wrong fix

The chunks path is **not** a per-write mirror to copy: the chunk FTS
index is built **once, at the end of `ingest_folder`, after the entire
per-file `ingest_one` loop** (pipeline.rs ~205–225) — i.e. after a bulk
write, on an already-populated table. `save_memory` is inherently
incremental (one memory, no batch boundary). A naive
`build_memories_fts_index()` call after each `save_memory` would build
the index on the first saved row, then the idempotent guard skips
forever, so every later memory is absent from the FTS index. Relying on
the 24h `spawn_compaction_task` to fold them in leaves a multi-hour
window in which a just-saved memory is unrecallable — unacceptable for
session memory.

## 3. Design — make `recall_memory` self-sufficient

In `Pipeline::recall_memory`, before the
`store.memories_hybrid_search(...)` call, ensure the memories search
surface is queryable:

1. `store.build_memories_fts_index().await?` — idempotent; creates the
   FTS index the first time there is ≥1 memory, no-ops thereafter. This
   alone clears the hard error.
2. `store.optimize_memories(min_age).await?` — folds rows appended
   since the last optimize into the existing FTS (and vector) index, so
   memories saved after index creation are recallable immediately, with
   no dependency on the 24h compaction task running.

`recall_memory_reranked` inherits the fix automatically (it delegates to
`recall_memory`). `save_memory` is unchanged. The 24h
`spawn_compaction_task` remains as the background fragment-compaction
path; this change only guarantees correctness on the read path
independent of it.

**Cost control (plan decides the exact gate):** an unconditional
`optimize(All)` on every recall is likely too expensive. The plan
benchmarks it and gates it — e.g. skip when no rows were added since the
last optimize (track a cheap row-count / version watermark on the
Pipeline, or read `memories_tbl` row count vs. a stored last-optimized
count). `build_memories_fts_index` is already cheap-when-built
(count_rows + list_indices), so step 1 is unconditional; step 2 is the
one to gate.

## 4. Open questions the plan must resolve empirically

1. **LanceDB 0.27 FTS semantics on appended, un-optimized rows.** Once
   the index exists, does a query that should match a not-yet-optimized
   row: (a) hard-error, (b) silently miss it, or (c) cover it via
   fragment scan? This determines whether step 3.2's `optimize` is a
   correctness requirement or only a latency optimization. Test: create
   index on N rows, append M more without optimize, query for the M.
2. **Is `spawn_compaction_task` actually spawned** in the CLI
   (`anno-rag-bin`) and MCP (`anno-rag-mcp`) entrypoints? If not,
   `optimize_memories` never runs anywhere except this new recall-path
   call, which strengthens the case for step 3.2 being mandatory.
3. **Recall-path `optimize` cost** on a realistic memory table, to set
   the gating threshold in 3’s cost-control note.

## 5. Testing

- Un-`#[ignore]` `reranked_memory_recall_returns_topk`
  (`crates/anno-rag/tests/rerank_integration.rs`).
- Add a plain-`recall_memory` integration test (save N memories →
  recall returns the relevant ones) so the regression is caught
  independent of the `rerank` feature.
- Staleness test: save memories, recall (forces index build), save
  more, recall again — assert the later memories are found (locks the
  §4.1 behavior whichever way it resolves).

## 6. Out of scope (separate task)

Scalar/vector memory indexes via `Store::setup_memory_indexes()`
(btree `created_at`, btree `session_id`, bitmap `kind`, vector IVF).
These are scale optimizations — memory vector search brute-forces
correctly on small tables, exactly how chunks behave below
`vector_index_threshold`. They fix no failure and warrant their own
threshold tuning, mirroring the chunks `maybe_build_index(threshold)`
split. Also out of scope: auditing whether the incremental-ingest
chunks path has a latent version of the same staleness bug (it is only
ever exercised via batch `ingest_folder`).

## 7. Risk

LOW–MEDIUM. Step 3.1 is a cheap idempotent call that directly removes
the hard error. Step 3.2 adds an `optimize` to the recall path; the
risk is latency, bounded by the cost-control gate and quantified by the
§4.3 benchmark before shipping. No change to `save_memory` or the
default (non-memory) paths. The only real unknown is §4.1, resolved
empirically by the staleness test before merge.
