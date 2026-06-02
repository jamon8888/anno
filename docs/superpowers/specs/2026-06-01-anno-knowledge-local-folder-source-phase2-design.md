# Anno Knowledge — Local Folder Source (Phase 2) Design

**Date:** 2026-06-01
**Status:** Draft for review
**Scope:** Add the first real `SourceConnector` — a local folder source — that
makes the knowledge path searchable end to end: walk a folder, extract with
Kreuzberg, pseudonymize through Anno's local PII pipeline, and write
chunk-level rows into the Phase 1 SQLite FTS store. After this phase,
`knowledge_search(mode=fast)` returns real pseudonymized hits from indexed
local files.

**Parent spec:** [`2026-05-29-anno-local-knowledge-service-multisource-design.md`](2026-05-29-anno-local-knowledge-service-multisource-design.md)
**Builds on:** [`../plans/2026-05-29-anno-local-knowledge-service-phase1.md`](../plans/2026-05-29-anno-local-knowledge-service-phase1.md)

---

## 1. Prerequisite

This phase depends on Phase 1 being merged: the `anno-knowledge-core` and
`anno-knowledge-store` crates, the SQLite/FTS migrations, and the lazy
`KnowledgeService` cell on `AnnoRagServer`. At the time of writing Phase 1 is
**not yet implemented** — this design is valid to author now, but Phase 2 is
only implementable once Phase 1 lands.

## 2. Goal

Today (after Phase 1) the `knowledge_*` tools exist but return empty results:
there is no connector that populates the store. Phase 2 closes the loop for
local files:

```text
knowledge_add_local_folder(path)
knowledge_sync(source_id)
knowledge_search(query)   -> real pseudonymized FTS hits
knowledge_forget(target)  -> removes indexed content
```

The product promise stays intact: raw file bytes and extracted text are visible
only to Anno and its local models; Claude receives pseudonymized snippets only.

## 3. Decisions Locked In Brainstorming

These were chosen explicitly and are not open for re-litigation in the plan:

- **End-to-end searchable (A).** Phase 2 produces pseudonymized, FTS-ready
  chunks — not just extraction + state.
- **Narrow privacy API on `Pipeline` (A1).** Add
  `Pipeline::pseudonymize_knowledge_object`. Do not duplicate vault/model
  loading; do not call the monolithic `ingest_one_counted`.
- **Synchronous bounded sync (S1).** `knowledge_sync` does discovery +
  extraction + pseudonymization + FTS write in one call, bounded by a per-run
  budget, with distinct per-object states persisted so the sync/privacy split
  remains possible later. Large folders resume across successive calls.
- **Discovery-only connector + orchestrating indexer (E1).**
  `anno-source-local` depends on `anno-knowledge-core` only and performs pure
  discovery + change-detection. A `KnowledgeIndexer` in the service layer
  orchestrates extraction (`ingest::extract`) + privacy + storage.
- **Tool surface M2.** Ship `knowledge_add_local_folder`, `knowledge_sync`,
  `knowledge_forget`; make `knowledge_sources` / `knowledge_status` /
  `knowledge_search` real.
- **No legal enrichment** on the knowledge path. Mandatory-clause checks, the
  legal KG, and legal entity projection stay on the existing
  `Pipeline::ingest_folder`. The knowledge path is source-neutral and generic.
- **Title and metadata are pseudonymized** before being written or exposed.
  A file path can contain client names, so the path passes through the vault
  too.

## 4. Non-Goals

- No vector projection / `knowledge_chunks_v1` / semantic or deep search
  (deferred to Phase 3).
- No `knowledge_open` tool (search snippets are sufficient for Phase 2).
- No background job queue / worker / claim model (S1 is synchronous bounded).
- No changes to the existing `Store`, `chunks` table, `Pipeline::ingest_folder`,
  `Pipeline::search`, `legal_ingest`, or `legal_search`.
- No dual-write from `Pipeline::ingest_folder` into the knowledge store.

## 5. Architecture

```text
knowledge_sync (MCP tool, S1 synchronous bounded)
   |
   v
KnowledgeIndexer            (service layer, in anno-rag-mcp)
   |   depends on anno-rag + anno-knowledge-store + anno-source-local
   |
   +-- 1. LocalFolderSource.discover(scope, budget)
   |        -> anno-source-local (core-only, no models)
   +-- 2. ingest::extract(path)
   |        -> anno-rag (Kreuzberg + OCR gating + chunking, reused)
   +-- 3. Pipeline::pseudonymize_knowledge_object(input)
   |        -> anno-rag (narrow API A1; loads NER + uses vault; no embed)
   +-- 4. KnowledgeControlStore.commit_object(..)
            -> anno-knowledge-store (single SQLite txn + FTS rows)
```

### 5.1 Components

