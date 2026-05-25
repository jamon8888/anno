# Anno Legal Graph v0 - Typed Graph Foundation

Date: 2026-05-25

## Context

The existing legal RAG plan correctly identifies a graph as valuable for French legal workflows: lawyers need to connect parties, documents, obligations, events, citations, courts, and review artifacts across a dossier. The current implementation already contains the right public shape:

- `LegalKnowledgeGraph` trait
- `NodeWrite` / `EdgeWrite`
- ingest-time graph batch construction
- named legal graph intents
- MCP tools such as `legal_graph_query`, `legal_extract_contract`, and `legal_timeline`

However, the concrete `LanceGraphStore` is still a no-op. The `lance-graph` dependency was added ahead of a real backend and pulls a large, security-sensitive transitive tree. The plan also depends on a pre-1.0 graph API whose exact Rust surface is not stable in the plan itself.

This spec keeps the product value of the graph while reducing architectural risk.

## Product Goal

Build a legal graph foundation that helps lawyers answer relationship-heavy questions with explicit provenance:

- Who are the parties in this dossier, and what roles do they play?
- Who owes what to whom, from which source chunk, and with what evidence?
- What is the procedural or contractual timeline?
- Which documents cite or appeal which prior documents or legal articles?
- Which fields can be extracted into review tables with source-backed justification?

The first version should prioritize trust, provenance, and correction loops over generic graph-query power.

## Non-Goals

- No generic user-authored Cypher surface in v0.
- No hard dependency on `lance-graph` in v0.
- No attempt to infer complex legal reasoning without source-backed extracted facts.
- No hidden graph-only state that cannot be traced back to document chunks.

## Design Decision

Implement **Legal Graph v0** as a typed, intent-driven graph backend behind the existing `LegalKnowledgeGraph` trait.

The backend persists normalized nodes and edges, but exposes only named legal intents to the rest of the product. A future **Legal Graph v1** may replace the backend with `lance-graph` if the typed v0 proves valuable and the backend API/security profile is acceptable.

## Architecture

### Stable Boundary

Keep the current trait as the application boundary:

- `upsert_batch(nodes, edges)`
- `delete_doc(doc_id)`
- `link_cross_documents(doc_id, hints)`
- `compact()`
- `cypher(query, params)` only as a compatibility method for existing intent plumbing

For v0, `cypher` should not be a general query engine. It should dispatch only supported intent templates or be replaced internally by typed methods while keeping the MCP wire format stable.

### Recommended v0 Backend

Use a small local typed store instead of `lance-graph`.

Preferred implementation: SQLite sidecar under `cfg.index_path()/legal_kg.sqlite`.

Rationale:

- recursive CTEs are enough for appeal/citation chains;
- joins are simpler and more reliable than encoding graph traversal into LanceDB filters;
- `rusqlite` already exists in the workspace dependency graph;
- audit surface is much smaller than pulling `lance-graph` and its cloud/Delta/DataFusion chain;
- the database is local, inspectable, backup-friendly, and easy to migrate.

LanceDB remains the retrieval store for chunks and legal enrichment projections. SQLite stores graph topology and normalized relationship metadata.

### Data Model

`legal_nodes`

- `id TEXT PRIMARY KEY`
- `label TEXT NOT NULL`
- `doc_id TEXT NULL`
- `chunk_id TEXT NULL`
- `normalized_key TEXT NOT NULL`
- `display TEXT NULL`
- `props_json TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- unique index on `(label, normalized_key)`

`legal_edges`

- `id TEXT PRIMARY KEY`
- `from_label TEXT NOT NULL`
- `from_key TEXT NOT NULL`
- `to_label TEXT NOT NULL`
- `to_key TEXT NOT NULL`
- `edge_type TEXT NOT NULL`
- `doc_id TEXT NULL`
- `chunk_id TEXT NULL`
- `props_json TEXT NOT NULL`
- `created_at TEXT NOT NULL`
- unique index on `(from_label, from_key, edge_type, to_label, to_key, doc_id, chunk_id)`

Every edge that came from extraction should carry source provenance through `doc_id`, `chunk_id`, and optional byte offsets in `props_json`.

### Named Intents

v0 supports these typed intents:

- `party_dossier(party)`
- `obligations_owed_by(party)`
- `citation_chain(article_ref)`
- `procedural_timeline(dossier_id)`
- `appeal_chain(doc_id, max_depth)`

Each intent returns rows with enough information to fetch or rehydrate supporting chunks. The MCP interface may remain `legal_graph_query`, but internally the implementation should map intent names to typed query functions instead of accepting arbitrary Cypher.

### Ingest Flow

1. Extract and pseudonymize chunks.
2. Write chunks and legal enrichment projection to LanceDB.
3. Convert legal facts to `NodeWrite` and `EdgeWrite`.
4. Persist graph batch in SQLite inside one transaction.
5. Mark enrichment status `ok` only after table and graph writes both succeed.
6. If graph write fails, mark pending and allow backlog drain to retry.

The current dual-write scaffolding is useful and should remain.

### Correction Loop

Human validation should write `Validation` nodes or equivalent validation rows linked to the source chunk/fact. The graph should distinguish:

- extracted facts;
- confirmed facts;
- rejected facts;
- corrected facts.

Queries used for legal review should prefer confirmed/corrected facts when available.

## Migration Path to v1

`lance-graph` should become a separate backend only after v0 meets product criteria.

Activation criteria:

- graph intents produce useful results on real legal corpora;
- extraction precision is high enough that graph traversal does not amplify noise;
- legal users rely on cross-document relationships, not only search;
- a dependency audit of `lance-graph` is clean or explicitly accepted;
- API spike proves batch upsert, delete-by-document, and constrained traversal are stable.

If adopted, `lance-graph` should implement the same trait and pass the same conformance tests as the SQLite backend.

## Testing

Required tests:

- unit tests for deterministic node/edge normalization;
- conformance tests for every `LegalKnowledgeGraph` backend;
- ingest dual-write test verifying graph rows are persisted;
- typed intent tests with synthetic dossiers;
- retry test for pending graph writes;
- deletion test ensuring `delete_doc` removes document-scoped graph state;
- privacy test ensuring graph rows store pseudonymized or normalized references, never raw PII unless explicitly protected.

The existing no-op backend is not acceptable as the only test target for graph behavior.

## Success Criteria

- `legal_graph_query` returns non-empty rows for a synthetic seeded dossier.
- `legal_extract_contract` can materialize parties and obligations with chunk provenance.
- `legal_timeline` can return ordered events for a dossier.
- A failed graph write is visible in `enrichment_status` and can be retried.
- No new high-risk dependency tree is introduced for v0.
- The product can later swap SQLite for `lance-graph` without changing MCP tool contracts.

## Risks

The main product risk is bad extraction quality. A graph built from noisy facts creates confident-looking wrong relationships. Mitigation: preserve provenance, expose confidence, and support validation/correction.

The main architecture risk is overfitting v0 queries to SQLite. Mitigation: keep a backend conformance test suite and avoid leaking SQL details beyond the backend module.

The main roadmap risk is building a graph before legal users need it. Mitigation: ship only the five named intents first and measure usage before adding generic traversal.

## Recommendation

Keep the legal graph plan, but revise its implementation sequence:

1. Ship Legal Graph v0 with a typed local backend and named intents.
2. Remove or avoid `lance-graph` until a dedicated backend PR.
3. Treat `lance-graph` as an optional v1 backend, not as the foundation required for the first useful legal graph.
