# MCP Client Corpus Control Plane Design

## Summary

Anno MCP must support indexing multiple client folders while preventing Claude Desktop from searching, extracting, or tabular-reviewing across client boundaries by accident. The current MCP can index multiple folders, but search and review tools operate over global indexes unless the caller manually constrains them with business filters.

This design introduces a dedicated corpus control plane. `corpus_id` becomes the stable public identity of a client folder, while existing backend ids such as knowledge `source_id`, legal `legal_folder_*`, tabular `review_id`, and document `doc_id` become bindings owned by that corpus. The implementation must not be a loose set of optional filters added to individual tools; every client-visible operation must resolve an effective corpus before touching knowledge, legal, or tabular data.

The design also fixes two baseline issues discovered during MCP testing: bundled Pdfium support for PDF ingestion and raw Cypher use in `legal_extract_case_file`.

## Goals

- Treat the root folder passed to `index(path=...)` as one client corpus.
- Introduce a corpus control plane with shared domain types and a registry used by MCP, knowledge, legal, and tabular code.
- Return a stable `corpus_id` from `index`.
- Require `corpus_id` for sensitive tools when more than one corpus exists.
- Allow cross-corpus usage only with an explicit admin override.
- Keep raw local paths out of MCP responses.
- Preserve backwards compatibility for deprecated tools where possible, while applying the same safety guards.
- Fix PDF ingestion in local Claude Desktop builds by enabling bundled Pdfium.
- Fix `legal_extract_case_file` for the SQLite graph backend.

## Non-Goals

- No implicit session-global selected corpus in the first implementation.
- No UI work in Claude Desktop.
- No remote storage or multi-tenant cloud service.
- No automatic split of every subfolder into its own corpus. Subfolders stay inside the indexed root corpus.
- No broad rewrite of the knowledge, legal, or tabular storage engines beyond the interfaces needed to enforce corpus scope.
- No reliance on Claude Desktop session state as the only isolation mechanism. A future connection-selected corpus can reuse this control plane, but the first implementation uses explicit `corpus_id` resolution.

## Current Behavior

`index(path, profile)` registers the folder as a knowledge source and, for `legal` or `all`, ingests legal chunks into the global LanceDB-backed legal store. `sources()` can list pseudonymized source entries, and `forget(target)` can remove a source by id, legal folder id, or path.

Search is not client-corpus scoped:

- `search.scope` means `all`, `knowledge`, or `legal`; it does not mean client folder.
- `knowledge_search` uses SQLite FTS over `knowledge_objects_fts` without a `source_id` or `scope_id` filter.
- `legal_search` filters by legal metadata such as `dossier_id`, parties, clause types, and risk flags, but current ingestion does not set a client-folder corpus filter.
- `review_create.scope_folder` is informational, and `review_add_rows` currently stores tabular rows with `folder_path: None`.

This allows an indexed folder A and indexed folder B to both participate in later searches unless the tool implementation applies a corpus filter.

## Code Review Findings

The design is driven by the following issues observed in the existing code:

### P0: Unified MCP Search Is Not Client Scoped

`SearchUnifiedParams` currently exposes only `query`, `top_k`, `mode`, `scope`, and legal `filters`. There is no `corpus_id` and no `allow_cross_corpus` override. The unified router calls knowledge and legal search without a client-folder constraint, so `scope` only selects the index family, not the active client dossier.

Required change:

- Add `corpus_id` and `allow_cross_corpus` to the unified search params.
- Resolve the effective corpus before any backend search runs.
- Refuse unscoped searches when more than one corpus exists.
- Include `corpus_id` in every hit returned to Claude Desktop.

### P0: Knowledge FTS Searches All Sources

`KnowledgeSearchRequest` has no source or corpus field, and `search_fast` queries `knowledge_objects_fts` directly with only `MATCH ?1`. That means all registered local-folder sources participate in FTS ranking.

Required change:

- Extend the knowledge search request with a source/corpus filter.
- Join or filter through `knowledge_objects` so FTS results are limited to the selected corpus.
- Return `source_id` or `corpus_id` in hits for auditability.

