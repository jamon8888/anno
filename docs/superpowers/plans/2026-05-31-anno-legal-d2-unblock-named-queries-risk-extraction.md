# Débloquer les outils D2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `legal_extract_contract`, `legal_risk_review`, and `legal_timeline` work by routing D2 call-sites through named SQL queries (not raw Cypher) and adding hybrid GLiNER+regex risk extraction (22 rules across 6 French law domains).

**Architecture:** Two lots in one crate (`anno-rag`). Lot 1 rewires `timeline` and `extract_contract` to use existing/new named SQL traversals, fixing the edge-type mismatch (`SOURCES` → `MENTIONS`). Lot 2 adds `TypedFact::Risk`, 22 deterministic regex rules, GLiNER `risk_indicator` consumption, enricher integration, and the `risk_findings` SQL query — making `risk_review` functional.

**Tech Stack:** Rust, SQLite (rusqlite), regex, once_cell, uuid v5, async-trait. No new dependencies.

**Spec:** [`docs/superpowers/specs/2026-05-29-anno-legal-d2-unblock-named-queries-risk-extraction-design.md`](../specs/2026-05-29-anno-legal-d2-unblock-named-queries-risk-extraction-design.md)

**Build/test commands:**
```powershell
# Check only (fast, no linking):
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast

# Unit tests for anno-rag:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag

# NEVER run cargo test --workspace or cargo build --workspace locally.
# ALWAYS check for running cargo/rustc first:
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/anno-rag/src/legal/rules.rs` | Modify | Add `TypedFact::Risk` variant + 22 `rule_risk_*` functions + `merge_gliner_risks` deduplication |
| `crates/anno-rag/src/legal/enricher.rs` | Modify | Map `TypedFact::Risk` → `NodeWrite::Risk` + edge; populate `risk_flags` from GLiNER entities |
| `crates/anno-rag/src/legal/kg.rs` | Modify | Add trait methods `contract_parties`, `contract_obligations`, `risk_findings` + SQLite implementations |
| `crates/anno-rag/src/legal/extract.rs` | Modify | Rewire `timeline()`, `extract_contract()`, `risk_review()` to use named methods instead of raw Cypher |
| `crates/anno-rag/tests/legal_graph_v0.rs` | Modify | Add integration tests for the 3 new SQL queries and the 3 rewired D2 workflows |

---

## LOT 1 — Câblage SQL + alignement edges

### Task 1: Rewire `timeline()` to use existing `procedural_timeline`

**Files:**
- Modify: `crates/anno-rag/src/legal/extract.rs:254-283`
- Test: `crates/anno-rag/src/legal/extract.rs` (existing inline tests)

This is the simplest change. The function currently sends raw Cypher to `kg.cypher(...)`. The trait method `kg.procedural_timeline(dossier_id)` already exists and returns the exact same columns with the correct edge `MENTIONS`.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `crates/anno-rag/src/legal/extract.rs` in the existing `#[cfg(test)] mod tests` block:

```rust
#[tokio::test]
async fn timeline_calls_procedural_timeline_not_cypher() {
    let kg = crate::legal::kg::tests::InMemoryKG::default();
    // InMemoryKG::cypher returns Ok(Vec::new()) — if timeline still calls
    // cypher with raw MATCH, it succeeds but returns empty. This test
    // documents the expected path through procedural_timeline().
    let result = timeline(&kg, "dossier-test").await.unwrap();
    assert_eq!(result.dossier_id, "dossier-test");
    assert!(result.events.is_empty()); // InMemoryKG has no data
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

This test passes already because `InMemoryKG::cypher` returns `Ok(Vec::new())`. We need it as a non-regression anchor.

- [ ] **Step 3: Replace raw Cypher call with named method**

In `crates/anno-rag/src/legal/extract.rs`, replace the `timeline()` function body (lines 254-283). Change from:

```rust
let rows = kg
    .cypher(
        "MATCH (d:Document {dossier_id:$dossier})-[:HAS_CHUNK]->(c:Chunk) \
         MATCH (c)-[:MENTIONS]->(e:Event) \
         RETURN e.kind AS kind, e.event_date AS event_date, \
                e.deadline_date AS deadline_date, c.chunk_id AS cid \
         ORDER BY e.event_date",
        HashMap::from([("dossier".to_string(), dossier_id.to_string())]),
    )
    .await?;
```

To:

```rust
let rows = kg.procedural_timeline(dossier_id).await?;
```

The rest of the function (mapping `rows` into `TimelineEvent` structs) stays unchanged — `procedural_timeline` returns the same column names (`kind`, `event_date`, `deadline_date`, `chunk_id` aliased as `cid` — check: the SQLite impl uses `chunk_id` not `cid`).

**Important:** Verify the column name. In `kg.rs:518-553`, `procedural_timeline_rows` returns `event_kind` (not `kind`) and `chunk_id` (not `cid`). Adjust the mapping in the `rows.iter().map(...)` block:

```rust
let events = rows
    .iter()
    .map(|r| TimelineEvent {
        kind: r.get("event_kind").or_else(|| r.get("kind")).cloned().unwrap_or_default(),
        event_date: r.get("event_date").cloned(),
        deadline_date: r.get("deadline_date").cloned(),
        chunk_id: r.get("chunk_id").or_else(|| r.get("cid")).cloned(),
    })
    .collect();
```

- [ ] **Step 4: Run tests to verify everything passes**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all existing tests pass + the new test passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/extract.rs
git commit -m "fix(legal): rewire timeline() to use procedural_timeline named query

Replace raw Cypher call with existing kg.procedural_timeline() method.
Fixes column name mapping (event_kind vs kind, chunk_id vs cid)."
```

---

### Task 2: Add `contract_parties` and `contract_obligations` to the trait and SQLite backend

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs:249-352` (trait) and `597-730` (impl)
- Modify: `crates/anno-rag/src/legal/kg.rs:1038-1095` (InMemoryKG)
- Test: `crates/anno-rag/tests/legal_graph_v0.rs`

- [ ] **Step 1: Write the failing integration tests**

Add to the bottom of `crates/anno-rag/tests/legal_graph_v0.rs`:

```rust
#[tokio::test]
async fn contract_parties_returns_party_linked_to_doc() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg.contract_parties(&doc_id.to_string()).await.expect("contract_parties");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("value").map(String::as_str), Some("org:acme"));
    assert_eq!(rows[0].get("role").map(String::as_str), Some("client"));
}

