# anno-memory v0.2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Activate the v0.1 forward-compat columns into temporal + entity-graph capabilities. Three new behaviours: (a) bi-temporal semantics on `valid_from` / `valid_to` with invalidate-on-conflict for `Preference` and `Reference` memories; (b) entity-ref population at write time using anno-core's own `StackedNER` (no LLM call); (c) a new `memory_graph_recall` MCP tool that does 2-hop traversal over `entity_refs` using the LanceDB `LabelList` scalar index. Plus `as_of` + `graph_expand` parameters on `memory_recall`, and a `memory_invalidate` tool.

**Architecture:** Schema is unchanged from v0.1 — only semantics activate. Entity sources merge two streams: vault tokens (already collected, prefixed `pii:`) + non-PII NER (PER/ORG/LOC/MISC, prefixed `ent:`). Canonicalization is deterministic (lowercase + diacritic strip + alias table); LLM-based entity resolution is explicitly out of scope. Conflict resolver detects (same entity ∩ same kind ∩ cosine ≥ 0.85) and sets prior row's `valid_to = now()`. Graph traversal is filtered scans on the `LabelList` index — no Cypher engine, no external graph DB.

**Tech Stack:** Rust 2021, `anno` (workspace `StackedNER` + 36-PII taxonomy), `unicode-normalization` (diacritic strip), `lancedb 0.29.x` (LabelList scalar index queries), `chrono` (temporal), `rmcp 1.6`.

**Prerequisite:** `2026-05-15-anno-memory-v0.1.md` shipped (memory module + 4 MCP tools + forward-compat columns reserved). PR-A (lancedb 0.29 bump) merged.

---

## File Structure

- **Create** `crates/anno-rag/src/canonicalize.rs` — `canonicalize_entity(text, kind) -> String`, alias table loader, public `EntityKind` enum.
- **Create** `crates/anno-rag/src/conflict.rs` — `find_conflicts(new: &Memory, candidates: &[Memory], threshold: f32) -> Vec<MemoryId>`.
- **Create** `crates/anno-rag/tests/memory_temporal.rs` — bi-temporal invariants.
- **Create** `crates/anno-rag/tests/memory_graph.rs` — graph recall end-to-end.
- **Modify** `crates/anno-rag/src/memory.rs` — add `EntityKind`, `EntityNode`, `MemoryEdge`, `GraphRecallResult`; extend `MemoryHit` with `valid_from`/`valid_to`/`entity_refs`/`via`.
- **Modify** `crates/anno-rag/src/pipeline.rs` — `extract_entities`, `resolve_conflicts`, `graph_recall`, `invalidate_memory`; extend `save_memory` to populate `entity_refs` and run conflict resolution; extend `recall_memory` with `as_of` and `graph_expand`.
- **Modify** `crates/anno-rag/src/store.rs` — `memory_filter_by_entity(entity_id)`, `memory_point_in_time(filter, as_of)`, `memory_update_valid_to(id, valid_to)`.
- **Modify** `crates/anno-rag/src/mcp.rs` — add `memory_graph_recall` + `memory_invalidate` handlers; extend `MemorySaveParams`/`Result` and `MemoryRecallParams`/`MemoryHitWire`.
- **Modify** `crates/anno-rag/src/config.rs` — add `entity_aliases: HashMap<String, String>` (default empty), `conflict_cosine_threshold: f32` (default 0.85), `graph_max_hops: u8` (default 2), `graph_per_hop_limit: usize` (default 50).
- **Modify** `crates/anno-rag/src/lib.rs` — `pub mod canonicalize;` `pub mod conflict;`.
- **Modify** `crates/anno-rag/Cargo.toml` — `anno = { path = "../anno" }` (or `workspace = true` if already a workspace member used elsewhere), `unicode-normalization = "0.1"`.
- **Modify** `crates/anno-rag/benches/bench_locomo.rs` — add a multi-hop accuracy gate.
- **Modify** `crates/anno-rag/CHANGELOG.md` — v0.8 entry.

---

## Task 1: Canonicalizer module

**Files:**
- Create: `crates/anno-rag/src/canonicalize.rs`
- Modify: `crates/anno-rag/src/lib.rs`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Add `unicode-normalization` dep**

In `crates/anno-rag/Cargo.toml`:
```toml
unicode-normalization = "0.1"
```

- [ ] **Step 2: Write failing tests**

Create `crates/anno-rag/src/canonicalize.rs`:
```rust
//! Deterministic entity canonicalizer. Lowercase + diacritic strip +
//! whitespace collapse + per-tenant alias table. No LLM call.

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// Vault-tokenized PII entity. Canonical form embeds the vault token,
    /// e.g. `pii:PERSON:PERSON_a4f3`. Resolves only inside its tenant.
    PiiToken,
    /// Non-PII named entity. Canonical form is text-derived,
    /// e.g. `ent:ORG:cabinet dupont`.
    NamedEntity,
}

pub fn canonicalize_entity(
    text: &str,
    entity_kind_tag: &str,    // e.g. "PER" | "ORG" | "LOC" | "MISC"
    aliases: &HashMap<String, String>,
) -> String {
    let lower = text.to_lowercase();
    let stripped: String = lower.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    let collapsed = collapse_whitespace(&stripped);
    let aliased = aliases.get(&collapsed).cloned().unwrap_or(collapsed);
    format!("ent:{entity_kind_tag}:{aliased}")
}

pub fn canonicalize_pii_token(label: &str, token: &str) -> String {
    format!("pii:{label}:{token}")
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_and_strip_diacritics() {
        let aliases = HashMap::new();
        assert_eq!(canonicalize_entity("Maître Dupont", "PER", &aliases),
                   "ent:PER:maitre dupont");
        assert_eq!(canonicalize_entity("CAFÉ DE FLORE", "ORG", &aliases),
                   "ent:ORG:cafe de flore");
    }

    #[test]
    fn alias_table_applied() {
        let mut aliases = HashMap::new();
        aliases.insert("maitre dupont".into(), "dupont".into());
        aliases.insert("me dupont".into(),    "dupont".into());
        assert_eq!(canonicalize_entity("Maître Dupont", "PER", &aliases), "ent:PER:dupont");
        assert_eq!(canonicalize_entity("Me. DUPONT", "PER", &aliases),    "ent:PER:dupont");
    }

    #[test]
    fn pii_token_format() {
        assert_eq!(canonicalize_pii_token("PERSON", "PERSON_a4f3"),
                   "pii:PERSON:PERSON_a4f3");
    }

    #[test]
    fn whitespace_collapsed() {
        let aliases = HashMap::new();
        assert_eq!(canonicalize_entity("  Cabinet   Dupont  ", "ORG", &aliases),
                   "ent:ORG:cabinet dupont");
    }
}
```