### P0: Legal Search Falls Back To Global LanceDB Search

`legal_search` applies legal metadata filters only when `LegalSearchFilters::has_any_filter()` is true. Without such filters, it calls the global chunk search. The legal metadata filters are business filters, not client-folder isolation, and ingestion currently does not stamp a corpus filter.

Required change:

- Add a corpus filter path independent of business legal filters.
- Restrict candidate chunks to document ids under the selected legal folder root.
- Compose corpus filtering with existing legal filters instead of replacing them.

### P0: Legal Document Identity Is Not Corpus-Qualified

`Pipeline::ingest_one_counted` computes `doc_id` from raw file bytes and skips ingestion when `Store::doc_exists(doc_id)` is already true. `Store::upsert` then keys chunk rows by `(doc_id, chunk_idx)`, and `chunk_id` is derived from `(doc_id, chunk_idx)`. This is fine for single-corpus idempotence, but it is not safe for multi-corpus client isolation: two different client folders containing the same file content can share the same `doc_id`, and the second folder can be skipped before the control plane can bind the document to that corpus.

Required change:

- Split document content identity from document instance identity.
- Introduce a corpus-qualified document instance id derived from `corpus_id`, normalized source path or relative path, and content hash.
- Keep a content hash/content id for dedupe, but do not use it as the sole legal `doc_id` exposed to extraction and tabular review.
- Update legal chunk keys, graph document nodes, enrichment rows, and tabular row validation to use the corpus-qualified document instance id.
- Add tests where corpus A and corpus B contain byte-identical files and both remain independently searchable, extractable, reviewable, and forgettable.

### P1: Tabular Reviews Do Not Enforce Their Scope

`review_create.scope_folder` is stored but not used as an operational guard. `review_add_rows` verifies only that a `doc_id` exists in the global index and writes tabular rows with `folder_path: None`.

Required change:

- Store authoritative corpus scope on each review.
- Validate every added `doc_id` against the review corpus.
- Persist row scope metadata rather than `None`.
- Make `review_extract` and `review_refine_cell` use the review corpus when reading source chunks.

### P1: Forget By UUID Leaves Legal Data Behind

`forget(target)` treats UUID targets as knowledge source ids and only removes knowledge data. Legal data is removed through `legal_folder_*` ids or explicit raw paths. For a folder indexed with `profile=all`, a user can remove the knowledge side while the legal chunks remain searchable.

Required change:

- Introduce corpus-aware forgetting.
- `forget(target=corpus_id)` must remove knowledge, legal, and tabular data for the corpus.
- Keep legacy `knowledge_forget` and `legal_folder_*` behavior, but make the main `forget` tool report which backend bindings it removed.

### P1: Some MCP Responses Still Expose Raw Paths

The MCP `SearchHitWire` shape includes `source_path` and `folder_path`. Those are useful internal fields, but they violate the corpus contract when returned directly to Claude Desktop. `sources()` already pseudonymizes labels, so search and extraction responses should follow the same rule.

Required change:

- Remove raw path fields from MCP hit responses.
- Replace them with `corpus_id`, pseudonymous corpus label, document label, and optional relative display label when safe.
- Keep raw paths internal to the control store and backend deletion/migration code.
- Add tests that corpus/search responses do not contain the indexed root path or absolute source paths.

### P1: Case-File Extraction Uses Raw Cypher

`legal_extract_case_file` reaches `extract_case_file`, which calls `kg.cypher(...)`. The SQLite graph backend does not support raw Cypher execution, so the tool fails before returning an empty or populated case-file result.

Required change:

- Replace raw Cypher calls with typed graph methods implemented by the SQLite backend.
- Keep the MCP shape stable.
- Add tests proving unknown dossiers return empty results instead of backend errors.

## Proposed Model

### Architecture Decision

Implement corpus scoping as a first-class control plane, not as backend-local filtering.

The workspace should add shared corpus crates:

