# MCP Folder Auto-Sync And Legal Output Isolation Design

## Summary

Anno MCP must let a user index a client folder once, come back later after adding documents, and get targeted search results for the selected corpus without accidentally indexing generated artifacts. The product goal is one Anno pipeline that can ingest a dossier or future connected sources, extract content, anonymize it, and make it available for RAG. The `knowledge` layer should be the corpus source-sync plane for that pipeline: local folders today, and connectors such as Outlook, Notion, OneDrive, or other client data sources later.

The current knowledge path already behaves like an incremental rescan when `knowledge_sync` is called explicitly: local files are discovered again and unchanged revisions are skipped by provider version. The current legal path is less clearly separated: `legal_ingest_impl` writes generated anonymized files to `folder/anon`, while `index(profile="legal" | "all")` also ingests the source folder recursively. Current code has defensive filters for the legal output directory and `.anon.*` files, but correctness still depends on every crawler and future connector remembering that generated artifacts live inside the source tree.

This design keeps client folders as pure source roots, moves generated legal outputs out of the source tree by default, adds defensive generated-file exclusions, and introduces bounded corpus sync behavior that can refresh newly added documents on the next MCP use without requiring a permanent filesystem watcher. It also clarifies that knowledge, legal, and tabular are not separate ingestion products: they are stages and indexes fed by the same private document pipeline.

## Goals

- Prevent `legal` and `profile="all"` from re-ingesting `anon/` outputs or `.anon.*` chains.
- Keep indexed client folders isolated by `corpus_id`.
- Clarify that `knowledge` owns source discovery, source versioning, and connector sync, while Anno's shared private document pipeline owns extraction, anonymization, and RAG/index outputs.
- Allow documents added after the first index to be picked up by a later MCP interaction.
- Keep sync incremental and bounded so normal search does not become an unbounded full re-index.
- Make freshness visible in MCP responses.
- Preserve the user's mental model that a connected folder is a living corpus, while still making sync work explicit and bounded.
- Provide an explicit `sync_corpus(corpus_id, ...)` tool for deterministic catch-up.
- Avoid hidden background work that consumes CPU or disk without a user-visible state.

## Non-Goals

- No permanent filesystem watcher in the first implementation.
- No automatic deletion of files that already exist in a user's `anon/` folder.
- No UI work in Claude Desktop.
- No cross-corpus auto-selection based only on Claude session state.
- No attempt to make legal semantic ingestion model-free.
- No removal of the knowledge layer; future source connectors depend on it.
- No broad rewrite of the existing knowledge or legal storage engines.

## Current Behavior

### Knowledge

`index(profile="general" | "all")` registers a local folder as a knowledge source, then calls `knowledge_sync`. `sync_local_scope` discovers files for that local folder each time it runs. For each discovered file, it computes an object id and provider version. If the same revision is already FTS-ready, the file is counted as `skipped_unchanged`; otherwise it is extracted, pseudonymized, and committed to the knowledge FTS store.

This means the knowledge side is already suitable for incremental catch-up, but only when a sync tool is called. There is no current watcher or automatic sync trigger.

The unclear part is architectural rather than mechanical: knowledge currently mixes source-plane responsibilities with a first RAG output:

- source synchronization: discover local-folder objects, compute stable ids, hash content, and detect changes;
- fast RAG output: extract, pseudonymize, and commit text to the knowledge FTS index.

For a local folder, this overlaps with legal ingest because both paths walk files, extract text, and anonymize chunks. For the target MCP architecture, knowledge must be the source-sync control plane for all corpus sources, while legal ingest should become a specialized stage of the same private document pipeline, not a separate crawler with its own source semantics.

### Legal

`legal_ingest_impl` creates its output directory as `folder.join("anon")`. `index(profile="legal" | "all")` calls legal ingest with `recursive: true`. The current legal walker skips the configured output directory and generated-looking `.anon.*` files, and the local knowledge walker skips a top-level `anon/` directory plus `.anon.*` files. This is better than an unfiltered recursive walk, but it leaves the invariant spread across multiple walkers.

The observed consequences are:

- older installed builds or adjacent tools can still leave `.anon.anon.*` chains in client folders;
- generated artifacts remain visible in the user folder even when they are not meant to be source documents;
- every new connector, stage, or output must remember generated-output exclusions;
- `profile="all"` still performs separate local-folder discovery and extraction paths for knowledge and legal outputs.

## Architecture Decision

Adopt three invariants:

1. A client folder is a source root only.
2. Generated artifacts are output resources owned by Anno and addressed through `corpus_id`, not recursive inputs under the client root.
3. Source synchronization is separate from private document processing. A connector produces versioned corpus objects; the shared Anno pipeline extracts, anonymizes, and feeds knowledge FTS, legal semantic search, tabular review, anonymized export, and future RAG/index outputs.

The recommended implementation is:

- Move default legal outputs to an internal data directory such as:

```text
<anno_data_dir>/corpora/<corpus_id>/outputs/legal-anon/
```

