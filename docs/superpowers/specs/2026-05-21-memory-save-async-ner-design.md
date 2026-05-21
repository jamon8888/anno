# Memory-Save Async NER Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce `memory_save` latency from 30–54 s to 4–5 s by making GLiNER2 NER inference non-blocking.

**Date:** 2026-05-21
**Crates affected:** `anno-rag` (pipeline, config, store), `anno-rag-mcp`

---

## Threat Model

LanceDB is a local file on the user's machine. The Candle e5-small embedder runs entirely on-device. Nothing in the `memory_save` path sends data to any external service.

The existing vault pseudonymization provides:
1. **Entity tracking** — `token_refs` / `entity_refs` for graph-recall and conflict resolution.
2. **LanceDB file confidentiality** — tokens instead of names in the on-disk file.
3. **GDPR Art. 17 right-to-be-forgotten** — vault deletion makes the token unresolvable.

The user does not need GDPR Art. 17 compliance. LanceDB file confidentiality is a nice-to-have, not a hard requirement. Therefore: **storing raw text in the local LanceDB file is acceptable** during and after the save, at the operator's discretion.

---

## Three NER Modes

A new config field `memory_ner_mode: MemoryNerMode` controls save behaviour.

```
MemoryNerMode
  Disabled   — no NER ever; raw text stored, no token/entity refs
  Async      — raw text stored immediately; NER enrichment in background  [default]
  Sync       — current behaviour; blocks until NER + vault complete
```

### Latency by mode (measured on this machine)

| Mode | MCP response time | NER runs? | token_refs populated? |
|---|---|---|---|
| `disabled` | ~4 s | never | no |
| `async` | ~4 s | background (~30 s later) | yes, after task |
| `sync` | 30–54 s | inline | yes, immediately |

---

## Fast Path (Disabled and Async immediate portion)

When mode is `disabled` or `async`, the pipeline does the minimum work before returning:

```
save_memory_fast(id, text, kind, session_id)
  1. embedder().embed_batch(&[text])          (~3 s, raw text)
  2. Memory { id, text: raw, embedding, token_refs: [], entity_refs: [], … }
  3. store.memory_insert(&m)                  (~1 s)
  4. return SavedMemory { id, stored_text: raw, token_refs: [], … }
```

The raw text embedding is **semantically better** than embedding the pseudonymized form (`PERSON_N` carries no meaning), so search quality is unchanged or improved.

Conflict resolution (auto-invalidate Preference/Reference on shared entity) is skipped at insert time — entity_refs are empty. For `async` mode, it runs in the background task once NER completes. For `disabled` mode, it never runs.

---

## Async NER Background Task

Spawned by the MCP layer immediately after the fast-path insert. Receives an `Arc<Pipeline>` clone (cheap), the original `text`, `id`, `kind`, `session_id`.

```
save_memory_ner_task(pipeline: Arc<Pipeline>, id, text, kind, session_id)
  1. detector_get_or_init().detect(text)
  2. Build entity_refs from detected entities
  3. Build token_refs (vault.pseudonymize_with_refs or just entity labels
     — vault pseudonymization is optional; spec leaves this to impl)
  4. Run conflict resolution for Preference / Reference kinds
     → store.memory_update_valid_to(prior_id, now) for each conflict
  5. store.memory_update_ner_fields(id, token_refs, entity_refs)
  6. tracing::info!(memory_id = %id, "NER enrichment complete")

On any error:
  7. tracing::error!(memory_id = %id, err = %e, "NER enrichment failed")
     (row remains with raw text and empty token_refs — acceptable)
```

The stored `text` field is **not updated** — raw text stays in LanceDB. Only `token_refs` and `entity_refs` are written. This avoids a re-embed and keeps the fast-path embedding intact.

---

## Store: New Method

`Store` gains one new method:

```rust
pub async fn memory_update_ner_fields(
    &self,
    id: &MemoryId,
    token_refs: Vec<TokenRef>,
    entity_refs: Vec<EntityRef>,
) -> Result<()>
```

Performs a targeted LanceDB update on the single row matching `id`. Uses the existing `memory_update_valid_to` pattern as a template.

---

## Config Changes