- `anno-corpus-core`: lightweight domain types and errors, with no storage or ML dependencies.
- `anno-corpus-store`: local SQLite registry and migration logic for corpus metadata and backend bindings.

Primary dependencies:

- `anno-rag-mcp` depends on both `anno-corpus-core` and `anno-corpus-store`.
- `anno-knowledge-core`, `anno-rag`, and `anno-rag-tabular` depend on `anno-corpus-core` only when they need to accept or return corpus-scoped requests.
- Backend store crates should not depend on MCP types.

This Cargo shape keeps `corpus_id` available across the system without pulling MCP or heavy RAG dependencies into lower-level crates.

### Corpus Domain

A corpus is the normalized root local folder passed to `index(path=...)`. A stable `corpus_id` is derived from that normalized root path using the same deterministic UUID style already used in the workspace, but `CorpusId` must live in `anno-corpus-core` rather than borrowing the knowledge `SourceId` namespace.

The control plane must canonicalize roots before id generation and overlap checks. On Windows, this means resolving equivalent slash styles, trailing separators, and case differences before deciding whether two roots are the same or nested.

A corpus maps to:

- the knowledge source id for the local folder;
- the legal folder root used to ingest LanceDB chunks;
- the corpus-qualified document instance ids produced by legal ingestion;
- tabular reviews created for that corpus.

MCP responses expose `corpus_id` and pseudonymous labels only. They do not expose raw filesystem paths.

Recommended domain objects:

- `CorpusId`: public stable id returned by MCP.
- `CorpusRoot`: normalized local root path plus pseudonymous display label.
- `CorpusBinding`: mapping from a corpus to a backend object such as `knowledge_source`, `legal_folder`, `legal_doc`, or `tabular_review`.
- `CorpusProfile`: indexed profile state for `knowledge`, `legal`, `tabular`, or `all`.
- `CorpusDocumentRef`: mapping from a corpus to one backend document instance, including backend kind, public document id, source-path hash, optional relative path hash, and content hash.
- `EffectiveCorpus`: resolved scope for one MCP call, either one selected corpus or an explicit cross-corpus scope.
- `CorpusGuard`: resolver that validates tool params before backend access.

Existing ids remain valid, but they no longer define user-visible scope:

- `source_id` is a knowledge backend id.
- `legal_folder_*` is a legal compatibility id.
- `scope_folder` is deprecated as an authority signal.
- `dossier_id` remains business/legal metadata and must not be used for client-folder isolation.
- `doc_id` identifies a corpus-qualified document instance. Existing content-derived legal ids are compatibility data only and must not be trusted globally.

### Document Instance Identity

The code currently treats legal `doc_id` as a content id. The control-plane design requires a separate document instance identity:

- `content_id`: deterministic hash of file bytes, useful for dedupe and change detection.
- `document_instance_id`: deterministic id for one occurrence of a document inside one corpus.

Recommended formula:

`document_instance_id = uuid_v5(CORPUS_NAMESPACE, "document" | corpus_id | normalized_relative_path | content_id)`

If a relative path cannot be established, use a normalized source-path hash. The raw path must not cross the MCP boundary.

This change is required before corpus-scoped legal and tabular operations can be correct. Otherwise, a byte-identical document in corpus B can be skipped because corpus A already stored the same content-derived `doc_id`.

### Control Store

The control plane should use a dedicated local SQLite database, for example `corpus.sqlite3`, stored next to the existing Anno local data stores.

Minimum tables:

- `corpora`: one row per indexed client root.
- `corpus_bindings`: backend ids owned by the corpus.
- `corpus_documents`: corpus-qualified document instance ids, backend document ids, source-path hashes, relative-path hashes, and content ids associated with the corpus.
- `corpus_index_runs`: latest index status, profile, counters, and failures for observability.

Raw paths may be stored internally when needed for re-indexing and deletion, but they must not be returned by MCP corpus or search tools. MCP responses should expose `corpus_id`, pseudonymous label, profiles, counters, and health status.

The registry must reject ambiguous ownership. If a requested root overlaps an existing corpus root, the default behavior should be a clear error. A future explicit admin override can allow nested corpora, but the first implementation should avoid ambiguous document-to-corpus mapping.

