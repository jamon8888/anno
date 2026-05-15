# anno-memory v0.1 — PII-safe Persistent Memory for Cowork Sessions

**Status:** Design — pending user review
**Date:** 2026-05-15
**Scope:** `anno-rag` crate only (new `memory` module + MCP tools). No changes to `anno-privacy-gateway`.

## 1. Motivation

Claude Cowork sessions running behind `anno-privacy-gateway` are currently
amnesiac: each conversation starts cold. Users in notarial / juridique workflows
repeat the same context every session ("le dossier Dupont, vente Bordeaux, acte
authentique du 12 mars"), which (a) wastes tokens, (b) bypasses the gateway's
PII vault on every re-introduction (re-tokenizing the same names with new
tokens), and (c) gives the platform no way to surface "we already discussed
this clause yesterday."

The Rust ecosystem already has memory servers worth studying — `ourmem/omem`
(LanceDB + MCP), `subcog` (hexagonal SQLite + FTS5 + usearch), `project-rag`
(LanceDB + FastEmbed), `gemini-memory` (LanceDB crate). All four ignore the
hard problem for our market: **PII must never sit in the memory store in
plaintext.** Bolt-on redaction (subcog's optional path) is not GDPR-defensible
in a notarial context.

anno-rag already ships every primitive needed to do this *correctly*: a
36-label PII detector (`detect.rs`), a reversible cloakpipe vault
(`vault.rs`), an embedded LanceDB store (`store.rs`), FastEmbed embeddings
(`embed.rs`), and an rmcp-based MCP server (`mcp.rs`). v0.6 is adding hybrid
retrieval (Tantivy FTS + dense + RRF). v0.1 of memory is a thin module on top
of that stack — not a new crate, not a new server, not a new vault.

## 2. Scope

**In scope (v0.1):**

- A new `memory` module in `anno-rag` with a `Memory` data type and a
  `MemoryStore` over a second LanceDB collection (`memories`) in the same
  store as `documents`.
- Three new MCP tools on the existing `AnnoRagServer`: `memory_save`,
  `memory_recall`, `memory_forget`. (Plus `memory_list` for inspection.)
- PII-tokenized storage: every memory is pseudonymized through the existing
  vault *before* it hits LanceDB. Recall detokenizes on the fly using the
  same vault.
- Per-tenant isolation reusing the existing `AnnoRagConfig` collection /
  vault-path primitives. One tenant = one vault + one (`documents`,
  `memories`) pair.
- Right-to-erasure: `memory_forget` removes the row *and* cascades to vault
  tokens no longer referenced by any other memory or document.
- A structured `tracing` event emitted on every memory tool call (tool
  name, tenant, no payload). Any log aggregator — including a future
  gateway audit pipeline — can pick it up. **No code changes to
  `anno-privacy-gateway` in v0.1.**

**Out of scope (deferred to v0.2+):**

- Passive capture — gateway watching the stream and auto-saving "facts."
  Explicit MCP calls only in v0.1.
- LLM-driven memory distillation / summarization.
- Cross-tenant "Spaces" (the ourmem feature).
- Time-decay / scoring beyond explicit forget.
- Memory editing (no `memory_update` — forget+save instead).
- Embedding the `kind` taxonomy into the retrieval signal. v0.1 stores
  `kind` as metadata only; retrieval is content-based.
- Conflict / contradiction detection across stored memories.

**Carried constraints:**

- The v0.5 peak-RSS cap of **< 1.5 GB** for `anno-rag` still holds. The
  second LanceDB collection adds negligible RSS; the Tantivy index for
  memories piggybacks on the v0.6 FTS infrastructure.
- The gateway remains a **pure proxy**. No stateful logic added to
  `anno-privacy-gateway`. Memory state lives entirely in the `anno-rag` MCP
  process.

## 3. Architecture

### 3.1 Pipeline placement

```
                ┌──────────────────┐
Cowork ───────► │ privacy-gateway  │ ────► Anthropic upstream
                │  (stateless)     │
                └──────────────────┘
   │                                     │
   │ MCP (stdio)                         │ HTTP
   ▼                                     ▼
┌──────────────────────────┐
│ anno-rag MCP server      │
│  ┌────────────────────┐  │
│  │  tools (existing)  │  │  search / rehydrate / detect / vault_stats
│  ├────────────────────┤  │
│  │  tools (NEW v0.1)  │  │  memory_save / memory_recall /
│  │                    │  │  memory_forget / memory_list
│  └────────────────────┘  │
│           │              │
│           ▼              │
│  ┌────────────────────┐  │
│  │   Pipeline         │  │  (existing)
│  │   ├ detect         │  │
│  │   ├ vault          │  │   ← reused
│  │   ├ embed          │  │   ← reused
│  │   └ store          │  │   ← reused, second collection added
│  └────────────────────┘  │
└──────────────────────────┘
```

Cowork connects to **two** MCPs: the privacy gateway (over Anthropic HTTP)
and the anno-rag MCP (over stdio). Memory is a tool surface on the latter.
The gateway never sees the memory store and never mutates it.

### 3.2 Three-layer model (per subcog)

Inside the `memory` module, three responsibilities are kept separate so
v0.2's passive-capture path can swap out the call site without touching
internals:

- **Persistence:** LanceDB collection `memories`. ACID-ish via LanceDB's
  versioned writes. Source of truth.
- **Vector:** dense embedding stored as a column on the persistence row
  (no separate usearch index — LanceDB already does dense KNN).
- **Lexical:** LanceDB native FTS index on the tokenized text
  (`Index::FTS(FtsIndexBuilder::default())`), built reusing the v0.6
  hybrid-retrieval helpers (`store::build_fts_index`,
  `store::hybrid_search`). Memories use the same `lancedb::rerankers::RRFReranker`
  as documents. The index keeps in sync with the data automatically — no
  separate lifecycle to manage. Standalone Tantivy is **not** used; upstream
  LanceDB is removing Tantivy support (lancedb/lancedb#2998) and exposing a
  native inverted index instead.

This matches subcog's hexagonal split *logically* but uses one storage
engine (LanceDB) instead of three (SQLite + SQLite-FTS + usearch). That is
deliberate — we already pay the LanceDB cost; adding two more engines
buys nothing.

### 3.3 Data flow — `memory_save`

```
caller text (may contain PII)
  └─ detect()                        → DetectedEntity[]
  └─ vault.pseudonymize()             → tokenized text + token refs
  └─ embed(tokenized)                 → Vec<f32>  (e5 passage prefix)
  └─ MemoryStore::insert(row)         → LanceDB append
  └─ Tantivy::add_document(tokenized) → FTS index update
  └─ return MemoryId
```

The plaintext is never persisted. Only the tokenized form goes to LanceDB
and Tantivy. PII material lives exclusively in the vault.

### 3.4 Data flow — `memory_recall`

```
caller query (may contain PII)
  └─ detect()                          → DetectedEntity[]
  └─ vault.pseudonymize()              → tokenized query + token refs
  └─ hybrid_search(tokenized, top_k)   → dense KNN + FTS, RRF reranked
  └─ for each hit: vault.rehydrate()   → plaintext memory
  └─ return MemoryHit[]
```

Rehydration uses the *caller's tenant's* vault, so a memory written with
`PERSON_a4f3 = "Sophie Wilson"` only ever resolves back to "Sophie Wilson"
inside that tenant.

## 4. Data model

```rust
/// One stored memory.
pub struct Memory {
    pub id: MemoryId,                    // UUIDv7 (time-ordered)
    pub session_id: Option<String>,      // None = cross-session
    pub kind: MemoryKind,                // metadata only in v0.1
    pub text: String,                    // PII-tokenized (e.g. "Le dossier PERSON_a4f3 …")
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,      // updated on recall hit
    pub token_refs: Vec<TokenRef>,       // vault tokens referenced by this row
}

pub enum MemoryKind {
    Fact,         // "le dossier Dupont concerne une vente Bordeaux"
    Preference,   // "préférer les actes en format A4 portrait"
    Reference,    // "voir clause 4.2 de l'acte du 12 mars"
    Context,      // free-form session context
}

pub struct TokenRef {
    pub label: String,                   // one of the 36 PII labels
    pub token: String,                   // e.g. "PERSON_a4f3"
}
```

`MemoryId` is UUIDv7 so list/page is naturally time-ordered without a
secondary index. `token_refs` is what makes GDPR erasure tractable —
`memory_forget` consults it to decide which vault tokens are now orphaned
and can be purged.

LanceDB schema (Arrow):

| column         | type                | notes                          |
|----------------|---------------------|--------------------------------|
| `id`           | `string`            | UUIDv7, primary lookup key     |
| `session_id`   | `string (nullable)` | for session-scoped filtering   |
| `kind`         | `string`            | `MemoryKind` discriminant      |
| `text`         | `string`            | tokenized text                 |
| `created_at`   | `timestamp[us]`     |                                |
| `accessed_at`  | `timestamp[us]`     | updated on recall hit          |
| `valid_from`   | `timestamp[us]`     | **forward-compat (v0.2):** populated as `created_at` in v0.1 |
| `valid_to`     | `timestamp[us] (nullable)` | **forward-compat (v0.2):** always null in v0.1 |
| `embedding`    | `fixed_size_list<f32, 384>` | e5-small dim, matches `documents` |
| `token_refs`   | `list<struct<label:string, token:string>>` | for forget cascade |
| `entity_refs`  | `list<string>`      | **forward-compat (v0.2):** always empty list in v0.1; populated by v0.2 entity extraction |

### 4.1 Scalar indexes (mandatory)

Without these, `memory_list` pagination and forget-cascade scans degrade
to full-table reads. All three are cheap and ship with v0.1:

| column        | index type   | purpose                                   |
|---------------|--------------|-------------------------------------------|
| `created_at`  | `BTree`      | time-ordered pagination, range filters    |
| `kind`        | `Bitmap`     | low-cardinality (4 values) filter         |
| `token_refs`  | `LabelList`  | `array_contains_*` for forget cascade     |
| `session_id`  | `BTree`      | session-scoped recall filter              |

`entity_refs` is also indexed `LabelList` even though it is always empty
in v0.1 — adding the index now means v0.2 can populate the column
without an index-rebuild step.

### 4.2 Forward-compat columns: why now

LanceDB's `Table::add_columns` is metadata-only for all-null additions,
so technically we could defer `valid_to` and `entity_refs` to v0.2 free
of cost. We add them now anyway for two reasons:

1. The schema is the contract. Downstream consumers (Cowork, eval
   tooling, an audit log) can pin against the v0.1 schema and remain
   compatible with v0.2 without re-binding.
2. Populating `valid_from = created_at` from row 1 means v0.2's
   temporal queries have a clean monotonically-increasing column from
   the very first memory, with no "before/after the migration"
   discontinuity.

`token_refs` and `entity_refs` are deliberately separate columns even
though both store "things this memory refers to." `token_refs` is the
**vault-cascade primitive** (cleanup of PII material on forget) and is
strictly tied to the 36 PII labels. `entity_refs` is the **graph
primitive** (multi-hop traversal in v0.2) and includes non-PII entities
(legal concepts, clause references, organisations not flagged as PII).
Conflating them would make v0.2 either drop graph nodes that happen to
be PII, or leak non-PII entities into the cascade logic.

## 5. MCP tools

All four tools live on the existing `AnnoRagServer` in `mcp.rs` and reuse
its `Pipeline` handle. Tool names use `snake_case` (rmcp convention,
matching existing tools `search` / `rehydrate`).

### 5.1 `memory_save`

```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemorySaveParams {
    /// Free-form text. May contain PII — will be pseudonymized through the vault.
    pub text: String,
    /// Memory kind. Defaults to Context.
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    /// Session scope. If None, the memory is cross-session.
    #[serde(default)]
    pub session_id: Option<String>,
}

pub struct MemorySaveResult {
    pub id: String,
    pub redacted_text: String,    // shown back to caller so they see what was stored
    pub token_count: usize,
}
```

### 5.2 `memory_recall`

```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryRecallParams {
    /// Query text.
    pub query: String,
    /// Max results.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Filter: only return memories with this session_id, plus all cross-session ones.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Filter: only return memories of these kinds.
    #[serde(default)]
    pub kinds: Option<Vec<MemoryKind>>,
}

pub struct MemoryHit {
    pub id: String,
    pub text: String,             // rehydrated plaintext
    pub kind: MemoryKind,
    pub created_at: String,
    pub score: f32,               // RRF score
}
```

### 5.3 `memory_forget`

```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryForgetParams {
    /// Either an explicit id, or a query to match against. Exactly one must be set.
    pub id: Option<String>,
    pub query: Option<String>,
    /// If `query` is set, forget at most this many top-scoring matches.
    #[serde(default = "default_forget_limit")]
    pub limit: usize,
    /// Dry run: report what *would* be forgotten without mutating.
    #[serde(default)]
    pub dry_run: bool,
}

pub struct MemoryForgetResult {
    pub forgotten_ids: Vec<String>,
    pub vault_tokens_purged: usize,    // tokens no longer referenced
}
```

Cascade logic: for each forgotten memory, walk its `token_refs`; for each
token, ask the store "is this token referenced by any other memory OR any
document row?" If no → drop it from the vault. The "any document row"
check requires extending `cloakpipe_core::Vault` with a reference-count
API, or doing the check at the LanceDB layer via filter scans. v0.1
chooses the LanceDB filter-scan approach to avoid touching cloakpipe.

### 5.4 `memory_list`

Pagination + filter, mostly for the admin CLI and tests. Returns metadata
only by default (no rehydrated text) to keep MCP responses small.

## 6. Privacy model

The differentiator vs ourmem/subcog/project-rag.

1. **Tokenize before persist.** `memory_save` runs the same
   `detect → pseudonymize` pipeline as document ingest. Plaintext PII
   never touches disk in the memory collection.
2. **Vault is per-tenant.** Reuses `AnnoRagConfig`'s vault path. A memory
   tokenized in tenant A cannot be rehydrated by tenant B — token strings
   collide harmlessly because each vault has independent token maps.
3. **Forget cascades.** GDPR Art. 17 erasure is real: `memory_forget`
   removes the row, then purges vault entries that no other row
   references. A user asking "forget Dupont" gets the row gone AND the
   vault token PERSON_a4f3 → "Dupont" gone (assuming no document also
   references it).
4. **Audit trail.** Gateway logs every memory MCP call event (timestamp,
   tool name, tenant) without seeing payload. This gives a "you can see
   every memory recall in the trace" story that transparent injection
   architectures cannot match.

### 6.1 Erasure SLO (compaction is mandatory)

`Table::delete` in LanceDB is **tombstone-based**: deleted rows are
marked invisible but the underlying parquet bytes remain on disk until
a compaction job (`Table::optimize` / `compact_files`) runs. For GDPR
Art. 17 compliance this matters — "right to erasure" cannot mean "we
will mark it invisible." The spec commits to:

- **SLO:** physical erasure within **24 hours** of `memory_forget`.
- **Mechanism:** a background task in the `anno-rag` MCP server runs
  `Table::optimize` on both the `memories` and `documents` collections
  daily (configurable via `AnnoRagConfig::compaction_cron`). The task
  reclaims deletion-marker rows older than 1 hour.
- **Tool response:** `memory_forget` returns `"logically forgotten;
  physical erasure within 24h"` in its `MemoryForgetResult` so the
  caller can be honest with end users.
- **Vault parity:** the vault-token purge happens at delete time
  (immediate), but the LanceDB row carrying the token-refs lingers
  until compaction. This is acceptable because the row's `text` column
  contains only the *token* (`PERSON_a4f3`), never the plaintext
  (`Sophie Wilson`) — the residual data on disk has no PII value
  without the (already-purged) vault entry.

The 24h SLO is a default, not a contract — tenants with stricter
requirements can run `Table::optimize` on demand after each forget at
the cost of more frequent rewrites.

## 7. Tenancy

v0.1 reuses anno-rag's existing single-tenant configuration. Multi-tenant
support is an `AnnoRagConfig` concern, not a memory concern — when v0.7
adds tenant routing to anno-rag, memory comes along for free. The
`memory` module never reads tenant identity directly; it only sees the
`Pipeline` it was constructed with, which already encapsulates one
tenant's vault + store.

## 8. Audit signal

The gateway does **not** see MCP traffic in the v0.4 architecture
(Cowork connects to the anno-rag MCP server directly over stdio). So
audit for memory operations cannot live in the gateway today.

v0.1 ships a single `tracing` event from inside each memory tool
handler, with a stable schema:

```
target: anno_rag::memory::audit
fields:
  tool      = "memory_save" | "memory_recall" | "memory_forget" | "memory_list"
  tenant    = <tenant id or "default">
  result    = "ok" | "error"
  duration_ms = <u64>
```

Operators wire this into whichever log aggregator they already use. When
a future gateway version proxies MCP, it can add an `AuditEvent` variant
that consumes the same schema. **No `anno-privacy-gateway` code change
in v0.1.**

## 9. Testing strategy

**Unit (in `anno-rag/src/memory.rs` and friends):**

- Round-trip: save text with PII → recall by paraphrase → returned text
  matches original plaintext byte-for-byte.
- Tokenization invariant: the persisted LanceDB row contains no string
  matching any of the input's detected entities.
- Forget cascade: write two memories sharing one entity, forget one,
  assert the vault still resolves the token; forget the second, assert
  the vault no longer resolves it.
- `memory_forget(dry_run=true)` mutates nothing.
- `session_id` filter isolation: writes scoped to session A are not
  returned in a recall scoped to session B (but cross-session ones are).

**Integration (`crates/anno-rag/tests/memory_mcp.rs`):**

- Spin up `AnnoRagServer` with a temp store. Call each new tool via rmcp
  client. Assert schemas match. Assert behaviour matches unit-level
  semantics over the MCP transport.

**Property tests (`proptest`):**

- Forget cascade reference-count never goes negative under arbitrary
  interleavings of save / forget.

**Eval:**

- Extend the v0.6 eval harness with a small `memories.toml` fixture (~20
  short memories, ~10 recall queries with ground truth). Add `recall@5`
  on memories as a separate metric.
- **Benchmark baseline:** run a subset of the LoCoMo benchmark
  (Long-Conversation-Memory, the de-facto standard used by Zep / mem0 /
  MemoryOS / A-MEM 2025 papers). Pick 50 conversation/question pairs,
  commit baseline `accuracy@1` and `latency_p95_ms` scores to
  `tests/fixtures/locomo_baseline.toml`. CI does **not** gate on the
  number in v0.1 — we just need the harness wired and a number on the
  table so v0.2 has something to beat. Without this baseline the team
  has no objective way to claim "PII-safe memory that competes with
  graph-based systems."
- CI does **not** gate on the bespoke `memories.toml` recall — the
  corpus is too small to be a reliable signal.

## 10. File layout

New files in `crates/anno-rag/src/`:

- `memory.rs` — `Memory`, `MemoryKind`, `MemoryId`, `TokenRef`, and the
  `MemoryStore` over LanceDB.

Modified files:

- `lib.rs` — `pub mod memory;` and re-exports.
- `mcp.rs` — four new tool handlers (`memory_save`, `memory_recall`,
  `memory_forget`, `memory_list`) and their param/result types.
- `pipeline.rs` — `Pipeline` gains `save_memory`, `recall_memory`,
  `forget_memory` methods that compose existing `detect` / `vault` /
  `embed` / `store` helpers. MCP handlers call these, never the lower
  layers directly.
- `store.rs` — second LanceDB collection plus a `memories_hybrid_search`
  reusing v0.6's RRF helper.
- `config.rs` — `memory_collection_name` (default `"memories"`),
  `memory_embedding_dim` (default 384, must equal documents dim).

New test file:

- `tests/memory_mcp.rs`.

## 11. Open questions

0. **Workspace `lancedb` pin (blocker).** Currently pinned to `=0.27.2`.
   Native `Index::FTS` and the bug-fixed `RRFReranker` require 0.29.x.
   Land a separate PR bumping the workspace pin and re-running the v0.6
   hybrid-retrieval smoke tests **before** anno-memory v0.1 merges. If
   v0.6 has not yet shipped when memory v0.1 starts, the version bump
   becomes a shared prerequisite of both efforts.
1. **`MemoryKind` taxonomy.** Four values feels right for a notarial
   workflow but is a guess. Worth a 1-hour pass with a sample of real
   Cowork sessions before locking it in.
2. **Decay.** v0.1 has no time-decay. Should `accessed_at` already be
   tracked so v0.2 can decay on it without a migration? **Recommendation:
   yes** — the column is cheap and forward-compatible.
3. **Session_id source.** Cowork must pass `session_id` for session
   scoping to work. The gateway already sees an `anthropic-session-id`
   header on streaming responses; if Cowork can plumb that into MCP tool
   calls, session scoping works out of the box. If not, v0.1 ships with
   cross-session memory only and session_id is documented as "wire it up
   in v0.2."
4. **Embedding asymmetry.** v0.6 fixed the e5 query/passage prefix bug
   for documents. Memory recall must apply the **query** prefix to the
   user query and the **passage** prefix when saving. The spec is
   explicit but the test suite needs an assertion that the wrong prefix
   would fail the round-trip test.

## 12. Success criteria

- All four MCP tools callable from a Cowork session via the existing
  rmcp stdio transport.
- Round-trip test (save → recall → plaintext match) green for ≥ 95% of
  paraphrased queries on a 20-memory fixture.
- Zero plaintext PII in the LanceDB `memories` collection on disk
  (verified by a grep-style test in the integration suite).
- `memory_forget` cascade leaves no orphaned vault tokens (property
  test green over 1000 iterations).
- Gateway tests still pass — no behavioural change to
  `anno-privacy-gateway`.
- Peak RSS for `anno-rag` MCP under combined document + memory load
  stays under 1.5 GB.
- LoCoMo baseline numbers (`accuracy@1`, `latency_p95_ms`) committed —
  even if modest — so v0.2 has a regression target.
- Daily compaction reduces tombstoned bytes within the 24h erasure SLO,
  verified by an integration test that deletes 100 rows and asserts
  on-disk file size shrinks after `Table::optimize`.

## 13. Planned extensions (v0.2 and beyond)

v0.1 is deliberately a flat KV store with hybrid retrieval and forward-
compat columns. v0.2 turns those columns into capabilities.

- **v0.2 — temporal + graph-aware memory** (see
  `2026-05-15-anno-memory-v0.2-design.md`):
  - Activate bi-temporal semantics on `valid_from` / `valid_to`:
    invalidate-on-conflict for `Preference` and `Reference` kinds.
  - Populate `entity_refs` at write time by reusing
    **anno-core's own `StackedNER`** — no LLM call needed. PII tokens
    (from `token_refs`) + non-PII NER entities together form the
    graph-node set.
  - New `memory_graph_recall` MCP tool: 2-hop traversal over
    `entity_refs` using LanceDB's `LabelList` index. No external
    graph database.
- **v0.3 and beyond (out of scope for both v0.1 and v0.2):**
  - Passive capture (gateway-side fact extraction from streamed
    completions).
  - Letta-style core-vs-archival split at the MCP surface, if a
    sub-second "what is the user's standing context" call becomes
    a hot path.
  - Cross-tenant "Spaces" (the ourmem feature).
  - Heat-based eviction (the MemoryOS pattern).
  - Optional embedded graph DB (Oxigraph) — only if a real multi-hop
    workload exceeds what `entity_refs + LabelList` can serve.
