# MCP UX Surface (Spec C) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the ~50-tool anno-rag MCP surface legible to an agent driving it from Claude Desktop — honest empty results, a shared response-envelope convention, search-hit→handle resolution, lifecycle guidance, and registry hygiene.

**Architecture:** One documented response-envelope convention (`status`/`message`/`hint` + payload) codifying the existing `corpus_required` shape, with each Spec C item implemented as an independent unit behind it. Work lands in 7 phases, each an independently shippable PR.

**Tech Stack:** Rust, rmcp 1.6 (`#[tool_router]`/`#[tool_handler]`), `serde_json`, LanceDB (`anno-rag` store), SQLite (`anno-corpus-store`), tokio.

**Spec:** [`docs/superpowers/specs/2026-06-24-mcp-ux-surface-design.md`](../specs/2026-06-24-mcp-ux-surface-design.md)

**Conventions for every task:**
- Build isolation: a `cargo`/`rustc` process must not already be running (`Get-Process cargo,rustc`). `CARGO_TARGET_DIR=E:\cargo-target` is set in `.cargo/config.toml`.
- Fast per-crate test: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate>`, or a single test: `cargo test -p <crate> <filter> -- --nocapture`.
- Run `cargo fmt` + `cargo clippy --jobs 2 -p <crate>` before each commit; commit `fmt` separately if it touches unrelated lines.
- Never run `cargo test --workspace` / `cargo nextest run --workspace` locally.

---

## File Structure

| File | Responsibility | Phase |
|------|----------------|-------|
| `crates/anno-rag-mcp/src/envelope.rs` (create) | Shared `status` constants + `envelope()` builder | 0 |
| `crates/anno-rag-mcp/src/lib.rs` (modify) | Wire envelope into tools; U2 statuses; U7 next_step; U8 not_ready; D2 aliases; list_tools filter | 0,1,3,4 |
| `crates/anno-rag-mcp/src/detect_label.rs` (create) | Clean category/source label serialization (U6 residual) | 0 |
| `crates/anno-corpus-store/src/store.rs` (modify) | `relative_path_by_document_id` forward lookup (U1) | 2 |
| `crates/anno-rag-mcp/src/search.rs` (modify) | Handle resolution on hits (U1); totalize matrix (D1) | 2,6 |
| `crates/anno-rag-mcp/src/wire.rs` (modify) | `SearchHitWire.document_handle` field (U1) | 2 |
| `crates/anno-rag/src/legal/store.rs` (modify) | `document_has_kg_nodes` per-doc check (U2) | 1 |
| `crates/anno-rag-mcp/src/warmup_history.rs` (create) | Persist/load `download_ms`/`load_ms` for ETA (D3) | 3 |
| `crates/anno-rag-mcp/src/deprecated.rs` (create) | Deprecated-tool registry + `list_tools` filter (U4/D2) | 4 |

> **Scope note:** This is one subsystem (MCP UX). The 7 phases are independently shippable; execute one PR per phase in the landing order below. If you prefer, stop after any phase — each leaves the tree green.

---

## Phase 0 — Envelope convention (§0) + clean detect labels (§2/U6)

### Task 1: Envelope helper module

**Files:**
- Create: `crates/anno-rag-mcp/src/envelope.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs` (add `mod envelope;` near the other `mod` lines, ~line 13-26)

- [ ] **Step 1: Write the failing test**

In `crates/anno-rag-mcp/src/envelope.rs`:

```rust
//! Shared response-envelope convention for MCP tool outputs.
//!
//! Every non-trivial tool response carries a top-level machine-stable `status`,
//! a human `message`, an actionable `hint`, plus a status-specific payload.
//! See `docs/superpowers/specs/2026-06-24-mcp-ux-surface-design.md` §0.

use serde_json::{json, Value};

/// Closed set of machine-stable tool statuses.
pub(crate) mod status {
    pub(crate) const OK: &str = "ok";
    pub(crate) const EMPTY: &str = "empty";
    pub(crate) const NOT_ENRICHED: &str = "not_enriched";
    pub(crate) const UNKNOWN_DOCUMENT: &str = "unknown_document";
    pub(crate) const CORPUS_REQUIRED: &str = "corpus_required";
    pub(crate) const SETUP_REQUIRED: &str = "setup_required";
    pub(crate) const NOT_READY: &str = "not_ready";
    pub(crate) const DEGRADED: &str = "degraded";
}