### Default Safety Rule

If zero corpus entries exist, search and review setup tools return a clear error: index a folder first.

If exactly one corpus exists, `corpus_id` may be omitted and the MCP infers the single corpus.

If two or more corpus entries exist, sensitive tools require `corpus_id`. Without it, the MCP refuses the call. Cross-corpus search is allowed only when the caller passes `allow_cross_corpus: true`.

### Corpus Resolution

Every sensitive MCP tool must start with corpus resolution:

1. Load registered corpora from the control store.
2. If the call supplies `corpus_id`, verify that it exists and is healthy enough for the requested operation.
3. If the call omits `corpus_id` and exactly one corpus exists, infer that corpus.
4. If the call omits `corpus_id` and multiple corpora exist, return a refusal unless `allow_cross_corpus` is explicitly true.
5. If `allow_cross_corpus` is true, return an explicit cross-corpus resolution and include corpus ids in every returned hit.

Backends receive `EffectiveCorpus`, not raw MCP params. This keeps the safety rule in one place and avoids drift between tools.

### Required Invariants

- A client-visible read must not reach knowledge, legal, or tabular storage before corpus resolution succeeds.
- A backend result crossing the MCP boundary must include `corpus_id` or be explicitly marked as cross-corpus aggregate metadata.
- `doc_id` input is never trusted globally; it must be checked against the selected corpus.
- Business filters such as legal `dossier_id`, parties, risks, or clause type refine results inside a corpus. They do not replace corpus isolation.
- Deprecated tools must call the same resolver as replacement tools.
- `forget(corpus_id)` is the canonical deletion path and must remove all backend bindings for that corpus.

## MCP Surface

### New Tools

`corpus_list`

- Lists indexed corpora.
- Returns `corpus_id`, pseudonymous label, indexed profiles, object counts, legal chunk counts, and health status.

`corpus_get`

- Returns details for one corpus.
- Includes knowledge and legal counters plus last indexing status.

`corpus_health`

- Reports whether each backend binding is present and consistent for one corpus.
- Flags partial indexes, orphan legal chunks, orphan knowledge sources, and tabular reviews whose documents no longer belong to the corpus.

### Modified Tools

`index(path, profile)`

- Creates or updates the corpus registry row before backend ingestion.
- Returns `corpus_id` in addition to the existing knowledge and legal summaries.
- Re-indexing the same root returns the same `corpus_id`.
- Fails by default when the requested root overlaps an existing corpus root.

`search(query, top_k, mode, scope, filters, corpus_id, allow_cross_corpus=false)`

- Applies corpus filtering to knowledge and legal branches.
- Refuses without `corpus_id` when multiple corpora exist unless `allow_cross_corpus` is true.
- Includes `corpus_id` in every hit for auditability.
- Does not return raw `source_path` or `folder_path`.

`knowledge_search(query, top_k, mode, corpus_id)`

- Deprecated alias remains functional.
- Applies the same corpus guard and source filter.

`legal_search(query, top_k, filters, corpus_id)`

- Applies corpus filtering before returning legal hits.
- Existing legal metadata filters still apply inside the selected corpus.

`legal_extract_contract(doc_id, corpus_id)`

- Verifies the document belongs to the selected corpus before extraction.

`legal_mandatory_clause_audit(doc_id, doc_type, corpus_id)`

- Verifies the document belongs to the selected corpus.

`legal_risk_review(scope_id, is_dossier, corpus_id)`

- Applies corpus guard for document or dossier scope.

`review_create(name, template_id, corpus_id)`

- Stores corpus scope on the review.
- `scope_folder` is retained only for compatibility and should not be the authoritative scope.

`review_add_rows(review_id, doc_ids, force_reextract)`

- Rejects any `doc_id` that is outside the review corpus.
- Stores row `folder_path` or `corpus_id` metadata instead of `None`.

`review_extract` and `review_refine_cell`

- Use the review's corpus scope.

`forget(target)`