#[tokio::test]
async fn contract_obligations_returns_obligation_via_mentions() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg
        .contract_obligations(&doc_id.to_string())
        .await
        .expect("contract_obligations");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("kind").map(String::as_str),
        Some("payment")
    );
    assert_eq!(
        rows[0].get("cid").map(String::as_str),
        Some(chunk_id.to_string().as_str())
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: FAIL — `contract_parties` method not found on trait.

- [ ] **Step 3: Add default methods to `LegalKnowledgeGraph` trait**

In `crates/anno-rag/src/legal/kg.rs`, add after `appeal_chain` (line ~351), before the closing `}` of the trait:

```rust
    /// Return parties linked to a document.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn contract_parties(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "contract_parties",
            HashMap::from([("doc_id".to_string(), doc_id.to_string())]),
        )
        .await
    }

    /// Return obligations linked to a document via MENTIONS edges.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn contract_obligations(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.cypher(
            "contract_obligations",
            HashMap::from([("doc_id".to_string(), doc_id.to_string())]),
        )
        .await
    }
```

- [ ] **Step 4: Add SQL implementations to `SqliteLegalGraphStore`**

In `crates/anno-rag/src/legal/kg.rs`, add these two private methods after `appeal_chain_rows` (line ~593):

```rust
    fn contract_parties_rows(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT json_extract(p.props_json, '$.canonical_name') AS value,
                            json_extract(party_edge.props_json, '$.role') AS role
                     FROM legal_edges party_edge
                     JOIN legal_nodes p
                       ON p.label = 'Party'
                      AND p.id = party_edge.from_key
                     WHERE party_edge.from_label = 'Party'
                       AND party_edge.to_label = 'Document'
                       AND party_edge.to_key = ?1
                       AND party_edge.edge_type = 'PARTY_TO'
                     ORDER BY value",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![doc_id])
        })
    }

    fn contract_obligations_rows(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT json_extract(o.props_json, '$.kind') AS kind,
                            json_extract(o.props_json, '$.text_pseudo') AS text,
                            c.id AS cid
                     FROM legal_nodes d
                     JOIN legal_edges chunk_edge
                       ON chunk_edge.from_label = 'Document'
                      AND chunk_edge.from_key = d.id
                      AND chunk_edge.to_label = 'Chunk'
                      AND chunk_edge.edge_type = 'HAS_CHUNK'
                     JOIN legal_nodes c
                       ON c.label = 'Chunk'
                      AND c.id = chunk_edge.to_key
                     JOIN legal_edges mention_edge
                       ON mention_edge.from_label = 'Chunk'
                      AND mention_edge.from_key = c.id
                      AND mention_edge.to_label = 'Obligation'
                      AND mention_edge.edge_type = 'MENTIONS'
                     JOIN legal_nodes o
                       ON o.label = 'Obligation'
                      AND o.id = mention_edge.to_key
                     WHERE d.label = 'Document'
                       AND d.id = ?1
                     ORDER BY c.id, o.id",
                )
                .map_err(sql_err)?;
            collect_rows(&mut stmt, params![doc_id])
        })
    }
```

- [ ] **Step 5: Register named queries in the `cypher` dispatcher + add trait overrides**

In the `cypher` method's match block (line ~680), add two new arms before `_ =>`:

```rust
            "contract_parties" => {
                self.contract_parties_rows(required_param(&params, "doc_id")?)
            }
            "contract_obligations" => {
                self.contract_obligations_rows(required_param(&params, "doc_id")?)
            }
```

And add overrides to the `impl LegalKnowledgeGraph for SqliteLegalGraphStore` block (after `appeal_chain`, line ~729):

```rust
    async fn contract_parties(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.contract_parties_rows(doc_id)
    }

    async fn contract_obligations(&self, doc_id: &str) -> Result<Vec<HashMap<String, String>>> {
        self.contract_obligations_rows(doc_id)
    }
```

- [ ] **Step 6: Run tests to verify they pass**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass, including the 2 new integration tests.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "feat(legal): add contract_parties + contract_obligations named SQL queries

New trait methods with default cypher-dispatcher fallback and direct
SQLite implementations. Uses PARTY_TO and MENTIONS edges (not SOURCES)."
```

---

### Task 3: Rewire `extract_contract()` to use named methods

**Files:**
- Modify: `crates/anno-rag/src/legal/extract.rs:108-160`
- Test: `crates/anno-rag/src/legal/extract.rs` (inline tests)

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` at the bottom of `extract.rs`:

```rust
#[tokio::test]
async fn extract_contract_uses_named_methods() {
    let kg = crate::legal::kg::tests::InMemoryKG::default();
    let result = extract_contract(&kg, "doc:contract-001").await.unwrap();
    assert_eq!(result.doc_id, "doc:contract-001");
    // InMemoryKG returns empty — this is a non-regression test.
    assert!(result.rows.is_empty());
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Passes because `InMemoryKG::cypher` returns empty.

- [ ] **Step 3: Replace raw Cypher calls with named methods**

Replace the body of `extract_contract()` in `crates/anno-rag/src/legal/extract.rs` (lines 108-160):

```rust
pub async fn extract_contract(
    kg: &dyn LegalKnowledgeGraph,
    doc_id: &str,
) -> Result<ContractReview> {
    let mut rows = Vec::new();

    // Parties linked to the document.
    let party_rows = kg.contract_parties(doc_id).await?;
    for r in &party_rows {
        rows.push(ReviewRow {
            field: format!(
                "party:{}",
                r.get("role").cloned().unwrap_or_else(|| "unknown".into())
            ),
            value: r.get("value").cloned().unwrap_or_default(),
            chunk_id: None,
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    // Obligations sourced by chunks of the document.
    let obl_rows = kg.contract_obligations(doc_id).await?;
    for r in &obl_rows {
        rows.push(ReviewRow {
            field: format!("obligation:{}", r.get("kind").cloned().unwrap_or_default()),
            value: r.get("text").cloned().unwrap_or_default(),
            chunk_id: r.get("cid").cloned(),
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    Ok(ContractReview {
        doc_id: doc_id.to_string(),
        rows,
    })
}
```

- [ ] **Step 4: Remove `use std::collections::HashMap;` if now unused at the top of `extract.rs`**

Check if `HashMap` is still used by other functions in the file. `extract_case_file()` and `risk_review()` still use `HashMap::from(...)` for `kg.cypher(...)` calls. Leave the import for now — it will be cleaned up when those are rewired too.

- [ ] **Step 5: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/extract.rs
git commit -m "fix(legal): rewire extract_contract to use named SQL queries

Replace 2 raw Cypher calls with kg.contract_parties() and
kg.contract_obligations(). Uses MENTIONS edge (matching enricher)."
```

---

### Task 4: Non-regression test — raw Cypher still rejected

**Files:**
- Test: `crates/anno-rag/tests/legal_graph_v0.rs`

- [ ] **Step 1: Add the non-regression test**

```rust
#[tokio::test]
async fn raw_cypher_still_rejected_by_sqlite_backend() {
    let (_dir, kg) = store().await;
    let result = kg
        .cypher(
            "MATCH (n) RETURN n",
            HashMap::from([("x".to_string(), "y".to_string())]),
        )
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not supported"),
        "expected 'not supported' error, got: {err}"
    );
}
```

- [ ] **Step 2: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: PASS.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "test(legal): non-regression — raw Cypher still rejected by SQLite backend"
```

---

## LOT 2 — Extraction de risques

### Task 5: Add `TypedFact::Risk` variant

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs:12-50`

- [ ] **Step 1: Add the variant to the enum**

In `crates/anno-rag/src/legal/rules.rs`, add after the `Event` variant (line 49), before the closing `}` of the enum:

```rust
    /// A detected legal risk in the text.
    Risk {
        /// Risk category, e.g. "clause_penale", "non_concurrence".
        category: String,
        /// Severity: "high", "medium", or "low".
        severity: String,
        /// Pseudonymized text of the risky segment.
        text_pseudo: String,
    },
```

- [ ] **Step 2: Verify check passes**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
```

Expected: PASS (the `match fact` in `enricher.rs:349` will give a non-exhaustive warning/error — proceed to fix it in the next step).

- [ ] **Step 3: Add the `Risk` arm to `facts_to_graph_writes` in `enricher.rs`**

In `crates/anno-rag/src/legal/enricher.rs`, in the `facts_to_graph_writes` function, add after the `TypedFact::Event` arm (line ~453), before the closing `}` of the for-loop:

```rust
            TypedFact::Risk {
                category,
                severity,
                text_pseudo,
            } => {
                let risk_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("{chunk_id}::{category}").as_bytes(),
                );
                nodes.push(NodeWrite::Risk {
                    risk_id,
                    severity: severity.clone(),
                    category: category.clone(),
                    text_pseudo: text_pseudo.clone(),
                });
                edges.push(EdgeWrite {
                    from_label: "Chunk",
                    from_key: chunk_key.clone(),
                    to_label: "Risk",
                    to_key: risk_id.to_string(),
                    edge_type: "MENTIONS",
                    props: HashMap::new(),
                });
            }
```

- [ ] **Step 4: Add `Risk` arm to `projection_from_facts` in `enricher.rs`**

In the same file, in the `projection_from_facts` function (line ~250), add a new arm for `Risk` in the `match fact` block, after `TypedFact::CourtRouting { .. } => {}`:

```rust
            TypedFact::Risk { category, severity, .. } => {
                risk_flags.push(format!("{category}:{severity}"));
            }
```

Also declare `let mut risk_flags = Vec::new();` alongside the other `let mut` declarations (line ~247, after `let mut event_kinds`).

And replace the hard-coded `risk_flags: Vec::new(),` (line 325) with `risk_flags,`.

- [ ] **Step 5: Verify check passes**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
```

Expected: PASS — no more non-exhaustive match warnings.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/rules.rs crates/anno-rag/src/legal/enricher.rs
git commit -m "feat(legal): add TypedFact::Risk variant + enricher integration

New Risk variant with category/severity/text_pseudo. Enricher maps it
to NodeWrite::Risk + Chunk-[MENTIONS]->Risk edge. Populates risk_flags
field in LegalChunkEnrichment (was hardcoded to empty)."
```

---

### Task 6: Implement risk rules — Group A (droit commun, 8 rules)

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs`

- [ ] **Step 1: Write failing tests for all 8 rules**

Add in the `#[cfg(test)] mod tests` block of `rules.rs`:

```rust
#[test]
fn risk_clause_penale_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "La clause pénale prévoit une indemnité de 5000 euros.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "clause_penale" && severity == "medium")));
}

#[test]
fn risk_responsabilite_illimitee_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le prestataire est exonéré de toute responsabilité.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "responsabilite_illimitee" && severity == "high")));
}

#[test]
fn risk_desequilibre_significatif_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Cette clause crée un déséquilibre significatif entre les parties.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "desequilibre_significatif" && severity == "high")));
}

#[test]
fn risk_tacite_reconduction_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le contrat est reconduit tacitement pour une durée identique.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "tacite_reconduction" && severity == "medium")));
}

#[test]
fn risk_clause_resolutoire_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Résiliation de plein droit sans mise en demeure préalable.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "clause_resolutoire" && severity == "high")));
}

#[test]
fn risk_renonciation_recours_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le client renonce à tout recours contre le fournisseur.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "renonciation_recours" && severity == "high")));
}

#[test]
fn risk_indexation_interdite_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le loyer est indexé sur le SMIC.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "indexation_interdite" && severity == "high")));
}

#[test]
fn risk_clause_leonine_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "L'associé est exonéré de toute contribution aux pertes.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "clause_leonine" && severity == "high")));
}

#[test]
fn no_risk_on_benign_text() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Les parties conviennent du prix suivant.", &[], &fwd);
    assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { .. })));
}
```

- [ ] **Step 2: Run tests — verify they fail**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: FAIL — `apply_all` doesn't produce any `Risk` facts.

- [ ] **Step 3: Implement the 8 rule functions**

Add these functions in `rules.rs` after `rule_procedural_event` (line ~185), before the `#[cfg(test)]` block:

```rust
// ── Risk rules — Group A: Droit commun des contrats ─────────────────────────

fn rule_risk_clause_penale(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+p[ée]nale|p[ée]nalit[ée]\s+forfaitaire|indemnit[ée]\s+forfaitaire\s+de\b.*\br[ée]siliation")
            .expect("valid clause_penale regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_penale".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_responsabilite_illimitee(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)responsabilit[ée]\s+(?:illimit[ée]e|sans\s+(?:limite|plafond))|exclusion\s+(?:totale\s+)?de\s+(?:toute\s+)?responsabilit[ée]|ne\s+pourra\s+[êe]tre\s+tenu\s+(?:d'aucune\s+)?responsabilit[ée]")
            .expect("valid responsabilite_illimitee regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "responsabilite_illimitee".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_desequilibre_significatif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)d[ée]s[ée]quilibre\s+significatif|avantage\s+(?:excessif|manifestement\s+disproportionn[ée])")
            .expect("valid desequilibre_significatif regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "desequilibre_significatif".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_tacite_reconduction(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)tacite(?:ment)?\s+reconduit|renouvellement\s+tacite|reconduction\s+tacite|reconduit\s+(?:automatiquement|de\s+plein\s+droit)")
            .expect("valid tacite_reconduction regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "tacite_reconduction".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_clause_resolutoire(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)r[ée]solu(?:tion)?\s+de\s+plein\s+droit|clause\s+r[ée]solutoire|r[ée]siliation\s+(?:automatique|imm[ée]diate|de\s+plein\s+droit)\s+sans\s+(?:pr[ée]avis|mise\s+en\s+demeure)")
            .expect("valid clause_resolutoire regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_resolutoire".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_renonciation_recours(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)renonce\s+(?:irr[ée]vocablement\s+)?[àa]\s+(?:tout\s+)?recours|renonciation\s+[àa]\s+(?:tout\s+)?recours")
            .expect("valid renonciation_recours regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "renonciation_recours".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_indexation_interdite(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)index[ée](?:e)?\s+sur\s+(?:le\s+)?(?:smic|smig|salaire\s+minimum|niveau\s+g[ée]n[ée]ral\s+des\s+(?:prix|salaires))")
            .expect("valid indexation_interdite regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "indexation_interdite".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_clause_leonine(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+l[ée]onine|exon[ée]r[ée](?:e)?\s+de\s+toute\s+(?:perte|contribution\s+aux\s+pertes)|attribut(?:ion)?\s+(?:de\s+)?(?:la\s+)?totalit[ée]\s+des\s+(?:b[ée]n[ée]fices|profits)")
            .expect("valid clause_leonine regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "clause_leonine".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}
```

- [ ] **Step 4: Wire all 8 rules into `apply_all`**

In `apply_all` (line ~66), add after the `rule_procedural_event` line and before `let _ = ...`:

```rust
    out.extend(rule_risk_clause_penale(pseudo_text));
    out.extend(rule_risk_responsabilite_illimitee(pseudo_text));
    out.extend(rule_risk_desequilibre_significatif(pseudo_text));
    out.extend(rule_risk_tacite_reconduction(pseudo_text));
    out.extend(rule_risk_clause_resolutoire(pseudo_text));
    out.extend(rule_risk_renonciation_recours(pseudo_text));
    out.extend(rule_risk_indexation_interdite(pseudo_text));
    out.extend(rule_risk_clause_leonine(pseudo_text));
```

- [ ] **Step 5: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all 9 new tests pass (8 positive + 1 negative).

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/rules.rs
git commit -m "feat(legal): add risk rules Group A — droit commun des contrats (8 rules)

A1 clause_penale, A2 responsabilite_illimitee, A3 desequilibre_significatif,
A4 tacite_reconduction, A5 clause_resolutoire, A6 renonciation_recours,
A7 indexation_interdite, A8 clause_leonine."
```

---

### Task 7: Implement risk rules — Group B (droit commercial, 4 rules)

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn risk_delai_paiement_excessif_90_jours() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le délai de paiement est de 90 jours.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "delai_paiement_excessif" && severity == "high")));
}

#[test]
fn risk_delai_paiement_30_jours_ok() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le délai de paiement est de 30 jours.", &[], &fwd);
    assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. } if category == "delai_paiement_excessif")));
}

#[test]
fn risk_rupture_brutale_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le contrat peut être résilié sans préavis.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "rupture_brutale" && severity == "high")));
}

#[test]
fn risk_exclusivite_sans_duree_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "L'exclusivité est accordée sans limite de durée.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "exclusivite_sans_duree" && severity == "high")));
}

#[test]
fn risk_non_sollicitation_detected() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Clause de non-sollicitation du personnel pendant 2 ans.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "non_sollicitation" && severity == "medium")));
}
```

- [ ] **Step 2: Run tests — verify they fail**

- [ ] **Step 3: Implement the 4 rule functions**

```rust
// ── Risk rules — Group B: Droit commercial ──────────────────────────────────

fn rule_risk_delai_paiement_excessif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)d[ée]lai\s+de\s+(?:paiement|r[èe]glement)\s+(?:de\s+)?(\d+)\s*jours")
            .expect("valid delai_paiement regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let days: u32 = cap[1].parse().ok()?;
            if days > 60 {
                Some(TypedFact::Risk {
                    category: "delai_paiement_excessif".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn rule_risk_rupture_brutale(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)r[ée]sili(?:ation|er)\s+(?:sans\s+(?:pr[ée]avis|motif)|[àa]\s+(?:tout\s+)?moment\s+sans\s+(?:pr[ée]avis|indemnit[ée]))|rupture\s+(?:brutale|sans\s+pr[ée]avis)")
            .expect("valid rupture_brutale regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "rupture_brutale".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_exclusivite_sans_duree(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)exclusivit[ée]\s+(?:sans\s+(?:limite|dur[ée]e)|pour\s+une\s+dur[ée]e\s+ind[ée]termin[ée]e)|exclusivit[ée].*\bperp[ée]tu")
            .expect("valid exclusivite_sans_duree regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "exclusivite_sans_duree".into(),
            severity: "high".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}

fn rule_risk_non_sollicitation(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)non[- ]sollicitation|interdiction\s+de\s+sollicit(?:er|ation)\s+(?:du\s+)?personnel|d[ée]bauchage")
            .expect("valid non_sollicitation regex")
    });
    RE.find_iter(text)
        .map(|m| TypedFact::Risk {
            category: "non_sollicitation".into(),
            severity: "medium".into(),
            text_pseudo: m.as_str().to_string(),
        })
        .collect()
}
```

- [ ] **Step 4: Wire into `apply_all`**

Add after the Group A calls:

```rust
    out.extend(rule_risk_delai_paiement_excessif(pseudo_text));
    out.extend(rule_risk_rupture_brutale(pseudo_text));
    out.extend(rule_risk_exclusivite_sans_duree(pseudo_text));
    out.extend(rule_risk_non_sollicitation(pseudo_text));
```

- [ ] **Step 5: Run tests — verify they pass**

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/rules.rs
git commit -m "feat(legal): add risk rules Group B — droit commercial (4 rules)

B1 delai_paiement_excessif (>60j), B2 rupture_brutale,
B3 exclusivite_sans_duree, B4 non_sollicitation."
```

---

### Task 8: Implement risk rules — Groups C, D, E, F (10 rules)

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs`

Same TDD pattern. Tests first, then implement, then wire. I'll list the functions without repeating the full scaffold — the pattern is identical to Tasks 6-7.

- [ ] **Step 1: Write failing tests for all 10 rules**

```rust
// Group C — Droit du travail
#[test]
fn risk_non_concurrence_sans_contrepartie() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Clause de non-concurrence applicable pendant 2 ans.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "non_concurrence_sans_contrepartie" && severity == "high")));
}

#[test]
fn risk_non_concurrence_with_contrepartie_ok() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Clause de non-concurrence avec contrepartie financière de 50% du salaire.", &[], &fwd);
    assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "non_concurrence_sans_contrepartie")));
}

#[test]
fn risk_periode_essai_excessive_6_mois() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "La période d'essai est de 6 mois.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "periode_essai_excessive" && severity == "high")));
}

#[test]
fn risk_periode_essai_3_mois_ok() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "La période d'essai est de 3 mois.", &[], &fwd);
    assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "periode_essai_excessive")));
}

#[test]
fn risk_mobilite_illimitee() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le salarié accepte une clause de mobilité sur tout le territoire.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "mobilite_illimitee")));
}

#[test]
fn risk_dedit_formation() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "En cas de démission, le salarié devra rembourser les frais de formation.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "dedit_formation")));
}

#[test]
fn risk_forfait_jours_sans_suivi() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le cadre est soumis à un forfait en jours de 218 jours par an.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "forfait_jours_sans_suivi")));
}

// Group D — Baux
#[test]
fn risk_solidarite_cessionnaire() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Le cédant reste solidairement responsable des loyers du cessionnaire.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, .. }
        if category == "solidarite_cessionnaire")));
}

#[test]
fn risk_bail_derogatoire_excessif_48_mois() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Bail dérogatoire de 48 mois.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "bail_derogatoire_excessif" && severity == "high")));
}

// Group E — RGPD
#[test]
fn risk_conservation_illimitee() {
    let fwd = fwd(&[]);
    let facts = apply_all(Uuid::nil(), "Les données sont conservées sans limite de durée.", &[], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "conservation_illimitee" && severity == "high")));
}
```

- [ ] **Step 2: Run tests — verify they fail**

- [ ] **Step 3: Implement the 10 rule functions**

Follow the exact same pattern as Tasks 6-7. Key differences:

**C1 `non_concurrence_sans_contrepartie`** — uses contextual look-ahead:
```rust
fn rule_risk_non_concurrence_sans_contrepartie(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)non[- ]concurrence").expect("valid non_concurrence regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)contrepartie\s+financi[èe]re").expect("valid mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 300).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "non_concurrence_sans_contrepartie".into(),
                severity: "high".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}
```

**C2 `periode_essai_excessive`** — numeric post-match (>4 months):
```rust
fn rule_risk_periode_essai_excessive(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)p[ée]riode\s+d'essai\s+(?:de\s+)?(\d+)\s*mois")
            .expect("valid periode_essai regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let months: u32 = cap[1].parse().ok()?;
            if months > 4 {
                Some(TypedFact::Risk {
                    category: "periode_essai_excessive".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}
```

**C3 `mobilite_illimitee`** — contextual look-ahead for zone:
```rust
fn rule_risk_mobilite_illimitee(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE_MATCH: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)clause\s+de\s+mobilit[ée]").expect("valid mobilite regex")
    });
    static RE_MITIGANT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)p[ée]rim[èe]tre|zone\s+g[ée]ographique\s+d[ée]finie|rayon\s+de")
            .expect("valid mobilite mitigant regex")
    });
    let mut out = Vec::new();
    for m in RE_MATCH.find_iter(text) {
        let window_end = (m.end() + 200).min(text.len());
        let window = &text[m.start()..window_end];
        if !RE_MITIGANT.is_match(window) {
            out.push(TypedFact::Risk {
                category: "mobilite_illimitee".into(),
                severity: "medium".into(),
                text_pseudo: m.as_str().to_string(),
            });
        }
    }
    out
}
```

**C4, C5, D1, D2** — simple regex (same pattern as Group A).

**D3 `bail_derogatoire_excessif`** — numeric post-match:
```rust
fn rule_risk_bail_derogatoire_excessif(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)bail\s+d[ée]rogatoire\s+(?:de\s+)?(\d+)\s*(mois|ans)")
            .expect("valid bail_derogatoire regex")
    });
    RE.captures_iter(text)
        .filter_map(|cap| {
            let value: u32 = cap[1].parse().ok()?;
            let unit = cap[2].to_lowercase();
            let months = if unit == "ans" { value * 12 } else { value };
            if months > 36 {
                Some(TypedFact::Risk {
                    category: "bail_derogatoire_excessif".into(),
                    severity: "high".into(),
                    text_pseudo: cap[0].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}
```

**E1, E2** — contextual look-ahead (same pattern as C1).

**E3 `conservation_illimitee`**, **F1 `cession_pi_totale`**, **F2 `cession_oeuvres_futures`** — simple regex.

- [ ] **Step 4: Wire all 10 rules into `apply_all`**

Add after Group B calls:

```rust
    // Group C — Droit du travail
    out.extend(rule_risk_non_concurrence_sans_contrepartie(pseudo_text));
    out.extend(rule_risk_periode_essai_excessive(pseudo_text));
    out.extend(rule_risk_mobilite_illimitee(pseudo_text));
    out.extend(rule_risk_dedit_formation(pseudo_text));
    out.extend(rule_risk_forfait_jours_sans_suivi(pseudo_text));
    // Group D — Baux
    out.extend(rule_risk_solidarite_cessionnaire(pseudo_text));
    out.extend(rule_risk_charges_locatives_illimitees(pseudo_text));
    out.extend(rule_risk_bail_derogatoire_excessif(pseudo_text));
    // Group E — RGPD
    out.extend(rule_risk_transfert_hors_ue(pseudo_text));
    out.extend(rule_risk_sous_traitance_sans_art28(pseudo_text));
    out.extend(rule_risk_conservation_illimitee(pseudo_text));
    // Group F — Propriété intellectuelle
    out.extend(rule_risk_cession_pi_totale(pseudo_text));
    out.extend(rule_risk_cession_oeuvres_futures(pseudo_text));
```

Note: Group A had 8 rules, Group B had 4, Groups C-F add 13 more. But the spec says 22 total: 8+4+5+3+3+2 = 25. The F group (2 rules) was not broken into a separate task. The count in the spec should be 25 not 22 — the implementation follows the spec's full catalogue regardless of the count label.

- [ ] **Step 5: Remove the `let _ = (chunk_id, entities);` line**

Now that `apply_all` will use `entities` in the next task (GLiNER fusion), remove the `let _ = (chunk_id, entities);` line (currently ~line 73). For now `entities` is still unused — add `let _ = (chunk_id, entities);` back temporarily, or just keep `let _ = chunk_id;` since entities will be used in Task 9.

- [ ] **Step 6: Run tests — verify they all pass**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/src/legal/rules.rs
git commit -m "feat(legal): add risk rules Groups C-F — travail, baux, RGPD, PI (13 rules)

C1-C5: non_concurrence, periode_essai, mobilite, dedit_formation, forfait_jours
D1-D3: solidarite_cessionnaire, charges_locatives, bail_derogatoire
E1-E3: transfert_hors_ue, sous_traitance_sans_art28, conservation_illimitee
F1-F2: cession_pi_totale, cession_oeuvres_futures"
```

---

### Task 9: GLiNER `risk_indicator` consumption + deduplication

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs`
- Modify: `crates/anno-rag/src/legal/enricher.rs`

- [ ] **Step 1: Write the failing test**

In `rules.rs` tests:

```rust
#[test]
fn gliner_risk_indicator_becomes_low_risk_fact() {
    let fwd = fwd(&[]);
    let entity = crate::legal::types::LegalEntity {
        label: "risk_indicator".into(),
        text: "clause potentiellement abusive".into(),
        byte_start: 10,
        byte_end: 40,
        confidence: 0.60,
    };
    let facts = apply_all(Uuid::nil(), "Texte anodin avec clause potentiellement abusive ici.", &[entity], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "clause_a_risque" && severity == "low")));
}

#[test]
fn gliner_risk_indicator_medium_when_high_confidence() {
    let fwd = fwd(&[]);
    let entity = crate::legal::types::LegalEntity {
        label: "risk_indicator".into(),
        text: "risque contractuel majeur".into(),
        byte_start: 0,
        byte_end: 25,
        confidence: 0.80,
    };
    let facts = apply_all(Uuid::nil(), "risque contractuel majeur", &[entity], &fwd);
    assert!(facts.iter().any(|f| matches!(f, TypedFact::Risk { category, severity, .. }
        if category == "clause_a_risque" && severity == "medium")));
}

#[test]
fn gliner_risk_indicator_below_threshold_ignored() {
    let fwd = fwd(&[]);
    let entity = crate::legal::types::LegalEntity {
        label: "risk_indicator".into(),
        text: "mention neutre".into(),
        byte_start: 0,
        byte_end: 14,
        confidence: 0.50,
    };
    let facts = apply_all(Uuid::nil(), "mention neutre", &[entity], &fwd);
    assert!(!facts.iter().any(|f| matches!(f, TypedFact::Risk { .. })));
}

#[test]
fn gliner_risk_deduped_with_regex_rule() {
    let fwd = fwd(&[]);
    // GLiNER detects the same span as the regex clause_penale rule
    let entity = crate::legal::types::LegalEntity {
        label: "risk_indicator".into(),
        text: "clause pénale".into(),
        byte_start: 4,
        byte_end: 17,
        confidence: 0.90,
    };
    let facts = apply_all(Uuid::nil(), "La clause pénale prévoit 5000 euros.", &[entity], &fwd);
    // Should have exactly one Risk fact (the regex rule wins, not duplicated)
    let risk_facts: Vec<_> = facts.iter().filter(|f| matches!(f, TypedFact::Risk { .. })).collect();
    assert_eq!(risk_facts.len(), 1);
    assert!(matches!(risk_facts[0], TypedFact::Risk { category, .. } if category == "clause_penale"));
}
```

- [ ] **Step 2: Run tests — verify they fail**

- [ ] **Step 3: Implement `merge_gliner_risks` function**

Add to `rules.rs`, after the risk rule functions:

```rust
/// Merge GLiNER `risk_indicator` entities with regex-detected risks.
///
/// Deduplicates by span overlap (±20 chars). When a GLiNER candidate
/// overlaps a regex-detected Risk, the regex version wins (it has a
/// specific category and severity). GLiNER-only candidates get
/// `category = "clause_a_risque"` with severity derived from confidence.
fn merge_gliner_risks(
    regex_risks: &[TypedFact],
    entities: &[LegalEntity],
    text: &str,
) -> Vec<TypedFact> {
    let mut out = Vec::new();
    let risk_entities: Vec<_> = entities
        .iter()
        .filter(|e| e.label == "risk_indicator" && e.confidence >= 0.55)
        .collect();

    for entity in &risk_entities {
        let overlaps_regex = regex_risks.iter().any(|fact| {
            if let TypedFact::Risk { text_pseudo, .. } = fact {
                if let Some(regex_pos) = text.find(text_pseudo.as_str()) {
                    let regex_end = regex_pos + text_pseudo.len();
                    let ent_start = entity.byte_start as usize;
                    let ent_end = entity.byte_end as usize;
                    // Overlap check with ±20 char tolerance
                    ent_start <= regex_end + 20 && ent_end + 20 >= regex_pos
                } else {
                    false
                }
            } else {
                false
            }
        });

        if !overlaps_regex {
            let severity = if entity.confidence >= 0.75 {
                "medium"
            } else {
                "low"
            };
            out.push(TypedFact::Risk {
                category: "clause_a_risque".into(),
                severity: severity.into(),
                text_pseudo: entity.text.clone(),
            });
        }
    }
    out
}
```

- [ ] **Step 4: Wire `merge_gliner_risks` into `apply_all`**

Replace `let _ = (chunk_id, entities);` (line ~73) with:

```rust
    let _ = chunk_id;
    // Collect regex-detected risks for deduplication against GLiNER
    let regex_risks: Vec<_> = out
        .iter()
        .filter(|f| matches!(f, TypedFact::Risk { .. }))
        .cloned()
        .collect();
    out.extend(merge_gliner_risks(&regex_risks, entities, pseudo_text));
```

- [ ] **Step 5: Populate `risk_flags` from GLiNER entities in enricher**

In `crates/anno-rag/src/legal/enricher.rs`, in `projection_from_facts` (after the `clause_types` block at line ~296), add:

```rust
    for entity in entities
        .iter()
        .filter(|entity| entity.label == "risk_indicator" || entity.label == "sanction")
    {
        risk_flags.push(entity.text.to_lowercase());
    }
```

This is in addition to the `risk_flags` populated from `TypedFact::Risk` (already wired in Task 5 Step 4).

- [ ] **Step 6: Run tests — verify they pass**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/src/legal/rules.rs crates/anno-rag/src/legal/enricher.rs
git commit -m "feat(legal): consume GLiNER risk_indicator entities with dedup

merge_gliner_risks(): GLiNER risk_indicator candidates become low/medium
Risk facts when they don't overlap a regex-detected risk (±20 chars).
Also populates risk_flags in LegalChunkEnrichment from GLiNER entities."
```

---

### Task 10: Add `risk_findings` SQL query + rewire `risk_review()`

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs`
- Modify: `crates/anno-rag/src/legal/extract.rs`
- Test: `crates/anno-rag/tests/legal_graph_v0.rs`

- [ ] **Step 1: Write the failing integration test**

In `crates/anno-rag/tests/legal_graph_v0.rs`, add a helper and test:

```rust
fn seeded_risk_graph(doc_id: Uuid, chunk_id: Uuid, risk_id: Uuid) -> (NodeBatch, EdgeBatch) {
    let mut nodes = NodeBatch::new();
    nodes.add_document(doc_id, Some("contract".into()), None, None, None, Some("dossier-risk".into()));
    nodes.add_chunk(chunk_id, doc_id, 0, 100, None);
    nodes.nodes.push(NodeWrite::Risk {
        risk_id,
        severity: "high".into(),
        category: "clause_penale".into(),
        text_pseudo: "clause pénale forfaitaire de 50%".into(),
    });

    let mut edges = EdgeBatch::new();
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Chunk",
        to_key: chunk_id.to_string(),
        edge_type: "HAS_CHUNK",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Risk",
        to_key: risk_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });
    (nodes, edges)
}

#[tokio::test]
async fn risk_findings_by_doc_id() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let risk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_risk_graph(doc_id, chunk_id, risk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg.risk_findings(&doc_id.to_string(), false).await.expect("risk_findings");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("severity").map(String::as_str), Some("high"));
    assert_eq!(rows[0].get("category").map(String::as_str), Some("clause_penale"));
}
```

- [ ] **Step 2: Run test — verify it fails**

- [ ] **Step 3: Add `risk_findings` to the trait**

In `crates/anno-rag/src/legal/kg.rs`, add to the trait after `contract_obligations`:

```rust
    /// Return risk findings for a document or dossier.
    ///
    /// # Errors
    /// Returns backend-specific graph errors.
    async fn risk_findings(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        let key = if is_dossier {
            "risk_findings_dossier"
        } else {
            "risk_findings_doc"
        };
        self.cypher(
            key,
            HashMap::from([("scope".to_string(), scope_id.to_string())]),
        )
        .await
    }
```

- [ ] **Step 4: Add SQL implementation**

Add private methods to `SqliteLegalGraphStore`:

```rust
    fn risk_findings_rows(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        let filter_col = if is_dossier {
            "json_extract(d.props_json, '$.dossier_id')"
        } else {
            "d.id"
        };
        let query = format!(
            "SELECT r.id AS rid,
                    json_extract(r.props_json, '$.severity') AS severity,
                    json_extract(r.props_json, '$.category') AS category,
                    json_extract(r.props_json, '$.text_pseudo') AS text
             FROM legal_nodes d
             JOIN legal_edges chunk_edge
               ON chunk_edge.from_label = 'Document'
              AND chunk_edge.from_key = d.id
              AND chunk_edge.to_label = 'Chunk'
              AND chunk_edge.edge_type = 'HAS_CHUNK'
             JOIN legal_nodes c
               ON c.label = 'Chunk'
              AND c.id = chunk_edge.to_key
             JOIN legal_edges mention_edge
               ON mention_edge.from_label = 'Chunk'
              AND mention_edge.from_key = c.id
              AND mention_edge.to_label = 'Risk'
              AND mention_edge.edge_type = 'MENTIONS'
             JOIN legal_nodes r
               ON r.label = 'Risk'
              AND r.id = mention_edge.to_key
             WHERE d.label = 'Document'
               AND {filter_col} = ?1
             ORDER BY
               CASE json_extract(r.props_json, '$.severity')
                 WHEN 'high' THEN 1
                 WHEN 'medium' THEN 2
                 ELSE 3
               END,
               r.id"
        );
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&query).map_err(sql_err)?;
            collect_rows(&mut stmt, params![scope_id])
        })
    }
```

Register in `cypher` dispatcher:

```rust
            "risk_findings_doc" => {
                self.risk_findings_rows(required_param(&params, "scope")?, false)
            }
            "risk_findings_dossier" => {
                self.risk_findings_rows(required_param(&params, "scope")?, true)
            }
```

Add trait override:

```rust
    async fn risk_findings(
        &self,
        scope_id: &str,
        is_dossier: bool,
    ) -> Result<Vec<HashMap<String, String>>> {
        self.risk_findings_rows(scope_id, is_dossier)
    }
```

- [ ] **Step 5: Rewire `risk_review()` in `extract.rs`**

Replace the body of `risk_review()` (lines 293-338):

```rust
pub async fn risk_review(
    kg: &dyn LegalKnowledgeGraph,
    scope_id: &str,
    is_dossier: bool,
) -> Result<RiskReview> {
    let rows = kg.risk_findings(scope_id, is_dossier).await?;

    let findings = rows
        .iter()
        .map(|r| {
            let severity = r.get("severity").cloned().unwrap_or_default();
            let recommendation = recommendation_for_severity(&severity);
            RiskFinding {
                risk_id: r.get("rid").cloned().unwrap_or_default(),
                severity,
                category: r.get("category").cloned().unwrap_or_default(),
                text_pseudo: r.get("text").cloned().unwrap_or_default(),
                recommendation,
            }
        })
        .collect();

    Ok(RiskReview {
        scope_id: scope_id.to_string(),
        findings,
    })
}
```

- [ ] **Step 6: Clean up `HashMap` import if unused**

Check if `std::collections::HashMap` is still used in `extract.rs`. After this change, `extract_contract` and `risk_review` no longer use it. Check `extract_case_file` — it still uses `kg.cypher(...)` with `HashMap`. Leave the import.

- [ ] **Step 7: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/src/legal/extract.rs crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "feat(legal): add risk_findings SQL query + rewire risk_review

New trait method risk_findings(scope_id, is_dossier) with SQLite impl.
Rewire risk_review() to use it instead of raw Cypher on SOURCES edge.
Severity-ordered results (high > medium > low)."
```

---

### Task 11: End-to-end integration test

**Files:**
- Modify: `crates/anno-rag/tests/legal_graph_v0.rs`

- [ ] **Step 1: Write integration test exercising all 3 D2 workflows**

```rust
#[tokio::test]
async fn d2_tools_end_to_end_contract_with_risk_and_timeline() {
    use anno_rag::legal::extract::{extract_contract, risk_review, timeline};

    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let risk_id = Uuid::now_v7();
    let event_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"event:audience");

    // Build a graph with parties, obligations, risks, and events
    let (mut nodes, mut edges) = seeded_contract_graph(doc_id, chunk_id);

    // Add Risk node
    nodes.nodes.push(NodeWrite::Risk {
        risk_id,
        severity: "high".into(),
        category: "responsabilite_illimitee".into(),
        text_pseudo: "exclusion totale de responsabilité".into(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Risk",
        to_key: risk_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    // Add Event node
    nodes.nodes.push(NodeWrite::Event {
        event_id,
        kind: "audience".into(),
        event_date: Some(Utc::now()),
        deadline_date: None,
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Event",
        to_key: event_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let doc_id_str = doc_id.to_string();

    // extract_contract — should find party + obligation
    let contract = extract_contract(&kg, &doc_id_str).await.expect("extract_contract");
    assert!(!contract.rows.is_empty(), "extract_contract returned no rows");
    assert!(contract.rows.iter().any(|r| r.field.starts_with("party:")));
    assert!(contract.rows.iter().any(|r| r.field.starts_with("obligation:")));

    // risk_review — should find the risk
    let risks = risk_review(&kg, &doc_id_str, false).await.expect("risk_review");
    assert_eq!(risks.findings.len(), 1);
    assert_eq!(risks.findings[0].severity, "high");
    assert_eq!(risks.findings[0].category, "responsabilite_illimitee");

    // timeline — should find the event
    let tl = timeline(&kg, "dossier-1").await.expect("timeline");
    assert!(!tl.events.is_empty(), "timeline returned no events");
    assert!(tl.events.iter().any(|e| e.kind == "audience"));
}
```

- [ ] **Step 2: Run tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: PASS — all 3 D2 tools return non-empty results from the seeded graph.

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag/tests/legal_graph_v0.rs
git commit -m "test(legal): end-to-end integration test for all 3 D2 tools

Verifies extract_contract, risk_review, and timeline all return
non-empty results from a seeded SQLite graph with parties, obligations,
risks, and events connected via MENTIONS/PARTY_TO/HAS_CHUNK edges."
```

---

### Task 12: Final check + cargo check

**Files:** none (verification only)

- [ ] **Step 1: Verify no cargo/rustc processes running**

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

- [ ] **Step 2: Run full crate check**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
```

Expected: zero warnings, zero errors.

- [ ] **Step 3: Run full test suite for anno-rag**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```

Expected: all tests pass.

- [ ] **Step 4: Also check anno-rag-mcp compiles (it depends on anno-rag)**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: zero errors. If `anno-rag-mcp` references the old Cypher calls, it will fail here — those calls are in `extract.rs` which we already fixed.