### New enum (mirrors `OcrMode` pattern)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNerMode {
    Disabled,
    Async,
    Sync,
}
```

### New field on `AnnoRagConfig`

```rust
/// NER mode for memory_save. Default: async.
/// - disabled: embed + store raw text only (~4 s)
/// - async: embed + store immediately, NER enriches in background (~4 s response)
/// - sync: full pipeline inline; blocks 30–54 s on CPU
#[serde(default = "default_memory_ner_mode")]
pub memory_ner_mode: MemoryNerMode,
```

Default: `MemoryNerMode::Async`.

Old config files without this field deserialize to `Async` via `#[serde(default)]`.

---

## API Contract: `memory_save` Response

`SavedMemory` gains a `ner_mode` field reflecting what ran synchronously:

```rust
pub struct SavedMemory {
    pub id: MemoryId,
    /// Text actually stored. Raw input for async/disabled; tokenized for sync.
    pub stored_text: String,      // renamed from redacted_text
    pub token_refs: Vec<TokenRef>,
    pub entity_refs: Vec<EntityRef>,
    pub invalidated_ids: Vec<String>,
    pub ner_mode: MemoryNerMode,  // new
}
```

`redacted_text` is kept as a deprecated alias in the MCP JSON output for one release cycle.

The MCP JSON response:

```json
{
  "id": "019e…",
  "stored_text": "Antoine Lefebvre approved the report.",
  "token_refs": [],
  "ner_mode": "async"
}
```

For `sync` mode, `stored_text` is the tokenized form (as today), `token_refs` is populated.

---

## MCP Layer: Dispatch

The MCP `memory_save` handler changes from a simple delegate to a two-step dispatch:

```rust
async fn memory_save(&self, Parameters(p): Parameters<MemorySaveParams>) -> String {
    let id = MemoryId::new();
    match self.cfg.memory_ner_mode {
        MemoryNerMode::Sync => {
            // existing path
            self.pipeline.save_memory_sync(&p.text, kind, p.session_id).await
        }
        MemoryNerMode::Async | MemoryNerMode::Disabled => {
            let r = self.pipeline
                .save_memory_fast(id, &p.text, kind, p.session_id.clone())
                .await?;
            if self.cfg.memory_ner_mode == MemoryNerMode::Async {
                let pipeline = Arc::clone(&self.pipeline);
                let text = p.text.clone();
                tokio::spawn(async move {
                    pipeline.save_memory_ner_task(r.id.clone(), text, kind2, session2).await;
                });
            }
            Ok(r)
        }
    }
}
```

---

## Error Handling

| Failure point | Outcome |
|---|---|
| `embed_batch` fails (fast path) | Error returned to caller; row not written |
| `memory_insert` fails (fast path) | Error returned to caller |
| Background NER task panics | Tokio catches the panic; row stays with empty token_refs; logged at ERROR |
| Background NER `detect` fails | Logged at WARN; row stays with empty token_refs |
| Background `memory_update_ner_fields` fails | Logged at WARN; row stays with empty token_refs |

No retry logic. No dead-letter queue. NER enrichment is best-effort.

---

## Testing

### Unit tests (`crates/anno-rag/src/pipeline.rs`)

1. **`save_memory_fast_returns_immediately`** — with a mock store (no NER model required), verify `save_memory` with `Disabled` returns a UUID and `token_refs: []` without calling `detector_get_or_init`.

2. **`save_memory_async_row_exists_before_ner`** — insert via `Async` path; verify row is in store before background task runs (query by id immediately after insert).

### Integration test (`#[ignore]`, slow)

3. **`save_memory_async_ner_enriches_row`** — save with `Async`; sleep 60 s; query row; assert `token_refs` is non-empty for a text with a known entity.

### Config tests

4. **`memory_ner_mode_defaults_to_async`** — `AnnoRagConfig::default().memory_ner_mode == Async`.

5. **`old_config_deserializes_to_async`** — JSON without `memory_ner_mode` field deserializes to `Async`.

6. **`memory_ner_mode_round_trips`** — `disabled` / `async` / `sync` serialize as snake_case and round-trip.

### Existing test impact

Phase 5 and Phase 6 test scripts check `redacted_text` / `stored_text` in the save response for `PERSON_N` tokens. With `Async` as the new default, those checks return raw text. Update assertions to check the **recall** path for tokens, not the save response.

---

## Out of Scope

- Re-embedding with tokenized text after NER (raw embedding has better semantic quality).
- Status-check tool for pending NER tasks (YAGNI — enrichment completes in ~30 s with no user-visible gap).
- Retry logic for failed NER tasks.
- Vault pseudonymization in `Async` / `Disabled` paths (RTBF not required).