- Accepts `corpus_id` as the preferred target.
- Removes knowledge objects, legal chunks, legal enrichment, legal graph data, and tabular review data for the corpus.
- Reports separate `knowledge_objects`, `legal_chunks`, and `tabular_rows_or_reviews` removal counts.

Deprecated legal and knowledge aliases must share the same guard logic as their replacement tools.

## Data Changes

### Corpus Control Plane

Add the control plane before modifying individual backend searches. The MCP server should have one service responsible for:

- normalizing and registering roots;
- generating stable `corpus_id` values;
- recording backend bindings produced by ingestion;
- recording corpus-qualified document instance ids and content ids;
- resolving `EffectiveCorpus` for tools;
- validating `doc_id` and review ownership;
- coordinating symmetric deletion.

The control plane is the only layer allowed to translate between public `corpus_id` and backend ids. Backend crates may store corpus ids on records for efficient filtering, but they should not independently invent corpus identities.

Code anchors:

- `crates/anno-rag-mcp/src/knowledge.rs` currently opens `knowledge.sqlite3` under `AnnoRagConfig.data_dir`; `corpus.sqlite3` should be colocated under the same data dir.
- `crates/anno-rag-mcp/src/lib.rs` currently keeps `SearchUnifiedParams` without corpus fields and routes both branches directly.
- `crates/anno-rag/src/config.rs` defines `data_dir` as the root for `vault.enc`, `index.lance`, models, and outputs; the corpus store should use this root rather than the LanceDB dataset directory.

### Knowledge

Extend `KnowledgeSearchRequest` with optional `source_id` or `corpus_id` filtering. The SQLite FTS query must join or filter through `knowledge_objects` so results are restricted to the selected source/scope before ranking and limiting. Hits must include `corpus_id` or `source_id` in the wire response.

When ingestion creates or updates a knowledge source, the MCP index flow must register the `knowledge_source` binding in `corpus_bindings`. If future knowledge ingestion can produce multiple source ids for one root, the registry should support multiple bindings per corpus.

Current code constraints:

- `KnowledgeSearchRequest` has only `query`, `mode`, and `top_k`.
- `KnowledgeSearchHit` does not include `source_id`, so the FTS query must return enough identity to map hits back to a corpus.
- `KnowledgeControlStore::search_fast` currently queries `knowledge_objects_fts` directly. It must join `knowledge_objects` by `object_id` and filter by source/scope ids supplied from `EffectiveCorpus`.
- `KnowledgeService::add_local_folder` currently passes the raw input path as the stable key. The corpus flow must register a normalized root first and pass that normalized value to knowledge registration, or explicitly bind legacy raw-path source ids during migration.

### Legal

Persist the mapping between `corpus_id` and legal folder root. Legal search must restrict candidate chunks to document ids registered in `corpus_documents` for the effective corpus. The existing store subtree helpers remain useful for migration and deletion, but the MCP query path should not expose or trust raw paths as the authority. This corpus filter must run even when no business legal filters are supplied.

For better future dossier workflows, legal ingestion should also stamp a corpus grouping key into legal enrichment metadata or graph document nodes. This key can coexist with business `dossier_id`.

When legal ingestion creates document ids, the MCP index flow must register corpus-qualified document instance ids in `corpus_documents`. The legal pipeline therefore needs a scoped ingest summary that reports document ids already present as well as newly indexed documents, so older indexed rows can be bound to the corpus registry without re-indexing. Legal extraction tools must validate requested `doc_id` values against this table before reading chunks, enrichment rows, or graph nodes.

Implementation baseline:

- `Pipeline::ingest_folder_scoped_summary` returns corpus-qualified legal document ids and content ids.
- MCP `index(profile=legal|all)` registers those legal document ids with backend kind `legal` in `corpus_documents`.
- `Pipeline::legal_search_scoped` resolves allowed document ids to chunk ids via `Store::chunk_ids_for_docs`, then calls `search_filtered_to_chunks`.
- `legal_search` and unified `search(scope=legal)` resolve the same `EffectiveCorpus`; `allow_cross_corpus=true` is required for global legal search when more than one corpus exists.