/// Build a status envelope: `{status, message, hint, ...payload}`.
/// `payload` must be a JSON object (or `Value::Null` for none); its keys are
/// merged at the top level so callers can attach `available`, `next_step`, etc.
pub(crate) fn envelope(status: &str, message: &str, hint: &str, payload: Value) -> Value {
    let mut base = json!({ "status": status, "message": message, "hint": hint });
    if let (Some(obj), Some(extra)) = (base.as_object_mut(), payload.as_object()) {
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_merges_payload_at_top_level() {
        let v = envelope(
            status::NOT_ENRICHED,
            "Document non enrichi.",
            "Réindexez via index(profile=legal).",
            json!({ "doc_id": "abc" }),
        );
        assert_eq!(v["status"], "not_enriched");
        assert_eq!(v["message"], "Document non enrichi.");
        assert_eq!(v["hint"], "Réindexez via index(profile=legal).");
        assert_eq!(v["doc_id"], "abc");
    }

    #[test]
    fn envelope_tolerates_null_payload() {
        let v = envelope(status::EMPTY, "Aucun résultat.", "Élargissez la requête.", Value::Null);
        assert_eq!(v["status"], "empty");
        assert!(v.get("doc_id").is_none());
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/anno-rag-mcp/src/lib.rs`, add alongside the existing `mod` declarations (after `mod corpus_sync;`, ~line 15):

```rust
mod envelope;
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test -p anno-rag-mcp envelope::tests -- --nocapture`
Expected: PASS (2 tests).

- [ ] **Step 4: fmt + clippy**

Run: `cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp`
Expected: no warnings on the new file.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/envelope.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): shared response-envelope convention (Spec C §0)"
```

---

### Task 2: Refactor `corpus_required` to use the envelope

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs:831-847` (the `CorpusGuardError::CorpusRequired` arm in `legal_search_impl`) and `:1335` (the second occurrence)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module at the bottom of `crates/anno-rag-mcp/src/lib.rs` (search for `mod tests` / `mod warmup_phase_tests`; add a fresh `#[cfg(test)]` block if simpler):

```rust
#[test]
fn corpus_required_envelope_has_convention_fields() {
    use crate::envelope::{envelope, status};
    let v = envelope(
        status::CORPUS_REQUIRED,
        "Plusieurs dossiers indexés.",
        "Relancez avec corpus_id/alias.",
        serde_json::json!({ "available": [] }),
    );
    assert_eq!(v["status"], "corpus_required");
    assert!(v["available"].is_array());
    assert!(v["message"].is_string());
    assert!(v["hint"].is_string());
}
```

- [ ] **Step 2: Run to verify it passes** (the helper already exists from Task 1)

Run: `cargo test -p anno-rag-mcp corpus_required_envelope -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Rewrite the two `CorpusRequired` arms to call `envelope()`**

Replace the `json!({...})` block at `lib.rs:833-846` with:

```rust
return Ok(crate::envelope::envelope(
    crate::envelope::status::CORPUS_REQUIRED,
    "Plusieurs dossiers indexés. Précisez un dossier ou demandez une recherche transversale.",
    "Relancez avec corpus_id/alias, ou allow_cross_corpus: true pour un contrôle de conflits.",
    serde_json::json!({
        "available": rows
            .iter()
            .map(|c| serde_json::json!({
                "corpus_id": c.corpus_id.as_string(),
                "alias": c.alias,
                "label": c.label_pseudo,
                "health": c.health,
            }))
            .collect::<Vec<_>>(),
    }),
));
```

Apply the equivalent change to the second occurrence near `lib.rs:1335` (same field names; confirm the surrounding `rows`/variable names match before saving).

- [ ] **Step 4: Run the existing corpus disambiguation tests**

Run: `cargo test -p anno-rag-mcp corpus -- --nocapture`
Expected: PASS (no behavioral change — same JSON keys).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "refactor(mcp): route corpus_required through envelope helper"
```

---

### Task 3: Clean detect labels — U6 residual (`e.source`)

**Files:**
- Create: `crates/anno-rag-mcp/src/detect_label.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs` (the `detect` handler, ~line 2437-2467, where `EntityInfo` is built; and `mod detect_label;`)

> PR #81 already cleaned `e.category` (`Custom("IBAN_FR")` → `"IBAN_FR"`). Verify whether `e.source` still uses `format!("{:?}", …)` before implementing; if both are already clean, skip to Step 5 and record that in the commit message.

- [ ] **Step 1: Write the failing test**

In `crates/anno-rag-mcp/src/detect_label.rs`:

```rust
//! Clean, parseable labels for detected-entity category and source,
//! replacing Rust `Debug` formatting in `detect` output. Spec C §2 (U6).

use cloakpipe_core::{DetectionSource, EntityCategory};

/// Stable lowercase source label: `"pattern"`, `"ner"`, `"heuristic"`, …
pub(crate) fn source_label(source: &DetectionSource) -> String {
    match source {
        DetectionSource::Pattern => "pattern",
        DetectionSource::Ner => "ner",
        DetectionSource::Heuristic => "heuristic",
        DetectionSource::Validator => "validator",
    }
    .to_string()
}

/// Stable category label: `Custom("IBAN_FR")` → `"IBAN_FR"`, else snake/lower of the variant.
pub(crate) fn category_label(category: &EntityCategory) -> String {
    match category {
        EntityCategory::Custom(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_category_drops_debug_wrapper() {
        let c = EntityCategory::Custom("IBAN_FR".to_string());
        assert_eq!(category_label(&c), "IBAN_FR");
    }

    #[test]
    fn source_is_lowercase_word() {
        assert_eq!(source_label(&DetectionSource::Pattern), "pattern");
        assert_eq!(source_label(&DetectionSource::Ner), "ner");
    }
}
```

> **Before coding:** confirm the real `DetectionSource` variants with
> `cargo doc`-free check: `rg "enum DetectionSource" -A8 vendor/ crates/` and adjust the match arms to the actual variant names. Do NOT invent variants.

- [ ] **Step 2: Register module + run test**

Add `mod detect_label;` in `lib.rs`. Run: `cargo test -p anno-rag-mcp detect_label::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Use the helpers in the `detect` handler**

In the `detect` handler (`lib.rs` ~2437-2467), where each `EntityInfo { category, source, … }` is built, replace any `format!("{:?}", e.category)` / `format!("{:?}", e.source)` with:

```rust
category: crate::detect_label::category_label(&e.category),
source: crate::detect_label::source_label(&e.source),
```

- [ ] **Step 4: Run detect tests**

Run: `cargo test -p anno-rag-mcp detect -- --nocapture`
Expected: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/detect_label.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "fix(mcp): clean detect source label, finish U6 (Spec C §2)"
```

---

## Phase 1 — Honest empty results on legal tools (§1/U2)

### Task 4: Per-document KG presence check

**Files:**
- Modify: `crates/anno-rag/src/legal/store.rs` (add `document_has_kg_nodes`)
- Test: same file's `#[cfg(test)] mod tests`

- [ ] **Step 1: Read the existing store to find the node table + an existing per-corpus query**

Run: `rg "document_ids_for_corpus|CREATE TABLE|fn .*corpus" crates/anno-rag/src/legal/store.rs`
Note the table that holds per-document graph nodes and the connection accessor pattern.

- [ ] **Step 2: Write the failing test**

Add to `crates/anno-rag/src/legal/store.rs` tests:

```rust
#[test]
fn document_has_kg_nodes_reports_presence() {
    let store = LegalStore::open_in_memory().expect("store"); // use the crate's existing test ctor
    let corpus = test_corpus_id();
    let doc = uuid::Uuid::now_v7();
    assert!(!store.document_has_kg_nodes(corpus, doc).unwrap(), "empty KG → false");
    store.insert_test_node(corpus, doc).expect("seed node"); // use the real insert path
    assert!(store.document_has_kg_nodes(corpus, doc).unwrap(), "after insert → true");
}
```

> Adjust ctor/seed calls (`open_in_memory`, `insert_test_node`, `test_corpus_id`) to the store's actual test helpers found in Step 1. If none exist, seed via the real enrichment insert used by `enricher.rs`.

- [ ] **Step 3: Implement `document_has_kg_nodes`**

```rust
/// True if the legal knowledge graph holds at least one node for `document_id`
/// within `corpus_id`. Cheap existence probe — `LIMIT 1`, no row materialization.
pub fn document_has_kg_nodes(
    &self,
    corpus_id: anno_corpus_core::CorpusId,
    document_id: uuid::Uuid,
) -> Result<bool, LegalStoreError> {
    let conn = self.conn()?; // match the store's existing connection accessor
    let mut stmt = conn.prepare(
        "SELECT 1 FROM legal_nodes \
         WHERE corpus_id = ?1 AND document_id = ?2 LIMIT 1",
    )?;
    let exists = stmt
        .query(rusqlite::params![corpus_id.as_string(), document_id.to_string()])?
        .next()?
        .is_some();
    Ok(exists)
}
```

> Replace `legal_nodes`, `corpus_id`, `document_id` column/table names with the real schema from Step 1. Match the crate's error type (`LegalStoreError` or whatever `store.rs` uses).

- [ ] **Step 4: Run test**

Run: `cargo test -p anno-rag document_has_kg_nodes -- --nocapture`
Expected: PASS.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag && cargo clippy --jobs 2 -p anno-rag
git add crates/anno-rag/src/legal/store.rs
git commit -m "feat(legal): document_has_kg_nodes per-doc presence probe (Spec C U2)"
```

---

### Task 5: Three honest statuses in `legal_risk_review`

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (the `legal_risk_review` impl, ~line 3242+)

- [ ] **Step 1: Find the impl + how it resolves the doc and reaches the legal store**

Run: `rg "legal_risk_review|risk_review_impl|fn .*risk" crates/anno-rag-mcp/src/lib.rs`
Note: how `doc_id`/`scope_id` is resolved (likely `resolve_doc_ref`), how the legal store is accessed, and the current empty return shape (`{"findings": []}`).

- [ ] **Step 2: Write the failing test (unknown_document path)**

Add a `tests` entry in `lib.rs`. Use the lightest reachable seam; if the impl needs a full pipeline, assert at the helper level instead by extracting a pure classifier:

```rust
#[test]
fn empty_status_classifier_picks_the_right_status() {
    use crate::envelope::status;
    // resolved? has_kg? findings_empty? -> expected status
    assert_eq!(crate::legal_empty_status(false, false, true), status::UNKNOWN_DOCUMENT);
    assert_eq!(crate::legal_empty_status(true, false, true), status::NOT_ENRICHED);
    assert_eq!(crate::legal_empty_status(true, true, true), status::EMPTY);
    assert_eq!(crate::legal_empty_status(true, true, false), status::OK);
}
```

- [ ] **Step 3: Implement the pure classifier**

Add a free function near the legal impls in `lib.rs`:

```rust
/// Decide the response status for a legal tool that produced no rows, from
/// three cheap facts: did the handle resolve, does the KG hold nodes for it,
/// is the result set empty. Spec C §1 (U2).
pub(crate) fn legal_empty_status(resolved: bool, has_kg: bool, is_empty: bool) -> &'static str {
    use crate::envelope::status;
    match (resolved, has_kg, is_empty) {
        (false, _, _) => status::UNKNOWN_DOCUMENT,
        (true, false, _) => status::NOT_ENRICHED,
        (true, true, true) => status::EMPTY,
        (true, true, false) => status::OK,
    }
}
```

- [ ] **Step 4: Run classifier test**

Run: `cargo test -p anno-rag-mcp empty_status_classifier -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Wire the classifier + envelope into `legal_risk_review`**

In the impl, after resolving the doc and querying findings, branch before returning:

```rust
let resolved = doc_resolved; // bool from resolve_doc_ref success
let has_kg = if resolved {
    legal_store.document_has_kg_nodes(corpus_id, doc_uuid).unwrap_or(false)
} else { false };
let st = legal_empty_status(resolved, has_kg, findings.is_empty());

if st != crate::envelope::status::OK {
    let (msg, hint) = match st {
        s if s == crate::envelope::status::UNKNOWN_DOCUMENT =>
            ("Document introuvable.", "Vérifiez le doc_id ou listez via sources()."),
        s if s == crate::envelope::status::NOT_ENRICHED =>
            ("Document non enrichi dans le graphe juridique.", "Réindexez via index(path, profile=\"legal\")."),
        _ => ("Aucun risque identifié dans ce document.", "Le document est enrichi ; aucun risque ne correspond aux filtres."),
    };
    return Ok(crate::envelope::envelope(st, msg, hint, serde_json::json!({ "findings": [] })));
}
// else: existing OK return, now wrapped with status: "ok"
```

- [ ] **Step 6: Run + commit**

Run: `cargo test -p anno-rag-mcp legal_risk -- --nocapture` → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): honest empty statuses in legal_risk_review (Spec C U2)"
```

---

### Task 6: Apply the same to the remaining legal D2/D3 tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`legal_extract_contract` ~3139, `legal_extract_case_file` ~3178, `legal_timeline` ~3207, `legal_mandatory_clause_audit` ~3282)

- [ ] **Step 1: For each tool, repeat Task 5 Step 5**

Use the same `legal_empty_status` + `envelope` pattern, with tool-appropriate empty payload key (`rows` / `events` / `clauses`) and message. Example for `legal_timeline`:

```rust
return Ok(crate::envelope::envelope(st, msg, hint, serde_json::json!({ "events": [] })));
```

- [ ] **Step 2: Add one classifier-routing test per tool** mirroring Task 5 Step 2 (assert the tool returns `status: not_enriched` when the KG is empty — use the lightest reachable seam, or assert via the shared `legal_empty_status` if the full path needs models).

- [ ] **Step 3: Run the legal suite**

Run: `cargo test -p anno-rag-mcp legal -- --nocapture`
Expected: PASS.

- [ ] **Step 4: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): honest empty statuses across legal D2/D3 tools (Spec C U2)"
```

---

## Phase 2 — Search-hit → document handle (§10/U1)

### Task 7: Forward lookup `relative_path_by_document_id`

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs` (near `document_id_by_relative_path`, ~line 331)

- [ ] **Step 1: Write the failing test**

Add near `document_id_by_relative_path_roundtrip` (store.rs:904):

```rust
#[test]
fn relative_path_by_document_id_roundtrip() {
    let store = test_store();
    let reg = register_test_corpus(&store);
    let doc_id = store
        .add_document(reg.corpus_id, "legal", None, Some("contrats/x.txt"), "content-1", None)
        .expect("add");
    let got = store
        .relative_path_by_document_id(reg.corpus_id, doc_id)
        .expect("lookup");
    assert_eq!(got.as_deref(), Some("contrats/x.txt"));
    let missing = store
        .relative_path_by_document_id(reg.corpus_id, uuid::Uuid::now_v7())
        .expect("lookup");
    assert_eq!(missing, None);
}
```

> Match the exact `add_document` signature/return from store.rs:261 (it currently takes `relative_path: Option<&str>`); adjust arg order/types to the real signature and how `document_id` is produced.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p anno-corpus-store relative_path_by_document_id -- --nocapture`
Expected: FAIL (method not found).

- [ ] **Step 3: Implement the forward lookup**

```rust
/// Resolve a document's corpus-relative path from its id. Inverse of
/// `document_id_by_relative_path`. Returns `None` if absent or stored without
/// a plaintext path. Spec C §10 (U1).
pub fn relative_path_by_document_id(
    &self,
    corpus_id: CorpusId,
    document_id: DocumentId,
) -> Result<Option<String>, CorpusStoreError> {
    let conn = self.conn()?;
    let mut stmt = conn.prepare(
        "SELECT relative_path FROM corpus_documents \
         WHERE corpus_id = ?1 AND document_id = ?2 LIMIT 1",
    )?;
    let mut rows = stmt.query(params![corpus_id.as_string(), document_id.to_string()])?;
    match rows.next()? {
        Some(r) => Ok(r.get::<_, Option<String>>(0)?),
        None => Ok(None),
    }
}
```

> Match the real `DocumentId` type and `CorpusStoreError` / conn accessor used elsewhere in store.rs.

- [ ] **Step 4: Run test → PASS**

Run: `cargo test -p anno-corpus-store relative_path_by_document_id -- --nocapture`

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt -p anno-corpus-store && cargo clippy --jobs 2 -p anno-corpus-store
git add crates/anno-corpus-store/src/store.rs
git commit -m "feat(corpus-store): relative_path_by_document_id forward lookup (Spec C U1)"
```

---

### Task 8: Verify the join key (`chunk.doc_id == corpus_documents.document_id`)

**Files:**
- Read-only verification + a guard test in `crates/anno-rag-mcp/src/search.rs`

- [ ] **Step 1: Trace the two ids**

Run:
```
rg "add_document\(" crates/anno-rag-mcp/src/lib.rs -B3 -A6
rg "document_id|doc_id" crates/anno-rag/src/ingest.rs
```
Confirm the `document.document_id` passed to `add_document` (lib.rs:764) is the **same** UUID assigned to LanceDB chunks (`ChunkRecord.doc_id`). Write findings into the commit message of Task 9.

- [ ] **Step 2: If they differ**, STOP and reconsider: either thread the chunk `doc_id` into `add_document`, or resolve via `source_path`/`folder_path` instead. Record the decision in the plan before continuing. If they match, proceed to Task 9.

> This task is a spike, not code. Its output is a yes/no that gates Task 9's join direction.

---

### Task 9: Attach `document_handle` to search hits

**Files:**
- Modify: `crates/anno-rag-mcp/src/wire.rs:7-18` (`SearchHitWire`)
- Modify: `crates/anno-rag-mcp/src/search.rs` (hit construction, ~line 560-575)

- [ ] **Step 1: Add the field (failing build)**

In `wire.rs`, add to `SearchHitWire`:

```rust
    /// Stable `alias/relative_path` handle for piping into legal tools.
    /// `None` when the doc isn't registered in corpus_documents.
    pub(crate) document_handle: Option<String>,
```

- [ ] **Step 2: Write the failing test**

Add to `search.rs` tests a pure builder test:

```rust
#[test]
fn handle_built_from_alias_and_relative_path() {
    assert_eq!(
        crate::search::build_handle(Some("corpus-01"), Some("contrats/x.txt")),
        Some("corpus-01/contrats/x.txt".to_string())
    );
    assert_eq!(crate::search::build_handle(None, Some("x.txt")), None);
    assert_eq!(crate::search::build_handle(Some("corpus-01"), None), None);
}
```

- [ ] **Step 3: Implement the pure builder**

In `search.rs`:

```rust
/// Build a `alias/relative_path` document handle, or `None` if either part
/// is missing. Spec C §10 (U1).
pub(crate) fn build_handle(alias: Option<&str>, relative_path: Option<&str>) -> Option<String> {
    match (alias, relative_path) {
        (Some(a), Some(p)) => Some(format!("{a}/{p}")),
        _ => None,
    }
}
```

- [ ] **Step 4: Populate it where hits are built**

At hit construction (search.rs ~560-575), look up the relative path and corpus alias and set the field. Resolve the alias from the already-available corpus rows; resolve the path via the new store method:

```rust
let relative_path = corpus_store
    .relative_path_by_document_id(corpus_id, h.doc_id)
    .ok()
    .flatten();
// `alias` is available from the effective-corpus rows already loaded for this search
let document_handle = crate::search::build_handle(alias.as_deref(), relative_path.as_deref());
// ...
SearchHitWire { /* existing fields */, document_handle }
```

> If `corpus_store`/`alias` aren't in scope at this point, thread them from the caller that already holds the `CorpusService`/effective corpus. Keep the lookup best-effort (`.ok().flatten()`), never failing the search.

- [ ] **Step 5: Run search tests**

Run: `cargo test -p anno-rag-mcp search -- --nocapture` and `cargo test -p anno-rag-mcp handle_built -- --nocapture`
Expected: PASS. Fix all other `SearchHitWire { … }` constructors the new field broke (add `document_handle: None` in tests/fixtures).

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/wire.rs crates/anno-rag-mcp/src/search.rs
git commit -m "feat(mcp): resolve search hits to document handles (Spec C U1/§10)"
```

---

## Phase 3 — Lifecycle: cold-start + warmup (§6 U7/U8) + ETA (§12/D3)

### Task 10: `anno_health.next_step` cold-start state machine

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (the `anno_health` impl — find via `rg "fn anno_health" crates/anno-rag-mcp/src/lib.rs`)

- [ ] **Step 1: Write the failing test (pure decision fn)**

```rust
#[test]
fn next_step_walks_setup_states() {
    use crate::envelope::status;
    // (vault_ok, models_present) -> (status, next_step)
    assert_eq!(crate::health_next_step(false, false), (status::SETUP_REQUIRED, Some("anno_init_vault")));
    assert_eq!(crate::health_next_step(true, false), (status::SETUP_REQUIRED, Some("download_models")));
    assert_eq!(crate::health_next_step(true, true), (status::OK, None));
}
```

- [ ] **Step 2: Implement the decision fn**

```rust
/// Single next setup action for the agent, from vault + model state. Spec C U7.
pub(crate) fn health_next_step(vault_ok: bool, models_present: bool) -> (&'static str, Option<&'static str>) {
    use crate::envelope::status;
    match (vault_ok, models_present) {
        (false, _) => (status::SETUP_REQUIRED, Some("anno_init_vault")),
        (true, false) => (status::SETUP_REQUIRED, Some("download_models")),
        (true, true) => (status::OK, None),
    }
}
```

- [ ] **Step 3: Run → PASS**

Run: `cargo test -p anno-rag-mcp next_step_walks -- --nocapture`

- [ ] **Step 4: Use it in `anno_health`**

Compute `vault_ok` (`anno_rag::vault::is_vault_key_usable()`, as in lib.rs:1054) and `models_present` (from `ModelInventoryService::inspect`), then add to the `anno_health` JSON:

```rust
let (st, next) = health_next_step(vault_ok, models_present);
// merge into the existing anno_health response object:
//   "status": st, "next_step": next
```

- [ ] **Step 5: Run health tests + commit**

Run: `cargo test -p anno-rag-mcp health -- --nocapture` → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): anno_health.next_step cold-start guidance (Spec C U7)"
```

---

### Task 11: Persisted warmup timings + ETA (§12/D3)

**Files:**
- Create: `crates/anno-rag-mcp/src/warmup_history.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`mod warmup_history;`; record on `Ready`; read for ETA)

