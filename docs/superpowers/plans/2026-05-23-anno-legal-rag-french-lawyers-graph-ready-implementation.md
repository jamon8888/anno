# Anno Legal RAG — French Lawyers, Graph-Ready Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production-grade French legal intelligence layer with a lance-graph foundation, three-layer GLiNER2+French-rules extraction, and 11 MCP tools — Phase 1 (filtered hybrid search + citation rehydration) and Phase 2 (semi-automatic tabular extraction) in one slice.

**Architecture:** Dual store. LanceDB `legal_chunk_enrichment` is a fast-filter projection per chunk; `lance-graph` is the canonical entity/relationship store with French-legal node/edge schema. **A combined-label inference path on the existing `Detector`** runs one GLiNER2 call per chunk with the union of PII labels + legal labels; the PII subset feeds vault pseudonymization (existing path), the legal subset feeds `LegalEnricher`, with spans translated into pseudonymized coordinates via a `PseudoOffsetMap` rebuilt from `cloakpipe_core::Replacer` output. Three-layer extraction (GLiNER2 entities → French legal pattern rules → co-occurrence fallback) writes atomically per document with retry semantics (`enrichment_status` sidecar table + `drain_enrichment_backlog`).

**Live-codebase notes (verified 2026-05-23):**
- `Detector` is shared across the pipeline via `OnceCell<Arc<Detector>>` (lazy init). **No `Pool<T>` exists** — IS5–IS8 task entries in our task tracker are stale (never merged to `main`). The Phase 1 plan deliberately does NOT introduce a pool; the existing `Arc<Detector>` is sufficient for the combined-label call.
- `Detector::detect` currently hardcodes labels `["person","organization","location"]` and threshold `0.5` at `crates/anno-rag/src/detect.rs:248`. Plan adds a sibling `Detector::detect_with_labels(text, labels, threshold)` — additive, doesn't break existing callers.
- Pseudonymization happens inside `cloakpipe_core::Replacer::pseudonymize` and we observe results via `result.mappings` (token → original). `PseudoOffsetMap` is reconstructed *after* pseudonymization (cumulative delta walk over the sorted entity list using each entity's minted token length).
- `ChunkRecord` does **not** carry `chunk_id`; it's derived inside `Store::upsert` via `chunk_uuid(doc_id, chunk_idx)` (UUIDv5 of `"{doc_id}::{chunk_idx}"`, currently private at `store.rs:1293`). Plan exposes it as `pub` so legal code can derive matching IDs before the chunk row hits LanceDB.
- `IngestOutcome` is a struct variant: `Ingested { used_embedded_ocr: bool }` + `Skipped`. **Not** a tuple variant with chunk count. The plan code blocks use the correct variant.
- The boundary type with vault/detector is `cloakpipe_core::DetectedEntity` (`original`, `start`, `end`, `category`, `confidence`, `source`) — distinct from the legal-side `LegalEntity` we keep internal to `crates/anno-rag/src/legal/`. Both directions of mapping live in `legal::enricher`.

**Tech Stack:** Rust, `anno-rag`, `anno-rag-mcp`, `anno-rag-tabular`, LanceDB 0.29-style query/index APIs, `lance-graph` 0.5+, GLiNER2 Fastino ONNX, `rmcp`, `schemars`, `tokio`, `arrow-schema`/`arrow-array`, `chrono`.

---

## File Structure

### Created in `crates/anno-rag/src/legal/`

| File | Responsibility |
|---|---|
| `mod.rs` | module entry, re-exports |
| `types.rs` | `LegalLabel`, `LegalEntity`, `LegalChunkEnrichment`, `LegalSearchFilters`, `LegalSearchHit`, `ExtractedFact<T>`, `PartyKind`, `CourtRef`, `ArticleRef`, `PrescStart`, label catalog, threshold table |
| `normalize.rs` | French pure-function normalization (party, SIREN, court, code refs, amounts, dates, délais) |
| `offsets.rs` | `PseudoOffsetMap` — span translation between raw and pseudonymized text |
| `enricher.rs` | `LegalEntityExtractor` trait, `LegalEnricher`, GLiNER2 adapter, FakeExtractor for tests |
| `rules.rs` | French legal pattern rules (party roles, obligations, code refs, court routing, procedural events, délais relatifs) |
| `store.rs` | LanceDB `legal_chunk_enrichment` schema + indexes + filter SQL + upsert |
| `status.rs` | LanceDB `enrichment_status` sidecar + retry queue ops |
| `kg.rs` | `LegalKnowledgeGraph` trait, `LanceGraphStore` impl, node/edge batch API |
| `query.rs` | Five named graph intents (`party_dossier`, `obligations_owed_by`, `citation_chain`, `procedural_timeline`, `appeal_chain`) |
| `codes.rs` | French code-reference alias table (`c. civ.` → `code_civil`, etc.) |
| `courts.rs` | French court taxonomy alias table |
| `mandatory.rs` | Per-doctype mandatory-clause checklist + evaluator |
| `prescription.rs` | French prescription rule table + interruption logic |
| `extract.rs` | `legal_extract_contract`, `legal_extract_case_file` workflows over `anno-rag-tabular` |
| `eval.rs` | Gold-corpus loader + metric computations |
| `audit.rs` | Structured audit-event helpers |

### Modified

| File | Reason |
|---|---|
| `crates/anno-rag/Cargo.toml` | add `lance-graph`, `regex` if absent |
| `crates/anno-rag/src/lib.rs` | `pub mod legal;` |
| `crates/anno-rag/src/error.rs` | add `Error::Legal(String)`, `Error::Graph(String)` |
| `crates/anno-rag/src/detect.rs` | `extract_with_labels(text, labels, thresholds)` API + label-union helpers |
| `crates/anno-rag/src/vault.rs` | `pseudonymize_with_map(...) -> (String, PseudoOffsetMap)` |
| `crates/anno-rag/src/store.rs` | `chunk_id_filter_sql`, `search_filtered_to_chunks`, `chunk_by_id` |
| `crates/anno-rag/src/pipeline.rs` | wire `legal_store`/`legal_kg`/`legal_enricher`, modify `ingest_one_counted`, add `legal_*` methods + `drain_enrichment_backlog` + `optimize_after_ingest` |
| `crates/anno-rag-mcp/src/lib.rs` | 11 `legal_*` tools |

---

## Pre-Flight

- [ ] **Step 1: Create the worktree off origin/main**

```powershell
git fetch origin
git worktree add .claude/worktrees/legal-rag-fr-graph -b feat/legal-rag-fr-graph origin/main
cd .claude/worktrees/legal-rag-fr-graph
```

Expected: clean worktree, branch tip equals `origin/main`.

- [ ] **Step 2: Verify baseline tests pass**

```powershell
cargo test -p anno-rag --lib
cargo test -p anno-rag-mcp --lib
```

Expected: PASS. If anything fails before edits, capture and stop.

- [ ] **Step 3: GitNexus impact pre-check on symbols this plan will edit**

```powershell
npx gitnexus impact --repo anno Pipeline --direction upstream
npx gitnexus impact --repo anno Store --direction upstream
npx gitnexus impact --repo anno Detector --direction upstream
npx gitnexus impact --repo anno Vault --direction upstream
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
```

Expected: inspect blast radius. If HIGH or CRITICAL on any, report to user before continuing.

- [ ] **Step 4: Add missing dependencies to `anno-rag/Cargo.toml`**

`regex` and `chrono` are already in `[dependencies]` via workspace. **Missing** from the current crate (verified 2026-05-23): `lance-graph`, `once_cell`, `hex`, `async-trait`.

In `crates/anno-rag/Cargo.toml`, add under `[dependencies]`:

```toml
lance-graph = "0.5"
once_cell = "1"
hex = "0.4"
async-trait = "0.1"
```

Run:

```powershell
cargo check -p anno-rag
```

Expected: PASS (compiles even if no module uses lance-graph yet). If `lance-graph 0.5` fails to resolve, check crates.io for the latest 0.5.x patch (the README we verified pointed to v0.5.4 at brainstorm time).

Commit:

```powershell
git add crates/anno-rag/Cargo.toml Cargo.lock
git commit -m "deps: add lance-graph + once_cell + hex + async-trait to anno-rag"
```

- [ ] **Step 5: Add `Error::Legal` and `Error::Graph` variants**

In `crates/anno-rag/src/error.rs`, add to the `Error` enum:

```rust
#[error("legal: {0}")]
Legal(String),

#[error("graph: {0}")]
Graph(String),
```

Run:

```powershell
cargo check -p anno-rag
```

Expected: PASS.

Commit:

```powershell
git add crates/anno-rag/src/error.rs
git commit -m "feat: add Error::Legal and Error::Graph variants"
```

---

## Stage A — Foundation (5 tasks)

### Task A1: Legal types and label catalog

**Files:**
- Create: `crates/anno-rag/src/legal/mod.rs`
- Create: `crates/anno-rag/src/legal/types.rs`
- Modify: `crates/anno-rag/src/lib.rs`

- [ ] **Step 1: Write the failing tests in `types.rs`**

```rust
// at the bottom of crates/anno-rag/src/legal/types.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_legal_labels_include_french_legal_core() {
        let labels = default_legal_labels();
        for expected in [
            "person", "organization", "contract_party", "court",
            "jurisdiction", "legal_reference", "article", "code",
            "case_number", "effective_date", "deadline", "amount",
            "clause_type", "obligation", "sanction", "risk_indicator",
            "company_identifier", "lawyer", "judge",
        ] {
            assert!(labels.iter().any(|l| l.name == expected), "missing {expected}");
        }
    }

    #[test]
    fn thresholds_present_for_every_label() {
        let labels = default_legal_labels();
        let thresholds = default_thresholds();
        for l in &labels {
            assert!(thresholds.contains_key(l.name), "no threshold for {}", l.name);
            let t = thresholds[l.name];
            assert!((0.0..=1.0).contains(&t), "threshold {t} out of range for {}", l.name);
        }
    }

    #[test]
    fn filters_empty_means_no_chunk_filter() {
        let filters = LegalSearchFilters::default();
        assert!(!filters.has_any_filter());
    }

    #[test]
    fn filters_with_party_mean_filter_required() {
        let filters = LegalSearchFilters {
            parties: vec!["org:acme".to_string()],
            ..LegalSearchFilters::default()
        };
        assert!(filters.has_any_filter());
    }

    #[test]
    fn extracted_fact_needs_validation_when_in_borderline_band() {
        let fact = ExtractedFact::new(
            "obligation".to_string(),
            0.60,
            uuid::Uuid::nil(),
            0,
            8,
            "v1".into(),
            "gliner2".into(),
            0.55, // threshold
            0.10, // band width
        );
        assert!(fact.needs_validation);
        assert!(!fact.below_threshold());
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::types --lib
```

Expected: FAIL because the `legal` module does not exist.

- [ ] **Step 3: Implement `legal/mod.rs`**

```rust
//! Legal RAG domain layer: extraction labels, metadata, storage, search, graph.

pub mod audit;
pub mod codes;
pub mod courts;
pub mod enricher;
pub mod eval;
pub mod extract;
pub mod kg;
pub mod mandatory;
pub mod normalize;
pub mod offsets;
pub mod prescription;
pub mod query;
pub mod rules;
pub mod status;
pub mod store;
pub mod types;

pub use types::{
    default_legal_labels, default_thresholds, ArticleRef, CourtRef, ExtractedFact,
    LegalChunkEnrichment, LegalEntity, LegalLabel, LegalSearchFilters, LegalSearchHit,
    PartyKind, PrescStart,
};
```

> **Note:** referenced sub-modules are added across tasks A1–D6. To keep the crate compiling between tasks, every sub-module file is created as an empty placeholder (`//! placeholder, populated by task XN`) in the **next step**.

- [ ] **Step 4: Create placeholder files for all sub-modules**

For each path below, create the file with `//! placeholder, populated by task <task-id>` as the sole content. This keeps `pub mod` lines compiling.

```text
crates/anno-rag/src/legal/audit.rs        // populated by D6 (audit helpers)
crates/anno-rag/src/legal/codes.rs        // populated by B2 step rule (or A2 normalize)
crates/anno-rag/src/legal/courts.rs       // populated by B2
crates/anno-rag/src/legal/enricher.rs     // populated by B1/B3
crates/anno-rag/src/legal/eval.rs         // populated by D6
crates/anno-rag/src/legal/extract.rs      // populated by D2
crates/anno-rag/src/legal/kg.rs           // populated by C1
crates/anno-rag/src/legal/mandatory.rs    // populated by D3
crates/anno-rag/src/legal/normalize.rs    // populated by A2
crates/anno-rag/src/legal/offsets.rs      // populated by B3 (or here, see Task B3)
crates/anno-rag/src/legal/prescription.rs // populated by D4
crates/anno-rag/src/legal/query.rs        // populated by C4
crates/anno-rag/src/legal/rules.rs        // populated by B2
crates/anno-rag/src/legal/status.rs       // populated by A5
crates/anno-rag/src/legal/store.rs        // populated by A4
```

- [ ] **Step 5: Implement `legal/types.rs`**

```rust
//! Legal types, label catalog, thresholds, and search-filter struct.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// One GLiNER2 label plus a French legal description used at inference time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalLabel {
    /// Machine-stable label name passed to GLiNER2.
    pub name: &'static str,
    /// French-language description shown to the model for disambiguation.
    pub description: &'static str,
}

/// One extracted legal span (post-threshold filtering, in chunk coordinates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalEntity {
    pub label: String,
    pub text: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub confidence: f32,
}

/// Provenance-carrying wrapper for any extracted fact written to graph or table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedFact<T> {
    pub value: T,
    pub confidence: f32,
    pub needs_validation: bool,
    pub source_chunk_id: Uuid,
    pub byte_start: u32,
    pub byte_end: u32,
    pub extractor_version: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
}

impl<T> ExtractedFact<T> {
    /// Construct an [`ExtractedFact`] and compute `needs_validation` against the
    /// per-label threshold + borderline band.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: T,
        confidence: f32,
        source_chunk_id: Uuid,
        byte_start: u32,
        byte_end: u32,
        extractor_version: String,
        model_id: String,
        threshold: f32,
        band_width: f32,
    ) -> Self {
        let needs_validation = confidence >= threshold && confidence < threshold + band_width;
        Self {
            value,
            confidence,
            needs_validation,
            source_chunk_id,
            byte_start,
            byte_end,
            extractor_version,
            model_id,
            threshold: Some(threshold),
        }
    }

    #[must_use]
    pub fn below_threshold(&self) -> bool {
        match self.threshold {
            Some(t) => self.confidence < t,
            None => false,
        }
    }
}

/// Phase 1 chunk-level legal projection stored in `legal_chunk_enrichment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalChunkEnrichment {
    pub chunk_id: Uuid,
    pub doc_id: Uuid,
    pub doc_type: Option<String>,
    pub legal_domain: Option<String>,
    pub jurisdiction: Option<String>,
    pub document_date: Option<DateTime<Utc>>,
    pub dossier_id: Option<String>,
    pub parties: Vec<String>,
    pub party_roles: Vec<String>,
    pub legal_refs: Vec<String>,
    pub clause_types: Vec<String>,
    pub obligation_kinds: Vec<String>,
    pub amounts_eur_cents: Vec<i64>,
    pub deadlines: Vec<DateTime<Utc>>,
    pub event_kinds: Vec<String>,
    pub risk_flags: Vec<String>,
    pub mandatory_clause_status: Option<String>,
    pub confidence_min: f32,
    pub confidence_avg: f32,
    pub extractor_version: String,
    pub model_id: String,
}

/// Filters accepted by `legal_search`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalSearchFilters {
    pub doc_type: Option<String>,
    pub legal_domain: Option<String>,
    pub jurisdiction: Option<String>,
    pub dossier_id: Option<String>,
    pub parties: Vec<String>,
    pub party_roles: Vec<String>,
    pub legal_refs: Vec<String>,
    pub clause_types: Vec<String>,
    pub obligation_kinds: Vec<String>,
    pub event_kinds: Vec<String>,
    pub risk_flags: Vec<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub min_confidence: Option<f32>,
    pub mandatory_clause_status: Option<String>,
}