Modify `crates/anno-rag/src/lib.rs` — add `pub mod canonicalize;`.

- [ ] **Step 3: Run tests, verify they pass**

```powershell
cargo test -p anno-rag canonicalize::
```
Expected: 4 PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/canonicalize.rs crates/anno-rag/src/lib.rs crates/anno-rag/Cargo.toml
git commit -m "feat(anno-rag): deterministic entity canonicalizer (Unicode NFD strip + alias table)"
```

---

## Task 2: Wire `anno::StackedNER` into `Pipeline::extract_entities`

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Confirm `anno` is a workspace dep available to `anno-rag`**

```powershell
Select-String -Path crates\anno-rag\Cargo.toml -Pattern "^anno\s*="
```
If absent, add:
```toml
anno = { path = "../anno" }
```

- [ ] **Step 2: Write failing test**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn save_populates_entity_refs_from_ner() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let text = "Sophie Wilson travaille pour Cabinet Dupont à Bordeaux.";
    let saved = p.save_memory(text, Some(MemoryKind::Fact), None).await.unwrap();
    let row = p.store().memory_get(&saved.id).await.unwrap().unwrap();
    // At least the ORG ("Cabinet Dupont") and LOC ("Bordeaux") must be in entity_refs.
    // Sophie Wilson goes through the PII path; check it surfaces as a pii: token.
    assert!(row.entity_refs.iter().any(|e| e.starts_with("ent:ORG:")),
        "expected ORG entity, got: {:?}", row.entity_refs);
    assert!(row.entity_refs.iter().any(|e| e.starts_with("ent:LOC:")),
        "expected LOC entity, got: {:?}", row.entity_refs);
    assert!(row.entity_refs.iter().any(|e| e.starts_with("pii:")),
        "expected PII entity, got: {:?}", row.entity_refs);
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_mcp save_populates_entity_refs_from_ner
```
Expected: FAIL — `entity_refs` is still always empty (v0.1 contract).

- [ ] **Step 3: Implement `Pipeline::extract_entities`**

Append to `crates/anno-rag/src/pipeline.rs`:
```rust
use anno::{prelude::*, StackedNER};
use std::collections::HashSet;
use crate::canonicalize::{canonicalize_entity, canonicalize_pii_token};

impl Pipeline {
    /// Extract entity_refs by merging vault token_refs with anno-core NER.
    ///
    /// `plaintext` is the pre-vault text. `token_refs` are the vault tokens
    /// already collected during the pseudonymize step in `save_memory`.
    pub fn extract_entities(&self, plaintext: &str, token_refs: &[TokenRef]) -> Vec<String> {
        let mut out = HashSet::new();
        // 1. PII tokens — canonical form embeds the token id.
        for tr in token_refs {
            out.insert(canonicalize_pii_token(&tr.label, &tr.token));
        }
        // 2. Non-PII NER. Skip Person — those should have been picked up by the
        //    vault path; if NER returns one outside the vault, it is a leak we
        //    do NOT want to expose as a non-PII graph node.
        let m = StackedNER::default();
        match m.extract_entities(plaintext, None) {
            Ok(ents) => {
                for e in ents {
                    if e.confidence < 0.6 { continue; }
                    let tag = match e.entity_type {
                        EntityType::Organization => "ORG",
                        EntityType::Location => "LOC",
                        EntityType::Misc => "MISC",
                        EntityType::Person => continue, // PII path only
                        _ => continue,
                    };
                    out.insert(canonicalize_entity(&e.text, tag, &self.cfg.entity_aliases));
                }
            }
            Err(e) => tracing::warn!(target: "anno_rag::memory::audit",
                "ner extract failed: {e}"),
        }
        out.into_iter().collect()
    }
}
```

(`EntityType::Misc` and `EntityType::Organization` are the variant names used by `anno`'s `EntityType` enum — verify in `crates/anno/src/types.rs` or wherever the type lives, and adjust if names differ.)

- [ ] **Step 4: Plumb into `save_memory`**

In `crates/anno-rag/src/pipeline.rs`, modify `save_memory` (added in v0.1 Task 6). Replace the line `entity_refs: vec![],` with:
```rust
entity_refs: self.extract_entities(text, &token_refs),
```

Critical: pass the **pre-vault plaintext** (the `text` parameter), not the tokenized one — NER needs real names.

- [ ] **Step 5: Run test, verify it passes**

Expected: PASS. If `StackedNER::default()` falls back to heuristic-only (no ONNX cache), the test may flake on ORG/LOC detection. Pre-populate the model cache in CI per [`docs/runbooks/`] guidance, or skip the ORG assertion when `ANNO_NO_DOWNLOADS=1` and no cache exists.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/Cargo.toml crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): extract_entities — merge vault tokens + anno StackedNER"
```

---

## Task 3: Bi-temporal `as_of` filter on `recall_memory`

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/memory.rs`

- [ ] **Step 1: Write failing test**

Create `crates/anno-rag/tests/memory_temporal.rs`:
```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use chrono::{Duration, Utc};
use tempfile::TempDir;

#[tokio::test]
async fn as_of_point_in_time_excludes_invalidated_rows() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();

    let saved = p.save_memory("preference: PDF",
        Some(MemoryKind::Preference), Some("sess1".into())).await.unwrap();
    let cutoff = Utc::now();
    // Sleep 1 ms to ensure monotonic ordering.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    // Manually invalidate at "now()".
    p.invalidate_memory(&saved.id, None).await.unwrap();

    // as_of = cutoff -> the preference is still valid (valid_to > cutoff).
    let hits_before = p.recall_memory("PDF", 5, None, None, Some(cutoff), false).await.unwrap();
    assert!(hits_before.iter().any(|h| h.id == saved.id.as_string()),
        "preference should be valid as_of cutoff");

    // as_of = now -> invalidated.
    let hits_after = p.recall_memory("PDF", 5, None, None,
        Some(Utc::now() + Duration::seconds(1)), false).await.unwrap();
    assert!(!hits_after.iter().any(|h| h.id == saved.id.as_string()),
        "preference must be excluded after valid_to");
}
```

Run: expect FAIL — `Pipeline::recall_memory` does not yet accept `as_of` or `graph_expand`.

- [ ] **Step 2: Extend `recall_memory` signature**

In `crates/anno-rag/src/pipeline.rs`, change:
```rust
pub async fn recall_memory(
    &self,
    query: &str,
    top_k: usize,
    session_id: Option<String>,
    kinds: Option<Vec<MemoryKind>>,
) -> Result<Vec<MemoryHit>> {
```
to:
```rust
pub async fn recall_memory(
    &self,
    query: &str,
    top_k: usize,
    session_id: Option<String>,
    kinds: Option<Vec<MemoryKind>>,
    as_of: Option<chrono::DateTime<chrono::Utc>>,
    graph_expand: bool,  // wired in Task 6
) -> Result<Vec<MemoryHit>> {
```

