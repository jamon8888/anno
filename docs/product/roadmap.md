# Anno Knowledge Service — Roadmap

> Living document. Captures items deferred across the Phase 1 → Phase 3+ designs
> and tracks them in one place so backlog doesn't get lost in spec footers.
> Updated 2026-06-02.

This roadmap covers the **knowledge service** trajectory specifically. It does
not duplicate the BACKLOG.md product backlog or the release plan; it focuses on
what each phase shipped, what's next, and what's parked.

---

## Status snapshot

| Phase | Status | Spec | Plan |
|------|--------|------|------|
| Phase 1 — Foundation (core types, store, 3 MCP tools) | ✅ Shipped (`87aeff2b`) | [multisource design §23 Ph1-2](../superpowers/specs/2026-05-29-anno-local-knowledge-service-multisource-design.md) | [phase 1 plan](../superpowers/plans/2026-05-29-anno-local-knowledge-service-phase1.md) |
| Phase 2 — Local folder source + privacy API | ✅ Shipped (`87aeff2b`) | [local folder source design](../superpowers/specs/2026-06-01-anno-knowledge-local-folder-source-phase2-design.md) | [phase 2 plan](../superpowers/plans/2026-06-01-anno-knowledge-local-folder-source-phase2.md) |
| Phase 2.5 — MCP surface consolidation | 🔵 Brainstormed | — | — |
| Phase 3 — Vector projection + semantic search | 📋 Spec'd, plan ready | [vector projection design](../superpowers/specs/2026-06-02-anno-knowledge-vector-projection-phase3-design.md) | [phase 3 plan](../superpowers/plans/2026-06-02-anno-knowledge-vector-projection-phase3.md) |
| Phase 4 — Outlook / Microsoft Graph connector | 🟡 Not started | [parent §18](../superpowers/specs/2026-05-29-anno-local-knowledge-service-multisource-design.md) | — |
| Phase 5+ — Additional sources | 🟡 Not started | — | — |

Legend: ✅ shipped · 🔵 design in progress · 📋 ready to execute · 🟡 backlog

---

## Near-term (next 1-3 months)

### Phase 2.5 — MCP Surface Consolidation

Façade-only unification of the indexing and search surface. Adds 5 new tools
(`index`, `search`, `sources`, `status`, `forget`) that route to the existing
implementations; 9 legacy tools (`legal_ingest`, `knowledge_search`,
`knowledge_sync`, etc.) stay functional with deprecated descriptions.

**Why this comes before Phase 3:** without consolidation, semantic search
becomes a 5th search surface; users have to choose between `search`,
`legal_search`, `knowledge_search`, and the future `knowledge_search(semantic)`.
Consolidating first means Phase 3 adds *capability* to `search`, not another
tool.

**Scope:** ~300-400 LOC of routing in `anno-rag-mcp/src/lib.rs`, plus tests.
No moteur changes. Cost: 1-2 weeks.

### Phase 3 — Vector Projection + Semantic Search

LanceDB `knowledge_chunks_v1` populated by an extended `knowledge_sync`. Adds
`Pipeline::embed_pseudonymized_chunks` + `pseudonymize_query`, RRF fusion
(k=60) over SQLite FTS + LanceDB cosine, mandatory query pseudonymization
before embedding. State machine extends with `vector_pending` / `vector_ready`.

**Plan ready to execute.** Branches naturally onto the unified `search` surface
from Phase 2.5: `search(query, mode="semantic")` becomes the new capability.

---

## Medium-term (3-6 months)

### Phase 4 — Outlook / Microsoft Graph connector

First external source. New `anno-source-microsoft` crate, PKCE OAuth +
keyring token storage, Graph delta sync (Inbox + Sent initially), email body
normalization through Kreuzberg. Validates the multi-source architecture
beyond local folders.

**Critical design decisions still open:**
- ACL projection (the `acl_hash` column placeholder from Phase 3 §6 gets
  populated here)
- Email-specific budgets (max messages per run, attachment policy)
- Token refresh / revocation handling

### Idempotence of `index(profile="legal")`

Phase 2.5 documents that the `legal` profile re-runs enrichment on each call
(matches current `legal_ingest` behavior). Making it idempotent requires either:
- Content-hash based skip in `Pipeline::ingest_one_counted` (same pattern as
  Phase 2 used for knowledge), or
- A `legal_objects` table tracking which `doc_id`s have been enriched at which
  version

Either approach is a contained refactor inside `anno-rag`. ~1 week.

### Deletion reconciliation (the `forgotten` counter)

Phase 2 indexer tracks `summary.forgotten` but always reports 0 — the
deletion detection logic was deferred. Adds:
- `KnowledgeControlStore::objects_under_scope(scope_id) -> Vec<(ObjectId, external_id)>`
- `KnowledgeControlStore::forget_object(object_id)` (per-object cousin of
  `forget_scope`)
- A diff pass in `sync_local_scope` (only on non-truncated walks) that
  forgets objects whose external_id is no longer present on disk

Out-of-scope at Phase 2 because the priority was end-to-end ingestion. Small
slice once revisited.

### Deep mode reranker

Phase 3 ships `Deep` as an alias for `Semantic`. A true `Deep` mode would
add a local cross-encoder reranker over the RRF-merged candidate set. New
model to download (~300 MiB typical), opt-in via config to avoid surprise
disk usage.