- [ ] **Step 1: Write the failing test**

In `crates/anno-rag-mcp/src/warmup_history.rs`:

```rust
//! Persisted last-known warmup durations, used to estimate ETA on next start.
//! Stored as a tiny JSON beside the models dir. Durations are not sensitive.
//! Spec C §12 (D3).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct WarmupHistory {
    pub(crate) download_ms: Option<u64>,
    pub(crate) load_ms: Option<u64>,
}

fn history_path(models_dir: &Path) -> PathBuf {
    models_dir.join("warmup_history.json")
}

pub(crate) fn load(models_dir: &Path) -> WarmupHistory {
    std::fs::read_to_string(history_path(models_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save(models_dir: &Path, h: &WarmupHistory) {
    if let Ok(s) = serde_json::to_string(h) {
        let _ = std::fs::write(history_path(models_dir), s);
    }
}

/// Remaining seconds in the current phase, or `None` with no history.
pub(crate) fn eta_seconds(last_phase_ms: Option<u64>, elapsed_ms: u64) -> Option<u64> {
    last_phase_ms.map(|total| total.saturating_sub(elapsed_ms) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_eta() {
        let tmp = tempfile::tempdir().unwrap(); // unique per-test, auto-cleaned
        let dir = tmp.path();
        let h = WarmupHistory { download_ms: Some(600_000), load_ms: Some(90_000) };
        save(dir, &h);
        assert_eq!(load(dir), h);
        assert_eq!(eta_seconds(Some(600_000), 60_000), Some(540));
        assert_eq!(eta_seconds(None, 1_000), None);
    }
}
```

