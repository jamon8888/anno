# Fast & Accurate Legal Search — Async Ingestion Jobs + Reranked Hybrid

**Date:** 2026-06-22
**Status:** Design approved, pending spec review
**Scope:** `anno-knowledge-store`, `anno-rag`, `anno-rag-mcp`

## Problem

The anno-rag MCP feature tour surfaced three blocking defects and one accuracy
gap that together make legal search unusable on a real corpus:

1. **Knowledge FTS index never built.** `Store::maybe_build_fts_index()` is only
   called from `Pipeline::optimize_after_ingest()` (the legal pipeline). The
   knowledge sync path (`indexer::sync_local_scope`) writes LanceDB rows but never
   triggers the inverted-index build, so `search(mode=fast)` on knowledge scope
   silently returns zero hits and `legacy_search` throws the raw Lance error
   *"Cannot perform full text search unless an INVERTED index has been created."*

2. **Legal ingestion times out.** `legal_ingest` runs synchronously with no
   per-file budget. The MCP transport enforces ~60s; a real corpus (60+ docs at
   ~6s each for GLiNER + embedder) blows past it. Because the ingest dies before
   `optimize_after_ingest()` runs, the legal hybrid index is never built —
   compounding defect (1) for the legal scope.

3. **`corpus_list` returns only a count.** It calls `corpus_count()` instead of
   `service.list()`, so there is no API path to discover corpus IDs. Today the
   only way to get an ID is to trigger a path-overlap error.

4. **Accuracy gap.** `legal_search` performs hybrid retrieval (vector + BM25 FTS
   + RRF fusion) but stops there. The cross-encoder reranker
   (`bge-reranker-v2-m3`, `Pipeline::search_reranked`) exists and is
   privacy-correct (rehydrates pseudonyms only for the rerank stage) but is
   wired only to the deprecated `search` tool's opt-in `rerank: true` — it is
   unreachable from the legal search path.

## Goals

- **Fast:** Ingestion never blocks the MCP window, regardless of corpus size.
- **Accurate:** Legal semantic search reranks by default with the cross-encoder.
- **Correct:** All search modes (fast / semantic / hybrid) return real results;
  no silent empty sets, no raw index errors.
- **Discoverable:** `corpus_list` returns usable corpus metadata.

## Non-Goals

- Distributed / multi-process job queues. Jobs run in-process via `tokio::spawn`.
- Reworking the SQLite FTS5 path — it is already correctly populated
  (`text_pseudo` → `body_pseudo`); only the LanceDB inverted index is missing.
- Tokenizer experiments (the locked v0.6 French stem config stays).

## Design

### Pillar 1 — Async ingestion jobs (Fast)

**Reuse the dormant `index_jobs` table** (`migrations.rs:132`), currently defined
but never written to.

#### Job lifecycle

```
legal_ingest / index(profile=legal)
        │
        ├─ insert index_jobs row (status=running, job_type=legal_ingest)
        ├─ tokio::spawn(background ingest loop)   ← detached, no MCP timeout
        └─ return { job_id, status: "running" } IMMEDIATELY

background loop (single long-running pass):
        for each file:
            ingest_one_counted(...)
            update index_jobs progress (files_done, files_total)
        drain_enrichment_backlog()
        optimize_after_ingest()      ← builds vector IVF + FTS index ONCE
        update index_jobs status=done

job_status(job_id) → { status, files_done, files_total, last_error }
```

**Decisions locked in design review:**
- **Single long-running job.** The detached background task runs to completion
  regardless of corpus size; no MCP timeout applies. The per-file time budget is
  removed for the async ingest path. (The synchronous `knowledge_sync` truncation
  path is unchanged.)
- **One ingest job per corpus at a time.** The `Pipeline` owns the model handles
  (GLiNER, embedder); parallel ingest would thrash them. A second ingest request
  for a corpus with a `running` job returns that existing `job_id` rather than
  spawning a duplicate.

#### Crash safety

On MCP server startup, sweep `index_jobs` for rows left `status=running` (process
died mid-ingest) and mark them `interrupted`. Re-running ingest is safe and cheap:
already-indexed files are skipped via content-hash (`revision_is_fts_ready`).

#### New MCP tool: `job_status`

```
job_status({ job_id }) →
  { ok, job_id, status: running|done|interrupted|failed,
    files_done, files_total, last_error }
```

### Pillar 2 — Reranked hybrid legal search (Accurate)

**Add `Pipeline::legal_search_reranked`** combining the existing hybrid retrieval
with the existing cross-encoder, made the default for legal semantic search.

```
legal_search_reranked(query, top_k, filters):
    1. hybrid retrieve over-fetch pool (rerank_pool_size = 30)
       via existing legal_search path (vector + BM25 FTS + RRF)
    2. rehydrate each candidate text_pseudo → plaintext
       (PRIVACY: embed + FTS already ran on pseudonymized query;
        plaintext is used ONLY for the rerank scoring stage)
    3. cross-encoder bge-reranker-v2-m3 scores (query, passage) pairs
    4. reorder by score desc, truncate to top_k
```

