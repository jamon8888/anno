# MCP Client Corpus Scoping Design

## Summary

Anno MCP must support indexing multiple client folders while preventing Claude Desktop from searching, extracting, or tabular-reviewing across client boundaries by accident. The current MCP can index multiple folders, but search and review tools operate over global indexes unless the caller manually constrains them with business filters. This design introduces an explicit `corpus_id` model for local client-folder scope and fixes two baseline issues discovered during MCP testing: bundled Pdfium support for PDF ingestion and raw Cypher use in `legal_extract_case_file`.

## Goals

- Treat the root folder passed to `index(path=...)` as one client corpus.
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

## Current Behavior

`index(path, profile)` registers the folder as a knowledge source and, for `legal` or `all`, ingests legal chunks into the global LanceDB-backed legal store. `sources()` can list pseudonymized source entries, and `forget(target)` can remove a source by id, legal folder id, or path.

Search is not client-corpus scoped:

- `search.scope` means `all`, `knowledge`, or `legal`; it does not mean client folder.
- `knowledge_search` uses SQLite FTS over `knowledge_objects_fts` without a `source_id` or `scope_id` filter.
- `legal_search` filters by legal metadata such as `dossier_id`, parties, clause types, and risk flags, but current ingestion does not set a client-folder corpus filter.
- `review_create.scope_folder` is informational, and `review_add_rows` currently stores tabular rows with `folder_path: None`.

This allows an indexed folder A and indexed folder B to both participate in later searches unless the tool implementation applies a corpus filter.

## Proposed Model

### Corpus

A corpus is the root local folder passed to `index(path=...)`. A stable `corpus_id` is derived from that root path using the existing stable-id pattern, and it maps to:

- the knowledge source id for the local folder;
- the legal folder root used to ingest LanceDB chunks;
- the document ids produced by legal ingestion;
- tabular reviews created for that corpus.

MCP responses expose `corpus_id` and pseudonymous labels only. They do not expose raw filesystem paths.

### Default Safety Rule

If zero corpus entries exist, search and review setup tools return a clear error: index a folder first.

If exactly one corpus exists, `corpus_id` may be omitted and the MCP infers the single corpus.

If two or more corpus entries exist, sensitive tools require `corpus_id`. Without it, the MCP refuses the call. Cross-corpus search is allowed only when the caller passes `allow_cross_corpus: true`.

## MCP Surface

### New Tools

`corpus_list`

- Lists indexed corpora.
- Returns `corpus_id`, pseudonymous label, indexed profiles, object counts, legal chunk counts, and health status.

`corpus_get`

- Returns details for one corpus.
- Includes knowledge and legal counters plus last indexing status.

### Modified Tools

`index(path, profile)`

- Returns `corpus_id` in addition to the existing knowledge and legal summaries.
- Re-indexing the same root returns the same `corpus_id`.

`search(query, top_k, mode, scope, filters, corpus_id, allow_cross_corpus=false)`

- Applies corpus filtering to knowledge and legal branches.
- Refuses without `corpus_id` when multiple corpora exist unless `allow_cross_corpus` is true.
- Includes `corpus_id` in every hit for auditability.

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

Deprecated legal and knowledge aliases must share the same guard logic as their replacement tools.

## Data Changes

### Knowledge

Extend `KnowledgeSearchRequest` with optional `source_id` or `corpus_id` filtering. The SQLite FTS query should join or filter through `knowledge_objects` so results are restricted to the selected source/scope. Hits should include `corpus_id` or `source_id` in the wire response.

### Legal

Persist the mapping between `corpus_id` and legal folder root. Legal search should be able to restrict candidate chunks to documents whose `source_path` is inside that root. The existing store already has subtree helpers for legal folder paths; the implementation should reuse them instead of exposing raw paths in MCP.

For better future dossier workflows, legal ingestion should also stamp a corpus grouping key into legal enrichment metadata or graph document nodes. This key can coexist with business `dossier_id`.

### Tabular

Add or use an existing review field for authoritative corpus scope. Fill row scope metadata when adding documents. Any tabular operation that traverses source documents must validate against the review corpus.

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
- Knowledge FTS filtering restricts by selected source/corpus.
- Legal folder subtree filtering returns only documents inside the selected corpus.
- Tabular `review_add_rows` rejects doc ids outside the review corpus.
- SQLite case-file graph methods return empty rows for unknown dossiers.
- SQLite case-file graph methods return expected rows for a synthetic dossier.

### MCP Integration Tests

- Index corpus A and corpus B.
- Search without `corpus_id` when A and B exist returns a refusal.
- Search with `corpus_id=A` returns no B hits.
- Search with `corpus_id=B` returns no A hits.
- Search with `allow_cross_corpus=true` can return both A and B.
- `review_create(corpus_id=A)` then `review_add_rows` with a B document returns a clear error.
- Deprecated `knowledge_search` and `legal_search` obey the same guard.
- `index(profile=all)` on the PDF fixture returns `ok:true`, `failed:0`.
- `legal_extract_case_file` does not fail with the SQLite Cypher error.

## Rollout

1. Land baseline fixes: bundled Pdfium and SQLite case-file extraction.
2. Add corpus registry and expose `corpus_list`/`corpus_get`.
3. Add corpus filtering and guard behavior to search tools.
4. Scope tabular reviews and row additions.
5. Run sustained MCP tests across two synthetic corpora and the existing multi-format fixture.

## Acceptance Criteria

- Multiple client folders can be indexed concurrently.
- With multiple corpora present, sensitive tools refuse unscoped calls.
- A selected `corpus_id` limits knowledge search, legal search, legal extraction, and tabular review operations to that corpus.
- Cross-corpus search requires `allow_cross_corpus: true`.
- PDF ingestion works in local Claude Desktop builds without a system Pdfium installation.
- `legal_extract_case_file` works on the SQLite graph backend.
- Raw filesystem paths are not returned by the MCP corpus or search responses.