- [ ] **Step 2: Register + run**

Add `mod warmup_history;` in `lib.rs`. Run: `cargo test -p anno-rag-mcp warmup_history::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Record timings on `Ready`**

In the warmup driver (lib.rs ~4191 where `WarmupPhase::Ready { elapsed_ms }` is set), compute `download_ms`/`load_ms` from the phase transitions and persist:

```rust
crate::warmup_history::save(
    &models_dir,
    &crate::warmup_history::WarmupHistory { download_ms, load_ms },
);
```

> Derive `models_dir` from `self.cfg` (the inventory service already knows it). Capture phase start timestamps you already track (`started_ms`) to compute the two durations.

- [ ] **Step 4: Expose `eta_seconds` in the warmup JSON**

In `status` (lib.rs ~1067-1097), for `Downloading`/`Loading`, load history once and add `eta_seconds` next to `elapsed_s`. Note `eta_seconds()` expects **milliseconds**, but the arms currently compute `elapsed_s` — derive `elapsed_ms` from the `started_ms` you already have (`now_ms.saturating_sub(started_ms)`) rather than multiplying the seconds value:

```rust
let hist = crate::warmup_history::load(&models_dir);
let now_ms = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
let elapsed_ms = now_ms.saturating_sub(*started_ms);
// Downloading arm:
"eta_seconds": crate::warmup_history::eta_seconds(hist.download_ms, elapsed_ms),
// Loading arm:
"eta_seconds": crate::warmup_history::eta_seconds(hist.load_ms, elapsed_ms),
```

- [ ] **Step 5: Run status test + commit**

Run: `cargo test -p anno-rag-mcp status -- --nocapture` → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/warmup_history.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): persisted warmup timings + eta_seconds (Spec C D3/§12)"
```