**Decisions locked in design review:**
- **Rerank ON by default for legal.** `search(scope=legal, mode=semantic)` reranks
  unless the caller passes `rerank=false`. Legal is the high-stakes path, so
  accuracy-first is the correct default. Cost: ~200-400ms to rerank 30 candidates
  on CPU INT8 ONNX.
- **Feature-flag fallback.** Reranking lives behind the existing `rerank` Cargo
  feature. A binary built without it falls back to RRF-only hybrid and emits a
  warning in the response, rather than failing.

### Pillar 3 — Bundled correctness fixes

These are prerequisites, not extras — Pillar 1's "build index at completion" step
is what fixes (1) and (2).

| # | Fix | Location |
|---|-----|----------|
| 1 | `sync_local_scope` calls `maybe_build_fts_index()` when `!summary.truncated` | `anno-rag-mcp/src/indexer.rs` |
| 2 | Lazy fallback: `Store::search()` catches "no inverted index" → build → retry once | `anno-rag/src/store.rs` |
| 3 | `corpus_list` returns `[{corpus_id, label, knowledge_sources, legal_documents}]` via `service.list()` | `anno-rag-mcp/src/lib.rs` |
| 4 | `search()` surfaces an `index_building` status instead of silent empty hits when the index is absent | `anno-rag-mcp/src/search.rs` |

## Component Boundaries

### `anno-knowledge-store`
- `index_jobs` CRUD: `insert_job`, `update_job_progress`, `set_job_status`,
  `get_job`, `list_interrupted_jobs`.
- Pure SQLite, no model loading. Unit-testable in-memory.

### `anno-rag`
- `Pipeline::legal_search_reranked` (behind `rerank` feature).
- Lazy FTS-build fallback in `Store::search`.

### `anno-rag-mcp`
- Job registry: spawn detached ingest, dedupe per-corpus, expose `job_status`.
- Startup interrupted-job sweep.
- `corpus_list` fix; `search` `index_building` status.

## Data Flow

```
INGEST (fast):
  client → legal_ingest → [insert job, spawn] → return job_id (instant)
                                  │
                          background: per-file ingest → optimize_after_ingest
                                  │                            (vector IVF + FTS)
                          client → job_status(job_id) → progress / done

SEARCH (accurate):
  client → search(scope=legal, mode=semantic)
         → legal_search_reranked
         → hybrid pool(30): vector + BM25 FTS + RRF   [pseudonymized query]
         → rehydrate pool                              [plaintext, rerank only]
         → cross-encoder rerank → top_k                [bge-reranker-v2-m3]
```

## Error Handling

- **Ingest failure mid-job:** record `last_error` on the job row, continue with
  remaining files (matches today's per-file `tracing::warn` + skip), final status
  `done` with `failed_count > 0` or `failed` if zero succeeded.
- **Missing index at search time:** lazy build + retry once; if build itself
  fails, return the error with an actionable `index_building`/`run job_status`
  hint — never a raw Lance string.
- **Rerank feature absent:** RRF-only fallback + warning, not an error.
- **Duplicate ingest:** return existing `job_id`, status `running`.

## Testing Strategy

- **Job lifecycle** (`anno-knowledge-store`, in-memory): running → progress →
  done; interrupted sweep marks stale `running` rows.
- **Rerank ordering** (`anno-rag`): fixture candidate pool, assert cross-encoder
  reorders vs RRF baseline; assert privacy invariant (FTS/embed see pseudonyms).
- **FTS-build-on-sync** (integration): index synthetic French fixtures, assert
  `search(mode=fast)` returns hits where it previously returned empty.
- **`corpus_list`**: assert IDs + counts returned.
- **Fixtures:** synthetic / consented French legal pages only — no real client
  PII (privacy rule).

## Privacy Invariants (must hold)

- Embedding and FTS lookup run on the **pseudonymized** query only.
- Plaintext rehydration happens **only** for the cross-encoder rerank stage, on
  candidates already retrieved.
- No secrets, vault passphrases, prompts, or legal matter text are logged.
- Job rows store IDs and counts only — never document text.

## Rollout

1. `anno-knowledge-store` job CRUD + tests.
2. `anno-rag` `legal_search_reranked` + lazy FTS fallback + tests.
3. `anno-rag-mcp` job registry, `job_status`, startup sweep, `corpus_list` fix.
4. `fmt` + `clippy -D warnings` (jobs 2) before PR.
5. Rebuild `anno-rag.exe`, re-run the MCP feature tour to confirm all search
   modes return results and a 60-doc legal ingest completes via job polling.