Current code constraints:

- `Pipeline::legal_search` falls back to `Store::search` when legal filters are empty, so corpus filtering needs its own candidate path independent of `LegalSearchFilters::has_any_filter()`.
- `Store::doc_ids_for_source_subtree` and `delete_folder_rows` already scan `source_path` under a root and can be reused for migration and deletion, but runtime search scoping should use `corpus_documents`.
- `Store::doc_exists` and `Store::upsert` are keyed around content-derived `doc_id`; this must change, or duplicate content across corpora will not be represented correctly.
- Legal enrichment schema has `doc_id` and business `dossier_id`, but no `corpus_id`. Add either `corpus_id` directly or rely on `corpus_documents` plus corpus-qualified `doc_id`; direct stamping is better for health checks and filtered query performance.
- `extract_case_file` uses raw `kg.cypher(...)`; the typed replacement methods should also accept optional `EffectiveCorpus` or validate returned document ids against the corpus before returning rows.

### Tabular

Add or use an existing review field for authoritative corpus scope. Fill row scope metadata when adding documents. Any tabular operation that traverses source documents must validate against the review corpus. A `doc_id` that exists globally but belongs to another corpus must be rejected.

Tabular reviews should bind to a corpus at creation time. `scope_folder` can remain as deprecated display/input compatibility, but the authoritative field should be `corpus_id`. Rows should store enough scope metadata to audit which corpus owned the source document when the row was created.

Implementation baseline:

- `review_create` accepts `corpus_id` and binds the created review through `CorpusBindingKind::TabularReview`.
- When exactly one corpus exists, review creation can bind to it implicitly; when multiple corpora exist, `corpus_id` is required.
- `review_add_rows` resolves the review corpus binding and rejects `doc_id` values not registered in `corpus_documents` for backend kind `legal`.
- Tabular rows and cells expose `delete_for_review(review_id)` helpers so corpus-aware deletion can remove review materialized data symmetrically.

Current code constraints:

- The current `tabular_reviews` Lance schema has `scope_folder`, but no `corpus_id`.
- `tabular_rows` has `folder_path`, but MCP writes `folder_path: None` in `review_add_rows`.
- The tabular chunk adapter reads chunks by global `doc_id`; it must validate the review corpus before calling `Pipeline::chunks_by_doc`, or expose a corpus-aware chunk source.
- Lance table schemas are append-only by convention in this crate, so add `corpus_id` fields at the end of schemas and preserve decode compatibility for older rows.

### Forget

The corpus registry must make deletion symmetric. Removing a corpus removes:

- the knowledge source and its FTS rows;
- legal chunks whose source path is inside the corpus root;
- legal enrichment rows for those document ids;
- legal graph nodes and edges for those document ids;
- tabular reviews, rows, columns, and cells scoped to the corpus.

Implementation baseline:

- `forget(target)` treats UUID targets as possible `corpus_id` values before the legacy knowledge-source UUID path.
- `CorpusStore::bindings_for_corpus` drives backend deletion buckets.
- `CorpusStore::delete_corpus_registry_rows` removes registry rows after backend cleanup.
- The forget response reports `knowledge_objects`, `legal_chunks`, and `tabular_reviews` buckets even when a bucket removes zero rows.

## Baseline Fixes

### Bundled Pdfium

The workspace dependency on `kreuzberg` must include `bundled-pdfium` so local Claude Desktop builds do not require a system `pdfium.dll`. This fixes the observed PDF extraction failure where `index(profile=all)` returned `ok:false` because PDF files failed to bind Pdfium.

Verification:

- `cargo tree -p anno-rag-bin -e features --prefix none` shows the `bundled-pdfium` feature.
- `cargo build --profile dev-fast -p anno-rag-bin` succeeds.
- MCP `index(path=piighost-test-multi-format, profile=all)` returns `ok:true` and `failed:0`.

### SQLite Case-File Extraction

`legal_extract_case_file` currently uses raw `kg.cypher(...)`, which fails on the SQLite graph backend. Replace raw Cypher calls with typed graph methods:

- `case_file_documents(dossier_id)`
- `case_file_parties(dossier_id)`
- `case_file_events(dossier_id)`

Implement these methods in the SQLite graph backend using parameterized SQL. The MCP API remains `legal_extract_case_file(dossier_id)`.

Verification:

- Unknown dossier returns an empty case-file result, not a backend error.
- A synthetic fixture with documents, parties, and events returns expected rows.
- MCP `legal_extract_case_file` succeeds on the SQLite backend.

## Testing Plan

### Unit Tests

- Corpus id generation is stable for the same root path.
- Corpus registry migrations create the expected control tables.
- Corpus registry rejects overlapping roots by default.
- Corpus bindings map one public `corpus_id` to knowledge, legal, and tabular backend ids.
- Byte-identical documents in two corpora produce distinct document instance ids and shared or equal content ids.
- Knowledge FTS filtering restricts by selected source/corpus.
- Legal folder subtree filtering returns only documents inside the selected corpus.
- Tabular `review_add_rows` rejects doc ids outside the review corpus.
- `forget(corpus_id)` removes knowledge, legal, and tabular data for a corpus indexed with `profile=all`.
- SQLite case-file graph methods return empty rows for unknown dossiers.
- SQLite case-file graph methods return expected rows for a synthetic dossier.

### MCP Integration Tests

- Index corpus A and corpus B.
- Search without `corpus_id` when A and B exist returns a refusal.
- Search with `corpus_id=A` returns no B hits.
- Search with `corpus_id=B` returns no A hits.
- Search hits include `corpus_id` and do not include raw absolute paths.
- Search with `allow_cross_corpus=true` can return both A and B.
- Corpus A and corpus B can contain the same byte-identical document; both corpora still return corpus-specific hits and `forget(corpus_id=A)` does not remove B's instance.
- `review_create(corpus_id=A)` then `review_add_rows` with a B document returns a clear error.
- `forget(corpus_id=A)` removes A from knowledge, legal, and tabular indexes while B remains searchable.
- Deprecated `knowledge_search` and `legal_search` obey the same guard.
- `index(profile=all)` on the PDF fixture returns `ok:true`, `failed:0`.
- `legal_extract_case_file` does not fail with the SQLite Cypher error.

## Rollout

1. Land baseline fixes: bundled Pdfium and SQLite case-file extraction.
2. Add `anno-corpus-core` and `anno-corpus-store` to the workspace.
3. Implement corpus registry migrations, stable id generation, overlap detection, and MCP `corpus_list`/`corpus_get`/`corpus_health`.
4. Introduce corpus-qualified document instance ids in the legal ingestion path while preserving content ids for dedupe/change detection.
5. Wire `index` to create corpus rows and backend bindings before and after ingestion.
6. Backfill the registry from existing knowledge sources and legal folder roots on startup or via a one-shot migration command.
7. Add `EffectiveCorpus` resolution and refusal behavior to the MCP router.
8. Add corpus filtering to knowledge and legal searches.
9. Scope tabular reviews and row additions.
10. Make `forget` corpus-aware and symmetric across knowledge, legal, and tabular data.
11. Run sustained MCP tests across two synthetic corpora and the existing multi-format fixture.

## Acceptance Criteria

- Multiple client folders can be indexed concurrently.
- Corpus architecture is implemented through shared corpus crates and a dedicated control store, not tool-local ad hoc filters.
- Byte-identical files in different corpora remain distinct document instances.
- With multiple corpora present, sensitive tools refuse unscoped calls.
- A selected `corpus_id` limits knowledge search, legal search, legal extraction, and tabular review operations to that corpus.
- Cross-corpus search requires `allow_cross_corpus: true`.
- `forget(corpus_id)` removes knowledge, legal, and tabular data for that corpus.
- PDF ingestion works in local Claude Desktop builds without a system Pdfium installation.
- `legal_extract_case_file` works on the SQLite graph backend.
- Raw filesystem paths are not returned by the MCP corpus or search responses.