---

### Task 12: `not_ready` envelope on model-requiring tools + proactive warmup

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`require_models`/`require_pipeline` gate — find via `rg "require_models|fn require_" crates/anno-rag-mcp/src/lib.rs`; and the stdio boot path ~4132)

- [ ] **Step 1: Write the failing test (pure builder)**

```rust
#[test]
fn not_ready_envelope_carries_warmup() {
    use crate::envelope::status;
    let v = crate::not_ready_envelope("downloading", 60, Some(540));
    assert_eq!(v["status"], status::NOT_READY);
    assert_eq!(v["warmup"]["phase"], "downloading");
    assert_eq!(v["warmup"]["eta_seconds"], 540);
}
```

- [ ] **Step 2: Implement the builder**

```rust
/// Standard `not_ready` response while models warm up. Spec C U8.
pub(crate) fn not_ready_envelope(phase: &str, elapsed_ms: u64, eta_seconds: Option<u64>) -> serde_json::Value {
    let eta_human: Option<&str> = match phase {
        "downloading" => Some("~10–15 min (premier lancement)"),
        "loading" => Some("~1–2 min"),
        _ => None,
    };
    let msg = match phase {
        "downloading" => "Téléchargement des modèles en cours (~10–15 min au premier lancement).",
        "loading" => "Chargement des modèles en cours (~1–2 min).",
        _ => "Initialisation des modèles en cours.",
    };
    crate::envelope::envelope(
        crate::envelope::status::NOT_READY,
        msg,
        "Réessayez dans un instant ; suivez la progression via status().",
        serde_json::json!({ "warmup": { "phase": phase, "elapsed_ms": elapsed_ms, "eta_seconds": eta_seconds, "eta_human": eta_human } }),
    )
}
```