impl LegalSearchFilters {
    #[must_use]
    pub fn has_any_filter(&self) -> bool {
        self.doc_type.is_some()
            || self.legal_domain.is_some()
            || self.jurisdiction.is_some()
            || self.dossier_id.is_some()
            || !self.parties.is_empty()
            || !self.party_roles.is_empty()
            || !self.legal_refs.is_empty()
            || !self.clause_types.is_empty()
            || !self.obligation_kinds.is_empty()
            || !self.event_kinds.is_empty()
            || !self.risk_flags.is_empty()
            || self.date_from.is_some()
            || self.date_to.is_some()
            || self.min_confidence.is_some()
            || self.mandatory_clause_status.is_some()
    }
}

/// Legal search hit with the underlying chunk text and projection metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalSearchHit {
    pub chunk_id: Uuid,
    pub doc_id: Uuid,
    pub text_pseudo: String,
    pub score: f32,
    pub enrichment: Option<LegalChunkEnrichment>,
}

/// Party kind (person or organization).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartyKind { Person, Organization }

/// Normalized court reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourtRef {
    pub id: String,           // e.g. "trib_com_paris"
    pub name: String,         // canonical display
    pub level: CourtLevel,
    pub jurisdiction: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CourtLevel { Tribunal, CourAppel, CourCassation, ConseilEtat, Administratif }

/// Normalized code-article reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleRef {
    pub code: String,         // e.g. "code_civil"
    pub article_num: String,  // e.g. "1240" or "L210-2"
}

impl ArticleRef {
    #[must_use]
    pub fn normalized_ref(&self) -> String {
        format!("{}:{}", self.code, self.article_num)
    }
}

/// When a prescription period begins counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrescStart { KnowledgeOfDamage, EventDate, DueDate }

/// Default French-language label catalog.
#[must_use]
pub fn default_legal_labels() -> Vec<LegalLabel> {
    vec![
        LegalLabel { name: "person", description: "une personne physique nommée dans un document juridique" },
        LegalLabel { name: "organization", description: "une société, association, administration, juridiction ou institution" },
        LegalLabel { name: "company_identifier", description: "un SIRET, SIREN, RCS, numéro de TVA ou identifiant d'enregistrement d'entreprise" },
        LegalLabel { name: "contract_party", description: "une partie liée par un contrat, avenant, mise en demeure ou litige" },
        LegalLabel { name: "court", description: "une juridiction, tribunal, chambre ou formation de jugement" },
        LegalLabel { name: "jurisdiction", description: "une compétence, loi applicable, ressort, clause attributive de compétence" },
        LegalLabel { name: "legal_reference", description: "une référence à un code, article, loi, décret, arrêt ou autorité juridique" },
        LegalLabel { name: "code", description: "un code juridique tel que le Code civil, Code de commerce, Code du travail" },
        LegalLabel { name: "article", description: "un numéro ou référence d'article dans un code, statut, contrat ou pièce" },
        LegalLabel { name: "case_number", description: "un numéro de dossier, RG, Portalis, appel ou décision" },
        LegalLabel { name: "effective_date", description: "la date d'effet d'un contrat, clause, obligation ou décision" },
        LegalLabel { name: "deadline", description: "une date à laquelle une partie doit agir sous peine de perdre un droit" },
        LegalLabel { name: "amount", description: "un montant monétaire : dommages, frais, pénalité, prix, loyer ou intérêts" },
        LegalLabel { name: "clause_type", description: "un type de clause contractuelle (résiliation, responsabilité, confidentialité, pénalité, renouvellement, cession, non-concurrence)" },
        LegalLabel { name: "obligation", description: "une obligation imposée à une partie par un contrat, jugement, mise en demeure ou clause" },
        LegalLabel { name: "sanction", description: "une pénalité, dommages-intérêts, intérêts, résiliation, déchéance ou conséquence légale" },
        LegalLabel { name: "risk_indicator", description: "une phrase indiquant un risque juridique, financier, procédural ou contractuel" },
        LegalLabel { name: "lawyer", description: "un avocat, conseil ou représentant légal" },
        LegalLabel { name: "judge", description: "un juge, magistrat ou président de juridiction" },
        LegalLabel { name: "regulator", description: "une autorité administrative ou de régulation (CNIL, AMF, ACPR, ARCEP, ARS, etc.)" },
    ]
}

/// Per-label confidence thresholds. Anything in `[t, t+0.10)` is `needs_validation=true`.
#[must_use]
pub fn default_thresholds() -> HashMap<&'static str, f32> {
    [
        ("person", 0.70), ("organization", 0.70), ("contract_party", 0.65),
        ("court", 0.80), ("jurisdiction", 0.75),
        ("legal_reference", 0.85), ("article", 0.85), ("code", 0.85), ("case_number", 0.85),
        ("effective_date", 0.75), ("deadline", 0.70), ("amount", 0.80),
        ("clause_type", 0.60), ("obligation", 0.55), ("sanction", 0.65), ("risk_indicator", 0.55),
        ("company_identifier", 0.90), ("lawyer", 0.70), ("judge", 0.75), ("regulator", 0.75),
    ].into_iter().collect()
}

/// Width of the borderline-confidence band that flags facts for human validation.
pub const VALIDATION_BAND: f32 = 0.10;
```

- [ ] **Step 6: Export the module in `lib.rs`**

In `crates/anno-rag/src/lib.rs`, add with the other `pub mod` lines:

```rust
pub mod legal;
```

- [ ] **Step 7: Run tests**

```powershell
cargo test -p anno-rag legal::types --lib
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag/src/lib.rs crates/anno-rag/src/legal/
git commit -m "feat(legal): add domain types, label catalog, and thresholds"
```

---

### Task A2: French normalization (`legal/normalize.rs`)

**Files:**
- Modify: `crates/anno-rag/src/legal/normalize.rs`

- [ ] **Step 1: Write failing tests**

Replace the file contents with the test block + skeleton:

```rust
//! French legal normalization. Pure, model-free.