| Unit | Crate | Responsibility | Depends on |
|------|-------|----------------|-----------|
| `LocalFolderSource`, `DiscoveredObject` | `anno-source-local` *(new)* | folder walk, type/budget filters, deterministic IDs, content-hash change detection; yields discovered objects. No models, no Kreuzberg. | `anno-knowledge-core` |
| `pseudonymize_knowledge_object`, `PrivacyIndexInput`, `PseudonymizedChunk` | `anno-rag` *(additions)* | reuse `detector` + `vault` to pseudonymize part text, title, and metadata into chunks. No embedding (Phase 3). | existing `Pipeline` internals |
| Source/account/scope CRUD; object/revision/part upsert + state transitions; chunk + FTS write; `forget` cascade; real `status` counts | `anno-knowledge-store` *(additions)* | replaces the Phase 1 `insert_test_chunk` placeholder with real write paths. | `anno-knowledge-core` |
| `KnowledgeIndexer` | `anno-rag-mcp` | orchestrates the sync flow; owns the per-run budget loop and state transitions. | all of the above |

### 5.2 Why discovery-only connector

`anno-source-local` stays small and core-only: it is testable on fixtures with
no model directory and no SQLite, and it does not duplicate the OCR-gating /
chunking / budget logic that already lives in `anno-rag::ingest`. The coupling
to `anno-rag` (extraction + privacy) is concentrated in the `KnowledgeIndexer`,
which already sits in `anno-rag-mcp` where that dependency exists. This mirrors
the parent spec's "source sync worker / privacy worker" separation (§14) while
running them in one synchronous call for the MVP.

## 6. `DiscoveredObject` and change detection

```rust
struct DiscoveredObject {
    external_id: String,      // canonical absolute path (or scope-root-relative)
    path: PathBuf,
    object_type: ObjectType,  // LocalFile
    content_hash: [u8; 32],
    mtime: DateTime<Utc>,
    byte_size: u64,
    title_raw: Option<String>, // file name; pseudonymized downstream
    metadata_raw: serde_json::Value, // { path, ext, mtime, size }; pseudonymized downstream
}
```

Deterministic IDs follow the parent spec §10, refined for local files so that
revision identity is **content-based**:

```text
object_id   = UUIDv5(scope_id + external_id)
provider_version (local file) = hex(content_hash)
revision_id = UUIDv5(object_id + provider_version)   // i.e. content-based
part_id     = UUIDv5(object_id + part_type)          // single FileBody part
chunk_id    = UUIDv5(revision_id + part_id + chunk_idx)
```

This refines the parent spec's `mtime + size + content hash` provider version:
`mtime + size` are used only as a cheap pre-check to decide whether a file must
be re-hashed, not as part of revision identity. A touch-only change (mtime
changes, bytes identical) yields the same `content_hash`, the same
`revision_id`, and is therefore skipped.

Skip rule: if a revision with the current `content_hash` already exists for the
object and the object state is `fts_ready`, the object is skipped as unchanged.
Otherwise a new revision is created, prior chunks and FTS rows for the object
are replaced, and the new revision becomes current.

## 7. `knowledge_sync` flow (S1)

1. Load the source's enabled scopes from SQLite.
2. `LocalFolderSource.discover(scope, budget)` returns a bounded list of
   `DiscoveredObject`. The budget caps files (and total bytes) per run.
3. For each discovered object, apply the skip rule (§6).
4. `ingest::extract(path)` produces a `KnowledgePart(FileBody)` with chunked
   text and char offsets, honoring existing OCR/budget gating.
5. `Pipeline::pseudonymize_knowledge_object(input)` returns pseudonymized chunks
   plus a pseudonymized title and pseudonymized metadata JSON.
6. A single SQLite transaction per object upserts object/revision/part, replaces
   chunk rows and FTS rows, and sets state to `fts_ready`.
7. If the budget is exhausted mid-folder, the run stops; remaining files stay in
   `discovered` and the next `knowledge_sync` resumes.
8. Deletion reconciliation: files present in the store for this scope but absent
   from the walk are set to `forgotten` and their chunks/FTS rows purged. This
   runs **only on a complete (non-truncated) walk**, to avoid false positives
   when the budget truncates the run.
9. Return a summary:
   `{ seen, skipped_unchanged, extracted, pseudonymized, fts_ready, forgotten, failed }`.

## 8. Privacy API (A1)

Added to `anno-rag::Pipeline`:

```rust
struct PrivacyIndexInput {
    object_id: ObjectId,
    revision_id: RevisionId,
    part_id: PartId,
    source_kind: SourceKind,
    object_type: ObjectType,
    title_raw: Option<String>,
    metadata_raw: serde_json::Value,
    // chunked extracted text from ingest::extract
    chunks: Vec<ExtractedChunk>, // text + char_start/char_end + idx
}

struct PseudonymizedChunk {
    chunk_id: ChunkId,
    chunk_idx: u32,
    title_pseudo: Option<String>,
    text_pseudo: String,
    metadata_pseudo_json: String,
    char_start: u32,
    char_end: u32,
}

impl Pipeline {
    async fn pseudonymize_knowledge_object(
        &self,
        input: PrivacyIndexInput,
    ) -> Result<Vec<PseudonymizedChunk>>;
}
```

Internally this reuses the existing `detector.detect_for_ingest(...)` (PII
subset only — legal labels are ignored for the knowledge path) and
`vault.pseudonymize_with_map(...)`, the same primitives
`ingest_one_counted` uses, but without legal enrichment, without embedding, and
without LanceDB. Title and metadata strings are pseudonymized through the same
vault so no raw names leak into searchable rows.