- [ ] **Step 3: Run → PASS**

Run: `cargo test -p anno-rag-mcp not_ready_envelope -- --nocapture`

- [ ] **Step 4: Return it from the model gate**

In `require_models` (or each model-requiring tool that currently errors when the pipeline isn't ready), when `warmup_phase != Ready`, return the `not_ready_envelope(...)` instead of a bare error string. Keep the `Failed` phase returning a clear error.

- [ ] **Step 5: Proactive warmup at boot**

In the stdio serve path (lib.rs ~4132, where warmup is currently lazy / `Idle`), spawn the warmup task at startup so the clock starts immediately:

```rust
// Kick warmup proactively so the download/load clock starts at boot.
// Tools still return `not_ready` until the phase reaches Ready.
let warmup_server = server.clone();
tokio::spawn(async move { warmup_server.run_warmup().await; });
```

> Reuse the existing warmup entrypoint (the fn that drives `Downloading→Loading→Ready`, ~4132-4205). Do NOT block `serve_stdio` on it. **Use a single-flight guard** (e.g., an `AtomicBool` or `OnceLock`) so that if a tool's lazy trigger fires concurrently with the boot-time spawn, only one warmup runs. Verify the existing `serve_stdio_lazy_warmup_phase_starts_idle` test — update it (or add a `serve_stdio_proactive_warmup_starts` variant) to reflect the new boot behavior.

- [ ] **Step 6: Run + commit**

Run: `cargo test -p anno-rag-mcp warmup -- --nocapture` → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): not_ready envelope + proactive warmup at boot (Spec C U8)"
```

---

## Phase 4 — Registry hygiene: hide deprecated (§4/U4) + canonical aliases (§11/D2)

### Task 13: Deprecated-tool registry + `list_tools` filter

**Files:**
- Create: `crates/anno-rag-mcp/src/deprecated.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs:3985-3988` (the `#[tool_handler] impl ServerHandler`)

- [ ] **Step 1: Confirm how `#[tool_handler]` generates `list_tools`**

Run: `rg "tool_handler|list_tools|call_tool" crates/anno-rag-mcp/src/lib.rs` and read the rmcp 1.6 macro docs note at the top of `lib.rs`. Confirm that `#[tool_handler]` exposes `self.tool_router` so we can override `list_tools` by calling `self.tool_router.list_tools(req, ctx).await?` then filtering. **Do not** prune tools at registration time — deprecated handlers must remain callable via `call_tool`.

- [ ] **Step 2: Write the failing test**

In `crates/anno-rag-mcp/src/deprecated.rs`:

```rust
//! Registry of deprecated tool names hidden from `list_tools` unless
//! `ANNO_EXPOSE_DEPRECATED=1`. Handlers stay callable. Spec C §4 (U4) + §11 (D2).

/// Tool names hidden by default. Includes legacy tools (U4) and the bare names
/// superseded by canonical names (D2).
pub(crate) const DEPRECATED_TOOLS: &[&str] = &[
    // U4 — legacy duplicates
    "legacy_search", "knowledge_search", "ingest", "reindex",
    "legal_ingest", "legal_search",
    // D2 — superseded bare names (canonical: forget_source/detokenize/service_status)
    "forget", "rehydrate", "status",
];

/// True when deprecated tools should still be advertised in `list_tools`.
pub(crate) fn expose_deprecated() -> bool {
    std::env::var("ANNO_EXPOSE_DEPRECATED").map(|v| v == "1").unwrap_or(false)
}

/// Filter a tool-name list, dropping deprecated names unless exposed.
pub(crate) fn visible<'a>(names: impl IntoIterator<Item = &'a str>, expose: bool) -> Vec<&'a str> {
    names.into_iter()
        .filter(|n| expose || !DEPRECATED_TOOLS.contains(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_deprecated_by_default() {
        let v = visible(["search", "forget", "legacy_search", "detect"], false);
        assert_eq!(v, vec!["search", "detect"]);
    }

    #[test]
    fn exposes_when_flagged() {
        let v = visible(["search", "forget"], true);
        assert_eq!(v, vec!["search", "forget"]);
    }
}
```

