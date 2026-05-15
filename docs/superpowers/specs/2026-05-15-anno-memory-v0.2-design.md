# anno-memory v0.2 — Temporal Validity + Graph-Aware Memory Without a Graph DB

**Status:** Design — pending v0.1 ship + user review
**Date:** 2026-05-15
**Scope:** `anno-rag` crate only. Activates the forward-compat columns reserved in v0.1.
**Depends on:** `2026-05-15-anno-memory-v0.1-design.md` shipped and stable.

## 1. Motivation

v0.1 ships a flat key-value memory store with hybrid retrieval and a PII
vault. It answers "what did the user mention that resembles this query?"
That is enough for one-turn recall but it has two structural blind
spots that the 2025/2026 memory-graph literature (Graphiti, Zep, mem0)
exists specifically to solve:

1. **Facts change.** A preference recorded in January ("envoyer les
   actes en PDF") is no longer true in May ("désormais en .docx pour
   le notaire associé"). v0.1 has both rows, sorts them by similarity,
   and returns whichever scores higher. The agent then contradicts
   itself or — worse — uses the stale fact.
2. **Multi-hop queries fail.** "Quels dossiers impliquent un client de
   Maître Dupont ?" requires hopping `client → notaire → dossier`. A
   text-similarity search over memories never finds that path; it only
   finds memories that *literally mention* "Dupont" plus "dossier."

The state-of-the-art answer is bi-temporal knowledge graphs (Graphiti)
or hybrid vector+graph stores with LLM extraction (mem0). Both impose
heavy costs: an external graph DB (Neo4j / FalkorDB / Kuzu), Python-only
runtimes, and expensive LLM extraction passes on every write.

**v0.2 takes a different bet.** The `anno` crate at the root of this
workspace already does named-entity recognition (`StackedNER` over a
36-PII-label taxonomy plus Person/Org/Loc/Misc). The PII tokens we
already collect for the vault-cascade primitive *are* graph nodes.
Adding non-PII entities is a single `anno::extract()` call we already
own. And LanceDB's `LabelList` scalar index makes 2-hop traversal a
filtered scan — no Cypher engine, no triple store, no new process.

The result is a temporal, entity-aware memory layer that lives entirely
inside one Rust process, with one storage engine, and competes on the
single feature that justifies graph memory in the first place:
multi-hop reasoning.

## 2. Scope

**In scope (v0.2):**

- **Activate bi-temporal semantics** on the `valid_from` / `valid_to`
  columns reserved in v0.1. Conflict resolution: invalidate-on-conflict
  for `Preference` and `Reference` kinds; `Fact` and `Context` are
  append-only.
- **Populate `entity_refs` at write time** using `anno::StackedNER` —
  the same crate already used elsewhere in the workspace. PII tokens
  (already collected) plus non-PII entities (PER, ORG, LOC, custom
  GLiNER labels) merge into one canonical entity-id list per memory.
- **New MCP tool `memory_graph_recall`** — 2-hop traversal over
  `entity_refs` using the `LabelList` index, returning the connected
  subgraph of memories with their temporal validity.
- **Extend `memory_recall`** with two optional flags:
  - `as_of: Option<DateTime<Utc>>` — point-in-time query; returns only
    memories valid at that timestamp.
  - `graph_expand: bool` — if true, take the top-k hybrid hits and
    expand by one hop along `entity_refs`.
- **Entity canonicalization layer.** "Maître Dupont" and "Mr Dupont"
  must collapse to the same graph node. v0.2 ships a deterministic
  canonicalizer (lowercase + diacritic strip + space-collapse + a
  per-tenant alias table). No LLM call.

**Out of scope (deferred to v0.3+):**

- Passive capture (gateway-side fact extraction). Still explicit MCP
  calls only.
- LLM-driven entity extraction. v0.2 uses anno-core NER. LLM extraction
  may be revisited if NER coverage proves insufficient on real corpora.
- External graph DB (Oxigraph / Neo4j / FalkorDB). Stay in LanceDB.
- 3+ hop traversal. v0.2 caps at 2 hops. Deeper paths add quadratic
  scan cost and the user value is marginal.
- Letta-style core-vs-archival surface split.
- Cross-tenant Spaces.
- Heat-based eviction.
- Community detection / clustering on the entity graph (a Graphiti
  feature).

**Carried constraints:**

- Peak RSS for the `anno-rag` MCP under combined document + memory
  load stays **< 1.5 GB** (same as v0.1).
- Gateway remains a **pure proxy**. No new state in
  `anno-privacy-gateway`.
- Entity extraction adds latency to `memory_save`. Budget: NER pass
  + canonicalization completes in **< 50 ms p95** on a typical 200-token
  memory using the ONNX-cached BERT or NuNER backends. Heuristic-only
  fallback (no model cache) completes in < 5 ms but with lower recall.

## 3. Architecture

### 3.1 What changes vs v0.1

```
                memory_save (v0.1 → v0.2)
                  └─ detect()             (existing)
                  └─ vault.pseudonymize() (existing)
                  └─ embed(tokenized)     (existing)
            NEW   └─ anno::extract(plaintext_pre_vault)
            NEW   └─ canonicalize_entities()
            NEW   └─ merge(token_refs + entity_ids) → entity_refs
            NEW   └─ check_conflicts(kind, entity_refs)
            NEW   └─    if conflict: set valid_to = now() on prior row
                  └─ store.insert(row)    (existing)


                memory_recall (v0.1 → v0.2)
                  └─ hybrid_search(...)               (existing)
            NEW   └─ if as_of: filter valid_from ≤ as_of < (valid_to ?? ∞)
            NEW   └─ if graph_expand: for each hit:
            NEW   └─    find memories sharing ≥1 entity_ref (1 hop)
                  └─ return MemoryHit[] (existing)


                memory_graph_recall  (NEW)
                  └─ canonicalize(seed_entity)
                  └─ store.filter("array_contains(entity_refs, $seed)")  → hop1
                  └─ for each hop1 row: collect other entity_refs        → frontier
                  └─ store.filter("array_contains_any(entity_refs, $frontier)")  → hop2
                  └─ optional as_of temporal filter
                  └─ return GraphRecallResult { seed, hop1, hop2, edges }
```

The entity extraction call happens on the **plaintext pre-vault** —
`anno::extract` needs real names to do NER. The extracted entity *ids*
are then canonicalized and stored alongside the tokenized text. PII
plaintext still never touches disk (the vault sees it, the NER sees it
in memory, neither persists it).

### 3.2 Three-layer model (unchanged from v0.1)

- **Persistence:** LanceDB `memories` collection. Same row shape, two
  columns activated (`valid_to`, `entity_refs`).
- **Vector:** dense embedding (unchanged).
- **Lexical:** LanceDB native FTS (unchanged).
- **Graph (new logical layer, same physical store):** `entity_refs`
  column with `LabelList` index. Traversal = filtered scan, not graph
  walk.

## 4. Entity extraction

### 4.1 Source of entities

Two sources merge into one `entity_refs: List<String>` per memory:

1. **Vault tokens (already collected in v0.1's `token_refs`).** Each
   `{label: "PERSON", token: "PERSON_a4f3"}` becomes an entity id
   `"pii:PERSON:PERSON_a4f3"`. The `pii:` prefix is mandatory — it
   distinguishes vault-tokenized entities (which only resolve inside
   the tenant) from canonical non-PII entities (which are stable
   strings).
2. **anno-core NER on the plaintext.** `anno::StackedNER::extract_entities`
   returns `Entity { text, entity_type, start, end, confidence }`. We
   keep entities with `confidence ≥ 0.6` and `entity_type ∈ {ORG, LOC,
   MISC, + custom GLiNER labels}`. Person entities are skipped here —
   they should already be in the PII path (path 1).

### 4.2 Canonicalization

Three deterministic steps, in order:

```rust
fn canonicalize(text: &str) -> String {
    let lower = text.to_lowercase();
    let stripped = strip_diacritics(&lower);          // "dupont" not "Dupont"
    let collapsed = collapse_whitespace(&stripped);   // single spaces
    let aliased = apply_alias_table(&collapsed);      // per-tenant aliases
    format!("ent:{}:{}", entity_type, aliased)
}
```

The per-tenant alias table (`AnnoRagConfig::entity_aliases`) is a
simple `HashMap<String, String>` loaded at startup: `{"me dupont" =>
"dupont", "maître dupont" => "dupont", "mr dupont" => "dupont"}`.
v0.2 ships an empty default table. Operators populate it for their
domain (notarial titles, common abbreviations) — or not, accepting
that some duplicates will exist.

This is deliberately dumb. mem0 and Graphiti use LLM-based entity
resolution (an LLM compares candidates and decides "is `Dupont` here
the same as `M. Dupont` we saw earlier?"). That is more accurate but
costs an LLM call per save and is non-deterministic. v0.2 picks
determinism and zero-cost — operators can add an LLM resolver in v0.3
if recall measurements show it is needed.

### 4.3 Fallback when models are absent

`anno::StackedNER::default()` falls back to pattern + heuristic
extraction if no ONNX backend is loaded. The fallback catches obvious
proper nouns (capitalized tokens not at sentence start) but misses
domain terms. v0.2 emits a `tracing::warn` once at startup if the
backend resolves to heuristic-only, and surfaces a counter
(`anno_memory_ner_backend = "heuristic" | "bert" | "nuner" | "gliner"`)
in the audit signal so operators can spot degraded extraction.

## 5. Bi-temporal semantics

### 5.1 Two timelines

| timeline           | column        | meaning                                    |
|--------------------|---------------|--------------------------------------------|
| **event time**     | `valid_from`, `valid_to` | when the *fact* was/is true in the world |
| **ingestion time** | `created_at`  | when we *learned* about it (immutable)     |

Every row gets `valid_from = created_at` at insert (matching v0.1
behaviour). `valid_to` is null at insert and gets set only by conflict
resolution (§5.2) or by an explicit `memory_invalidate` call (§6.4).

### 5.2 Conflict resolution

When `memory_save` lands a new row, the resolver runs **only for
`Preference` and `Reference` kinds**. `Fact` and `Context` are
append-only — multiple facts about the same entity coexist; the agent
decides how to reconcile them at read time.

Resolver logic for `Preference` / `Reference`:

```
new_row.entity_refs ∩ existing_row.entity_refs ≠ ∅
  AND existing_row.kind == new_row.kind
  AND existing_row.valid_to IS NULL
  AND cosine_sim(new_row.embedding, existing_row.embedding) ≥ 0.85
  → existing_row.valid_to = new_row.created_at
```

The 0.85 cosine similarity threshold is the conservative guard: two
preferences sharing an entity but talking about different attributes
("PDF format" vs "envoyer le vendredi") should both stay valid. Tuned
on the v0.1 LoCoMo subset, default value subject to revision.

### 5.3 Point-in-time queries

`memory_recall(query, as_of: Some(t))` filters:

```sql
valid_from <= t AND (valid_to IS NULL OR valid_to > t)
```

This is the standard bi-temporal slice. Indexes on `valid_from` and
`valid_to` (both `BTree`) make it cheap.

### 5.4 What v0.2 explicitly does not do

- **Retroactive corrections.** If we learn in May that a January
  preference was actually wrong from the start (event time = January,
  ingestion time = May), v0.1's invalidate-on-conflict marks the prior
  row's `valid_to = now()`, not `= valid_from_of_new_row`. A future
  version could expose a `correct_as_of` parameter; v0.2 keeps the
  shape simple.
- **Branching histories** (Graphiti's full bi-temporal model with
  contradicting facts coexisting). v0.2 says "the most recent
  non-invalidated preference wins."

## 6. New MCP tools and changes

All on the existing `AnnoRagServer`. v0.2 adds two tools and extends two.

### 6.1 `memory_recall` — extended

```rust
pub struct MemoryRecallParams {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub kinds: Option<Vec<MemoryKind>>,
    /// NEW in v0.2. Point-in-time slice. Default: now.
    #[serde(default)]
    pub as_of: Option<DateTime<Utc>>,
    /// NEW in v0.2. One-hop entity expansion on top-k hits.
    #[serde(default)]
    pub graph_expand: bool,
}

pub struct MemoryHit {
    pub id: String,
    pub text: String,
    pub kind: MemoryKind,
    pub created_at: String,
    pub valid_from: String,                     // NEW
    pub valid_to: Option<String>,               // NEW
    pub entity_refs: Vec<String>,               // NEW
    pub score: f32,
    pub via: HitProvenance,                     // NEW: "hybrid" | "graph_expand"
}
```

### 6.2 `memory_graph_recall` — NEW

```rust
pub struct MemoryGraphRecallParams {
    /// Free-text seed entity. Will be canonicalized.
    pub entity: String,
    /// Hop budget. v0.2 caps at 2.
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
    /// Per-hop result cap to prevent explosion.
    #[serde(default = "default_per_hop_limit")]
    pub per_hop_limit: usize,
    /// Point-in-time slice.
    #[serde(default)]
    pub as_of: Option<DateTime<Utc>>,
}

pub struct GraphRecallResult {
    pub seed: String,                       // canonical id
    pub seed_resolved: Option<String>,      // plaintext if seed was a vault token
    pub nodes: Vec<EntityNode>,             // all entities encountered
    pub edges: Vec<MemoryEdge>,             // (entity_a, memory_id, entity_b)
    pub memories: Vec<MemoryHit>,           // rehydrated memories along the path
}

pub struct EntityNode {
    pub id: String,                         // canonical id e.g. "ent:ORG:cabinet dupont"
    pub display: String,                    // plaintext (rehydrated if PII)
    pub kind: EntityKind,                   // PiiToken | NamedEntity
    pub mention_count: u32,
}

pub struct MemoryEdge {
    pub from: String,                       // entity id
    pub via: String,                        // memory id linking them
    pub to: String,                         // entity id
}
```

### 6.3 `memory_save` — extended

The MCP surface gains one new optional parameter and one new field in
the result:

```rust
pub struct MemorySaveParams {
    pub text: String,
    #[serde(default)]
    pub kind: Option<MemoryKind>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// NEW. Override entity extraction (e.g. caller already knows the entities).
    #[serde(default)]
    pub entity_refs_override: Option<Vec<String>>,
}

pub struct MemorySaveResult {
    pub id: String,
    pub redacted_text: String,
    pub token_count: usize,
    pub entity_refs: Vec<String>,           // NEW — what NER found
    pub invalidated_ids: Vec<String>,       // NEW — prior rows we set valid_to on
}
```

### 6.4 `memory_invalidate` — NEW

For explicit invalidation outside the auto-resolver:

```rust
pub struct MemoryInvalidateParams {
    pub id: String,
    /// When the fact stopped being true. Defaults to now().
    #[serde(default)]
    pub at: Option<DateTime<Utc>>,
}
```

A no-op if `valid_to` is already set. Cannot un-invalidate (would
require an audit story we are not paying for in v0.2).

## 7. Privacy considerations

The privacy model from v0.1 is preserved: plaintext PII never persists
to LanceDB. v0.2 introduces three new privacy-relevant surfaces.

1. **NER runs on plaintext.** `anno::extract` sees real names before the
   vault tokenizes them. This is acceptable because NER runs in-process
   in the same Rust binary that already sees the plaintext (the
   pipeline detects PII *to know what to tokenize*). The NER pass adds
   no new exposure surface. Logging in NER backends is disabled by
   default; v0.2 explicitly sets `RUST_LOG` filters to suppress any
   per-entity debug logging in production builds.
2. **Entity canonicalization can collide across PII boundaries.** Two
   different real-world people both canonicalize to `"ent:PER:dupont"`
   if neither is in the vault. v0.2 mitigates this by routing all
   `Person` entities through the vault (path 1 in §4.1), never through
   path 2. Non-PII collisions on ORG/LOC are accepted — they are the
   intended behaviour (two memories about "Cabinet Dupont" should
   share a node).
3. **`memory_graph_recall` plaintext rehydration.** Graph results
   include rehydrated entity display names for PII tokens. The same
   tenant-scoped vault lookup applies. A graph call from tenant A
   cannot resolve PII tokens from tenant B even if the canonical ids
   collide (they will not — PII canonical ids embed the tenant-scoped
   token).
4. **Forget cascade through entity_refs.** When `memory_forget` purges
   a row, `entity_refs` entries that no other memory references can
   optionally be reported in the cascade response (so an operator can
   see "this entity is no longer mentioned anywhere"). v0.2 does
   **not** drop the canonical id itself — entity ids are derived from
   text, not stored as a separate registry. The cascade for non-PII
   entities is implicit (the node disappears once no memory points
   at it).

## 8. Data model — what changes

Schema is **unchanged** from v0.1. v0.2 only changes the *semantics*
of `valid_from`, `valid_to`, and `entity_refs`. This is the whole point
of the v0.1 forward-compat work: no migration, no rewrite, no version
discontinuity.

What does change:

- `valid_to` becomes non-trivially populated by the conflict resolver.
- `entity_refs` becomes non-empty.
- Two new scalar indexes activate (already created in v0.1, never
  used):
  - `BTree` on `valid_to` (was implicit in v0.1 via the column
    creation; v0.2 ensures it is built).
  - `LabelList` on `entity_refs` (created in v0.1, populated in v0.2).

## 9. Testing strategy

**Unit tests:**

- Canonicalization is deterministic and case/diacritic-insensitive.
- Alias table application: `"maître dupont"` → `"dupont"`.
- Conflict resolver: same kind + same entity + high cosine sim →
  prior `valid_to` set; same kind + same entity + low cosine sim →
  both rows stay live.
- Conflict resolver: `Fact` kind never invalidates prior rows.
- Point-in-time slice: row with `valid_from=t1`, `valid_to=t2`
  appears at `as_of=t1` and `as_of=t2-1ns`, not at `as_of=t2`.
- 2-hop traversal: planted graph
  (`PERSON_a → ORG_x → PERSON_b → ORG_y`) returns `ORG_y` from a hop-2
  query seeded at `PERSON_a`, but not at hop-1.

**Property tests:**

- For any sequence of `memory_save` + `memory_invalidate` calls, no
  memory is ever simultaneously valid-at-now AND has `valid_to <= now`.
- For any entity `e`, `memory_graph_recall(e, max_hops=2)` is a
  superset of `memory_graph_recall(e, max_hops=1)`, which is a
  superset of `memory_recall(e)` filtered to memories whose
  `entity_refs` contains `e`.

**Integration tests (`crates/anno-rag/tests/memory_graph.rs`):**

- End-to-end via rmcp client: save three memories forming a
  notarial-domain mini-graph, query `memory_graph_recall`, assert the
  expected subgraph shape.

**Eval:**

- LoCoMo subset from v0.1 re-run with v0.2 features enabled. Targets:
  - `accuracy@1` improves vs v0.1 baseline by ≥ 10 percentage points
    on the **multi-hop** subset (LoCoMo categorises questions; we
    measure only on the multi-hop bucket).
  - `latency_p95_ms` for `memory_recall` (with `graph_expand=false`)
    does not regress more than 5 ms vs v0.1.
  - `memory_save` p95 stays under v0.1 + 50 ms (the NER budget).

## 10. File layout

Modified files in `crates/anno-rag/src/`:

- `memory.rs` — add `EntityKind`, `EntityNode`, `MemoryEdge`,
  `GraphRecallResult`. Extend `Memory` operations with the conflict
  resolver.
- `mcp.rs` — extend `memory_save` / `memory_recall` params and results;
  add `memory_graph_recall` and `memory_invalidate` handlers.
- `pipeline.rs` — add `extract_entities`, `canonicalize`,
  `resolve_conflicts`, `graph_recall` methods composing the existing
  helpers.
- `store.rs` — add `filter_by_entity` and `point_in_time_slice` queries
  on the `memories` collection.
- `config.rs` — add `entity_aliases: HashMap<String, String>` and
  `conflict_cosine_threshold: f32` (default 0.85).

New file:

- `canonicalize.rs` — the deterministic 4-step canonicalizer plus
  alias loader.

New test files:

- `tests/memory_temporal.rs` — bi-temporal invariants.
- `tests/memory_graph.rs` — graph recall.

No changes to `anno-privacy-gateway`. No new crate dependencies
(`anno` is already a workspace member; `unicode-normalization` for
diacritic stripping is the only candidate addition — already used
elsewhere via tokenizers? confirm at implementation time).

## 11. Open questions

1. **NER backend at deploy time.** Production deployments will want
   one of the ONNX backends, not the heuristic fallback. Spec the
   deployment runbook (`docs/runbooks/`) to require model caches
   pre-populated. CI runs against the heuristic backend to keep test
   times reasonable; integration tests gated on a model cache run in
   a separate nightly workflow.
2. **Conflict resolver scope.** Restricting auto-invalidation to
   `Preference` and `Reference` is conservative. If real corpora
   show that `Context` memories also need it (e.g. session context
   from yesterday should not contradict today's), revisit. Easiest
   evidence: a multi-day LoCoMo trace.
3. **Per-hop result limits.** `per_hop_limit` default of 50 is a
   guess. The interesting failure mode is "popular entity": an
   organisation that appears in 5000 memories blows the hop-1 set
   before any useful filtering. v0.2 caps and emits a
   `tracing::warn` on truncation; v0.3 may add ranking inside hops.
4. **Bi-directional aliases.** `alias_table["maître dupont"] =
   "dupont"` is one-way. If a user later types `"dupont"`, it stays
   `"dupont"`. That is correct. But if the source data has
   `"M. Dupont"` and the user queries `"Maître Dupont"`, both must
   canonicalize to the same id. Two entries in the table cover this.
   Operators need to be told.
5. **Embedding the conflict signal.** v0.2 uses cosine similarity on
   the e5 embedding as the conflict guard. e5 was tuned for retrieval,
   not paraphrase detection. If the threshold proves too noisy, an
   alternative is to fall back to the FTS index for token overlap.
   Decide on real data.

## 12. Success criteria

- `memory_save` populates non-empty `entity_refs` for ≥ 90% of inputs
  in the LoCoMo subset (measured against a hand-labelled gold
  entity set on 50 messages).
- `memory_graph_recall` returns the expected hop-2 subgraph on the
  planted-graph integration test.
- Conflict resolver correctly invalidates prior preferences on a
  20-conversation regression fixture (precision ≥ 0.9, recall ≥ 0.8).
- LoCoMo multi-hop `accuracy@1` improves ≥ 10 percentage points over
  v0.1 baseline.
- `memory_save` p95 latency stays under v0.1 + 50 ms.
- Peak RSS unchanged from v0.1 (< 1.5 GB).
- Zero plaintext PII in `memories.entity_refs` for `Person` entities
  (they go through the PII path, never the canonicalizer-as-text
  path). Verified by an integration test grepping the on-disk
  collection.

## 13. What v0.3 might look like

v0.2 is intentionally the last spec that lives inside one Rust
process with one storage engine. v0.3 candidates, in rough priority:

- **Passive capture** at the gateway. Stream-side fact extraction
  with optional opt-in per tenant.
- **Letta-style core surface.** A `memory_core` tool returning the
  small standing-context set under 10ms p99, distinct from
  hybrid+graph recall.
- **Optional embedded graph store** (Oxigraph) if 2-hop scans on
  LanceDB stop being adequate. Only justified by measured query
  latencies, not by aesthetic preference for "real graphs."
- **LLM-driven entity resolution** for cases where deterministic
  canonicalization underperforms (e.g. multilingual aliasing, role
  resolution like "le notaire associé" → a named person).
- **Community detection** on the entity graph (Graphiti's
  hierarchical communities) for high-level memory summarisation.
- **Cross-tenant Spaces** for shared memory between cooperating
  tenants — the ourmem feature.

None of these are committed. They exist here to make explicit what
v0.2 is *not* trying to be.