Embedding is intentionally excluded; `embed_pseudonymized_chunks` arrives in
Phase 3 with the vector projection.

## 9. Graceful degradation (models absent)

Pseudonymization needs the local NER model. When models are absent:

- discovery and Kreuzberg extraction still run (states advance to
  `extracted_pending_privacy`);
- nothing is written to FTS;
- the tool returns a partial summary plus a structured error
  `{ code: "models_missing", next_action: "download_models" }`.

Extracted objects are pseudonymized on the next `knowledge_sync` once models are
present. `knowledge_status` and `knowledge_search(mode=fast)` continue to work
with no model directory.

## 10. Tool surface (M2)

New tools:

- `knowledge_add_local_folder(path)` — creates a `LocalFolder` source, a
  synthetic `"local"` account, and a scope for the folder, with pseudonymized
  display labels. Returns the `source_id`. Must not load models.
- `knowledge_sync(source_id?)` — runs the bounded S1 flow. With no argument,
  syncs all enabled local-folder sources. Returns the summary (§7).
- `knowledge_forget(target)` — `target` is a source, account, scope, or object.
  Cascade-deletes SQLite rows and FTS rows (no LanceDB in Phase 2) and performs
  best-effort orphan vault-token cleanup. Must not load models.

Made real (no longer placeholders):

- `knowledge_sources` — lists configured sources from SQLite.
- `knowledge_status` — real counts and per-state progress.
- `knowledge_search` — fast FTS over indexed chunks.

`anno_health.available_tools` is updated with the three new tool names.

Out of scope: `knowledge_open`, `knowledge_configure_source`,
`knowledge_disconnect`, semantic/deep modes.

## 11. Error handling

Per-object failures are isolated — one failure does not abort the run. The
failing object is set to `failed_retryable` or `failed_permanent` with
`last_error`, visible in `knowledge_status`. Error categories used:
`unsupported_file_type`, `budget_exceeded`, `extraction_failed`,
`pseudonymization_failed`, `fts_store_failed`, `models_missing`. MCP responses
are JSON with `{ ok, error: { code, message, next_action } }` per parent spec
§21. Logging records counts/ids/state/error-class/durations only — never raw
text, file bodies, or pseudonymization internals.

## 12. Test strategy

**`anno-source-local` (pure, no models, no SQLite):**

- Deterministic `object_id` / `revision_id` for the same path + content.
- Change detection: content edit changes `content_hash` and `revision_id`;
  touch-only (mtime change, same bytes) behavior is defined and tested.
- Type and budget filters.
- Deletion detection on a complete walk.

**`anno-knowledge-store`:**

- Source/account/scope CRUD round-trip.
- Object/revision/part upsert and state transitions.
- Chunk + FTS row write and replacement on re-sync (no duplicate FTS rows).
- `forget` cascade removes objects, chunks, and FTS rows.
- `status` counts reflect inserted/forgotten objects and per-state progress.

**`pseudonymize_knowledge_object` (anno-rag, model-gated or mocked detector):**

- Chunk text, title, and metadata are all pseudonymized.
- No raw PII string survives into any returned field.

**`KnowledgeIndexer` integration (model-gated):**

- Fixture folder -> `add_local_folder` -> `sync` -> `search(fast)` returns
  pseudonymized hits.
- Re-sync of an unchanged folder is idempotent (skips, no new revisions).
- Editing a file produces a new revision and replaces its FTS rows.
- `forget` removes the indexed content from search.

**Non-regression / privacy:**

- Existing `search`, `legal_ingest`, `legal_search` behavior unchanged.
- `knowledge_search(mode=fast)` and `knowledge_status` work with no model
  directory.
- Models-absent `sync` returns `models_missing` and leaves objects in
  `extracted_pending_privacy` without FTS rows.

## 13. Acceptance criteria

- `knowledge_add_local_folder` registers a source/account/scope without loading
  models.
- `knowledge_sync` indexes a fixture folder end to end and reports a correct
  summary; a second sync skips unchanged files.
- `knowledge_search(mode=fast)` returns pseudonymized hits from indexed files
  and never returns raw source text.
- `knowledge_forget` removes a scope/source/object from SQLite and FTS.
- `knowledge_status` reports real per-state counts.
- No edits to `crates/anno-rag/src/store.rs`, `Pipeline::ingest_folder`,
  `Pipeline::search`, `legal_ingest`, or `legal_search`.
- `anno-source-local` has no dependency on `anno-rag`, MCP, SQLite, or Kreuzberg.
- Targeted crate tests pass; `npx gitnexus detect-changes` reports only the
  expected knowledge/source/MCP files.

## 14. Deferred to later phases

All items deferred from this spec are tracked in [`docs/product/roadmap.md`](../../product/roadmap.md):

- Phase 3: vector projection + semantic search (spec'd separately)
- Deletion reconciliation (`forgotten` counter scaffolding)
- Outlook/Microsoft connector + attachments + additional sources
- Optional dual-write from `Pipeline::ingest_folder`
- Background job queue / daemon split