- [ ] **Step 3: Register + run**

Add `mod deprecated;` in `lib.rs`. Run: `cargo test -p anno-rag-mcp deprecated::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 4: Filter `list_tools`**

Override `list_tools` in the `ServerHandler` impl. Call through `self.tool_router` (the `IntoService`-generated router that backs the macro), then filter:

```rust
async fn list_tools(
    &self,
    req: Option<rmcp::model::PaginatedRequestParam>,
    ctx: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
    let mut result = self.tool_router.list_tools(req, ctx).await?;
    let expose = crate::deprecated::expose_deprecated();
    result.tools.retain(|t| expose || !crate::deprecated::DEPRECATED_TOOLS.contains(&t.name.as_ref()));
    Ok(result)
}
```

> Use `self.tool_router.list_tools(req, ctx).await?` — this is the public path exposed by rmcp's `ToolRouter`. Do NOT prune from the registration side (e.g., unregistering entries from the router); that would affect dispatch and break calls to deprecated tools that still need to work.

- [ ] **Step 5: Add a log line on deprecated-tool calls**

In `call_tool` (or each deprecated handler), emit once: `tracing::warn!(tool = name, "deprecated tool called; see canonical replacement");`. Keep the handler functional.

- [ ] **Step 6: Run + commit**

Run: `cargo test -p anno-rag-mcp -- --nocapture` (crate-level) → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/deprecated.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): hide deprecated tools from list_tools behind flag (Spec C U4)"
```

---

### Task 14: Canonical-named tools with deprecated aliases (D2)

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (add `forget_source`, `detokenize`, `service_status` tools delegating to existing impls)

- [ ] **Step 1: Add canonical tools that delegate to the existing impl fns**

The existing `forget`/`rehydrate`/`status` `#[tool]` methods call inner impls (`forget_impl_routing`, the rehydrate impl, `status_impl`). Add three new `#[tool]` methods with canonical names that call the **same** inner impls:

```rust
#[tool(description = "Remove an indexed source (UUID, legal corpus id, or folder path). Canonical name for the former 'forget'. Does not load models.")]
async fn forget_source(&self, Parameters(p): Parameters<ForgetParams>) -> String {
    self.forget_impl_routing(p).await
}

#[tool(description = "Replace pseudo-tokens (PERSON_1, EMAIL_2, …) with original PII from the local vault. Canonical name for the former 'rehydrate'.")]
async fn detokenize(&self, Parameters(params): Parameters<RehydrateParams>) -> String {
    self.rehydrate_impl(params).await // use the real inner fn name
}

#[tool(description = "Anno-wide index health: source counts, chunks, vault, model state. Canonical name for the former 'status'. Does not load models.")]
async fn service_status(&self) -> String {
    self.status_impl().await // use the real inner fn name
}
```

> Find the exact inner-impl names with `rg "fn rehydrate|fn status|forget_impl" crates/anno-rag-mcp/src/lib.rs` and call those, so canonical + alias share one code path (DRY).

- [ ] **Step 2: Mark the old names deprecated in their descriptions**

Update the `#[tool(description = …)]` on `forget`/`rehydrate`/`status` to start with `"Deprecated — use forget_source/detokenize/service_status. Continues to work."` (mirrors the existing deprecation wording at lib.rs:2245/2987).

(They're already in `DEPRECATED_TOOLS` from Task 13, so they vanish from `list_tools` by default.)

- [ ] **Step 3: Test — canonical and alias produce identical output**

```rust
#[tokio::test]
async fn canonical_status_matches_legacy_status() {
    let server = crate::test_server().await; // use the crate's test ctor
    assert_eq!(server.service_status().await, server.status().await);
}
```

Run: `cargo test -p anno-rag-mcp canonical_status_matches -- --nocapture`
Expected: PASS.

- [ ] **Step 4: fmt + clippy + commit**

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): canonical tool names + deprecated aliases (Spec C D2/§11)"
```

---

## Phase 5 — Cross-referenced descriptions (§5/U5)

### Task 15: Rewrite overlapping-tool descriptions to cross-reference siblings

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`#[tool(description = …)]` on the status/forget/rehydrate families)

- [ ] **Step 1: Edit descriptions**

Rewrite each so it states its precise scope and points to siblings. Concrete replacements:

- `memory_forget` (lib.rs:2696): `"Forget conversational memories by id or query (cascades to vault tokens). For an indexed source, use forget_source; for knowledge folders, use knowledge_forget. Returns the 24h erasure SLO note."`
- `knowledge_forget`: `"Remove a knowledge folder from the knowledge index. For conversational memory use memory_forget; for legal/indexed sources use forget_source."`
- `legal_rehydrate_citation` (lib.rs:3088): keep, append `" For free-text token replacement, use detokenize."`
- `corpus_health` (lib.rs:2341): append `" For anno-wide health use service_status; for knowledge use knowledge_status; for privacy workflow use privacy_status."`
- `knowledge_status`: append `" For anno-wide health use service_status."`
- `privacy_status` (lib.rs:2407): append `" For anno-wide health use service_status."`
- `anno_health` (lib.rs:2498): append `" For index/source counts use service_status."`

- [ ] **Step 2: Guard test — descriptions mention their canonical siblings**

```rust
#[tokio::test]
async fn descriptions_cross_reference_siblings() {
    let server = crate::test_server().await;
    let tools = server.tool_router.list_all(); // or the generated listing accessor
    let by = |name: &str| tools.iter().find(|t| t.name == name).unwrap().description.clone().unwrap_or_default();
    assert!(by("memory_forget").contains("forget_source"));
    assert!(by("corpus_health").contains("service_status"));
}
```