- Keep raw client paths internal. MCP responses expose `corpus_id`, pseudonymous labels, sync state, and export handles, not absolute output paths unless a user explicitly asks for an export destination.
- Add defensive generated-file exclusions to all local recursive discovery paths, including legal ingest and knowledge local-folder discovery.
- Add an explicit export operation for users who need generated anonymized files in a chosen location.

## Source Layer And Shared Pipeline

`knowledge` is the general source-sync layer and the first fast-retrieval sink for a corpus. The target source layer must support more than local folders:

- local folders;
- Outlook mailboxes or folders;
- Notion pages/databases;
- OneDrive/SharePoint folders;
- other future source connectors.

The target shape is:

```text
Source connector
  -> SourceObject { source_id, scope_id, external_id, content_hash, metadata, fetch handle }
  -> CorpusSourceObject { corpus_id, object_id, provider_version, relative label }
  -> PrivateDocumentRevision:
       - extracted chunks
       - PII/legal detection bundles
       - pseudonymized chunks
       - offset maps
       - embedding inputs
  -> RAG and workflow outputs:
       - knowledge_fast_fts
       - legal_semantic_index
       - legal_graph
       - tabular_review_documents
       - anonymized_export
       - future vector/graph sinks
```

`knowledge_sync` should own source enumeration, source versions, deletion reconciliation, and connector-specific budgets. The private document pipeline should then process changed source objects through extraction, anonymization, and the requested RAG/workflow outputs. The fast knowledge FTS index is one output of that pipeline. Legal indexing should consume the same private document revision rather than rediscovering the folder as an unrelated ingest job.

This does not require a full storage rewrite in the first implementation. The first implementation can still call existing knowledge and legal functions, but the public design and future plan should describe one corpus source graph and one shared private document pipeline, not two independent local-folder crawlers.

## Codebase-Derived Constraints

The current code already points toward a source-driven pipeline architecture:

- `anno-knowledge-core::SourceKind` includes `LocalFolder`, `MicrosoftOutlook`, `MicrosoftOneDrive`, `MicrosoftSharePoint`, `Gmail`, `GoogleDrive`, `Slack`, and `Notion`. These source kinds exist in the domain model, while the current MCP sync path only implements local-folder discovery.
- Knowledge IDs are provider-oriented: `SourceId`, `AccountId`, `ScopeId`, `ObjectId`, `RevisionId`, `PartId`, and `ChunkId` are deterministic from source kind, account key, scope key, external id, and provider version.
- `anno-knowledge-store` persists `knowledge_sources`, `source_accounts`, `source_scopes`, `knowledge_objects`, `knowledge_revisions`, `knowledge_parts`, `knowledge_chunks`, and FTS rows. It already filters search by `source_id` or `scope_id`.
- The corpus registry currently binds a corpus to backend ids and records `corpus_documents`, but the documents currently represent backend document instances, mainly legal documents. It does not yet have a first-class corpus source-object or private-document-revision catalog.
- Legal ingest still creates its own legal `doc_id` and writes LanceDB vectors, legal enrichment rows, graph rows, and anonymized markdown output.
- Tabular review currently consumes legal/RAG `doc_id` values and validates them through `corpus_documents(backend_kind="legal")`.

Implication: the missing architecture piece is not another folder watcher. It is a stable relationship between corpus source objects, private document revisions, and RAG/workflow outputs.

The target registry needs to distinguish:

```text
CorpusSourceObject
  corpus_id
  source_id
  scope_id
  object_id
  provider_version
  content_id
  relative_label_hash
  source_state

PrivateDocumentRevision
  corpus_id
  object_id
  provider_version
  content_id
  revision_state      // extracted, anonymized, failed, stale

PipelineOutput
  corpus_id
  object_id
  provider_version
  output_kind         // knowledge_fast, legal_semantic, legal_graph, tabular_ready, export
  output_id           // legal doc_id, knowledge revision id, tabular row/doc id, etc.
  status              // ready, pending, failed, stale
```

This lets a connector say "this Outlook message changed" once, then the system can extract/anonymize it once and decide which outputs need refresh. Legal may still write a legal-specific `doc_id`, but that id must be traceable back to the source object and private document revision.

## Processing Model

The intended flow for a local folder should be:

1. `index(path, profile)` registers or resolves a corpus.
2. The local folder is registered as a corpus source/scope.
3. Source sync discovers source objects and provider versions once.
4. The shared private document pipeline processes changed objects:
   - extract source content into chunks;
   - run PII and legal detection once when legal-capable outputs are requested;
   - pseudonymize chunks and retain offset maps;
   - prepare embedding inputs and private document metadata.
5. Requested outputs are written from that private document revision:
   - `knowledge_fast` writes PII-safe FTS for fast RAG.
   - `legal_semantic` writes vectors, legal enrichment, graph rows, and legal `doc_id` registration.
   - `tabular_ready` remains a consumer of legal-ready documents unless a future tabular stage can operate directly from private document revisions.
   - `anonymized_export` writes user-requested anonymized files outside the source root.
6. Search and review tools read pipeline/output freshness for the selected `corpus_id`.

The first implementation can preserve current calls internally:

```text
index(profile="all")
  -> source_sync(local_folder)
  -> run_existing_knowledge_sync(source_id)
  -> run_existing_legal_ingest(corpus_id, source root)
  -> record output status against the same corpus/source change-set
```

That is not perfect deduplication yet, but it fixes the control-plane semantics. Later work can avoid repeated extraction by sharing extracted chunks, detection bundles, pseudonymized chunks, and source-object snapshots between outputs.

## Redundancy Assessment

`knowledge_sync` and `legal_ingest` are redundant only at the local-folder mechanics layer:

- both walk files;
- both call `ingest::extract`;
- both pseudonymize chunks;
- both need the detector/vault path.

They are not redundant in output semantics:

- The source-sync part of knowledge is required because future connectors need provider identity, sync state, deletion reconciliation, and budgets.
- The fast knowledge output produces PII-safe FTS for RAG over source objects.
- Legal output produces embeddings, legal enrichment rows, graph entities, mandatory-clause status, legal search, and legal document ids consumed by tabular review.

The right fix is therefore not to remove knowledge or fold everything into legal. The right fix is to make source sync explicit and make all Anno outputs depend on one shared extraction/anonymization pipeline.

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
sync_corpus(corpus_id, sources?, outputs?, max_files?, max_millis?)
```

Default behavior:

- `sources` defaults to all enabled sources bound to the corpus.
- `outputs` defaults to `["knowledge_fast"]` for opportunistic sync.
- An explicit user-requested full dossier sync may pass `outputs=["knowledge_fast","legal_semantic"]` when legal was part of the corpus profile.
- `max_files` and `max_millis` apply per call.
- The response reports source sync, private document pipeline, and output summaries separately.

Example response:

```json
{
  "ok": true,
  "corpus_id": "...",
  "freshness": "fresh",
  "sources": {
    "seen": 22,
    "changed": 1,
    "unchanged": 21,
    "deleted": 0,
    "truncated": false
  },
  "pipeline": {
    "extracted": 1,
    "anonymized": 1,
    "failed": 0,
    "truncated": false
  },
  "knowledge": {
    "seen": 22,
    "skipped_unchanged": 21,
    "indexed": 1,
    "failed": 0,
    "truncated": false
  },
  "legal": {
    "ran": false,
    "reason": "output not requested"
  }
}
```

## Opportunistic Auto-Sync

### Why It Must Feel Watched

From the user's point of view, "I connected this client folder" usually means "Anno knows what is in this folder." If the user adds a document and then asks a targeted question, a stale answer looks like a search or RAG failure even when the only missing step is another sync.

The product requirement is therefore not real-time filesystem watching. The requirement is that the MCP never silently presents an old index as definitely current. Search and review responses must make freshness visible, and selected-corpus operations should either catch up within a clear budget or report that the index may be stale.

Do not add a permanent watcher in v1. A watcher is fragile in Claude Desktop/MCP because the process can be restarted, folders can be large or remote, and hidden background model work can surprise users with CPU, disk, and startup cost. Instead, run a lightweight corpus freshness check before user-facing corpus operations:

- `search`
- `corpus_health`
- `sources`
- future selected-folder tools

If the selected corpus appears stale, run a bounded sync only when it will not surprise the user with heavy model startup.

Recommended default:

- For `search(scope="knowledge")` or mixed `scope="all"`, attempt a small source sync plus `knowledge_fast` output only if dependencies are already ready or the sync can be performed within a configured budget.
- For `search(scope="legal")`, do not silently perform full legal ingest before search. Return freshness metadata and let the caller invoke `sync_corpus(..., outputs=["legal_semantic"])` or `index(profile="legal" | "all")`.
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
- `index(profile="all")` records one source change-set for the corpus and reports pipeline plus knowledge/legal output summaries separately.
- Knowledge sync indexes a file added after first index and skips unchanged files.
- A source object changed by a connector can pass through the same extraction/anonymization pipeline and feed knowledge/legal outputs without creating two unrelated source identities.
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
3. Add `sync_corpus` with budgeted source sync and `knowledge_fast` output first.
4. Add freshness metadata to corpus/search responses.
5. Add optional bounded opportunistic sync before selected corpus operations.
6. Add legal output support to `sync_corpus` with explicit `outputs=["legal_semantic"]` or `outputs=["knowledge_fast","legal_semantic"]`.
7. Add `export_anonymized`.

This order fixes the `anon/` feedback loop before making any automatic sync behavior more active.

## Acceptance Criteria

- Re-running `index(profile="all")` on the same folder does not ingest Anno-generated files.
- Adding one document after first index and running `sync_corpus` indexes only the new or changed document.
- `profile="all"` treats knowledge and legal as outputs of the same source object and private document pipeline, not two unrelated source crawls.
- Future non-folder connectors can feed the same extraction, anonymization, and RAG output pipeline.
- Search responses disclose whether the selected corpus index is fresh.
- Automatic sync never performs an unbounded recursive re-index.
- Legal generated outputs are not written into the client source root by default.
- Multi-corpus search remains constrained by `corpus_id`.