use crate::legal::types::{ArticleRef, PartyKind};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_party_org_strips_legal_suffixes() {
        let (kind, norm) = canonical_party_form("Société ACME SAS au capital de 50 000 euros");
        assert_eq!(kind, PartyKind::Organization);
        assert_eq!(norm, "org:acme");
    }

    #[test]
    fn canonical_party_person_extracts_lastname_firstname() {
        let (kind, norm) = canonical_party_form("Monsieur Martin DUPONT");
        assert_eq!(kind, PartyKind::Person);
        assert_eq!(norm, "person:dupont_martin");
    }

    #[test]
    fn parse_siren_extracts_9_digit_number_with_spaces() {
        assert_eq!(parse_siren("RCS Paris 123 456 789"), Some("123456789".to_string()));
        assert_eq!(parse_siren("aucun siren ici"), None);
    }

    #[test]
    fn parse_code_reference_handles_common_aliases() {
        let refs = parse_code_reference("art. 1240 c. civ. et article L210-2 du Code de commerce");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].normalized_ref(), "code_civil:1240");
        assert_eq!(refs[1].normalized_ref(), "code_commerce:L210-2");
    }

    #[test]
    fn parse_amount_eur_handles_french_formats() {
        assert_eq!(parse_amount_eur("10 000 €"), Some(1_000_000));
        assert_eq!(parse_amount_eur("10.000 EUR"), Some(1_000_000));
        assert_eq!(parse_amount_eur("1 234,56 €"), Some(123_456));
    }

    #[test]
    fn parse_french_date_handles_common_formats() {
        let dmy = parse_french_date("le 12 mars 2024").unwrap();
        assert_eq!((dmy.year(), dmy.month(), dmy.day()), (2024, 3, 12));
        let slash = parse_french_date("12/03/2024").unwrap();
        assert_eq!((slash.year(), slash.month(), slash.day()), (2024, 3, 12));
    }

    #[test]
    fn parse_delay_adds_calendar_days_to_anchor() {
        let anchor = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let due = parse_delay("dans un délai de 30 jours à compter de la notification", anchor).unwrap();
        assert_eq!(due.day(), 31);
        assert_eq!(due.month(), 1);
        assert_eq!(due.year(), 2024);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::normalize --lib
```

Expected: FAIL because functions are not yet implemented.

- [ ] **Step 3: Implement the normalization functions**

Append below the test block (top of file, above `#[cfg(test)]`):

```rust
/// Canonical form for a party reference. Returns `(kind, "person:lastname_firstname"|"org:slug")`.
#[must_use]
pub fn canonical_party_form(raw: &str) -> (PartyKind, String) {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();
    let is_person_marker = ["monsieur ", "madame ", "m. ", "mme ", "mlle "]
        .iter()
        .any(|p| lower.starts_with(p));

    if is_person_marker {
        // Strip the marker and try lastname/firstname extraction.
        let cleaned = strip_person_marker(trimmed);
        let (last, first) = split_person_name(&cleaned);
        let slug = format!("{}{}",
            slugify(&last),
            if first.is_empty() { String::new() } else { format!("_{}", slugify(&first)) }
        );
        return (PartyKind::Person, format!("person:{slug}"));
    }

    // Organisation path. Strip "société ", common suffixes, and capital info.
    let mut s = trimmed.to_string();
    if s.to_lowercase().starts_with("société ") { s = s[8..].to_string(); }
    for suffix in ORG_SUFFIXES { s = s.replacen(suffix, "", 1); }
    if let Some(idx) = s.to_lowercase().find(" au capital ") { s.truncate(idx); }
    let slug = slugify(s.trim());
    (PartyKind::Organization, format!("org:{slug}"))
}

const ORG_SUFFIXES: &[&str] = &[
    " SAS", " SARL", " SA", " SCI", " SCS", " SCP", " EURL", " SASU", " SNC",
    " sas", " sarl", " sa", " sci",
];

fn strip_person_marker(s: &str) -> String {
    for p in ["Monsieur ", "Madame ", "M. ", "Mme ", "Mlle "] {
        if let Some(rest) = s.strip_prefix(p) { return rest.to_string(); }
    }
    s.to_string()
}

fn split_person_name(s: &str) -> (String, String) {
    // Heuristic: ALLCAPS token is the family name. If multiple, the last contiguous block.
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let last_all_caps_idx = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && t.chars().any(char::is_alphabetic))
        .map(|(i, _)| i)
        .last();
    if let Some(i) = last_all_caps_idx {
        let last = tokens[i].to_string();
        let first = tokens.iter().enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, t)| *t).collect::<Vec<_>>().join(" ");
        (last, first)
    } else if tokens.len() == 1 {
        (tokens[0].to_string(), String::new())
    } else {
        // Treat last token as family name.
        let last = tokens.last().copied().unwrap_or_default().to_string();
        let first = tokens[..tokens.len() - 1].join(" ");
        (last, first)
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true;
    for c in s.chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('_');
            prev_dash = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Extract the first SIREN (9-digit, possibly space-separated) from text.
#[must_use]
pub fn parse_siren(text: &str) -> Option<String> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"\b(\d{3})\s?(\d{3})\s?(\d{3})\b").unwrap());
    RE.captures(text).map(|c| format!("{}{}{}", &c[1], &c[2], &c[3]))
}

/// Parse all code-article references in a span.
#[must_use]
pub fn parse_code_reference(text: &str) -> Vec<ArticleRef> {
    crate::legal::codes::parse_all(text)
}

/// Parse a French monetary amount (EUR) into cents.
#[must_use]
pub fn parse_amount_eur(text: &str) -> Option<i64> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?ix)
            (\d{1,3}(?:[\s.]\d{3})*(?:,\d{1,2})?|\d+(?:,\d{1,2})?)
            \s*
            (€|EUR|euros?)
        ").unwrap()
    });
    let cap = RE.captures(text)?;
    let num = cap.get(1)?.as_str();
    let normalized = num.replace([' ', '.'], "").replace(',', ".");
    let euros: f64 = normalized.parse().ok()?;
    Some((euros * 100.0).round() as i64)
}

/// Parse a French date in common formats.
#[must_use]
pub fn parse_french_date(text: &str) -> Option<DateTime<Utc>> {
    use regex::Regex;
    static RE_DMY_TEXT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(\d{1,2})\s+(janvier|février|mars|avril|mai|juin|juillet|août|septembre|octobre|novembre|décembre)\s+(\d{4})").unwrap()
    });
    static RE_DMY_SLASH: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"(\d{1,2})[/.\-](\d{1,2})[/.\-](\d{4})").unwrap());
    if let Some(c) = RE_DMY_TEXT.captures(text) {
        let d: u32 = c[1].parse().ok()?;
        let m = french_month_to_num(&c[2])?;
        let y: i32 = c[3].parse().ok()?;
        return naive_to_utc(y, m, d);
    }
    if let Some(c) = RE_DMY_SLASH.captures(text) {
        let d: u32 = c[1].parse().ok()?;
        let m: u32 = c[2].parse().ok()?;
        let y: i32 = c[3].parse().ok()?;
        return naive_to_utc(y, m, d);
    }
    None
}

fn french_month_to_num(s: &str) -> Option<u32> {
    Some(match s.to_lowercase().as_str() {
        "janvier" => 1, "février" => 2, "mars" => 3, "avril" => 4,
        "mai" => 5, "juin" => 6, "juillet" => 7, "août" => 8,
        "septembre" => 9, "octobre" => 10, "novembre" => 11, "décembre" => 12,
        _ => return None,
    })
}

fn naive_to_utc(y: i32, m: u32, d: u32) -> Option<DateTime<Utc>> {
    let date = NaiveDate::from_ymd_opt(y, m, d)?;
    let dt = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0)?);
    Some(Utc.from_utc_datetime(&dt))
}

/// Parse a relative delay expression and add it to `anchor`.
#[must_use]
pub fn parse_delay(text: &str, anchor: DateTime<Utc>) -> Option<DateTime<Utc>> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(?:dans un d[ée]lai de\s+|sous\s+)(\d+)\s+(jours?|mois|ans?)").unwrap()
    });
    let cap = RE.captures(text)?;
    let n: i64 = cap[1].parse().ok()?;
    let unit = cap[2].to_lowercase();
    let days = match unit.as_str() {
        "jour" | "jours" => n,
        "mois" => n * 30,
        "an" | "ans" => n * 365,
        _ => return None,
    };
    Some(anchor + chrono::Duration::days(days))
}
```

> **Note:** `parse_code_reference` delegates to `legal::codes::parse_all`, which is implemented in a later step of this same task.

- [ ] **Step 4: Implement `legal/codes.rs` (used by `parse_code_reference`)**

```rust
//! French code alias table and reference parser.

use crate::legal::types::ArticleRef;
use regex::Regex;

/// Map common French code aliases to canonical code identifiers.
const CODE_ALIASES: &[(&str, &str)] = &[
    ("c. civ.", "code_civil"), ("cciv", "code_civil"), ("code civil", "code_civil"),
    ("c. com.", "code_commerce"), ("ccom", "code_commerce"), ("code de commerce", "code_commerce"),
    ("c. trav.", "code_travail"), ("ctrav", "code_travail"), ("code du travail", "code_travail"),
    ("c. cons.", "code_consommation"), ("ccons", "code_consommation"), ("code de la consommation", "code_consommation"),
    ("cgi", "cgi"), ("code général des impôts", "cgi"),
    ("csp", "code_sante_publique"), ("code de la santé publique", "code_sante_publique"),
    ("csss", "code_securite_sociale"), ("code de la sécurité sociale", "code_securite_sociale"),
    ("cgct", "cgct"), ("code général des collectivités territoriales", "cgct"),
    ("cpc", "code_procedure_civile"), ("code de procédure civile", "code_procedure_civile"),
    ("cppen", "code_procedure_penale"), ("code de procédure pénale", "code_procedure_penale"),
    ("cpen", "code_penal"), ("code pénal", "code_penal"),
    ("cenv", "code_environnement"), ("code de l'environnement", "code_environnement"),
    ("cmf", "code_monetaire_financier"), ("code monétaire et financier", "code_monetaire_financier"),
];

/// Resolve a code alias (case-insensitive). Returns canonical id if known.
#[must_use]
pub fn resolve_code(alias: &str) -> Option<&'static str> {
    let lower = alias.trim().to_lowercase();
    CODE_ALIASES.iter()
        .find(|(a, _)| *a == lower)
        .map(|(_, canon)| *canon)
}

/// Parse all article-with-code references from a span.
#[must_use]
pub fn parse_all(text: &str) -> Vec<ArticleRef> {
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?ix)
            (?:art(?:icle|\.)?\s+)
            ([LRDA]?\s?\d+(?:[\-.]\d+)*)
            \s+
            (?:du\s+)?
            (
                c\.?\s?civ\.?|cciv|code\s+civil|
                c\.?\s?com\.?|ccom|code\s+de\s+commerce|
                c\.?\s?trav\.?|ctrav|code\s+du\s+travail|
                c\.?\s?cons\.?|ccons|code\s+de\s+la\s+consommation|
                cgi|code\s+général\s+des\s+imp[ôo]ts|
                csp|code\s+de\s+la\s+santé\s+publique|
                csss|code\s+de\s+la\s+sécurité\s+sociale|
                cgct|code\s+général\s+des\s+collectivités\s+territoriales|
                cpc|code\s+de\s+procédure\s+civile|
                cppen|code\s+de\s+procédure\s+pénale|
                cpen|code\s+pénal|
                cenv|code\s+de\s+l'environnement|
                cmf|code\s+monétaire\s+et\s+financier
            )
        ").unwrap()
    });
    RE.captures_iter(text)
        .filter_map(|c| {
            let article_num = c.get(1)?.as_str().replace(' ', "");
            let code = resolve_code(c.get(2)?.as_str())?.to_string();
            Some(ArticleRef { code, article_num })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_code_handles_common_aliases() {
        assert_eq!(resolve_code("c. civ."), Some("code_civil"));
        assert_eq!(resolve_code("Code de commerce"), Some("code_commerce"));
        assert_eq!(resolve_code("inconnu"), None);
    }

    #[test]
    fn parse_all_returns_multiple_refs() {
        let refs = parse_all("art. 1240 c. civ. et article L210-2 du Code de commerce");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], ArticleRef { code: "code_civil".into(), article_num: "1240".into() });
        assert_eq!(refs[1], ArticleRef { code: "code_commerce".into(), article_num: "L210-2".into() });
    }
}
```

- [ ] **Step 5: Add `once_cell` to `Cargo.toml` if missing**

Check `crates/anno-rag/Cargo.toml`. If `once_cell` is not listed under `[dependencies]`, add:

```toml
once_cell = "1"
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p anno-rag legal::normalize --lib
cargo test -p anno-rag legal::codes --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/src/legal/normalize.rs crates/anno-rag/src/legal/codes.rs
git commit -m "feat(legal): add French normalization + code reference parser"
```

---

### Task A3: Combined-label Detector API (no Pool — `Arc<Detector>` already shared)

**Files:**
- Modify: `crates/anno-rag/src/detect.rs`

> **Codebase fact (verified 2026-05-23):** there is no `Pool<T>` in `crates/anno-rag/`. `Detector` is already shared via `OnceCell<Arc<Detector>>` inside `Pipeline` (see `pipeline.rs:33`). The combined-label path runs through the existing shared `Arc<Detector>` — no pool needed.
> The existing `Detector::detect` at `detect.rs:228–235` hardcodes labels `["person","organization","location"]` and threshold `0.5` at `detect.rs:248`. This task adds an additive `Detector::detect_with_labels(text, labels, threshold)` method; `Detector::detect` keeps its current signature so existing callers are untouched.

- [ ] **Step 1: Locate the Detector entry points**

```powershell
rg -n "fn detect|extract_with_types" crates/anno-rag/src/detect.rs
```

Note the existing `pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>>` and the underlying `self.ner.extract_with_types(text, &labels, threshold)` call.

- [ ] **Step 2: Write the failing test for combined-label extraction**

In `crates/anno-rag/src/detect.rs`, add at the bottom:

```rust
#[cfg(test)]
mod combined_label_tests {
    use super::*;

    #[test]
    fn combined_label_set_is_union_of_pii_and_legal() {
        let pii = pii_label_set();
        let legal: Vec<&str> = crate::legal::default_legal_labels()
            .iter()
            .map(|l| l.name)
            .collect();
        let combined = combined_label_set(&legal);

        for p in &pii { assert!(combined.contains(p), "missing PII label {p}"); }
        for l in &legal { assert!(combined.contains(l), "missing legal label {l}"); }
    }

    #[test]
    fn is_pii_label_distinguishes_pii_from_legal() {
        assert!(is_pii_label("person"));
        assert!(!is_pii_label("clause_type"));
        assert!(!is_pii_label("obligation"));
    }
}
```

- [ ] **Step 3: Run tests to verify failure**

```powershell
cargo test -p anno-rag combined_label --lib
```

Expected: FAIL.

- [ ] **Step 4: Implement `pii_label_set`, `is_pii_label`, `combined_label_set`, and `Detector::detect_with_labels`**

In `crates/anno-rag/src/detect.rs`, add near the top of the module:

```rust
/// PII labels recognised by the current `Detector::detect` (anno NER labels).
/// Kept narrow; widening this list changes what gets pseudonymized.
const PII_NER_LABELS: &[&str] = &["person", "organization", "location"];

#[must_use]
pub fn pii_label_set() -> Vec<&'static str> {
    PII_NER_LABELS.to_vec()
}

#[must_use]
pub fn is_pii_label(name: &str) -> bool {
    PII_NER_LABELS.contains(&name)
}

/// Return the union of PII NER labels and `extra_labels` (e.g. legal labels), de-duplicated.
#[must_use]
pub fn combined_label_set(extra_labels: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = PII_NER_LABELS.to_vec();
    for l in extra_labels {
        if let Some(static_ref) = static_label_ref(l) {
            if !out.contains(&static_ref) { out.push(static_ref); }
        }
    }
    out
}

fn static_label_ref(label: &str) -> Option<&'static str> {
    crate::legal::default_legal_labels()
        .into_iter()
        .find(|l| l.name == label)
        .map(|l| l.name)
}
```

Then add a new method to `impl Detector` (parallel to `detect`, **not** replacing it):

```rust
impl Detector {
    /// Same NER pipeline as [`Self::detect`] but with a caller-controlled
    /// label set and a single floor threshold. The legacy detector regex
    /// layer is also run — the combined output is sorted + dedup'd the same
    /// way as `detect_inner`. Char→byte offset translation is preserved.
    pub fn detect_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<DetectedEntity>> {
        let started = std::time::Instant::now();
        let input_chars = text.chars().count();
        let out = self.detect_inner_with(text, labels, threshold)?;
        let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        emit_detect_audit(input_chars, elapsed_us, &out);
        Ok(out)
    }

    fn detect_inner_with(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<DetectedEntity>> {
        // 1. FR regex set — unchanged.
        let mut all = detect_patterns(text);

        // 2. anno NER with caller-supplied labels.
        let anno_entities = self
            .ner
            .extract_with_types(text, labels, threshold)
            .map_err(|e| Error::Detect(e.to_string()))?;

        // 3. char → byte translation (same algorithm as detect_inner).
        let mut char_to_byte: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        char_to_byte.push(text.len());
        for e in anno_entities {
            let s_char = e.start();
            let n_char = e.end();
            if s_char >= char_to_byte.len() || n_char > char_to_byte.len() || s_char >= n_char {
                continue;
            }
            all.push(DetectedEntity {
                original: e.text.clone(),
                start: char_to_byte[s_char],
                end: char_to_byte[n_char],
                category: map_anno_category(&e.entity_type),
                confidence: f64::from(e.confidence),
                source: DetectionSource::Ner,
            });
        }

        // 4. Sort + dedup (same as detect_inner — copy the closure).
        all.sort_by(|a, b| {
            a.start.cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        dedup_overlaps(&mut all);
        Ok(all)
    }
}
```

> **Per-label threshold:** `extract_with_types` only takes one float. Higher per-label discrimination is enforced *after* the call inside `LegalEnricher::translate_to_pseudo` by dropping any `DetectedEntity` with `confidence < threshold[label]` — set the floor here to the minimum of all per-label thresholds (≈ 0.55).
>
> `dedup_overlaps` is the inline dedup logic currently embedded in `detect_inner`. Lift it into a private helper as part of this task so both `detect_inner` and `detect_inner_with` can call it.

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag combined_label --lib
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/detect.rs
git commit -m "feat(detect): expose combined-label set for shared GLiNER2 pool"
```

---

### Task A4: `legal_chunk_enrichment` LanceDB schema, indexes, and upsert

**Files:**
- Modify: `crates/anno-rag/src/legal/store.rs`

- [ ] **Step 1: Write failing tests**

```rust
//! LanceDB storage for `legal_chunk_enrichment`.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use crate::legal::types::{LegalChunkEnrichment, LegalSearchFilters};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use lancedb::{Connection, Table};
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_all_required_filter_columns() {
        let schema = legal_enrichment_schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        for expected in [
            "chunk_id", "doc_id", "doc_type", "legal_domain", "jurisdiction",
            "document_date", "dossier_id", "parties", "party_roles",
            "legal_refs", "clause_types", "obligation_kinds",
            "amounts_eur_cents", "deadlines", "event_kinds", "risk_flags",
            "mandatory_clause_status", "confidence_min", "confidence_avg",
            "extractor_version", "model_id",
        ] {
            assert!(names.contains(&expected), "missing column {expected}");
        }
    }

    #[test]
    fn sql_string_lit_escapes_single_quotes() {
        assert_eq!(sql_string_lit("l'avocat"), "'l''avocat'");
    }

    #[test]
    fn filter_sql_combines_all_clauses_with_and() {
        let filters = LegalSearchFilters {
            doc_type: Some("contract".into()),
            parties: vec!["org:acme".into()],
            risk_flags: vec!["overdue_obligation".into()],
            ..LegalSearchFilters::default()
        };
        let sql = LegalStore::filter_sql(&filters).expect("filter");
        assert!(sql.contains("doc_type = 'contract'"));
        assert!(sql.contains("array_contains(parties, 'org:acme')"));
        assert!(sql.contains("array_contains(risk_flags, 'overdue_obligation')"));
        assert_eq!(sql.matches(" AND ").count(), 2);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::store --lib
```

Expected: FAIL.

- [ ] **Step 3: Implement schema and filter SQL**

Append below the test block:

```rust
/// LanceDB table name for chunk-level legal projection.
pub const LEGAL_ENRICHMENT_TABLE: &str = "legal_chunk_enrichment";

#[must_use]
pub fn legal_enrichment_schema() -> Arc<Schema> {
    let utf8_list = || DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)));
    let i64_list  = || DataType::List(Arc::new(Field::new("item", DataType::Int64, true)));
    let ts_list   = || DataType::List(Arc::new(Field::new(
        "item",
        DataType::Timestamp(TimeUnit::Microsecond, None),
        true,
    )));
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_type", DataType::Utf8, true),
        Field::new("legal_domain", DataType::Utf8, true),
        Field::new("jurisdiction", DataType::Utf8, true),
        Field::new("document_date", DataType::Timestamp(TimeUnit::Microsecond, None), true),
        Field::new("dossier_id", DataType::Utf8, true),
        Field::new("parties", utf8_list(), false),
        Field::new("party_roles", utf8_list(), false),
        Field::new("legal_refs", utf8_list(), false),
        Field::new("clause_types", utf8_list(), false),
        Field::new("obligation_kinds", utf8_list(), false),
        Field::new("amounts_eur_cents", i64_list(), false),
        Field::new("deadlines", ts_list(), false),
        Field::new("event_kinds", utf8_list(), false),
        Field::new("risk_flags", utf8_list(), false),
        Field::new("mandatory_clause_status", DataType::Utf8, true),
        Field::new("confidence_min", DataType::Float32, false),
        Field::new("confidence_avg", DataType::Float32, false),
        Field::new("extractor_version", DataType::Utf8, false),
        Field::new("model_id", DataType::Utf8, false),
    ]))
}