> Use whatever accessor exposes the registered tool list/descriptions (from Task 13 Step 1). If none is ergonomic, assert against the description string constants directly.

- [ ] **Step 2b: Run → PASS**

Run: `cargo test -p anno-rag-mcp descriptions_cross_reference -- --nocapture`

- [ ] **Step 3: fmt + commit** (descriptions only — no clippy-relevant code)

```bash
cargo fmt -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "docs(mcp): cross-reference status/forget/rehydrate tool descriptions (Spec C U5)"
```

---

## Phase 6 — `search` description (§3/U3) + totalize the matrix (§9/D1)

### Task 16: Make `(fast, legal)` degrade instead of error (D1)

**Files:**
- Modify: `crates/anno-rag-mcp/src/search.rs:90-95` (the `(Some("fast"), "legal")` arm) and the downstream consumer of `explicit_fast_legal_error`

- [ ] **Step 1: Find the downstream hard-error site**

Run: `rg "explicit_fast_legal_error" crates/anno-rag-mcp/src/`
Identify where `true` currently produces an error response.

- [ ] **Step 2: Write the failing test**

In `search.rs` tests:

```rust
#[test]
fn fast_legal_degrades_not_errors() {
    let mut warnings = vec![];
    let plan = search_execution_plan(Some("fast".into()), "legal", &mut warnings);
    // After D1: still fast, but no hard-error flag — a warning carries the degrade note.
    assert!(!plan.explicit_fast_legal_error, "fast+legal must not hard-error");
    assert!(warnings.iter().any(|w| w.contains("semantic")), "degrade note present");
}
```

- [ ] **Step 3: Change the arm to degrade**

Replace the `(Some("fast"), "legal")` arm (search.rs:90-95) with:

```rust
(Some("fast"), "legal") => {
    warnings.push(
        "legal scope skipped in fast mode (requires models). Use mode='semantic' for legal ranking."
            .to_string(),
    );
    SearchExecutionPlan {
        mode_used: "fast",
        knowledge: SearchBackendMode::Skipped,
        legal: SearchBackendMode::Skipped,
        explicit_fast_legal_error: false,
    }
}
```

- [ ] **Step 4: At the downstream site, surface `status: degraded` when any warning fired**

Where the unified `search` response is assembled, when `warnings` is non-empty (or legal was skipped under an explicit request), set the envelope `status` to `degraded` with the warnings as `hint`s, instead of returning an error. Keep `status: ok` when no warnings.

- [ ] **Step 5: Remove the now-dead `explicit_fast_legal_error` error path**

If `explicit_fast_legal_error` is no longer read anywhere after Step 4, delete the field and its initializers (all the `explicit_fast_legal_error: false` lines) to keep the struct honest. Re-run `rg "explicit_fast_legal_error"` to confirm zero remaining readers before deleting.

- [ ] **Step 6: Run + commit**

Run: `cargo test -p anno-rag-mcp search -- --nocapture` → PASS

```bash
cargo fmt -p anno-rag-mcp && cargo clippy --jobs 2 -p anno-rag-mcp
git add crates/anno-rag-mcp/src/search.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): search mode×scope is total — fast+legal degrades (Spec C D1/§9)"
```

---

### Task 17: Example-driven `search` description (U3)

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs:2258` (the `search` `#[tool(description = …)]`)

- [ ] **Step 1: Replace the dense matrix prose with a rule + examples**

```rust
#[tool(
    description = "Search Anno's local indexes. Default: just pass a query — scope='all', auto mode. \
Set scope to narrow: scope='legal' (contracts/case files), scope='knowledge' (notes/folders). \
Set mode only to tune cost: mode='fast' skips model loading (lexical only; legal results are skipped with a 'degraded' status), mode='semantic' forces model-backed ranking. \
Examples: search(query='clause de résiliation', scope='legal'); search(query='réunion Q3', scope='knowledge', mode='fast'); search(query='pénalités'). \
Returns hits with a document_handle (alias/relative_path) you can pass to legal tools. Pseudonymous labels; no raw paths."
)]
```

- [ ] **Step 2: Guard test — description names the key levers**

```rust
#[tokio::test]
async fn search_description_has_examples_and_handle() {
    let server = crate::test_server().await;
    let d = server_tool_description(&server, "search"); // reuse the accessor from Task 15
    assert!(d.contains("Examples:"));
    assert!(d.contains("document_handle"));
    assert!(d.contains("degraded"));
}
```

- [ ] **Step 3: Run → PASS**

Run: `cargo test -p anno-rag-mcp search_description -- --nocapture`

- [ ] **Step 4: fmt + commit**

```bash
cargo fmt -p anno-rag-mcp
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "docs(mcp): example-driven search description (Spec C U3/§3)"
```

---

## Final verification (after all phases that you landed)

- [ ] **Run the full MCP crate suite**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: all PASS.

- [ ] **Run touched dependency crates**

Run: `cargo test -p anno-corpus-store && cargo test -p anno-rag legal`
Expected: PASS.

- [ ] **fmt + clippy across touched crates**

Run: `cargo fmt --all && cargo clippy --jobs 2 -p anno-rag-mcp -p anno-corpus-store -p anno-rag`
Expected: clean.

- [ ] **Smoke the surface (optional, models present)**

Run: `cargo run -p anno-rag-bin -- mcp` then issue `anno_health` and `search(query="test")` from a client; confirm `status`/`next_step`/`document_handle` appear.

---

## Landing order recap (1 PR per phase)

1. Phase 0 — envelope convention + U6 labels
2. Phase 1 — U2 honest empties
3. Phase 2 — U1 handle resolution
4. Phase 3 — U7/U8/D3 lifecycle + ETA
5. Phase 4 — U4 hide deprecated + D2 canonical names
6. Phase 5 — U5 cross-references
7. Phase 6 — D1 totalize matrix + U3 description