---

## Long-term / 6 months+

### Internal pipeline unification

Phase 2.5 unified the *MCP surface* but kept two ingestion pipelines internally.
The long-term goal (parent spec §17 "compatibility enhancement") is to make
`Pipeline::ingest_folder` a thin shim that:

1. Calls `KnowledgeIndexer::sync_local_scope` (knowledge becomes the primary
   write path)
2. Runs a separate "legal enrichment pass" on the newly written knowledge
   objects, projecting selected fields into a `legal_objects` projection table
3. `legal_search` queries the new projection table; `Store::chunks` becomes
   read-only and eventually retired

**Why this is long-term:** the parent spec flags `Store` as CRITICAL impact
(78 symbols, 31 direct deps). Doing this safely requires:
- Confidence the knowledge plan handles legal workflows fully (Phase 3+4
  validate this)
- Migration tooling to backfill `legal_objects` from existing `chunks`
- A staged rollout (dual-write → read-from-projection → retire `chunks`)

Realistic estimate: a 2-3 month effort, only justified once the multi-source
architecture is proven.

### Removal of deprecated MCP tools

Phase 2.5 leaves 9 tools deprecated-but-functional. Removal becomes a future
phase once usage tracking (via `anno_health` analytics or a server-side log)
confirms no clients still call them. Practical sequence:
1. Phase 2.5 ships → deprecation banners visible
2. 2-3 months of organic migration
3. Audit of inbound tool calls
4. Cleanup PR removes the deprecated handlers and their tests

### `Auto` search mode

Phase 3 explicitly does *not* ship `Auto`. The parent spec §15 describes it:
"Fast when models are absent or index lag is high. Semantic when vectors are
ready and embedder is warm or acceptable to load." Worth doing once we have
production telemetry on what users actually pick.

### Re-embed pipeline on model change

Phase 3 records `embedding_model` + `embedding_fingerprint` on every vector
row to *detect* stale vectors when the embedding model changes. The *action*
on detection — re-embedding the affected chunks — is deferred. Triggered by:
- A new embedder model becoming default (e.g. moving from
  `multilingual-e5-small` to a larger or newer one)
- A user-initiated `reindex_vectors` MCP tool (would belong to this phase)

### Daemon split (`anno-rag daemon`)

Parent spec §5 mentions this as Phase 7. Justified only if duplicate Bert
instances across Claude Desktop + Codex become a measurable RAM problem in
practice. Not committed; revisit if customer telemetry surfaces the issue.

### Filter pushdown in `search_semantic`

The Phase 3 schema for `knowledge_chunks_v1` carries `source_id`,
`account_id`, `scope_id`, `source_kind`, `object_type` precisely so future
filters can push down to LanceDB without a SQLite round-trip. The API surface
(adding `filters` recognition in `search_semantic`) waits for a real query
use case to avoid premature design.

### Background job queue

Sync is synchronous bounded today (S1 — chosen explicitly in Phase 2 / Phase 3
brainstorms). Splitting into source-sync worker + privacy worker + vector
worker would be needed if:
- A user starts a sync on a large corpus and wants to monitor / cancel mid-run
- Multiple sources need to sync concurrently with shared model budget

Adds queue table + worker lifecycle + claim/retry semantics. Material refactor
of `KnowledgeIndexer`. Deferred until S1 is shown to be insufficient.

---

## Not committed (acknowledged-only)

These items appeared in design discussions but are *not* on any phase
trajectory. They live here so the team doesn't re-discover them as "missing":

- **ACL/permissions in search results** — the `acl_hash` column placeholder
  in `knowledge_chunks_v1` (Phase 3 §6 note) was designed for Outlook tenant
  ACLs. Local folders don't need it. SharePoint/Drive eventually will.
- **Multi-user / multi-tenant deployment** — Anno is local-first single-user
  today. Server deployment changes the privacy model fundamentally; out of
  scope until product direction shifts.
- **Streaming embedding for very large files** — single files >10MB pseudo
  text would benefit from streaming through the embedder rather than batch.
  Mechanically possible; YAGNI today.
- **Custom embedder per source** — different source types could use
  different embedders (e.g. code embeddings for source code, e5 for docs).
  Architecturally feasible (per-row `embedding_model` already in schema);
  no demand signal yet.
- **`knowledge_open(object_id)`** — Phase 2 design mentioned this; not in
  any plan. Adds a "get full snippet context" tool. Snippets in search
  results have proven sufficient so far.
- **Tabular review integration into `index`** — `anno-rag-tabular` has its
  own storage and workflow (Harvey/Legora style). Folding it into `index`
  as `profile="tabular"` is possible but mixes very different abstractions
  (RAG search vs structured extraction). Probably stays separate.

---

## How to update this document

When closing a phase:
- Move it from "Near-term" / "Medium-term" to the status snapshot table with
  a ✅ and the merge commit
- Move newly-deferred items from that phase's spec into the appropriate
  section here (with a short justification)

When a "Not committed" item gets a demand signal:
- Promote it to Medium-term or Long-term with a brief scoping note
- Optionally open a brainstorm/spec for the next phase

When a Long-term item gets executed:
- Open its own spec under `docs/superpowers/specs/`
- Cross-reference from this roadmap → spec
- Update status when the spec is plan-ready or shipped