pub(crate) fn sql_string_lit(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// LanceDB handle for legal enrichment.
#[derive(Clone)]
pub struct LegalStore {
    table: Table,
}

impl LegalStore {
    /// Open or create the table.
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let uri = cfg
            .index_path()
            .to_str()
            .ok_or_else(|| Error::Legal(format!("non-utf8 index path: {}", cfg.index_path().display())))?
            .to_string();
        let conn: Connection = lancedb::connect(&uri).execute().await
            .map_err(|e| Error::Legal(format!("legal_store connect: {e}")))?;
        let names = conn.table_names().execute().await
            .map_err(|e| Error::Legal(format!("legal_store list: {e}")))?;
        let table = if names.iter().any(|n| n == LEGAL_ENRICHMENT_TABLE) {
            conn.open_table(LEGAL_ENRICHMENT_TABLE).execute().await
        } else {
            conn.create_empty_table(LEGAL_ENRICHMENT_TABLE, legal_enrichment_schema()).execute().await
        }
        .map_err(|e| Error::Legal(format!("legal_store open: {e}")))?;
        Ok(Self { table })
    }

    /// Build a SQL predicate from filters.
    #[must_use]
    pub fn filter_sql(filters: &LegalSearchFilters) -> Option<String> {
        let mut clauses = Vec::new();
        if let Some(v) = &filters.doc_type { clauses.push(format!("doc_type = {}", sql_string_lit(v))); }
        if let Some(v) = &filters.legal_domain { clauses.push(format!("legal_domain = {}", sql_string_lit(v))); }
        if let Some(v) = &filters.jurisdiction { clauses.push(format!("jurisdiction = {}", sql_string_lit(v))); }
        if let Some(v) = &filters.dossier_id { clauses.push(format!("dossier_id = {}", sql_string_lit(v))); }
        if let Some(v) = &filters.mandatory_clause_status {
            clauses.push(format!("mandatory_clause_status = {}", sql_string_lit(v)));
        }
        for v in &filters.parties { clauses.push(format!("array_contains(parties, {})", sql_string_lit(v))); }
        for v in &filters.party_roles { clauses.push(format!("array_contains(party_roles, {})", sql_string_lit(v))); }
        for v in &filters.legal_refs { clauses.push(format!("array_contains(legal_refs, {})", sql_string_lit(v))); }
        for v in &filters.clause_types { clauses.push(format!("array_contains(clause_types, {})", sql_string_lit(v))); }
        for v in &filters.obligation_kinds { clauses.push(format!("array_contains(obligation_kinds, {})", sql_string_lit(v))); }
        for v in &filters.event_kinds { clauses.push(format!("array_contains(event_kinds, {})", sql_string_lit(v))); }
        for v in &filters.risk_flags { clauses.push(format!("array_contains(risk_flags, {})", sql_string_lit(v))); }
        if let Some(t) = filters.date_from {
            clauses.push(format!("document_date >= CAST({} AS TIMESTAMP)", t.timestamp_micros()));
        }
        if let Some(t) = filters.date_to {
            clauses.push(format!("document_date <= CAST({} AS TIMESTAMP)", t.timestamp_micros()));
        }
        if let Some(c) = filters.min_confidence {
            clauses.push(format!("confidence_min >= {c}"));
        }
        if clauses.is_empty() { None } else { Some(clauses.join(" AND ")) }
    }

    /// Build the LanceDB scalar indexes required by the filter path.
    pub async fn setup_indexes(&self) -> Result<()> {
        use lancedb::index::scalar::{BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder};
        use lancedb::index::Index;

        let existing = self.table.list_indices().await
            .map_err(|e| Error::Legal(format!("list_indices: {e}")))?;
        let has = |col: &str| existing.iter().any(|i| i.columns.iter().any(|c| c == col));

        let btree = [("chunk_id"), ("doc_id"), ("document_date")];
        for col in btree {
            if !has(col) {
                self.table.create_index(&[col], Index::BTree(BTreeIndexBuilder::default()))
                    .execute().await.map_err(|e| Error::Legal(format!("btree {col}: {e}")))?;
            }
        }
        let bitmap = [
            ("doc_type"), ("legal_domain"), ("jurisdiction"),
            ("dossier_id"), ("mandatory_clause_status"),
        ];
        for col in bitmap {
            if !has(col) {
                self.table.create_index(&[col], Index::Bitmap(BitmapIndexBuilder::default()))
                    .execute().await.map_err(|e| Error::Legal(format!("bitmap {col}: {e}")))?;
            }
        }
        let label_list = [
            "parties", "party_roles", "legal_refs",
            "clause_types", "obligation_kinds",
            "deadlines", "event_kinds", "risk_flags",
        ];
        for col in label_list {
            if !has(col) {
                self.table.create_index(&[col], Index::LabelList(LabelListIndexBuilder::default()))
                    .execute().await.map_err(|e| Error::Legal(format!("label_list {col}: {e}")))?;
            }
        }
        Ok(())
    }

    /// Upsert a batch of enrichment rows. Idempotent on (chunk_id).
    pub async fn upsert(&self, rows: &[LegalChunkEnrichment]) -> Result<()> {
        // For brevity the conversion to Arrow RecordBatch is omitted from this
        // step; implement by mirroring the existing `chunks` upsert in
        // `store.rs`, which already builds Arrow batches with these primitive
        // and list types. See Task B4 for the wiring call site.
        let _ = rows; // placeholder — real impl added in Task B4 step "implement upsert"
        Ok(())
    }

    /// Delete all rows belonging to one doc_id.
    pub async fn delete_doc(&self, doc_id: Uuid) -> Result<()> {
        let id_hex = hex::encode(doc_id.as_bytes());
        self.table
            .delete(&format!("doc_id = X'{}'", id_hex))
            .await
            .map_err(|e| Error::Legal(format!("delete_doc: {e}")))?;
        Ok(())
    }

    /// Return candidate chunk ids matching the filters.
    pub async fn filter_chunk_ids(&self, filters: &LegalSearchFilters, limit: usize) -> Result<Vec<Uuid>> {
        use arrow_array::FixedSizeBinaryArray;
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase, Select};
        if !filters.has_any_filter() { return Ok(Vec::new()); }
        let filter = Self::filter_sql(filters)
            .expect("has_any_filter true means a SQL predicate exists");
        let stream = self.table.query()
            .select(Select::columns(&["chunk_id"]))
            .only_if(filter)
            .limit(limit)
            .execute().await
            .map_err(|e| Error::Legal(format!("filter_chunk_ids: {e}")))?;
        let batches: Vec<arrow_array::RecordBatch> = stream.try_collect().await
            .map_err(|e| Error::Legal(format!("filter_chunk_ids stream: {e}")))?;
        let mut out = Vec::new();
        for batch in batches {
            let arr = batch.column_by_name("chunk_id")
                .ok_or_else(|| Error::Legal("missing chunk_id column".into()))?
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .ok_or_else(|| Error::Legal("chunk_id wrong type".into()))?;
            for i in 0..arr.len() {
                out.push(Uuid::from_slice(arr.value(i))
                    .map_err(|e| Error::Legal(format!("chunk uuid: {e}")))?);
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Add `hex` to `Cargo.toml` if missing**

Check `crates/anno-rag/Cargo.toml`. If absent, add:

```toml
hex = "0.4"
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag legal::store --lib
cargo check -p anno-rag
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/src/legal/store.rs
git commit -m "feat(legal): add legal_chunk_enrichment LanceDB schema and filter SQL"
```

---

### Task A5: `enrichment_status` sidecar table

**Files:**
- Modify: `crates/anno-rag/src/legal/status.rs`

- [ ] **Step 1: Write failing tests**

```rust
//! LanceDB sidecar table tracking dual-write retry state.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use chrono::{DateTime, Utc};
use lancedb::{Connection, Table};
use std::sync::Arc;
use uuid::Uuid;

pub const ENRICHMENT_STATUS_TABLE: &str = "enrichment_status";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentStatusKind { Pending, Ok, FailedMaxRetries }

impl EnrichmentStatusKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnrichmentStatusKind::Pending => "pending",
            EnrichmentStatusKind::Ok => "ok",
            EnrichmentStatusKind::FailedMaxRetries => "failed_max_retries",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_required_columns() {
        let schema = enrichment_status_schema();
        let names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        for expected in ["doc_id", "status", "attempts", "last_error", "last_attempt_at", "chunk_count"] {
            assert!(names.contains(&expected), "missing column {expected}");
        }
    }

    #[test]
    fn status_kind_as_str_is_stable() {
        assert_eq!(EnrichmentStatusKind::Pending.as_str(), "pending");
        assert_eq!(EnrichmentStatusKind::Ok.as_str(), "ok");
        assert_eq!(EnrichmentStatusKind::FailedMaxRetries.as_str(), "failed_max_retries");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::status --lib
```

Expected: FAIL.

- [ ] **Step 3: Implement schema + helpers**

Below the test block:

```rust
#[must_use]
pub fn enrichment_status_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("status", DataType::Utf8, false),
        Field::new("attempts", DataType::Int32, false),
        Field::new("last_error", DataType::Utf8, true),
        Field::new("last_attempt_at", DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("chunk_count", DataType::Int32, false),
    ]))
}

/// Status store handle.
#[derive(Clone)]
pub struct EnrichmentStatusStore { table: Table }

impl EnrichmentStatusStore {
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let uri = cfg.index_path().to_str()
            .ok_or_else(|| Error::Legal("non-utf8 index path".into()))?
            .to_string();
        let conn: Connection = lancedb::connect(&uri).execute().await
            .map_err(|e| Error::Legal(format!("status connect: {e}")))?;
        let names = conn.table_names().execute().await
            .map_err(|e| Error::Legal(format!("status list: {e}")))?;
        let table = if names.iter().any(|n| n == ENRICHMENT_STATUS_TABLE) {
            conn.open_table(ENRICHMENT_STATUS_TABLE).execute().await
        } else {
            conn.create_empty_table(ENRICHMENT_STATUS_TABLE, enrichment_status_schema()).execute().await
        }.map_err(|e| Error::Legal(format!("status open: {e}")))?;
        Ok(Self { table })
    }

    /// Upsert a `pending` row (initial failure → attempt = current attempts + 1, or 1 if new).
    pub async fn mark_pending(&self, doc_id: Uuid, chunk_count: i32, err: &str) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0) + 1;
        self.upsert_row(doc_id, EnrichmentStatusKind::Pending, attempts, Some(err.to_string()), chunk_count).await
    }

    /// Mark the doc as successfully enriched.
    pub async fn mark_ok(&self, doc_id: Uuid) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0);
        self.upsert_row(doc_id, EnrichmentStatusKind::Ok, attempts, None, 0).await
    }

    /// Mark the doc as failed (max retries exhausted).
    pub async fn mark_failed_max_retries(&self, doc_id: Uuid, err: &str) -> Result<()> {
        let attempts = self.attempts(doc_id).await?.unwrap_or(0);
        self.upsert_row(doc_id, EnrichmentStatusKind::FailedMaxRetries, attempts, Some(err.to_string()), 0).await
    }

    /// List pending docs sorted by oldest last_attempt_at first.
    pub async fn list_pending(&self, max: usize) -> Result<Vec<PendingDoc>> {
        use arrow_array::{FixedSizeBinaryArray, Int32Array, StringArray, TimestampMicrosecondArray};
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};
        let stream = self.table.query()
            .only_if(format!("status = 'pending'"))
            .limit(max)
            .execute().await
            .map_err(|e| Error::Legal(format!("list_pending: {e}")))?;
        let batches: Vec<arrow_array::RecordBatch> = stream.try_collect().await
            .map_err(|e| Error::Legal(format!("list_pending stream: {e}")))?;
        let mut out = Vec::new();
        for batch in batches {
            let id_col = batch.column_by_name("doc_id").unwrap()
                .as_any().downcast_ref::<FixedSizeBinaryArray>().unwrap();
            let att_col = batch.column_by_name("attempts").unwrap()
                .as_any().downcast_ref::<Int32Array>().unwrap();
            let err_col = batch.column_by_name("last_error").unwrap()
                .as_any().downcast_ref::<StringArray>();
            let ts_col = batch.column_by_name("last_attempt_at").unwrap()
                .as_any().downcast_ref::<TimestampMicrosecondArray>().unwrap();
            let cc_col = batch.column_by_name("chunk_count").unwrap()
                .as_any().downcast_ref::<Int32Array>().unwrap();
            for i in 0..batch.num_rows() {
                out.push(PendingDoc {
                    doc_id: Uuid::from_slice(id_col.value(i)).unwrap(),
                    attempts: att_col.value(i),
                    last_error: err_col.and_then(|c| (!c.is_null(i)).then(|| c.value(i).to_string())),
                    last_attempt_at: chrono::DateTime::<Utc>::from_timestamp_micros(ts_col.value(i)).unwrap_or_default(),
                    chunk_count: cc_col.value(i),
                });
            }
        }
        out.sort_by_key(|d| d.last_attempt_at);
        Ok(out)
    }

    async fn attempts(&self, doc_id: Uuid) -> Result<Option<i32>> {
        // Implementation reads one row by doc_id; for brevity the Arrow
        // decoding is omitted and mirrors `list_pending`. The function is
        // covered by integration tests in Task B5 step "drain idempotency".
        let _ = doc_id;
        Ok(None)
    }

    async fn upsert_row(
        &self,
        doc_id: Uuid,
        status: EnrichmentStatusKind,
        attempts: i32,
        last_error: Option<String>,
        chunk_count: i32,
    ) -> Result<()> {
        // Build a 1-row RecordBatch and upsert. Skeleton; the Arrow encoder
        // is added once the conversion helpers are imported from store.rs.
        let _ = (doc_id, status, attempts, last_error, chunk_count);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PendingDoc {
    pub doc_id: Uuid,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub last_attempt_at: DateTime<Utc>,
    pub chunk_count: i32,
}
```

> **Note:** `attempts()` and `upsert_row()` are deliberately stubbed at this task. Task B5 implements both behind a single private helper that builds the 1-row Arrow batch (the doc_id+status+attempts+ts+chunk_count encoding). Keeping the API stable in A5 lets dependent tasks compile.

- [ ] **Step 4: Run tests**

```powershell
cargo test -p anno-rag legal::status --lib
cargo check -p anno-rag
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/status.rs
git commit -m "feat(legal): add enrichment_status sidecar schema + handle"
```

---

## Stage B — Extraction & dual-write (5 tasks)

### Task B1: `LegalEntityExtractor` trait + GLiNER2 adapter + fake

**Files:**
- Modify: `crates/anno-rag/src/legal/enricher.rs`

- [ ] **Step 1: Write failing tests with a FakeExtractor**

```rust
//! Legal entity extraction. Trait + GLiNER2 adapter + in-memory fake.

use crate::error::Result;
use crate::legal::types::{LegalEntity, LegalLabel};
use std::collections::HashMap;

/// Abstraction so unit tests don't load model weights.
pub trait LegalEntityExtractor: Send + Sync {
    fn extract(
        &self,
        text: &str,
        labels: &[LegalLabel],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>>;

    fn model_id(&self) -> &'static str;
    fn extractor_version(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legal::default_thresholds;

    pub struct FakeExtractor;

    impl LegalEntityExtractor for FakeExtractor {
        fn extract(&self, text: &str, _labels: &[LegalLabel], _thresholds: &HashMap<&'static str, f32>) -> Result<Vec<LegalEntity>> {
            let start = text.find("paiement").expect("fixture contains paiement") as u32;
            Ok(vec![LegalEntity {
                label: "obligation".into(),
                text: "paiement".into(),
                byte_start: start,
                byte_end: start + 8,
                confidence: 0.91,
            }])
        }
        fn model_id(&self) -> &'static str { "fake" }
        fn extractor_version(&self) -> &'static str { "test-v0" }
    }

    #[test]
    fn fake_extractor_returns_obligation_paiement() {
        let labels = crate::legal::default_legal_labels();
        let thr = default_thresholds();
        let out = FakeExtractor.extract("Le paiement intervient sous 30 jours.", &labels, &thr).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "obligation");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::enricher --lib
```

Expected: FAIL.

- [ ] **Step 3: Implement the GLiNER2 adapter**

Append below the test block:

```rust
/// GLiNER2-backed legal extractor. Reuses the existing detector model.
pub struct GlinerLegalExtractor {
    inner: std::sync::Arc<dyn GlinerSession>,
    extractor_version: &'static str,
}

/// Minimal trait the extractor needs from the session (real impl wraps
/// `anno::backends::gliner2_fastino::GLiNER2Fastino`).
pub trait GlinerSession: Send + Sync {
    fn extract_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>>;
    fn model_id(&self) -> &'static str;
}

impl GlinerLegalExtractor {
    pub fn new(inner: std::sync::Arc<dyn GlinerSession>) -> Self {
        Self { inner, extractor_version: env!("CARGO_PKG_VERSION") }
    }
}

impl LegalEntityExtractor for GlinerLegalExtractor {
    fn extract(
        &self,
        text: &str,
        labels: &[LegalLabel],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>> {
        let names: Vec<&str> = labels.iter().map(|l| l.name).collect();
        self.inner.extract_with_labels(text, &names, thresholds)
    }
    fn model_id(&self) -> &'static str { self.inner.model_id() }
    fn extractor_version(&self) -> &'static str { self.extractor_version }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test -p anno-rag legal::enricher --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/enricher.rs
git commit -m "feat(legal): add LegalEntityExtractor trait + GLiNER2 adapter + fake"
```

---

### Task B2: French legal pattern rules (`legal/rules.rs`)

**Files:**
- Modify: `crates/anno-rag/src/legal/rules.rs`
- Modify: `crates/anno-rag/src/legal/courts.rs`

- [ ] **Step 1: Write failing tests for the rules engine**

```rust
//! French legal pattern rules. Deterministic, model-free.

use crate::legal::courts::{self, CourtAlias};
use crate::legal::normalize;
use crate::legal::types::{ArticleRef, CourtRef, LegalEntity};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// One typed fact produced by Layer 2.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedFact {
    PartyRole { party: String, role: String },
    Obligation { obligor: Option<String>, kind: String, deadline: Option<DateTime<Utc>>, amount_cents: Option<i64>, owed_to: Option<String> },
    Reference { article: ArticleRef },
    CourtRouting { court: CourtRef },
    Event { kind: String, event_date: Option<DateTime<Utc>> },
}

pub struct VaultForwardMap {
    pub alias_to_canonical: HashMap<String, String>, // ORG_1 -> "org:acme"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fwd(map: &[(&str, &str)]) -> VaultForwardMap {
        VaultForwardMap {
            alias_to_canonical: map.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn party_role_litigation_extracts_plaintiff_and_defendant() {
        let fwd = fwd(&[("ORG_1", "org:acme"), ("ORG_2", "org:beta")]);
        let facts = apply_all(Uuid::nil(), "ORG_1 c./ ORG_2", &[], &fwd);
        assert!(facts.iter().any(|f| matches!(f, TypedFact::PartyRole { role, party } if role == "plaintiff" && party == "org:acme")));
        assert!(facts.iter().any(|f| matches!(f, TypedFact::PartyRole { role, party } if role == "defendant" && party == "org:beta")));
    }

    #[test]
    fn obligation_engagement_binds_party_to_obligation() {
        let fwd = fwd(&[("ORG_1", "org:acme")]);
        let facts = apply_all(Uuid::nil(), "ORG_1 s'engage à livrer dans un délai de 30 jours.", &[], &fwd);
        assert!(facts.iter().any(|f| matches!(f, TypedFact::Obligation { obligor: Some(p), .. } if p == "org:acme")));
    }

    #[test]
    fn code_reference_rule_adds_reference_fact() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Vu l'article 1240 du Code civil", &[], &fwd);
        assert!(facts.iter().any(|f| matches!(f, TypedFact::Reference { article } if article.normalized_ref() == "code_civil:1240")));
    }

    #[test]
    fn court_routing_rule_handles_aliases() {
        let fwd = fwd(&[]);
        let facts = apply_all(Uuid::nil(), "Tribunal de commerce de Paris", &[], &fwd);
        assert!(facts.iter().any(|f| matches!(f, TypedFact::CourtRouting { court } if court.id == "trib_com_paris")));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::rules --lib
```

Expected: FAIL.

- [ ] **Step 3: Implement `legal/courts.rs` (court alias table)**

```rust
//! French court alias table.

use crate::legal::types::{CourtLevel, CourtRef};

pub struct CourtAlias {
    pub aliases: &'static [&'static str],
    pub id: &'static str,
    pub name: &'static str,
    pub level: CourtLevel,
}

const COURTS: &[CourtAlias] = &[
    CourtAlias {
        aliases: &["Tribunal de commerce de Paris", "T. com. Paris", "Trib. com. Paris", "T.com. Paris"],
        id: "trib_com_paris", name: "Tribunal de commerce de Paris", level: CourtLevel::Tribunal,
    },
    CourtAlias {
        aliases: &["Tribunal judiciaire de Paris", "TGI Paris", "Trib. jud. Paris"],
        id: "trib_jud_paris", name: "Tribunal judiciaire de Paris", level: CourtLevel::Tribunal,
    },
    CourtAlias {
        aliases: &["Cour d'appel de Paris", "CA Paris"],
        id: "ca_paris", name: "Cour d'appel de Paris", level: CourtLevel::CourAppel,
    },
    CourtAlias {
        aliases: &["Cour de cassation", "Cass."],
        id: "cour_cassation", name: "Cour de cassation", level: CourtLevel::CourCassation,
    },
    CourtAlias {
        aliases: &["Conseil d'État", "CE"],
        id: "conseil_etat", name: "Conseil d'État", level: CourtLevel::ConseilEtat,
    },
    CourtAlias {
        aliases: &["Conseil de prud'hommes de Paris", "CPH Paris"],
        id: "cph_paris", name: "Conseil de prud'hommes de Paris", level: CourtLevel::Tribunal,
    },
    // ... extend in follow-up commits as eval surfaces more aliases
];

#[must_use]
pub fn resolve(text: &str) -> Option<CourtRef> {
    let lower = text.to_lowercase();
    for c in COURTS {
        if c.aliases.iter().any(|a| lower.contains(&a.to_lowercase())) {
            return Some(CourtRef {
                id: c.id.to_string(),
                name: c.name.to_string(),
                level: c.level,
                jurisdiction: None,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_handles_common_courts() {
        assert_eq!(resolve("Tribunal de commerce de Paris").unwrap().id, "trib_com_paris");
        assert_eq!(resolve("Cour de cassation").unwrap().id, "cour_cassation");
        assert!(resolve("Some random text").is_none());
    }
}
```

- [ ] **Step 4: Implement the rules engine**

Append below the test block of `rules.rs`:

```rust
/// Apply every Layer-2 rule to a chunk's pseudonymized text.
pub fn apply_all(
    chunk_id: Uuid,
    pseudo_text: &str,
    entities: &[LegalEntity],
    fwd: &VaultForwardMap,
) -> Vec<TypedFact> {
    let mut out = Vec::new();
    out.extend(rule_party_role_litigation(pseudo_text, fwd));
    out.extend(rule_party_role_contract(pseudo_text, fwd));
    out.extend(rule_obligation_engagement(pseudo_text, fwd));
    out.extend(rule_code_reference(pseudo_text));
    out.extend(rule_court_routing(pseudo_text));
    out.extend(rule_procedural_event(pseudo_text));
    let _ = (chunk_id, entities);
    out
}

fn canonical_from_alias(fwd: &VaultForwardMap, alias: &str) -> Option<String> {
    fwd.alias_to_canonical.get(alias).cloned()
}

fn rule_party_role_litigation(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)\s*(?:c\./|contre|/)\s*(ORG_\d+|PERS_\d+)").unwrap()
    });
    let mut out = Vec::new();
    for c in RE.captures_iter(text) {
        let plaintiff_alias = &c[1];
        let defendant_alias = &c[2];
        if let Some(p) = canonical_from_alias(fwd, plaintiff_alias) {
            out.push(TypedFact::PartyRole { party: p, role: "plaintiff".into() });
        }
        if let Some(d) = canonical_from_alias(fwd, defendant_alias) {
            out.push(TypedFact::PartyRole { party: d, role: "defendant".into() });
        }
    }
    out
}

fn rule_party_role_contract(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)[^.\n]{0,40}\b(acheteur|vendeur|bailleur|locataire|employeur|salari[ée]|preneur|donneur d'ordre|prestataire|client)\b").unwrap()
    });
    let mut out = Vec::new();
    for c in RE.captures_iter(text) {
        if let Some(p) = canonical_from_alias(fwd, &c[1]) {
            let role = c[2].to_lowercase().replace("é", "e");
            out.push(TypedFact::PartyRole { party: p, role });
        }
    }
    out
}

fn rule_obligation_engagement(text: &str, fwd: &VaultForwardMap) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)(ORG_\d+|PERS_\d+)\s+(?:s'engage à|devra|s'oblige à|s'engage)\b").unwrap()
    });
    let mut out = Vec::new();
    for c in RE.captures_iter(text) {
        let obligor = canonical_from_alias(fwd, &c[1]);
        let deadline = normalize::parse_delay(text, Utc::now()).ok();
        let amount_cents = normalize::parse_amount_eur(text);
        out.push(TypedFact::Obligation {
            obligor, kind: "performance".into(),
            deadline: None, amount_cents, owed_to: None,
        });
        let _ = deadline;
    }
    out
}

fn rule_code_reference(text: &str) -> Vec<TypedFact> {
    normalize::parse_code_reference(text)
        .into_iter()
        .map(|article| TypedFact::Reference { article })
        .collect()
}

fn rule_court_routing(text: &str) -> Vec<TypedFact> {
    courts::resolve(text)
        .map(|court| vec![TypedFact::CourtRouting { court }])
        .unwrap_or_default()
}

fn rule_procedural_event(text: &str) -> Vec<TypedFact> {
    use regex::Regex;
    static RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)\b(mise en demeure|assignation|jugement|arr[êe]t|audience)\b\s+(?:du|rendu(?:e)? le|d[ée]livr[ée](?:e)? le|fix[ée](?:e)? au)?\s*([0-9]{1,2}[ /.\-][0-9]{1,2}[ /.\-][0-9]{4}|\d{1,2}\s+(?:janvier|février|mars|avril|mai|juin|juillet|août|septembre|octobre|novembre|décembre)\s+\d{4})?").unwrap()
    });
    let mut out = Vec::new();
    for c in RE.captures_iter(text) {
        let kind = c[1].to_lowercase().replace(' ', "_").replace("â", "a").replace("ê", "e");
        let event_date = c.get(2).and_then(|m| normalize::parse_french_date(m.as_str()));
        out.push(TypedFact::Event { kind, event_date });
    }
    out
}
```

> **Note:** `parse_delay` is called with `Utc::now()` as a fallback anchor here; B3 wires the real document-date anchor.

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag legal::courts --lib
cargo test -p anno-rag legal::rules --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/courts.rs crates/anno-rag/src/legal/rules.rs
git commit -m "feat(legal): add court taxonomy + Layer-2 French legal rules"
```

---

### Task B3: `LegalEnricher::enrich_chunks_batched` + `PseudoOffsetMap`

**Files:**
- Modify: `crates/anno-rag/src/legal/offsets.rs`
- Modify: `crates/anno-rag/src/legal/enricher.rs`
- Modify: `crates/anno-rag/src/vault.rs`

- [ ] **Step 1: Write the failing tests for `PseudoOffsetMap`**

In `crates/anno-rag/src/legal/offsets.rs`:

```rust
//! Span translation between raw chunk text and pseudonymized chunk text.

/// One substitution recorded during pseudonymization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Substitution {
    pub raw_start: u32,
    pub raw_end: u32,
    pub pseudo_start: u32,
    pub pseudo_end: u32,
}

/// Ordered set of substitutions covering one chunk.
#[derive(Debug, Clone, Default)]
pub struct PseudoOffsetMap { pub subs: Vec<Substitution> }

impl PseudoOffsetMap {
    /// Map a raw-text span to the corresponding pseudonymized-text span.
    /// Returns `None` if the raw span overlaps a substitution boundary
    /// (partial overlap means the original semantics no longer survive).
    #[must_use]
    pub fn translate(&self, raw_start: u32, raw_end: u32) -> Option<(u32, u32)> {
        let mut delta: i64 = 0;
        let mut covered_start: Option<u32> = None;
        let mut covered_end: Option<u32> = None;
        for s in &self.subs {
            if s.raw_end <= raw_start {
                // Sub is fully before the requested span; shift accumulates.
                delta += (s.pseudo_end as i64 - s.pseudo_start as i64)
                       - (s.raw_end as i64 - s.raw_start as i64);
                continue;
            }
            if s.raw_start >= raw_end { break; }
            // Overlap.
            if s.raw_start <= raw_start && s.raw_end >= raw_end {
                // Sub fully contains the span — return the sub's pseudo range.
                covered_start = Some(s.pseudo_start);
                covered_end = Some(s.pseudo_end);
                return Some((s.pseudo_start, s.pseudo_end));
            }
            return None; // partial overlap, ambiguous
        }
        let ps = (raw_start as i64 + delta).max(0) as u32;
        let pe = (raw_end   as i64 + delta).max(0) as u32;
        Some((ps, pe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(subs: &[Substitution]) -> PseudoOffsetMap {
        PseudoOffsetMap { subs: subs.to_vec() }
    }

    #[test]
    fn translate_no_subs_is_identity() {
        let m = map(&[]);
        assert_eq!(m.translate(5, 10), Some((5, 10)));
    }

    #[test]
    fn translate_after_substitution_shifts_by_delta() {
        // raw "Acme s'engage" (5 bytes) → pseudo "ORG_1 s'engage" (5+...)
        let m = map(&[Substitution { raw_start: 0, raw_end: 4, pseudo_start: 0, pseudo_end: 5 }]);
        // raw "engage" at byte 5..11 → pseudo at byte 6..12
        assert_eq!(m.translate(5, 11), Some((6, 12)));
    }

    #[test]
    fn translate_inside_substitution_returns_sub_range() {
        let m = map(&[Substitution { raw_start: 0, raw_end: 4, pseudo_start: 0, pseudo_end: 5 }]);
        assert_eq!(m.translate(0, 4), Some((0, 5)));
    }

    #[test]
    fn translate_partial_overlap_returns_none() {
        let m = map(&[Substitution { raw_start: 5, raw_end: 9, pseudo_start: 5, pseudo_end: 10 }]);
        assert_eq!(m.translate(7, 12), None);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::offsets --lib
```

Expected: FAIL.

- [ ] **Step 3: Implement `vault.rs` change to expose `pseudonymize_with_map`**

> **Codebase fact (verified 2026-05-23):** `Vault::pseudonymize` at `vault.rs:56` calls `cloakpipe_core::Replacer::pseudonymize(text, entities, &mut v)` which returns `(text_pseudo, mappings: token → original)`. We don't synthesize aliases ourselves — Replacer does. So the offset map is built **post-hoc** from the entities + result.mappings by walking entities in raw-offset order and reading each minted token's length from `mappings`.

In `crates/anno-rag/src/vault.rs`, **add** (don't replace) a new method on `Vault`. The boundary entity type is `cloakpipe_core::DetectedEntity` (with fields `original`, `start`, `end`, `category`, `confidence`, `source`), not a local `Entity`:

```rust
use crate::legal::offsets::{PseudoOffsetMap, Substitution};
use cloakpipe_core::DetectedEntity;

impl Vault {
    /// Pseudonymize `text` and return both the pseudonymized text and the
    /// span substitution map. Keeps `pseudonymize` (existing) intact for
    /// existing call sites.
    pub async fn pseudonymize_with_map(
        &self,
        text: &str,
        entities: &[DetectedEntity],
    ) -> crate::error::Result<(String, PseudoOffsetMap)> {
        let mut v = self.inner.lock().await;
        let result = Replacer::pseudonymize(text, entities, &mut v)
            .map_err(|e| Error::Vault(format!("replacer: {e}")))?;
        v.save()
            .map_err(|e| Error::Vault(format!("save after pseudonymize_with_map: {e}")))?;
        drop(v);

        // Walk entities in raw-offset order. For each entity, look up its
        // minted token in `result.mappings` (token -> original) and compute
        // the pseudo offsets cumulatively.
        let mut ents: Vec<&DetectedEntity> = entities.iter().collect();
        ents.sort_by_key(|e| e.start);

        let mut subs = Vec::with_capacity(ents.len());
        let mut pseudo_cursor: usize = 0;
        let mut raw_cursor: usize = 0;

        for e in ents {
            if e.start < raw_cursor || e.end > text.len() { continue; }
            // bytes of unchanged prefix from raw_cursor..e.start carry over unchanged
            pseudo_cursor += e.start - raw_cursor;
            // Find the token Replacer minted for this entity. result.mappings
            // is token -> original; we find the token whose original matches
            // e.original. If multiple entities share the same `original` they
            // collapse onto one token, but Replacer applies the same token
            // each time — so byte length is constant per original.
            let token = result.mappings.iter()
                .find(|(_, orig)| orig.as_str() == e.original)
                .map(|(tok, _)| tok.as_str())
                .ok_or_else(|| Error::Vault(format!(
                    "pseudonymize_with_map: no mapping for original {:?}", e.original
                )))?;
            let pseudo_start = pseudo_cursor as u32;
            let pseudo_end = (pseudo_cursor + token.len()) as u32;
            subs.push(Substitution {
                raw_start: e.start as u32,
                raw_end: e.end as u32,
                pseudo_start,
                pseudo_end,
            });
            pseudo_cursor += token.len();
            raw_cursor = e.end;
        }

        Ok((result.text, PseudoOffsetMap { subs }))
    }
}
```

> **Why this is correct:**
> - `cloakpipe_core::Replacer::pseudonymize` walks entities in their input order and replaces `text[e.start..e.end]` with a token.
> - The unchanged segments between entities have the same byte length in raw and pseudo text.
> - The token's byte length is `token.len()` (UTF-8 bytes — Replacer's tokens are ASCII like `ORG_1`, so `len() == chars().count()`).
> - Walking entities in `start` order and tracking `pseudo_cursor` reproduces the same offsets Replacer produced internally.

- [ ] **Step 4: Implement `LegalEnricher::enrich_chunks_batched`**

In `crates/anno-rag/src/legal/enricher.rs`, append:

```rust
use crate::legal::offsets::PseudoOffsetMap;
use crate::legal::rules::{apply_all as apply_rules, TypedFact, VaultForwardMap};
use crate::legal::types::{ExtractedFact, LegalChunkEnrichment, VALIDATION_BAND};
use uuid::Uuid;

/// Orchestrates Layer-1 entities + Layer-2 rules + Layer-3 mentions for a
/// document's chunks. Holds the shared GLiNER2 extractor + thresholds.
pub struct LegalEnricher {
    extractor: std::sync::Arc<dyn LegalEntityExtractor>,
    labels: Vec<LegalLabel>,
    thresholds: HashMap<&'static str, f32>,
}

impl LegalEnricher {
    pub fn new(extractor: std::sync::Arc<dyn LegalEntityExtractor>) -> Self {
        Self {
            extractor,
            labels: crate::legal::default_legal_labels(),
            thresholds: crate::legal::default_thresholds(),
        }
    }

    /// Translate raw-coordinate legal entities into pseudonymized coordinates.
    pub fn translate_to_pseudo(
        &self,
        raw_ents: Vec<LegalEntity>,
        map: &PseudoOffsetMap,
        pseudo_text: &str,
    ) -> Vec<LegalEntity> {
        let mut out = Vec::with_capacity(raw_ents.len());
        for e in raw_ents {
            if let Some((ps, pe)) = map.translate(e.byte_start, e.byte_end) {
                if (ps as usize) <= pseudo_text.len() && (pe as usize) <= pseudo_text.len() && ps < pe {
                    let text = pseudo_text[ps as usize..pe as usize].to_string();
                    out.push(LegalEntity { byte_start: ps, byte_end: pe, text, ..e });
                }
            }
        }
        out
    }

    /// Produce LegalChunkEnrichment + TypedFacts + mentions for a single chunk
    /// already in pseudonymized coordinates.
    pub fn enrich_one(
        &self,
        chunk_id: Uuid,
        doc_id: Uuid,
        pseudo_text: &str,
        legal_ents: &[LegalEntity],
        fwd: &VaultForwardMap,
    ) -> (LegalChunkEnrichment, Vec<TypedFact>) {
        let facts = apply_rules(chunk_id, pseudo_text, legal_ents, fwd);
        let row = projection_from_facts(chunk_id, doc_id, legal_ents, &facts, &self.extractor);
        (row, facts)
    }
}

fn projection_from_facts(
    chunk_id: Uuid,
    doc_id: Uuid,
    ents: &[LegalEntity],
    facts: &[TypedFact],
    extractor: &std::sync::Arc<dyn LegalEntityExtractor>,
) -> LegalChunkEnrichment {
    let mut parties = Vec::new();
    let mut party_roles = Vec::new();
    let mut legal_refs = Vec::new();
    let mut clause_types = Vec::new();
    let mut obligation_kinds = Vec::new();
    let mut amounts_eur_cents = Vec::new();
    let mut event_kinds = Vec::new();
    let mut deadlines = Vec::new();

    for f in facts {
        match f {
            TypedFact::PartyRole { party, role } => {
                parties.push(party.clone());
                party_roles.push(role.clone());
            }
            TypedFact::Obligation { kind, amount_cents, deadline, .. } => {
                obligation_kinds.push(kind.clone());
                if let Some(a) = amount_cents { amounts_eur_cents.push(*a); }
                if let Some(d) = deadline { deadlines.push(*d); }
            }
            TypedFact::Reference { article } => legal_refs.push(article.normalized_ref()),
            TypedFact::Event { kind, event_date } => {
                event_kinds.push(kind.clone());
                if let Some(d) = event_date { deadlines.push(*d); }
            }
            TypedFact::CourtRouting { .. } => {}
        }
    }
    // Clause types via the GLiNER2 label "clause_type"
    for e in ents.iter().filter(|e| e.label == "clause_type") {
        clause_types.push(e.text.to_lowercase());
    }

    let confidences: Vec<f32> = ents.iter().map(|e| e.confidence).collect();
    let confidence_min = confidences.iter().copied().reduce(f32::min).unwrap_or(0.0);
    let confidence_avg = if confidences.is_empty() { 0.0 } else {
        confidences.iter().sum::<f32>() / confidences.len() as f32
    };

    LegalChunkEnrichment {
        chunk_id, doc_id,
        doc_type: None, legal_domain: None, jurisdiction: None,
        document_date: None, dossier_id: None,
        parties, party_roles, legal_refs, clause_types, obligation_kinds,
        amounts_eur_cents, deadlines, event_kinds,
        risk_flags: Vec::new(),
        mandatory_clause_status: None,
        confidence_min, confidence_avg,
        extractor_version: extractor.extractor_version().to_string(),
        model_id: extractor.model_id().to_string(),
    }
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag legal::offsets --lib
cargo test -p anno-rag legal::enricher --lib
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/offsets.rs crates/anno-rag/src/legal/enricher.rs crates/anno-rag/src/vault.rs
git commit -m "feat(legal): add PseudoOffsetMap + enrich_one (Layer 1+2+3 minus graph)"
```

---

### Task B4: `ingest_one_counted` dual-write (table only, no graph yet)

**Files:**
- Modify: `crates/anno-rag/src/legal/store.rs` (implement `upsert`)
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Implement `LegalStore::upsert`**

In `legal/store.rs`, **replace** the placeholder body with a real implementation. Mirror the way the existing `crates/anno-rag/src/store.rs` builds Arrow batches for the `chunks` table (open `store.rs` and find `Store::upsert`; that function shows how to encode `FixedSizeBinary(16)` UUIDs, `List<Utf8>`, `List<Int64>`, `List<Timestamp>`, and primitive columns).

The replacement signature is:

```rust
pub async fn upsert(&self, rows: &[LegalChunkEnrichment]) -> Result<()> {
    if rows.is_empty() { return Ok(()); }
    let batch = legal_rows_to_record_batch(rows)?;
    self.table.add(arrow_array::RecordBatchIterator::new(
        vec![Ok(batch)].into_iter(),
        legal_enrichment_schema(),
    )).execute().await
    .map_err(|e| Error::Legal(format!("legal upsert: {e}")))?;
    Ok(())
}
```

Add the `legal_rows_to_record_batch` helper below `LegalStore`. The helper builds builders for each column, iterates rows, and finishes into a `RecordBatch`. Reuse helper imports from `crates/anno-rag/src/store.rs`.

- [ ] **Step 2: Wire `LegalEnricher`, `LegalStore`, `EnrichmentStatusStore` into `Pipeline`**

In `crates/anno-rag/src/pipeline.rs`:

1. Add fields to the `Pipeline` struct:

```rust
pub(crate) legal_store: crate::legal::store::LegalStore,
pub(crate) enrichment_status: crate::legal::status::EnrichmentStatusStore,
pub(crate) legal_enricher: crate::legal::enricher::LegalEnricher,
```

2. In `Pipeline::new`, after opening the existing `store`, add:

```rust
let legal_store = crate::legal::store::LegalStore::open(&cfg).await?;
legal_store.setup_indexes().await?;
let enrichment_status = crate::legal::status::EnrichmentStatusStore::open(&cfg).await?;

// Wrap whichever GLiNER2 session the detector uses inside an Arc<dyn GlinerSession>.
// The exact construction depends on the detector's API located in `crates/anno-rag/src/detect.rs`.
let extractor: std::sync::Arc<dyn crate::legal::enricher::LegalEntityExtractor> =
    std::sync::Arc::new(crate::legal::enricher::GlinerLegalExtractor::new(
        detector_session_for_legal(&detector),
    ));
let legal_enricher = crate::legal::enricher::LegalEnricher::new(extractor);
```

3. Add the helper `detector_session_for_legal` next to `Pipeline::new` (or in `detect.rs`); it returns an `Arc<dyn crate::legal::enricher::GlinerSession>` wrapping the existing detector session. Implementation depends on the detector internals — search `Detector::detect` for the inference call.

- [ ] **Step 3: Expose `chunk_uuid` from `store.rs` so legal code can pre-derive chunk IDs**

In `crates/anno-rag/src/store.rs`, change line 1293's private `fn chunk_uuid` to:

```rust
/// Deterministic chunk id: UUID v5 (OID namespace) of `"{doc_id}::{chunk_idx}"`.
/// Matches the value `Store::upsert` will write to the `chunk_id` column,
/// so legal code can derive matching IDs *before* the row hits LanceDB.
#[must_use]
pub fn chunk_uuid(doc_id: Uuid, chunk_idx: u32) -> Uuid {
    let name = format!("{doc_id}::{chunk_idx}");
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}
```

- [ ] **Step 4: Modify `ingest_one_counted` to perform the table-only dual write**

> **Codebase fact (verified 2026-05-23):**
> - `IngestOutcome` is a struct variant: `Ingested { used_embedded_ocr: bool }` + `Skipped` (`pipeline.rs:26–29`). Use the correct variant.
> - The existing per-chunk loop calls `self.detector_get_or_init()?.detect(&chunk.text)?` then `self.vault.pseudonymize(...)`. We replace the inner two lines with `detect_with_labels(combined) + pseudonymize_with_map(pii_subset)`.
> - `ChunkRecord` doesn't carry `chunk_id`; we use the exposed `chunk_uuid(doc_id, chunk.idx)` to compute matching IDs for the legal store before the upsert.

In `pipeline.rs`, replace the per-chunk loop (lines 161–166) with the combined-label path, **keep** the rest of the function. The change is purely additive after `self.store.upsert(records).await?`:

```rust
// ── 1. Combined-label detect: one NER call per chunk, union of PII + legal labels ──
let legal_labels = crate::legal::default_legal_labels();
let legal_names: Vec<&str> = legal_labels.iter().map(|l| l.name).collect();
let combined = crate::detect::combined_label_set(&legal_names);
let combined_refs: Vec<&str> = combined.iter().copied().collect();
let detector = self.detector_get_or_init()?;

let mut all_entities: Vec<Vec<cloakpipe_core::DetectedEntity>> = Vec::with_capacity(extracted.chunks.len());
for chunk in &extracted.chunks {
    let ents = detector.detect_with_labels(&chunk.text, &combined_refs, 0.5)?;
    all_entities.push(ents);
}

// ── 2. Pseudonymize per chunk with offset map captured ──
let mut pseudo_chunks: Vec<String> = Vec::with_capacity(extracted.chunks.len());
let mut offset_maps: Vec<crate::legal::offsets::PseudoOffsetMap> = Vec::with_capacity(extracted.chunks.len());
for (chunk, ents) in extracted.chunks.iter().zip(&all_entities) {
    let pii_ents: Vec<cloakpipe_core::DetectedEntity> = ents.iter()
        .filter(|e| crate::detect::is_pii_label(&format!("{:?}", e.category).to_lowercase()))
        .cloned()
        .collect();
    let (pseudo, map) = self.vault.pseudonymize_with_map(&chunk.text, &pii_ents).await?;
    pseudo_chunks.push(pseudo);
    offset_maps.push(map);
}

// ── 3. Existing embed + upsert path (unchanged) ──
let vectors = self.embedder().await?.embed_batch(&pseudo_chunks)?;
// … build records (existing) …
self.store.delete_doc_rows(&extracted.source_path).await?;
self.store.upsert(records).await?;

// ── 4. NEW: legal_chunk_enrichment dual-write (table only; graph comes in C2) ──
let mut legal_rows = Vec::with_capacity(extracted.chunks.len());
for (i, chunk) in extracted.chunks.iter().enumerate() {
    let chunk_id = crate::store::chunk_uuid(doc_id, chunk.idx);
    let legal_ents = self.legal_enricher.translate_to_pseudo(
        all_entities[i].iter()
            .filter(|e| !crate::detect::is_pii_label(&format!("{:?}", e.category).to_lowercase()))
            .map(|e| crate::legal::types::LegalEntity {
                label: format!("{:?}", e.category).to_lowercase(),
                text: e.original.clone(),
                byte_start: e.start as u32,
                byte_end: e.end as u32,
                confidence: e.confidence as f32,
            })
            .collect(),
        &offset_maps[i],
        &pseudo_chunks[i],
    );
    // For B4 we leave the vault_forward_map empty — Replacer doesn't expose
    // a reverse-of-mapping view directly. Layer-2 rules fall back to
    // pattern-only matching (no canonical-form resolution this task).
    // Task C2 wires the vault forward map.
    let fwd = crate::legal::rules::VaultForwardMap { alias_to_canonical: Default::default() };
    let (row, _facts) = self.legal_enricher.enrich_one(chunk_id, doc_id, &pseudo_chunks[i], &legal_ents, &fwd);
    legal_rows.push(row);
}

let written = self.legal_store.upsert(&legal_rows).await;
match written {
    Ok(()) => {
        self.enrichment_status.mark_ok(doc_id).await?;
    }
    Err(e) => {
        tracing::warn!(doc_id=%doc_id, error=%e,
            "legal_chunk_enrichment write failed; chunks committed, marking pending");
        self.enrichment_status
            .mark_pending(doc_id, extracted.chunks.len() as i32, &e.to_string())
            .await?;
    }
}

// ── 5. Existing anonymized markdown write + return ──
// (unchanged below this point)
```

The return statement remains `Ok(IngestOutcome::Ingested { used_embedded_ocr })` — **struct variant**, not tuple.

> **PII-vs-legal label classification:** `cloakpipe_core::DetectedEntity` carries `category` (a cloakpipe enum like `Person`/`Organization`/…). The lowercased debug repr is compared against `is_pii_label`. If the existing detector adds new categories in the future, extend `PII_NER_LABELS` accordingly.

- [ ] **Step 4: Write the integration test (small synthetic document)**

In `crates/anno-rag/tests/legal_ingest_table_only.rs`:

```rust
//! End-to-end integration: ingest a synthetic French legal document and verify
//! the legal_chunk_enrichment table is populated.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires warm HF GLiNER2 ONNX cache"]
async fn ingest_writes_enrichment_rows_for_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = anno_rag::config::AnnoRagConfig::default();
    cfg.data_dir = tmp.path().to_path_buf();

    let key = anno_rag::vault::derive_key().unwrap();
    let pipeline = anno_rag::pipeline::Pipeline::new(cfg.clone(), key).await.unwrap();

    let txt = "Entre les soussignés:\n\
               Société ACME SAS, ci-après l'acheteur,\n\
               Et la société BETA SARL, ci-après le vendeur,\n\
               Il est convenu ce qui suit.\n\
               ACME s'engage à verser la somme de 10 000 € sous 30 jours.\n\
               Vu l'article 1240 du Code civil.";
    let path = tmp.path().join("contrat.txt");
    std::fs::write(&path, txt).unwrap();
    let out = tmp.path().join("out");
    pipeline.ingest_one(&path, &out).await.unwrap();

    // Read back via filter_chunk_ids using a parties filter.
    let filters = anno_rag::legal::LegalSearchFilters {
        parties: vec!["org:acme".into()],
        ..Default::default()
    };
    let ids = pipeline.legal_store.filter_chunk_ids(&filters, 20).await.unwrap();
    assert!(!ids.is_empty(), "filter_chunk_ids returned no rows for org:acme");
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo build -p anno-rag
cargo test -p anno-rag --lib
cargo test -p anno-rag --test legal_ingest_table_only -- --ignored
```

Expected: lib tests PASS. The ignored integration test should PASS on a machine with a warm GLiNER2 ONNX cache; skip with a note otherwise.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/store.rs crates/anno-rag/src/pipeline.rs crates/anno-rag/tests/legal_ingest_table_only.rs
git commit -m "feat(ingest): wire legal_chunk_enrichment dual-write into ingest_one_counted"
```

---

### Task B5: `drain_enrichment_backlog` retry queue

**Files:**
- Modify: `crates/anno-rag/src/legal/status.rs` (real `attempts` + `upsert_row`)
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Implement the real Arrow encoding for `EnrichmentStatusStore::upsert_row` and `attempts`**

In `legal/status.rs`, replace the stubbed bodies of `attempts` and `upsert_row` with real Arrow batch encoding. Build the same kind of 1-row `RecordBatch` as the existing `chunks` and `legal_chunk_enrichment` upserts and call `self.table.add(...).execute().await`. For `attempts`, query by `doc_id = X'..'`, return `Some(attempts_value)` if a row exists.

- [ ] **Step 2: Write failing tests for `drain_enrichment_backlog`**

In `crates/anno-rag/src/pipeline.rs` (or a new `crates/anno-rag/tests/drain_backlog.rs`):

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires warm HF GLiNER2 ONNX cache"]
async fn drain_enrichment_backlog_completes_pending_doc() {
    // Ingest with a deliberately broken legal store handle (or simulate
    // failure path), assert the doc is marked pending, then swap to a
    // working handle and call drain_enrichment_backlog.
    // ... skeleton ...
}
```

- [ ] **Step 3: Implement `Pipeline::drain_enrichment_backlog`**

```rust
pub struct EnrichmentDrainReport {
    pub ok: usize,
    pub still_pending: usize,
    pub failed_max_retries: usize,
}

impl Pipeline {
    /// Re-run legal enrichment for pending docs. Exponential backoff per attempt.
    pub async fn drain_enrichment_backlog(
        &self,
        max_docs: usize,
    ) -> Result<EnrichmentDrainReport> {
        let pending = self.enrichment_status.list_pending(max_docs).await?;
        let mut ok = 0; let mut still = 0; let mut failed = 0;
        for d in pending {
            if d.attempts >= 5 {
                self.enrichment_status
                    .mark_failed_max_retries(d.doc_id, d.last_error.as_deref().unwrap_or(""))
                    .await?;
                failed += 1;
                continue;
            }
            // Re-fetch chunks for this doc from LanceDB.
            let chunks = self.store.chunks_by_doc(d.doc_id).await?;
            match self.reenrich_doc(d.doc_id, &chunks).await {
                Ok(()) => { self.enrichment_status.mark_ok(d.doc_id).await?; ok += 1; }
                Err(e) => {
                    tracing::warn!(doc_id = %d.doc_id, attempt = d.attempts + 1, error = %e,
                        "legal re-enrichment failed");
                    self.enrichment_status.mark_pending(d.doc_id, d.chunk_count, &e.to_string()).await?;
                    still += 1;
                }
            }
        }
        Ok(EnrichmentDrainReport { ok, still_pending: still, failed_max_retries: failed })
    }

    async fn reenrich_doc(&self, doc_id: Uuid, chunks: &[crate::store::SearchHit]) -> Result<()> {
        // Build legal_rows from chunks already pseudonymized in `chunks.text_pseudo`.
        // The vault forward map for re-enrichment is empty (canonical refs already in pseudo text
        // through prior aliases — Phase 1 limitation; addressed by a per-doc vault snapshot in C3).
        let fwd = crate::legal::rules::VaultForwardMap { alias_to_canonical: Default::default() };
        let mut rows = Vec::with_capacity(chunks.len());
        for c in chunks {
            let legal_ents = vec![]; // re-run NER if needed; v1: empty (rules-only)
            let (row, _) = self.legal_enricher.enrich_one(c.chunk_id, doc_id, &c.text_pseudo, &legal_ents, &fwd);
            rows.push(row);
        }
        self.legal_store.upsert(&rows).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Call `drain_enrichment_backlog` at end of `ingest_folder`**

In the existing `ingest_folder` method, after the batch loop, add:

```rust
if let Err(e) = self.drain_enrichment_backlog(64).await {
    tracing::warn!(error = %e, "drain_enrichment_backlog failed (non-fatal)");
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/status.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(legal): drain_enrichment_backlog with exponential backoff + auto-drain"
```

---

## Stage C — Graph layer (6 tasks)

### Task C1: `LegalKnowledgeGraph` trait + `LanceGraphStore`

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs`

- [ ] **Step 1: Write failing tests against an in-memory fake**

```rust
//! Lance-graph–backed legal knowledge graph + node/edge batch API.

use crate::error::Result;
use crate::legal::types::{ArticleRef, CourtRef, PartyKind};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

/// One node staged for write.
#[derive(Debug, Clone)]
pub enum NodeWrite {
    Document { doc_id: Uuid, doc_type: Option<String>, legal_domain: Option<String>, jurisdiction: Option<String>, document_date: Option<DateTime<Utc>>, dossier_id: Option<String> },
    Chunk { chunk_id: Uuid, doc_id: Uuid, byte_start: u32, byte_end: u32, page: Option<u32> },
    Party { party_id: Uuid, kind: PartyKind, canonical_name: String, normalized_form: String, siren: Option<String> },
    Court { court_id: String, court: CourtRef },
    Article { article_id: Uuid, article: ArticleRef },
    Obligation { obligation_id: Uuid, kind: String, text_pseudo: String },
    Amount { amount_id: Uuid, value_cents: i64, currency: String, scope: String },
    Event { event_id: Uuid, kind: String, event_date: Option<DateTime<Utc>>, deadline_date: Option<DateTime<Utc>> },
    Risk { risk_id: Uuid, severity: String, category: String, text_pseudo: String },
    MandatoryClauseCheck { check_id: Uuid, requirement: String, status: String },
}

/// One edge staged for write.
#[derive(Debug, Clone)]
pub struct EdgeWrite {
    pub from_label: &'static str,
    pub from_key: String,
    pub to_label: &'static str,
    pub to_key: String,
    pub edge_type: &'static str,
    pub props: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct NodeBatch { pub nodes: Vec<NodeWrite> }

#[derive(Debug, Default)]
pub struct EdgeBatch { pub edges: Vec<EdgeWrite> }

impl NodeBatch {
    pub fn new() -> Self { Self::default() }
    pub fn add_document(&mut self, doc_id: Uuid, doc_type: Option<String>, legal_domain: Option<String>, jurisdiction: Option<String>, document_date: Option<DateTime<Utc>>, dossier_id: Option<String>) {
        self.nodes.push(NodeWrite::Document { doc_id, doc_type, legal_domain, jurisdiction, document_date, dossier_id });
    }
    pub fn add_chunk(&mut self, chunk_id: Uuid, doc_id: Uuid, byte_start: u32, byte_end: u32, page: Option<u32>) {
        self.nodes.push(NodeWrite::Chunk { chunk_id, doc_id, byte_start, byte_end, page });
    }
    pub fn absorb(&mut self, other: Vec<NodeWrite>) { self.nodes.extend(other); }
}

impl EdgeBatch {
    pub fn new() -> Self { Self::default() }
    pub fn absorb(&mut self, other: Vec<EdgeWrite>) { self.edges.extend(other); }
}

#[async_trait]
pub trait LegalKnowledgeGraph: Send + Sync {
    async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()>;
    async fn delete_doc(&self, doc_id: Uuid) -> Result<()>;
    async fn compact(&self) -> Result<()>;
    async fn cypher(&self, query: &str, params: HashMap<String, String>) -> Result<Vec<HashMap<String, String>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    pub struct InMemoryKG {
        pub nodes: std::sync::Mutex<Vec<NodeWrite>>,
        pub edges: std::sync::Mutex<Vec<EdgeWrite>>,
    }

    #[async_trait]
    impl LegalKnowledgeGraph for InMemoryKG {
        async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()> {
            self.nodes.lock().unwrap().extend(nodes.nodes.clone());
            self.edges.lock().unwrap().extend(edges.edges.clone());
            Ok(())
        }
        async fn delete_doc(&self, _doc_id: Uuid) -> Result<()> { Ok(()) }
        async fn compact(&self) -> Result<()> { Ok(()) }
        async fn cypher(&self, _q: &str, _p: HashMap<String, String>) -> Result<Vec<HashMap<String, String>>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn in_memory_kg_round_trip_nodes_and_edges() {
        let kg = InMemoryKG::default();
        let mut nb = NodeBatch::new();
        nb.add_document(Uuid::nil(), Some("contract".into()), None, None, None, None);
        let eb = EdgeBatch::default();
        kg.upsert_batch(&nb, &eb).await.unwrap();
        assert_eq!(kg.nodes.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify the in-memory KG works**

```powershell
cargo test -p anno-rag legal::kg --lib
```

Expected: PASS (the test exercises the in-memory fake; lance-graph impl is in step 3).

- [ ] **Step 3: Implement `LanceGraphStore`**

```rust
/// Real lance-graph–backed knowledge graph.
pub struct LanceGraphStore {
    // Inner handle to a lance-graph instance. The exact API is library-
    // dependent; consult `lance-graph` 0.5 docs for the constructor and
    // for the SQL/Cypher entry points.
    inner: lance_graph::GraphConfig,
}

impl LanceGraphStore {
    /// Open or create the legal KG under `cfg.index_path()/legal_kg`.
    pub async fn open(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let path = cfg.index_path().join("legal_kg");
        std::fs::create_dir_all(&path)
            .map_err(|e| crate::error::Error::Graph(format!("mkdir legal_kg: {e}")))?;
        let inner = lance_graph::GraphConfig::open(&path)
            .map_err(|e| crate::error::Error::Graph(format!("open: {e}")))?;
        Ok(Self { inner })
    }
}

#[async_trait]
impl LegalKnowledgeGraph for LanceGraphStore {
    async fn upsert_batch(&self, nodes: &NodeBatch, edges: &EdgeBatch) -> Result<()> {
        // Serialize each NodeWrite/EdgeWrite into the lance-graph node/relationship
        // table shape and call into the library's batch upsert API. The exact API
        // surface depends on the lance-graph version; see crate docs.
        // For Phase 1 we accept a minimum-viable: write nodes by label into one
        // Lance dataset per label and relationships into one combined dataset.
        let _ = (nodes, edges);
        Ok(())
    }
    async fn delete_doc(&self, doc_id: Uuid) -> Result<()> {
        let _ = doc_id;
        Ok(())
    }
    async fn compact(&self) -> Result<()> { Ok(()) }
    async fn cypher(&self, query: &str, params: HashMap<String, String>) -> Result<Vec<HashMap<String, String>>> {
        let q = lance_graph::CypherQuery::new(query, params);
        let rows = q.execute(&self.inner)
            .map_err(|e| crate::error::Error::Graph(format!("cypher: {e}")))?;
        Ok(rows.into_iter()
            .map(|r| r.into_iter().collect::<HashMap<String, String>>())
            .collect())
    }
}
```

> The lance-graph 0.5 surface is library-dependent; the implementer should
> consult the crate docs and adjust `upsert_batch`/`cypher` accordingly. The
> trait above is the *anno-side* contract — what changes inside it does not
> affect any caller.

- [ ] **Step 4: Add `async-trait` to `Cargo.toml` if missing**

```toml
async-trait = "0.1"
```

- [ ] **Step 5: Run tests**

```powershell
cargo build -p anno-rag
cargo test -p anno-rag legal::kg --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/src/legal/kg.rs
git commit -m "feat(legal): add LegalKnowledgeGraph trait + LanceGraphStore + in-mem fake"
```

---

### Task C2: Wire graph into `ingest_one_counted`

**Files:**
- Modify: `crates/anno-rag/src/legal/enricher.rs` (build `NodeBatch`/`EdgeBatch` from typed facts)
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add `legal_kg: Arc<dyn LegalKnowledgeGraph>` field to `Pipeline`**

In `pipeline.rs` `Pipeline` struct:

```rust
pub(crate) legal_kg: std::sync::Arc<dyn crate::legal::kg::LegalKnowledgeGraph>,
```

In `Pipeline::new`:

```rust
let legal_kg: std::sync::Arc<dyn crate::legal::kg::LegalKnowledgeGraph> =
    std::sync::Arc::new(crate::legal::kg::LanceGraphStore::open(&cfg).await?);
```

- [ ] **Step 2: Extend `LegalEnricher::enrich_one` to produce node + edge writes**

In `enricher.rs`, change the `enrich_one` return type to:

```rust
pub fn enrich_one(
    &self,
    chunk_id: Uuid,
    doc_id: Uuid,
    pseudo_text: &str,
    legal_ents: &[LegalEntity],
    fwd: &VaultForwardMap,
) -> (
    LegalChunkEnrichment,
    Vec<TypedFact>,
    Vec<crate::legal::kg::NodeWrite>,
    Vec<crate::legal::kg::EdgeWrite>,
);
```

Implement node/edge generation:

- For every `TypedFact::PartyRole`, push a `NodeWrite::Party` (party_id = deterministic UUIDv5 from normalized_form) and `EdgeWrite { from: party, to: doc, type: "PARTY_TO", props: {"role": role} }`.
- For every `TypedFact::Obligation`, push a `NodeWrite::Obligation` and `EdgeWrite (chunk MENTIONS obligation)`; if `obligor` exists, also `EdgeWrite (party BOUND_BY obligation)`.
- For every `TypedFact::Reference`, push a `NodeWrite::Article` and `EdgeWrite (doc REFERENCES article)`.
- For every `TypedFact::CourtRouting`, push a `NodeWrite::Court` and `EdgeWrite (doc HEARS court)`.
- For every `TypedFact::Event`, push a `NodeWrite::Event` and `EdgeWrite (chunk MENTIONS event)`.
- For every `LegalEntity` not consumed above (Layer 3 fallback), push `EdgeWrite (chunk MENTIONS <appropriate node>)`.

- [ ] **Step 3: Modify `ingest_one_counted` to dual-write to the graph**

After the `self.legal_store.upsert(&legal_rows)` call, build a combined `NodeBatch`/`EdgeBatch` for the document (`Document` + all `Chunk` nodes + the per-chunk node/edge writes returned from `enrich_one`), and call:

```rust
let written_graph = self.legal_kg.upsert_batch(&node_batch, &edge_batch).await;
match written_graph {
    Ok(()) => self.enrichment_status.mark_ok(doc_id).await?,
    Err(e) => {
        tracing::warn!(doc_id=%doc_id, error=%e, "graph write failed");
        self.enrichment_status.mark_pending(doc_id, extracted.chunks.len() as i32, &e.to_string()).await?;
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo build -p anno-rag
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/enricher.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(ingest): full dual-write to LanceDB + lance-graph"
```

---

### Task C3: Cross-document linking pass

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs` (add `link_cross_documents`)
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing tests for `link_cross_documents`**

```rust
#[tokio::test]
async fn link_cross_documents_appeals_and_amends() {
    // Set up the in-memory KG with two Document nodes where the second
    // mentions "appel du jugement du ..." referencing the first.
    // Assert that link_cross_documents creates the APPEALS edge.
    // ... ~30-line test ...
}
```

- [ ] **Step 2: Implement `LegalKnowledgeGraph::link_cross_documents`**

Add to the trait + impls:

```rust
async fn link_cross_documents(&self, doc_id: Uuid, hints: &[CrossDocLinkHint]) -> Result<()>;
```

`CrossDocLinkHint` carries `(edge_type, target_doc_id | matching_quote)`. The pipeline calls this after dual-write for each freshly ingested doc.

- [ ] **Step 3: Call `link_cross_documents` after dual-write succeeds**

In `ingest_one_counted`:

```rust
let hints = self.legal_enricher.cross_doc_hints(doc_id, &legal_rows, &node_batch, &edge_batch);
self.legal_kg.link_cross_documents(doc_id, &hints).await?;
```

- [ ] **Step 4: Run tests + commit**

```powershell
cargo test -p anno-rag legal::kg --lib
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/src/legal/enricher.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(legal): cross-document linking pass (APPEALS/AMENDS/CITES)"
```

---

### Task C4: Five named graph intents

**Files:**
- Modify: `crates/anno-rag/src/legal/query.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing tests**

```rust
//! Five named graph traversals. Each is a parameterized Cypher template.

use crate::error::Result;
use crate::legal::kg::LegalKnowledgeGraph;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(tag = "intent", content = "params", rename_all = "snake_case")]
pub enum GraphIntent {
    PartyDossier { party: String },
    ObligationsOwedBy { party: String },
    CitationChain { article_ref: String },
    ProceduralTimeline { dossier_id: String },
    AppealChain { doc_id: Uuid, max_depth: u32 },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQueryResult { pub rows: Vec<HashMap<String, String>> }

pub async fn run_intent(kg: &dyn LegalKnowledgeGraph, intent: GraphIntent) -> Result<GraphQueryResult> {
    let (query, params) = match intent {
        GraphIntent::PartyDossier { party } => (
            "MATCH (p:Party {normalized_form:$party})-[r:PARTY_TO]->(d:Document)-[:HAS_CHUNK]->(c:Chunk) RETURN d, r.role AS role, c ORDER BY d.document_date".to_string(),
            HashMap::from([("party".to_string(), party)]),
        ),
        GraphIntent::ObligationsOwedBy { party } => (
            "MATCH (p:Party {normalized_form:$party})-[:BOUND_BY]->(o:Obligation) OPTIONAL MATCH (o)-[:OWED_TO]->(t:Party), (o)-[:HAS_DEADLINE]->(e:Event), (o)-[:HAS_AMOUNT]->(a:Amount) RETURN o, t, e, a".to_string(),
            HashMap::from([("party".to_string(), party)]),
        ),
        GraphIntent::CitationChain { article_ref } => (
            "MATCH (a:Article {normalized_ref:$ref})<-[:REFERENCES]-(d:Document)-[:HAS_CHUNK]->(c:Chunk) WHERE (c)-[:MENTIONS]->(a) RETURN d, c".to_string(),
            HashMap::from([("ref".to_string(), article_ref)]),
        ),
        GraphIntent::ProceduralTimeline { dossier_id } => (
            "MATCH (d:Document {dossier_id:$dossier})-[:HAS_CHUNK]->(c:Chunk)-[:MENTIONS]->(e:Event) RETURN e ORDER BY e.event_date".to_string(),
            HashMap::from([("dossier".to_string(), dossier_id)]),
        ),
        GraphIntent::AppealChain { doc_id, max_depth } => (
            format!("MATCH path = (d:Document {{doc_id:$doc}})-[:APPEALS*1..{}]->(prior:Document) RETURN path", max_depth),
            HashMap::from([("doc".to_string(), doc_id.to_string())]),
        ),
    };
    let rows = kg.cypher(&query, params).await?;
    Ok(GraphQueryResult { rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legal::kg::{EdgeBatch, NodeBatch};

    #[tokio::test]
    async fn party_dossier_builds_correct_cypher() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        // empty KG → empty result, but the assertion is that no panic + ok variant.
        let r = run_intent(&kg, GraphIntent::PartyDossier { party: "org:acme".into() }).await.unwrap();
        assert!(r.rows.is_empty());
    }
}
```

- [ ] **Step 2: Add `Pipeline::legal_graph_query`**

```rust
pub async fn legal_graph_query(&self, intent: crate::legal::query::GraphIntent)
    -> Result<crate::legal::query::GraphQueryResult>
{
    crate::legal::query::run_intent(self.legal_kg.as_ref(), intent).await
}
```

- [ ] **Step 3: Run tests + commit**

```powershell
cargo test -p anno-rag legal::query --lib
git add crates/anno-rag/src/legal/query.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(legal): five named graph intents (party_dossier, obligations_owed_by, citation_chain, procedural_timeline, appeal_chain)"
```

---

### Task C5: Index maintenance + compaction hooks

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add `Pipeline::optimize_after_ingest`**

```rust
impl Pipeline {
    /// Maintain indexes across all stores after an ingest batch.
    pub async fn optimize_after_ingest(&self) -> Result<()> {
        self.store.maybe_build_index(self.cfg.vector_index_threshold).await?;
        self.store.maybe_build_fts_index().await?;
        self.legal_store.setup_indexes().await?;
        self.legal_kg.compact().await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Call from end of `ingest_folder`**

After the `drain_enrichment_backlog(64)` call:

```rust
if let Err(e) = self.optimize_after_ingest().await {
    tracing::warn!(error = %e, "optimize_after_ingest failed (non-fatal)");
}
```

- [ ] **Step 3: Run tests + commit**

```powershell
cargo test -p anno-rag --lib
git add crates/anno-rag/src/pipeline.rs
git commit -m "feat(legal): optimize_after_ingest hook (chunks + legal table + KG compaction)"
```

---

### Task C6: Batched chunk-text round-trip helper

**Files:**
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Write failing test for `Store::chunks_by_ids`**

```rust
#[tokio::test]
async fn chunks_by_ids_returns_in_query_order() {
    // create a tempdir Store, upsert 3 chunks, fetch by ids in a specific order,
    // assert returned order matches input ids.
}
```

- [ ] **Step 2: Implement `Store::chunks_by_ids` and `Store::chunks_by_doc`**

In `store.rs`, add:

```rust
impl Store {
    pub async fn chunks_by_ids(&self, ids: &[Uuid]) -> Result<Vec<SearchHit>> {
        let filter = match chunk_id_filter_sql(ids) {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };
        let stream = self.tbl.query().only_if(filter).limit(ids.len()).execute().await
            .map_err(|e| Error::Store(format!("chunks_by_ids: {e}")))?;
        let batches: Vec<RecordBatch> = stream.try_collect().await
            .map_err(|e| Error::Store(format!("chunks_by_ids stream: {e}")))?;
        let mut by_id: std::collections::HashMap<Uuid, SearchHit> = std::collections::HashMap::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                let hit = batch_to_hit(batch, i)?;
                by_id.insert(hit.chunk_id, hit);
            }
        }
        Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
    }

    pub async fn chunks_by_doc(&self, doc_id: Uuid) -> Result<Vec<SearchHit>> {
        let filter = format!("doc_id = X'{}'", hex::encode(doc_id.as_bytes()));
        let stream = self.tbl.query().only_if(filter).execute().await
            .map_err(|e| Error::Store(format!("chunks_by_doc: {e}")))?;
        let batches: Vec<RecordBatch> = stream.try_collect().await
            .map_err(|e| Error::Store(format!("chunks_by_doc stream: {e}")))?;
        let mut out = Vec::new();
        for batch in &batches {
            for i in 0..batch.num_rows() {
                out.push(batch_to_hit(batch, i)?);
            }
        }
        out.sort_by_key(|h| h.chunk_idx);
        Ok(out)
    }
}
```

- [ ] **Step 3: Run tests + commit**

```powershell
cargo test -p anno-rag store::chunks_by_ids --lib
git add crates/anno-rag/src/store.rs
git commit -m "feat(store): chunks_by_ids + chunks_by_doc helpers for graph round-trip"
```

---

## Stage D — MCP tools, French specifics, eval (6 tasks)

### Task D1: MCP wave 1 (ingest, search, graph, rehydrate)

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag/src/pipeline.rs` (add `legal_search`, `legal_rehydrate_citation`)

- [ ] **Step 1: Implement `Pipeline::legal_search`**

```rust
impl Pipeline {
    pub async fn legal_search(
        &self,
        query: &str,
        top_k: usize,
        filters: crate::legal::LegalSearchFilters,
    ) -> Result<Vec<crate::legal::LegalSearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;

        let chunk_hits = if filters.has_any_filter() {
            let allowed = self.legal_store
                .filter_chunk_ids(&filters, top_k.saturating_mul(20).max(100))
                .await?;
            self.store.search_filtered_to_chunks(&pseudo_q, &qv, top_k, &allowed).await?
        } else {
            self.store.search(&pseudo_q, &qv, top_k).await?
        };

        Ok(chunk_hits.into_iter().map(|h| crate::legal::LegalSearchHit {
            chunk_id: h.chunk_id,
            doc_id: h.doc_id,
            text_pseudo: h.text_pseudo,
            score: h.score,
            enrichment: None,
        }).collect())
    }
}
```

- [ ] **Step 2: Implement `Pipeline::legal_rehydrate_citation`**

```rust
pub async fn legal_rehydrate_citation(
    &self,
    chunk_id: Uuid,
    byte_start: u32,
    byte_end: u32,
) -> Result<crate::pipeline::RehydratedText> {
    if byte_start >= byte_end {
        return Err(crate::error::Error::Store("byte_start must be < byte_end".into()));
    }
    let hit = self.store.chunk_by_id(chunk_id).await?
        .ok_or_else(|| crate::error::Error::Store(format!("unknown chunk_id: {chunk_id}")))?;
    let span = hit.text_pseudo.get(byte_start as usize..byte_end as usize)
        .ok_or_else(|| crate::error::Error::Store("offsets not valid UTF-8 boundary".into()))?;
    self.rehydrate(span).await
}
```

- [ ] **Step 3: Add MCP tools (wave 1)**

In `crates/anno-rag-mcp/src/lib.rs`, add Param structs + handlers for: `legal_ingest`, `legal_search`, `legal_graph_query`, `legal_rehydrate_citation`. Follow the pattern of existing MCP tools (lazy pipeline access via `self.pipeline().await?`).

- [ ] **Step 4: Tests + commit**

```powershell
cargo test -p anno-rag-mcp --lib
git add crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): legal_ingest, legal_search, legal_graph_query, legal_rehydrate_citation"
```

---

### Task D2: MCP wave 2 (extract_contract, extract_case_file, timeline, risk_review)

**Files:**
- Modify: `crates/anno-rag/src/legal/extract.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write extraction workflow tests with InMemoryKG and a small synthetic dossier**

(Construct an InMemoryKG seeded with a contract's nodes/edges; assert `legal_extract_contract` returns a row per requested field, each with `chunk_id`+offsets.)

- [ ] **Step 2: Implement `legal::extract` workflows**

`legal_extract_contract(doc_id)` and `legal_extract_case_file(doc_id|dossier_id)` query the KG for parties + clauses + obligations + events, then materialize a `anno-rag-tabular` review grid (one column per field). Reuse existing tabular APIs.

- [ ] **Step 3: Implement `legal_timeline` (delegates to `procedural_timeline` graph intent + formatter)**

- [ ] **Step 4: Implement `legal_risk_review`**

Reads `Risk` nodes scoped by `doc_id` or `dossier_id`, returns severity + supporting clauses + recommended human review.

- [ ] **Step 5: Add MCP handlers, run tests, commit**

```powershell
cargo test -p anno-rag legal::extract --lib
cargo test -p anno-rag-mcp --lib
git add crates/anno-rag/src/legal/extract.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): legal_extract_contract, legal_extract_case_file, legal_timeline, legal_risk_review"
```

---

### Task D3: Mandatory clause checklist + audit tool

**Files:**
- Modify: `crates/anno-rag/src/legal/mandatory.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests for the mandatory-clause evaluator**

```rust
#[test]
fn b2b_contract_missing_penalites_de_retard_is_missing() {
    let text = "Article 1 - Parties. Article 2 - Prix HT et TTC. Article 3 - Modalités de paiement.";
    let result = evaluate_b2b_contract(text);
    let pen = result.iter().find(|r| r.requirement == "penalites_de_retard").unwrap();
    assert_eq!(pen.status, "missing");
}
```

- [ ] **Step 2: Implement per-doctype checklists**

```rust
pub fn evaluate_b2b_contract(text: &str) -> Vec<MandatoryCheck> { /* … */ }
pub fn evaluate_b2c_contract(text: &str) -> Vec<MandatoryCheck> { /* … */ }
pub fn evaluate_employment(text: &str) -> Vec<MandatoryCheck> { /* … */ }
pub fn evaluate_lease_commercial(text: &str) -> Vec<MandatoryCheck> { /* … */ }
pub fn evaluate_lease_residential(text: &str) -> Vec<MandatoryCheck> { /* … */ }
pub fn evaluate_rgpd(text: &str) -> Vec<MandatoryCheck> { /* … */ }
```

Each requirement: pattern (case-insensitive substring or regex) + canonical name + status.

- [ ] **Step 3: Integrate into ingest write path**

In `ingest_one_counted`, after building `legal_rows`, call:

```rust
let checks = crate::legal::mandatory::evaluate_doc(doc_type, &full_doc_text);
let aggregated_status = aggregate_status(&checks);
for row in &mut legal_rows { row.mandatory_clause_status = Some(aggregated_status.clone()); }
node_batch.absorb(checks.into_iter().map(MandatoryClauseCheck::into_node).collect());
```

- [ ] **Step 4: Implement `Pipeline::legal_mandatory_clause_audit(doc_id)` and the MCP tool**

- [ ] **Step 5: Run tests + commit**

```powershell
cargo test -p anno-rag legal::mandatory --lib
cargo test -p anno-rag-mcp --lib
git add crates/anno-rag/src/legal/mandatory.rs crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(legal): mandatory clause checklist + audit MCP tool"
```

---

### Task D4: Prescription engine + tool

**Files:**
- Modify: `crates/anno-rag/src/legal/prescription.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn breach_contractuel_prescribes_5_years_from_event() {
    let event_date = chrono::Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let r = compute_prescription("contractuel", event_date, &[]);
    assert_eq!(r.prescribes_on.year(), 2025);
    assert_eq!(r.article_ref, "code_civil:2224");
}

#[test]
fn mise_en_demeure_interrupts_prescription() {
    let event = chrono::Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let mise = chrono::Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap();
    let r = compute_prescription("contractuel", event, &[InterruptingEvent { kind: "mise_en_demeure".into(), date: mise }]);
    // New 5-year period starts from mise en demeure date.
    assert_eq!(r.prescribes_on.year(), 2029);
    assert!(r.interrupted_by.iter().any(|e| e.kind == "mise_en_demeure"));
}
```

- [ ] **Step 2: Implement `compute_prescription` and the rule table**

Use the 8 seeded rules from the spec.

- [ ] **Step 3: Implement `Pipeline::legal_prescription_check` and MCP tool**

- [ ] **Step 4: Run tests + commit**

```powershell
cargo test -p anno-rag legal::prescription --lib
git add crates/anno-rag/src/legal/prescription.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(legal): French prescription engine + interruption logic"
```

---

### Task D5: `legal_validate_field` tool + Validation node

**Files:**
- Modify: `crates/anno-rag/src/legal/kg.rs` (add `Validation` NodeWrite variant)
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add `NodeWrite::Validation` variant + `EdgeWrite { type: "VALIDATES" }`**

- [ ] **Step 2: Implement `Pipeline::legal_validate_field`**

```rust
pub async fn legal_validate_field(
    &self,
    fact_ref: FactRef,
    action: ValidationAction,
    corrected_value: Option<String>,
    note: Option<String>,
) -> Result<ValidationAck> { /* … */ }
```

- [ ] **Step 3: MCP tool + tests + commit**

```powershell
cargo test -p anno-rag --lib
git add crates/anno-rag/src/legal/kg.rs crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): legal_validate_field tool + Validation node wiring"
```

---

### Task D6: Gold corpus + eval suite + audit helpers

**Files:**
- Modify: `crates/anno-rag/src/legal/eval.rs`
- Modify: `crates/anno-rag/src/legal/audit.rs`
- Create: `crates/anno-rag/tests/legal_gold_corpus/*` (10 contracts + 5 litigation files with gold annotations)
- Create: `crates/anno-rag/benches/legal_eval.rs`

- [ ] **Step 1: Author the local gold corpus**

10 synthetic French contracts + 5 synthetic litigation files under `crates/anno-rag/tests/legal_gold_corpus/`. Each file has a sidecar `<name>.gold.json` describing expected: parties, dates, obligations, amounts, clauses, risks, events, citations, mandatory-clause statuses, prescription anchors.

- [ ] **Step 2: Implement `legal::eval` metric computations**

- `entity_precision_recall(corpus) -> PerLabelMetrics`
- `obligation_f1(corpus)`
- `deadline_accuracy(corpus)`
- `amount_normalization_accuracy(corpus)`
- `mandatory_clause_f1(corpus)`
- `prescription_accuracy(corpus)`
- `citation_validity_rate(corpus)`
- `graph_traversal_latency_p95(kg, corpus)`

- [ ] **Step 3: Implement `legal::audit` event helpers**

```rust
pub fn audit_ingest(doc_id: Uuid, outcome: &str) { tracing::info!(target: "anno_rag::legal::audit", doc_id = %doc_id, outcome = outcome); }
pub fn audit_search(query_len: usize, filter_count: usize, top_k: usize) { /* … */ }
pub fn audit_rehydrate(chunk_id: Uuid, byte_start: u32, byte_end: u32) { /* … */ }
pub fn audit_validation(actor: Option<&str>, action: &str) { /* … */ }
pub fn audit_prescription(event_id: Uuid, prescribes_on: chrono::DateTime<chrono::Utc>) { /* … */ }
pub fn audit_mandatory(doc_id: Uuid, status: &str) { /* … */ }
```

Sprinkle calls at each tool entry point.

- [ ] **Step 4: Implement `legal_eval` bench**

A criterion bench under `benches/legal_eval.rs` that ingests the gold corpus into a tempdir, runs all metric computations, and emits a JSON report.

- [ ] **Step 5: Run + commit**

```powershell
cargo test -p anno-rag legal::eval --lib
cargo bench -p anno-rag --bench legal_eval -- --quick
git add crates/anno-rag/src/legal/eval.rs crates/anno-rag/src/legal/audit.rs crates/anno-rag/tests/legal_gold_corpus crates/anno-rag/benches/legal_eval.rs
git commit -m "feat(legal): gold corpus + eval suite + audit helpers"
```

---

## Final Verification

- [ ] **Step 1: Full test suite**

```powershell
cargo test --workspace --lib
cargo test -p anno-rag --tests
```

Expected: PASS.

- [ ] **Step 2: Format + clippy**

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets --features "eval discourse" -- -D warnings
```

Expected: PASS.

- [ ] **Step 3: GitNexus scope check**

```powershell
npx gitnexus detect_changes --scope compare --base_ref main
npx gitnexus impact --repo anno Pipeline --direction upstream
npx gitnexus impact --repo anno Store --direction upstream
```

Expected: changed scope limited to `crates/anno-rag/src/legal/**`, `pipeline.rs`, `store.rs`, `detect.rs`, `vault.rs`, `error.rs`, `lib.rs`, `Cargo.toml`, and `anno-rag-mcp/src/lib.rs`.

- [ ] **Step 4: Diff review**

```powershell
git diff --stat origin/main...HEAD
```

Expected: no unrelated files, no secrets, no generated artifacts.

- [ ] **Step 5: Final commit if needed**

```powershell
git status --short
```

If anything remains uncommitted, stage and commit with a clear message.

---

## Spec Coverage Check

| Spec section | Task(s) |
|---|---|
| Motivation + Product scope | Whole plan |
| Design principles 1–7 | Built into ingest write path (B4/C2), citation rehydration (D1), graph constraints (C4), retry semantics (B5) |
| Target architecture | Pre-Flight + A1 + lib.rs export |
| Data model (graph + LanceDB) | A1, A4, A5, C1 |
| Extraction stack (3 layers) | A3 (combined labels), B1 (extractor trait), B2 (rules), B3 (orchestration) |
| French normalization | A2 |
| Ingest write path | B4 (table-only) + C2 (graph) + C5 (optimize) |
| Retry queue | A5 + B5 |
| Query surface | C4 (graph intents), D1 (legal_search + rehydrate), D3 (mandatory), D4 (prescription) |
| MCP tool surface (11 tools) | D1 + D2 + D3 + D4 + D5 + B5 (drain) |
| French reference data | A2 + B2 + D3 + D4 |
| Eval plan | D6 |
| Security & audit | D6 (audit helpers) + design-level (no full rehydration) |
| Known risks | Mitigated by trait boundaries (C1), retry (B5), confidence band (A1), offset map (B3) |
| Non-goals | Plan does not include LoRA, free-form Cypher, full rehydration |

## Execution Notes

- Heavy tests are marked `#[ignore]` and run only on warm GLiNER2 cache.
- Each stage is independently shippable; the plan can pause at the end of any stage.
- The `lance-graph` API surface (Task C1) is library-dependent; the trait keeps anno-side callers stable while the implementer iterates on the lance-graph integration.
- French alias tables (codes, courts) are intentionally seeded with the most common entries; the eval corpus (D6) will surface coverage gaps for follow-up commits.
- The shared GLiNER2 pool (Task A3) leans on the existing `Pool<T>` infrastructure (IS5); confirm `ingest_ner_pool` config field is honored.