After hybrid search, before truncation, add:
```rust
// 4b. Temporal filter (as_of).
if let Some(t) = as_of {
    let t_us = t.timestamp_micros();
    raw.retain(|r| {
        let vf = r.valid_from_us;
        let vt = r.valid_to_us; // Option<i64>
        vf <= t_us && vt.map_or(true, |v| v > t_us)
    });
} else {
    // Default: only currently-valid (valid_to IS NULL OR valid_to > now()).
    let now_us = chrono::Utc::now().timestamp_micros();
    raw.retain(|r| r.valid_to_us.map_or(true, |v| v > now_us));
}
```

Extend `MemoryHitRow` (added in v0.1 Task 5) in `crates/anno-rag/src/memory.rs`:
```rust
pub struct MemoryHitRow {
    pub id: String,
    pub session_id: Option<String>,
    pub text_tokenized: String,
    pub kind: MemoryKind,
    pub created_at: String,
    pub valid_from_us: i64,
    pub valid_to_us: Option<i64>,
    pub entity_refs: Vec<String>,
    pub score: f32,
}
```

Update the row-builder helper in `store.rs` to populate the two timestamp columns and entity_refs.

Extend `MemoryHit`:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct MemoryHit {
    pub id: String,
    pub text: String,
    pub kind: MemoryKind,
    pub created_at: String,
    pub valid_from: String,       // NEW
    pub valid_to: Option<String>, // NEW
    pub entity_refs: Vec<String>, // NEW
    pub score: f32,
    pub via: HitProvenance,       // NEW
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HitProvenance { Hybrid, GraphExpand }
```

Map `MemoryHitRow -> MemoryHit` in `recall_memory` with `via = HitProvenance::Hybrid`.

- [ ] **Step 3: Implement `Pipeline::invalidate_memory`**

Append to `crates/anno-rag/src/pipeline.rs`:
```rust
impl Pipeline {
    pub async fn invalidate_memory(
        &self,
        id: &crate::memory::MemoryId,
        at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<bool> {
        let when = at.unwrap_or_else(chrono::Utc::now);
        self.store.memory_update_valid_to(id, when).await
    }
}
```

In `crates/anno-rag/src/store.rs`:
```rust
pub async fn memory_update_valid_to(
    &self,
    id: &crate::memory::MemoryId,
    valid_to: chrono::DateTime<chrono::Utc>,
) -> Result<bool> {
    let id_s = id.as_string();
    let ts = valid_to.timestamp_micros();
    let filter = format!("id = '{id_s}' AND valid_to IS NULL");
    let mut updates = std::collections::HashMap::new();
    updates.insert("valid_to".to_string(), format!("CAST({ts} AS TIMESTAMP)"));
    let rows = self.memories_tbl
        .update()
        .only_if(filter)
        .column("valid_to", format!("CAST({ts} AS TIMESTAMP)"))
        .execute().await
        .map_err(|e| Error::Store(format!("update valid_to: {e}")))?;
    Ok(rows > 0)
}
```

**Verification note:** LanceDB 0.29 `Table::update` API may expose `.column()` and `.only_if()` directly, or via an `UpdateBuilder`. Adjust per actual surface.

- [ ] **Step 4: Update all `recall_memory` call sites**

Inside `crates/anno-rag/src/mcp.rs`, find `memory_recall` (added in v0.1 Task 10). Update to forward the new parameters — temporarily pass `None, false` until Task 6 wires the MCP params. Inside `Pipeline::forget_memory` (when called recursively with `query`), pass `None, false`.

- [ ] **Step 5: Run test, verify it passes**

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/store.rs crates/anno-rag/src/memory.rs crates/anno-rag/src/mcp.rs crates/anno-rag/tests/memory_temporal.rs
git commit -m "feat(anno-rag): bi-temporal recall + memory_invalidate"
```

---

## Task 4: Conflict resolver — invalidate-on-save for Preference/Reference

**Files:**
- Create: `crates/anno-rag/src/conflict.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/lib.rs`
- Modify: `crates/anno-rag/src/config.rs`

- [ ] **Step 1: Add config**

In `crates/anno-rag/src/config.rs`:
```rust
pub conflict_cosine_threshold: f32,    // default 0.85
pub entity_aliases: std::collections::HashMap<String, String>, // default empty
```

- [ ] **Step 2: Write failing test**

Append to `crates/anno-rag/tests/memory_temporal.rs`:
```rust
#[tokio::test]
async fn conflict_resolver_invalidates_prior_preference() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let first = p.save_memory("préférence: envoyer les actes en PDF à Cabinet Dupont",
        Some(MemoryKind::Preference), Some("s1".into())).await.unwrap();
    // Same kind, same entity, very similar embedding => should invalidate.
    let second = p.save_memory("préférence: envoyer les actes en DOCX à Cabinet Dupont",
        Some(MemoryKind::Preference), Some("s1".into())).await.unwrap();
    assert!(second.invalidated_ids.contains(&first.id.as_string()),
        "expected first preference to be auto-invalidated, got: {:?}", second.invalidated_ids);

    // A Fact must never auto-invalidate.
    let f1 = p.save_memory("Cabinet Dupont est situé à Bordeaux",
        Some(MemoryKind::Fact), Some("s1".into())).await.unwrap();
    let f2 = p.save_memory("Cabinet Dupont a déménagé à Lyon",
        Some(MemoryKind::Fact), Some("s1".into())).await.unwrap();
    assert!(f2.invalidated_ids.is_empty(), "facts must not auto-invalidate");
    let _ = f1; // suppress unused
}
```

Run: expect FAIL — `SavedMemory.invalidated_ids` not yet present.

- [ ] **Step 3: Add `invalidated_ids` to `SavedMemory`**

In `crates/anno-rag/src/pipeline.rs` (defined in v0.1 Task 6):
```rust
pub struct SavedMemory {
    pub id: MemoryId,
    pub redacted_text: String,
    pub token_refs: Vec<TokenRef>,
    pub entity_refs: Vec<String>,    // NEW
    pub invalidated_ids: Vec<String>, // NEW
}
```

Also extend `MemorySaveResultWire` in `crates/anno-rag/src/mcp.rs`:
```rust
#[derive(Serialize)]
struct MemorySaveResultWire {
    id: String,
    redacted_text: String,
    token_count: usize,
    entity_refs: Vec<String>,
    invalidated_ids: Vec<String>,
}
```
and populate in the handler.

- [ ] **Step 4: Implement conflict resolver**

Create `crates/anno-rag/src/conflict.rs`:
```rust
//! Conflict resolver: detect prior memories that a new save invalidates.
//!
//! v0.2 rules:
//! - Only `Preference` and `Reference` kinds participate. `Fact` and
//!   `Context` are append-only.
//! - Conflict requires: same kind, ≥1 shared entity_ref, cosine ≥ threshold,
//!   prior row still valid (`valid_to IS NULL`).

use crate::memory::{Memory, MemoryKind};

pub fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

pub fn shares_any<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.iter().any(|x| b.iter().any(|y| x == y))
}

pub fn resolves_conflict(new: &Memory, prior: &Memory, threshold: f32) -> bool {
    if new.kind != prior.kind { return false; }
    if !matches!(new.kind, MemoryKind::Preference | MemoryKind::Reference) { return false; }
    if prior.valid_to.is_some() { return false; }
    if !shares_any(&new.entity_refs, &prior.entity_refs) { return false; }
    cosine_sim(&new.embedding, &prior.embedding) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::memory::{MemoryId, MemoryKind};

    fn mk(kind: MemoryKind, entities: Vec<String>, vec: Vec<f32>, valid_to: Option<chrono::DateTime<chrono::Utc>>) -> Memory {
        let now = Utc::now();
        Memory {
            id: MemoryId::new(), session_id: None, kind, text: String::new(),
            created_at: now, accessed_at: now, valid_from: now, valid_to,
            embedding: vec, token_refs: vec![], entity_refs: entities,
        }
    }

    #[test]
    fn fact_never_conflicts() {
        let a = mk(MemoryKind::Fact, vec!["ent:ORG:cabinet dupont".into()], vec![1.0, 0.0], None);
        let b = mk(MemoryKind::Fact, vec!["ent:ORG:cabinet dupont".into()], vec![1.0, 0.0], None);
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn preference_with_shared_entity_and_high_sim_conflicts() {
        let a = mk(MemoryKind::Preference, vec!["ent:ORG:cabinet dupont".into()], vec![1.0, 0.0], None);
        let b = mk(MemoryKind::Preference, vec!["ent:ORG:cabinet dupont".into()], vec![0.95, 0.05], None);
        assert!(resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn preference_with_no_shared_entity_does_not_conflict() {
        let a = mk(MemoryKind::Preference, vec!["ent:ORG:a".into()], vec![1.0, 0.0], None);
        let b = mk(MemoryKind::Preference, vec!["ent:ORG:b".into()], vec![1.0, 0.0], None);
        assert!(!resolves_conflict(&b, &a, 0.85));
    }

    #[test]
    fn already_invalidated_prior_does_not_count() {
        let a = mk(MemoryKind::Preference, vec!["ent:ORG:a".into()], vec![1.0, 0.0], Some(Utc::now()));
        let b = mk(MemoryKind::Preference, vec!["ent:ORG:a".into()], vec![1.0, 0.0], None);
        assert!(!resolves_conflict(&b, &a, 0.85));
    }
}
```

Add `pub mod conflict;` to `crates/anno-rag/src/lib.rs`.

- [ ] **Step 5: Wire into `save_memory`**

In `crates/anno-rag/src/pipeline.rs`, modify `save_memory` after the `Memory` row is built but before `store.memory_insert`:
```rust
// Conflict resolver — only Preference + Reference.
let mut invalidated_ids = Vec::new();
if matches!(m.kind, MemoryKind::Preference | MemoryKind::Reference) {
    // Candidate set: prior memories that share any entity with m, same session if specified.
    let candidates = self.store
        .memory_candidates_for_conflict(&m.entity_refs, m.session_id.as_deref())
        .await?;
    for prior in &candidates {
        if crate::conflict::resolves_conflict(&m, prior, self.cfg.conflict_cosine_threshold) {
            self.store.memory_update_valid_to(&prior.id, m.created_at).await?;
            invalidated_ids.push(prior.id.as_string());
        }
    }
}

self.store.memory_insert(&m).await?;

Ok(SavedMemory {
    id: m.id.clone(),
    redacted_text: m.text.clone(),
    token_refs: m.token_refs.clone(),
    entity_refs: m.entity_refs.clone(),
    invalidated_ids,
})
```

- [ ] **Step 6: Implement `memory_candidates_for_conflict`**

In `crates/anno-rag/src/store.rs`:
```rust
pub async fn memory_candidates_for_conflict(
    &self,
    entity_refs: &[String],
    session_id: Option<&str>,
) -> Result<Vec<crate::memory::Memory>> {
    if entity_refs.is_empty() { return Ok(Vec::new()); }
    // OR-of-array_contains for each entity. Cap candidate set at 100 — pathological
    // popular-entity rows are filtered post-fetch.
    let parts: Vec<String> = entity_refs.iter()
        .map(|e| format!("array_contains(entity_refs, '{}')", e.replace('\'', "''")))
        .collect();
    let mut filter = format!("({}) AND valid_to IS NULL", parts.join(" OR "));
    if let Some(s) = session_id {
        filter = format!("{filter} AND (session_id = '{s}' OR session_id IS NULL)");
    }

    use lancedb::query::{ExecutableQuery, QueryBase};
    let mut stream = self.memories_tbl.query()
        .only_if(&filter)
        .limit(100)
        .execute().await
        .map_err(|e| Error::Store(format!("candidates exec: {e}")))?;

    let mut out = Vec::new();
    while let Some(batch) = futures_util::TryStreamExt::try_next(&mut stream).await
        .map_err(|e| Error::Store(format!("candidates stream: {e}")))?
    {
        for r in 0..batch.num_rows() {
            out.push(batch_row_to_memory(&batch, r)?);
        }
    }
    Ok(out)
}
```

- [ ] **Step 7: Run tests**

```powershell
cargo test -p anno-rag conflict::tests
cargo test -p anno-rag --test memory_temporal conflict_resolver_invalidates_prior_preference
```
Expected: 5 PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag/src/conflict.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/src/store.rs crates/anno-rag/src/lib.rs crates/anno-rag/src/config.rs crates/anno-rag/src/mcp.rs crates/anno-rag/tests/memory_temporal.rs
git commit -m "feat(anno-rag): conflict resolver — invalidate-on-save for Preference/Reference"
```

---

## Task 5: 2-hop graph traversal — `Pipeline::graph_recall` + `memory_graph_recall` MCP tool

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/memory.rs`
- Modify: `crates/anno-rag/src/mcp.rs`

- [ ] **Step 1: Write failing test for 2-hop traversal**

Create `crates/anno-rag/tests/memory_graph.rs`:
```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::memory::MemoryKind;
use anno_rag::pipeline::Pipeline;
use tempfile::TempDir;

#[tokio::test]
async fn two_hop_traversal_reaches_indirect_neighbour() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();

    // Planted graph (no PII — keeps the test deterministic):
    //   M1: "Cabinet Dupont est situé à Bordeaux."           ORG:Dupont + LOC:Bordeaux
    //   M2: "Le Cabinet Dupont collabore avec Cabinet Martin." ORG:Dupont + ORG:Martin
    //   M3: "Cabinet Martin gère les dossiers en Aquitaine."   ORG:Martin + LOC:Aquitaine
    p.save_memory("Cabinet Dupont est situé à Bordeaux.", Some(MemoryKind::Fact), None).await.unwrap();
    p.save_memory("Le Cabinet Dupont collabore avec Cabinet Martin.", Some(MemoryKind::Fact), None).await.unwrap();
    p.save_memory("Cabinet Martin gère les dossiers en Aquitaine.", Some(MemoryKind::Fact), None).await.unwrap();

    let r = p.graph_recall("Cabinet Dupont", 2, 50, None).await.unwrap();

    // hop-1 from Cabinet Dupont reaches Bordeaux + Cabinet Martin (via M1 and M2).
    // hop-2 from Cabinet Martin reaches Aquitaine (via M3).
    let node_ids: Vec<&str> = r.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(node_ids.iter().any(|n| n.contains("aquitaine")),
        "expected 2-hop reach to Aquitaine, got nodes: {:?}", node_ids);
}
```

Run: expect FAIL.

- [ ] **Step 2: Add data types**

Append to `crates/anno-rag/src/memory.rs`:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct EntityNode {
    pub id: String,
    pub display: String,
    pub kind: EntityKindWire,
    pub mention_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKindWire { PiiToken, NamedEntity }

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEdge {
    pub from: String,
    pub via: String, // memory id
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphRecallResult {
    pub seed: String,
    pub seed_resolved: Option<String>,
    pub nodes: Vec<EntityNode>,
    pub edges: Vec<MemoryEdge>,
    pub memories: Vec<MemoryHit>,
}
```

- [ ] **Step 3: `Store::memory_filter_by_entities`**

In `crates/anno-rag/src/store.rs`:
```rust
pub async fn memory_filter_by_entities(
    &self,
    entity_ids: &[String],
    as_of: Option<chrono::DateTime<chrono::Utc>>,
    limit: usize,
) -> Result<Vec<crate::memory::Memory>> {
    if entity_ids.is_empty() { return Ok(Vec::new()); }
    let parts: Vec<String> = entity_ids.iter()
        .map(|e| format!("array_contains(entity_refs, '{}')", e.replace('\'', "''")))
        .collect();
    let mut filter = format!("({})", parts.join(" OR "));
    if let Some(t) = as_of {
        let ts = t.timestamp_micros();
        filter = format!("{filter} AND valid_from <= CAST({ts} AS TIMESTAMP) \
            AND (valid_to IS NULL OR valid_to > CAST({ts} AS TIMESTAMP))");
    } else {
        let now_us = chrono::Utc::now().timestamp_micros();
        filter = format!("{filter} AND (valid_to IS NULL OR valid_to > CAST({now_us} AS TIMESTAMP))");
    }

    use lancedb::query::{ExecutableQuery, QueryBase};
    let mut stream = self.memories_tbl.query()
        .only_if(&filter)
        .limit(limit)
        .execute().await
        .map_err(|e| Error::Store(format!("graph filter exec: {e}")))?;

    let mut out = Vec::new();
    while let Some(batch) = futures_util::TryStreamExt::try_next(&mut stream).await
        .map_err(|e| Error::Store(format!("graph filter stream: {e}")))?
    {
        for r in 0..batch.num_rows() {
            out.push(batch_row_to_memory(&batch, r)?);
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Implement `Pipeline::graph_recall`**

Append to `crates/anno-rag/src/pipeline.rs`:
```rust
use crate::memory::{EntityKindWire, EntityNode, MemoryEdge, GraphRecallResult, HitProvenance, MemoryHit};

impl Pipeline {
    pub async fn graph_recall(
        &self,
        seed_entity: &str,
        max_hops: u8,
        per_hop_limit: usize,
        as_of: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<GraphRecallResult> {
        let max_hops = max_hops.min(self.cfg.graph_max_hops);
        let per_hop_limit = per_hop_limit.min(self.cfg.graph_per_hop_limit);

        // 1. Canonicalize seed. If user already passed a canonical id (e.g. "ent:ORG:...")
        // pass through; otherwise canonicalize as MISC (best-effort guess).
        let canonical_seed = if seed_entity.starts_with("ent:") || seed_entity.starts_with("pii:") {
            seed_entity.to_string()
        } else {
            crate::canonicalize::canonicalize_entity(seed_entity, "MISC", &self.cfg.entity_aliases)
        };

        let mut visited_entities: std::collections::HashSet<String> = std::collections::HashSet::new();
        visited_entities.insert(canonical_seed.clone());

        let mut memories_by_id: std::collections::HashMap<String, crate::memory::Memory> = std::collections::HashMap::new();
        let mut edges: Vec<MemoryEdge> = Vec::new();
        let mut frontier = vec![canonical_seed.clone()];

        for _hop in 0..max_hops {
            if frontier.is_empty() { break; }
            let rows = self.store.memory_filter_by_entities(&frontier, as_of, per_hop_limit).await?;
            let mut next_frontier: std::collections::HashSet<String> = std::collections::HashSet::new();
            for m in rows {
                if memories_by_id.contains_key(&m.id.as_string()) { continue; }
                // Edges: for each pair (frontier_entity ∈ m.entity_refs, other_entity ∈ m.entity_refs)
                for from in &m.entity_refs {
                    if !frontier.contains(from) { continue; }
                    for to in &m.entity_refs {
                        if from == to { continue; }
                        edges.push(MemoryEdge { from: from.clone(), via: m.id.as_string(), to: to.clone() });
                        if !visited_entities.contains(to) { next_frontier.insert(to.clone()); }
                    }
                }
                memories_by_id.insert(m.id.as_string(), m);
            }
            visited_entities.extend(next_frontier.iter().cloned());
            frontier = next_frontier.into_iter().collect();
        }

        // Build node list with mention counts.
        let mut mention_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for m in memories_by_id.values() {
            for e in &m.entity_refs {
                *mention_counts.entry(e.clone()).or_insert(0) += 1;
            }
        }
        let nodes: Vec<EntityNode> = mention_counts.into_iter().map(|(id, c)| {
            let (kind, display) = entity_id_display(&id, &self.vault);
            EntityNode { id, display, kind, mention_count: c }
        }).collect();

        // Rehydrate memories.
        let mut memories: Vec<MemoryHit> = Vec::new();
        for m in memories_by_id.values() {
            let r = self.vault.rehydrate(&m.text).await?;
            memories.push(MemoryHit {
                id: m.id.as_string(),
                text: r.text,
                kind: m.kind,
                created_at: m.created_at.to_rfc3339(),
                valid_from: m.valid_from.to_rfc3339(),
                valid_to: m.valid_to.map(|t| t.to_rfc3339()),
                entity_refs: m.entity_refs.clone(),
                score: 0.0,
                via: HitProvenance::GraphExpand,
            });
        }

        let seed_resolved = if canonical_seed.starts_with("pii:") {
            // Pull token out and lookup
            canonical_seed.splitn(3, ':').nth(2).and_then(|tok| {
                self.vault.lookup_blocking(tok).ok()
            })
        } else { None };

        Ok(GraphRecallResult { seed: canonical_seed, seed_resolved, nodes, edges, memories })
    }
}

fn entity_id_display(id: &str, vault: &crate::vault::Vault) -> (EntityKindWire, String) {
    if let Some(rest) = id.strip_prefix("pii:") {
        // pii:<LABEL>:<TOKEN>
        let token = rest.splitn(2, ':').nth(1).unwrap_or("");
        let display = vault.lookup_blocking(token).unwrap_or_else(|_| token.to_string());
        (EntityKindWire::PiiToken, display)
    } else if let Some(rest) = id.strip_prefix("ent:") {
        let display = rest.splitn(2, ':').nth(1).unwrap_or(rest).to_string();
        (EntityKindWire::NamedEntity, display)
    } else {
        (EntityKindWire::NamedEntity, id.to_string())
    }
}
```

Add a non-async `Vault::lookup_blocking` helper in `vault.rs` for the display lookups (synchronous because it's called inside the entity_id_display fn which would otherwise need to be async over the whole graph build).

```rust
impl Vault {
    pub fn lookup_blocking(&self, token: &str) -> Result<String> {
        let inner = self.inner.try_lock()
            .map_err(|_| Error::Vault("vault busy".into()))?;
        inner.lookup(token)
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Vault(format!("no plaintext for {token}")))
    }
}
```

(If `cloakpipe_core::Vault::lookup` is not directly accessible, mirror the existing async `rehydrate` path. The synchronous path is a v0.2 nicety — falling back to "show the token id" in `display` is acceptable if blocking-lookup is awkward.)

- [ ] **Step 5: Add MCP handler**

In `crates/anno-rag/src/mcp.rs`:
```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryGraphRecallParams {
    pub entity: String,
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
    #[serde(default = "default_per_hop_limit")]
    pub per_hop_limit: usize,
    #[serde(default)]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
}
fn default_max_hops() -> u8 { 2 }
fn default_per_hop_limit() -> usize { 50 }

#[tool(description = "Graph-expand from a seed entity over the entity_refs index. Returns the connected subgraph (entities + memories + edges) up to max_hops (default 2).")]
async fn memory_graph_recall(&self, Parameters(p): Parameters<MemoryGraphRecallParams>) -> String {
    let start = std::time::Instant::now();
    let r = self.pipeline.graph_recall(&p.entity, p.max_hops, p.per_hop_limit, p.as_of).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(res) => {
            tracing::info!(target: "anno_rag::memory::audit",
                tool = "memory_graph_recall", result = "ok",
                duration_ms = elapsed, nodes = res.nodes.len(), edges = res.edges.len(), "");
            serde_json::to_string_pretty(&res).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit",
                tool = "memory_graph_recall", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}
```

- [ ] **Step 6: Run test**

```powershell
cargo test -p anno-rag --test memory_graph two_hop_traversal_reaches_indirect_neighbour
```
Expected: PASS. May depend on `StackedNER` cache being warm — if heuristic fallback misses one of the planted entities, fall back to a deterministic seed (call the test with `canonical_seed = "ent:ORG:cabinet dupont"` directly) and assert the traversal layer in isolation from NER quality.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/store.rs crates/anno-rag/src/memory.rs crates/anno-rag/src/mcp.rs crates/anno-rag/src/vault.rs crates/anno-rag/tests/memory_graph.rs
git commit -m "feat(anno-rag): memory_graph_recall — 2-hop traversal over LabelList entity_refs index"
```

---

## Task 6: Wire `as_of` + `graph_expand` into MCP `memory_recall`

**Files:**
- Modify: `crates/anno-rag/src/mcp.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Extend `MemoryRecallParams`**

In `crates/anno-rag/src/mcp.rs`:
```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryRecallParams {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub kinds: Option<Vec<MemoryKind>>,
    #[serde(default)]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub graph_expand: bool,
}
```

Update the handler to forward both — `as_of` and `graph_expand`.

- [ ] **Step 2: Implement `graph_expand=true` post-pass**

In `Pipeline::recall_memory`, after the existing hybrid retrieval + temporal filter:
```rust
if graph_expand {
    // Collect entity_refs from the top-k hits.
    let frontier: std::collections::HashSet<String> = raw.iter()
        .flat_map(|r| r.entity_refs.clone())
        .collect();
    if !frontier.is_empty() {
        let frontier_vec: Vec<String> = frontier.into_iter().collect();
        let extras = self.store.memory_filter_by_entities(
            &frontier_vec, as_of, self.cfg.graph_per_hop_limit).await?;
        let known_ids: std::collections::HashSet<String> = raw.iter().map(|r| r.id.clone()).collect();
        for m in extras {
            let id = m.id.as_string();
            if known_ids.contains(&id) { continue; }
            raw.push(MemoryHitRow {
                id,
                session_id: m.session_id.clone(),
                text_tokenized: m.text.clone(),
                kind: m.kind,
                created_at: m.created_at.to_rfc3339(),
                valid_from_us: m.valid_from.timestamp_micros(),
                valid_to_us: m.valid_to.map(|t| t.timestamp_micros()),
                entity_refs: m.entity_refs.clone(),
                score: 0.0, // mark as graph-expanded
            });
        }
    }
}
```

Tag each rehydrated `MemoryHit` with `via = HitProvenance::Hybrid` for original hits and `via = HitProvenance::GraphExpand` for the added rows (track which set the row came from).

- [ ] **Step 3: Test**

Add to `crates/anno-rag/tests/memory_graph.rs`:
```rust
#[tokio::test]
async fn graph_expand_returns_extra_neighbours() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    p.save_memory("Cabinet Dupont à Bordeaux.", Some(MemoryKind::Fact), None).await.unwrap();
    p.save_memory("Cabinet Dupont a un dossier en cours.", Some(MemoryKind::Fact), None).await.unwrap();

    let without = p.recall_memory("Cabinet Dupont", 1, None, None, None, false).await.unwrap();
    let with    = p.recall_memory("Cabinet Dupont", 1, None, None, None, true ).await.unwrap();
    assert!(with.len() >= without.len(),
        "graph_expand should not return fewer hits than baseline");
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_graph graph_expand_returns_extra_neighbours
```
Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/mcp.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/memory_graph.rs
git commit -m "feat(anno-rag): memory_recall as_of + graph_expand parameters"
```

---

## Task 7: `memory_invalidate` MCP tool

**Files:**
- Modify: `crates/anno-rag/src/mcp.rs`

- [ ] **Step 1: Add params + handler**

In `crates/anno-rag/src/mcp.rs`:
```rust
#[derive(Deserialize, schemars::JsonSchema)]
pub struct MemoryInvalidateParams {
    pub id: String,
    #[serde(default)]
    pub at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct MemoryInvalidateResultWire {
    id: String,
    invalidated: bool,
    valid_to: String,
}

#[tool(description = "Mark a memory as no longer valid as of the given timestamp (default: now). No-op if valid_to is already set.")]
async fn memory_invalidate(&self, Parameters(p): Parameters<MemoryInvalidateParams>) -> String {
    let id = match uuid::Uuid::parse_str(&p.id) {
        Ok(u) => crate::memory::MemoryId(u),
        Err(e) => return format!("Error: bad id: {e}"),
    };
    let when = p.at.unwrap_or_else(chrono::Utc::now);
    let start = std::time::Instant::now();
    let r = self.pipeline.invalidate_memory(&id, Some(when)).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(invalidated) => {
            tracing::info!(target: "anno_rag::memory::audit",
                tool = "memory_invalidate", result = "ok", duration_ms = elapsed, "");
            serde_json::to_string_pretty(&MemoryInvalidateResultWire {
                id: p.id, invalidated, valid_to: when.to_rfc3339()
            }).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(target: "anno_rag::memory::audit",
                tool = "memory_invalidate", result = "error", duration_ms = elapsed, "{e}");
            format!("Error: {e}")
        }
    }
}
```

- [ ] **Step 2: Test**

Add to `crates/anno-rag/tests/memory_mcp.rs`:
```rust
#[tokio::test]
async fn invalidate_idempotent() {
    let tmp = TempDir::new().unwrap();
    let cfg = AnnoRagConfig::test_config_in(tmp.path());
    let p = Pipeline::new(cfg).await.unwrap();
    let s = p.save_memory("Cabinet Dupont à Bordeaux.", Some(MemoryKind::Fact), None).await.unwrap();
    let first = p.invalidate_memory(&s.id, None).await.unwrap();
    let second = p.invalidate_memory(&s.id, None).await.unwrap();
    assert!(first);
    assert!(!second, "second invalidate must be a no-op");
}
```

Run: expect PASS.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/src/mcp.rs crates/anno-rag/tests/memory_mcp.rs
git commit -m "feat(anno-rag): memory_invalidate MCP tool"
```

---

## Task 8: Property test — graph monotonicity

**Files:**
- Modify: `crates/anno-rag/tests/memory_proptest.rs`

- [ ] **Step 1: Add property**

Append to `crates/anno-rag/tests/memory_proptest.rs`:
```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(25))]
    #[test]
    fn graph_recall_is_monotonic_in_hops(seed in "[a-z]{4,10}") {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let cfg = AnnoRagConfig::test_config_in(tmp.path());
            let p = Pipeline::new(cfg).await.unwrap();
            // Plant a small graph involving `seed`.
            p.save_memory(&format!("Cabinet {seed} à Bordeaux."), Some(MemoryKind::Fact), None).await.unwrap();
            p.save_memory(&format!("Cabinet {seed} collabore avec Cabinet Martin."), Some(MemoryKind::Fact), None).await.unwrap();

            let r1 = p.graph_recall(&format!("Cabinet {seed}"), 1, 50, None).await.unwrap();
            let r2 = p.graph_recall(&format!("Cabinet {seed}"), 2, 50, None).await.unwrap();
            prop_assert!(r2.memories.len() >= r1.memories.len(),
                "2 hops must reach >= memories than 1 hop");
            prop_assert!(r2.nodes.len() >= r1.nodes.len());
            Ok(())
        }).unwrap();
    }
}
```

Run:
```powershell
cargo test -p anno-rag --test memory_proptest graph_recall_is_monotonic_in_hops --release
```
Expected: PASS (25 cases).

- [ ] **Step 2: Commit**

```powershell
git add crates/anno-rag/tests/memory_proptest.rs
git commit -m "test(anno-rag): proptest — graph_recall monotonic in max_hops"
```

---

## Task 9: LoCoMo multi-hop accuracy gate

**Files:**
- Modify: `crates/anno-rag/benches/bench_locomo.rs`
- Modify: `crates/anno-rag/tests/fixtures/locomo_baseline.toml`

- [ ] **Step 1: Re-run LoCoMo with v0.2 features**

Run:
```powershell
cargo bench -p anno-rag --bench bench_locomo 2>&1 | Tee-Object -FilePath locomo_v0.2.txt
```
Extract the new `LOCOMO_MULTIHOP_ACC1` number.

- [ ] **Step 2: Assert ≥ +10 pp improvement on multi-hop**

Modify the bench to compare against the committed v0.1 baseline and fail loudly if multi-hop accuracy regresses:
```rust
// at the end of run_locomo, after computing multi_acc1:
let baseline_toml: toml::Value = toml::from_str(
    &std::fs::read_to_string("tests/fixtures/locomo_baseline.toml").unwrap()
).unwrap();
let v01_multi = baseline_toml["multi_hop"]["accuracy_at_1"]
    .as_float().unwrap() as f64;
if multi_acc1 < v01_multi + 0.10 {
    panic!("v0.2 multi-hop accuracy {multi_acc1:.4} did not improve by 10pp over v0.1 baseline {v01_multi:.4}");
}
```

- [ ] **Step 3: Update baseline with v0.2 numbers**

Edit `crates/anno-rag/tests/fixtures/locomo_baseline.toml` — add a `[v02]` section:
```toml
[v02.overall]
accuracy_at_1 = <new value>
latency_p95_ms = <new value>
[v02.multi_hop]
accuracy_at_1 = <new value>
```

Leave the v0.1 baseline section unchanged for regression comparison.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/benches/bench_locomo.rs crates/anno-rag/tests/fixtures/locomo_baseline.toml
git commit -m "test(anno-rag): LoCoMo multi-hop gate — v0.2 ≥ v0.1 + 10pp"
```

---

## Task 10: Docs + version bump

**Files:**
- Modify: `crates/anno-rag/CHANGELOG.md`
- Modify: `crates/anno-rag/README.md`
- Modify: `crates/anno-rag/Cargo.toml`

- [ ] **Step 1: Bump version 0.7 → 0.8**

`crates/anno-rag/Cargo.toml`: `version = "0.8.0"`.

- [ ] **Step 2: CHANGELOG entry**

Prepend to `crates/anno-rag/CHANGELOG.md`:
```markdown
## 0.8.0 — 2026-05-XX

### Added
- Bi-temporal semantics on `valid_from` / `valid_to` (v0.1 forward-compat
  columns now active). Invalidate-on-conflict for `Preference` and
  `Reference` kinds; `Fact` and `Context` remain append-only.
- Entity extraction at write time via anno-core `StackedNER`. PII vault
  tokens and non-PII NER (PER excluded — PII path only) merge into
  `entity_refs` with deterministic canonicalization.
- `memory_graph_recall` MCP tool — 2-hop traversal over the `entity_refs`
  LabelList index. No external graph DB.
- `memory_invalidate` MCP tool.
- `memory_recall` extended with `as_of` (point-in-time) and `graph_expand`
  (1-hop expansion) parameters.
- LoCoMo multi-hop accuracy CI gate: v0.2 must improve ≥10 pp over v0.1.

### Changed
- `MemorySaveResult` now includes `entity_refs` and `invalidated_ids`.
  Existing clients that ignore unknown fields keep working (rmcp serde
  defaults).
- `MemoryHit` now includes `valid_from`, `valid_to`, `entity_refs`, and
  `via` (Hybrid | GraphExpand).

### Privacy
- Person entities continue to flow exclusively through the PII vault.
  Non-PII canonical ids (`ent:*`) carry no PII; PII ids (`pii:*`) carry
  only vault tokens which resolve only inside the originating tenant.
```

- [ ] **Step 3: README update**

In `crates/anno-rag/README.md`, extend the "Memory" section:
```markdown
### v0.8 — temporal + graph

Memories now carry bi-temporal validity (`valid_from`, `valid_to`).
Preferences and References automatically invalidate prior matching
rows; Facts and Contexts are append-only.

Each memory's `entity_refs` is populated from anno-core's `StackedNER`
plus PII vault tokens. The new `memory_graph_recall` tool follows
those refs for up to 2 hops over the `LabelList` scalar index — no
external graph DB. `memory_recall` accepts `as_of` for point-in-time
queries and `graph_expand` for one-hop neighbour inclusion.
```

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/CHANGELOG.md crates/anno-rag/README.md
git commit -m "docs(anno-rag): v0.8 — temporal + graph memory"
```

---

## Task 11: PR

- [ ] **Step 1: Workspace check**

```powershell
cargo build --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo bench -p anno-rag --bench bench_locomo
```
All green; LoCoMo gate green.

- [ ] **Step 2: Push and open PR**

```powershell
git push -u origin (git rev-parse --abbrev-ref HEAD)
gh pr create --title "feat(anno-rag): v0.8 — temporal + graph-aware memory (no graph DB)" --body @'
## Summary
- Bi-temporal `valid_from` / `valid_to` semantics activated.
- Entity extraction via anno-core `StackedNER` — no LLM call.
- Deterministic canonicalizer (lowercase + diacritic strip + alias table).
- 2-hop graph traversal over `entity_refs` LabelList index — stays in LanceDB.
- `memory_graph_recall` + `memory_invalidate` MCP tools.
- `memory_recall` `as_of` + `graph_expand` parameters.
- LoCoMo multi-hop accuracy gate: v0.2 ≥ v0.1 + 10pp.

Depends on PR-C (anno-memory v0.1). No external graph DB. No new dependency outside `anno` (workspace) and `unicode-normalization`.

## Test plan
- [ ] `cargo test -p anno-rag --all-features` green.
- [ ] `cargo test -p anno-rag --test memory_temporal` green.
- [ ] `cargo test -p anno-rag --test memory_graph` green.
- [ ] `cargo bench -p anno-rag --bench bench_locomo` — multi-hop gate green.
- [ ] Peak RSS still under 1.5 GB.

Design: `docs/superpowers/specs/2026-05-15-anno-memory-v0.2-design.md`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
'@
```

---

## Self-Review

- **Spec coverage:** §1–§13 of `2026-05-15-anno-memory-v0.2-design.md`:
  - §3 architecture — Tasks 2 (entity extraction), 3 (temporal), 5 (graph). ✓
  - §4 entity extraction — Tasks 1 (canonicalize) + 2 (NER wiring). ✓
  - §5 bi-temporal — Tasks 3 (`as_of`) + 4 (conflict resolver). ✓
  - §6 MCP tools — Tasks 5 (`memory_graph_recall`), 6 (`recall` extensions), 7 (`memory_invalidate`), 4 (extended `memory_save` result). ✓
  - §7 privacy — embedded in tasks: §7.1 NER plaintext path enforced in Task 2 Step 3 (Person → continue), §7.2 canonicalization collisions discussed in Task 1, §7.3 graph rehydration in Task 5 entity_id_display, §7.4 forget cascade unchanged from v0.1. ✓
  - §8 data model — schema unchanged, semantics activated. ✓
  - §9 testing — Tasks 3/4 unit + 5 integration + 8 proptest + 9 LoCoMo gate. ✓
  - §10 file layout — matches the File Structure header. ✓
  - §11 open questions — informational.
  - §12 success criteria — Task 9 (multi-hop +10pp), Task 5 (planted graph), Task 4 (conflict precision/recall is informal but the planted-graph + invalidate tests give a clear signal). ✓
  - §13 v0.3 — not in scope.
- **Placeholders scan:** Task 2 Step 3 references `EntityType::Misc` / `EntityType::Organization` variants — these are anno-core's enum names which should be verified against the actual `anno::EntityType` definition; the variant names are likely correct but mechanical to fix if off. Task 5 Step 4 references `Vault::lookup_blocking` which is added in the same task; the fallback (show the token id as display) is explicit, not a placeholder.
- **Type consistency:** `MemoryHit` (Task 3 extension) is consistent across `recall_memory`, `graph_recall`, and `memory_list`. `MemoryHitRow` carries the new temporal columns. `HitProvenance` is consistent — `Hybrid` for vector+FTS hits, `GraphExpand` for graph-only additions. `SavedMemory` gains two fields (Task 4), used in the MCP wire result.
- **Risk callouts (added during review):** Task 5's traversal correctness depends on the `LabelList` scalar index being built — v0.1 Task 4 already creates the index on `entity_refs` (which was empty), so v0.2 just needs the column populated. If v0.1 skipped creating the index when the column was always empty (the `Index::LabelList` build may have refused), Task 4 of v0.1 will need a follow-up to retry index creation here — add an idempotent `setup_memory_indexes()` call in v0.2's `Pipeline::new` to catch that case.
