# Anno Legal RAG GLiNER2 LanceDB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first production slice of the legal RAG architecture: legal enrichment metadata, LanceDB-filtered legal search, citation-scoped rehydration, and Claude Desktop MCP tools.

**Architecture:** Keep `Detector` dedicated to PII and introduce a separate `LegalEnricher` domain layer. Store Phase 1 metadata in a `legal_chunk_enrichment` LanceDB table keyed by `chunk_id`, then use those rows to build chunk filters for the existing hybrid `chunks` search. Reuse the existing citation verifier and tabular-review storage instead of inventing a second validation grid.

**Tech Stack:** Rust, `anno-rag`, `anno-rag-mcp`, LanceDB 0.29-style query/index APIs, Arrow schemas/builders, GLiNER2 Fastino, rmcp, Tokio tests.

---

## File Structure

- Create `crates/anno-rag/src/legal/mod.rs`  
  Legal public module, re-exports, label catalog.
- Create `crates/anno-rag/src/legal/types.rs`  
  Domain structs: `LegalEntity`, `LegalChunkEnrichment`, `LegalSearchFilters`, `LegalSearchHit`.
- Create `crates/anno-rag/src/legal/enricher.rs`  
  `LegalEntityExtractor` trait, `LegalEnricher`, GLiNER2 adapter, model-free tests with fake extractor.
- Create `crates/anno-rag/src/legal/store.rs`  
  LanceDB `legal_chunk_enrichment` schema, upsert, filter, scalar-index setup.
- Modify `crates/anno-rag/src/lib.rs`  
  Export `pub mod legal`.
- Modify `crates/anno-rag/src/store.rs`  
  Add chunk-filtered hybrid search helper and UUID SQL literal helper.
- Modify `crates/anno-rag/src/pipeline.rs`  
  Add `legal_search`, `legal_rehydrate_citation`, and legal enrichment wiring.
- Modify `crates/anno-rag-mcp/src/lib.rs`  
  Add `legal_search` and `legal_rehydrate_citation` MCP tools.
- Add tests under existing crate test modules where possible; keep heavyweight GLiNER2 tests ignored.

## Pre-Flight

- [ ] **Step 1: Create or switch to an isolated worktree**

Run from `C:\Users\NMarchitecte\anno`:

```powershell
git fetch origin
git worktree add .claude/worktrees/legal-rag-gliner2 -b codex/legal-rag-gliner2 origin/main
```

Expected: a clean worktree at `C:\Users\NMarchitecte\anno\.claude\worktrees\legal-rag-gliner2`.

- [ ] **Step 2: Verify baseline**

```powershell
git status --short --branch
cargo test -p anno-rag --lib
cargo test -p anno-rag-mcp --lib
```

Expected: clean git status and passing baseline tests. If tests fail before edits, capture the failure and stop.

- [ ] **Step 3: Run GitNexus impact checks before symbol edits**

```powershell
npx gitnexus impact --repo anno Pipeline --direction upstream
npx gitnexus impact --repo anno Store --direction upstream
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
```

Expected: inspect direct callers/importers. If risk is HIGH or CRITICAL, report the blast radius before editing.

---

### Task 1: Legal Types and Label Catalog

**Files:**
- Create: `crates/anno-rag/src/legal/mod.rs`
- Create: `crates/anno-rag/src/legal/types.rs`
- Modify: `crates/anno-rag/src/lib.rs`

- [ ] **Step 1: Write the failing tests in `types.rs`**

Add this test module at the bottom of the new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_legal_labels_include_contract_and_litigation_core() {
        let labels = default_legal_labels();
        assert!(labels.iter().any(|l| l.name == "contract_party"));
        assert!(labels.iter().any(|l| l.name == "obligation"));
        assert!(labels.iter().any(|l| l.name == "deadline"));
        assert!(labels.iter().any(|l| l.name == "legal_reference"));
        assert!(labels.iter().any(|l| l.name == "risk_indicator"));
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
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::types --lib
```

Expected: FAIL because `legal` module and types do not exist.

- [ ] **Step 3: Implement `legal/mod.rs`**

```rust
//! Legal RAG domain layer: extraction labels, metadata, storage, and search filters.

pub mod enricher;
pub mod store;
pub mod types;

pub use types::{
    default_legal_labels, LegalChunkEnrichment, LegalEntity, LegalLabel, LegalSearchFilters,
    LegalSearchHit,
};
```

- [ ] **Step 4: Implement `legal/types.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A GLiNER2 label plus a natural-language description used for extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalLabel {
    /// Machine-stable label name passed to GLiNER2.
    pub name: &'static str,
    /// Description shown to the model for disambiguation.
    pub description: &'static str,
}

/// One extracted legal span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalEntity {
    /// Entity label, for example `obligation` or `legal_reference`.
    pub label: String,
    /// Extracted text in the pseudonymized chunk.
    pub text: String,
    /// Byte start offset inside the pseudonymized chunk text.
    pub byte_start: u32,
    /// Byte end offset inside the pseudonymized chunk text.
    pub byte_end: u32,
    /// Model confidence in `[0, 1]`.
    pub confidence: f32,
}

/// Phase 1 legal metadata stored beside a chunk in `legal_chunk_enrichment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalChunkEnrichment {
    /// Chunk UUID, matching `chunks.chunk_id`.
    pub chunk_id: Uuid,
    /// Document UUID, matching `chunks.doc_id`.
    pub doc_id: Uuid,
    /// Coarse document type such as `contract`, `litigation_file`, or `letter`.
    pub doc_type: Option<String>,
    /// Legal domain such as `commercial`, `employment`, or `real_estate`.
    pub legal_domain: Option<String>,
    /// Jurisdiction or forum when detected.
    pub jurisdiction: Option<String>,
    /// Source document date when known.
    pub document_date: Option<DateTime<Utc>>,
    /// Normalized party refs such as `org:acme` or vault aliases such as `ORG_1`.
    pub parties: Vec<String>,
    /// Normalized legal entity refs for LabelList filtering.
    pub legal_entities: Vec<String>,
    /// Normalized law/code/article references.
    pub legal_refs: Vec<String>,
    /// Normalized amount strings.
    pub amounts: Vec<String>,
    /// Normalized deadline strings.
    pub deadlines: Vec<String>,
    /// Low-cardinality risk tags.
    pub risk_flags: Vec<String>,
    /// Lowest confidence over extracted fields.
    pub confidence_min: f32,
    /// Average confidence over extracted fields.
    pub confidence_avg: f32,
    /// Extractor implementation version.
    pub extractor_version: String,
    /// Model id used by the extractor.
    pub model_id: String,
}

/// Legal filters accepted by `legal_search`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalSearchFilters {
    /// Restrict by document type.
    pub doc_type: Option<String>,
    /// Restrict by legal domain.
    pub legal_domain: Option<String>,
    /// Restrict by jurisdiction.
    pub jurisdiction: Option<String>,
    /// Restrict to chunks that mention at least one party ref.
    pub parties: Vec<String>,
    /// Restrict to chunks with at least one legal reference.
    pub legal_refs: Vec<String>,
    /// Restrict to chunks carrying at least one risk flag.
    pub risk_flags: Vec<String>,
    /// Restrict to documents on or after this UTC timestamp.
    pub date_from: Option<DateTime<Utc>>,
    /// Restrict to documents before or at this UTC timestamp.
    pub date_to: Option<DateTime<Utc>>,
}

impl LegalSearchFilters {
    /// Returns true when at least one filter would constrain search results.
    #[must_use]
    pub fn has_any_filter(&self) -> bool {
        self.doc_type.is_some()
            || self.legal_domain.is_some()
            || self.jurisdiction.is_some()
            || !self.parties.is_empty()
            || !self.legal_refs.is_empty()
            || !self.risk_flags.is_empty()
            || self.date_from.is_some()
            || self.date_to.is_some()
    }
}

/// Legal search result: a normal chunk hit plus legal metadata if present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalSearchHit {
    /// Retrieved chunk id.
    pub chunk_id: Uuid,
    /// Retrieved document id.
    pub doc_id: Uuid,
    /// Pseudonymized chunk text.
    pub text_pseudo: String,
    /// RRF/vector relevance score.
    pub score: f32,
    /// Legal enrichment attached to this hit.
    pub enrichment: Option<LegalChunkEnrichment>,
}

/// Default labels for French contracts and litigation files.
#[must_use]
pub fn default_legal_labels() -> Vec<LegalLabel> {
    vec![
        LegalLabel { name: "person", description: "a natural person named in a legal document" },
        LegalLabel { name: "organization", description: "a company, association, administration, court, or institution" },
        LegalLabel { name: "company_identifier", description: "a SIRET, SIREN, RCS, VAT number, or company registration identifier" },
        LegalLabel { name: "contract_party", description: "a party bound by a contract, amendment, formal notice, or dispute" },
        LegalLabel { name: "court", description: "a court, tribunal, chamber, or judicial formation" },
        LegalLabel { name: "jurisdiction", description: "a jurisdiction, governing law, forum, venue, or competent court clause" },
        LegalLabel { name: "legal_reference", description: "a citation to a code, article, law, decree, case, or legal authority" },
        LegalLabel { name: "article", description: "an article number or article reference in a code, statute, contract, or exhibit" },
        LegalLabel { name: "case_number", description: "a docket, case, appeal, RG, Portalis, or decision number" },
        LegalLabel { name: "effective_date", description: "the date when a contract, clause, obligation, or decision takes effect" },
        LegalLabel { name: "deadline", description: "a date by which a party must perform an action or lose a right" },
        LegalLabel { name: "amount", description: "a monetary amount, damages amount, fee, penalty, price, rent, or interest" },
        LegalLabel { name: "clause_type", description: "a type of contractual clause such as termination, liability, confidentiality, penalty, renewal, assignment, or non-compete" },
        LegalLabel { name: "obligation", description: "a duty imposed on a party by a contract, judgment, formal notice, law, or clause" },
        LegalLabel { name: "sanction", description: "a penalty, damages, interest, termination, forfeiture, or legal consequence" },
        LegalLabel { name: "risk_indicator", description: "a sentence or phrase indicating legal, financial, procedural, or contractual risk" },
    ]
}
```

- [ ] **Step 5: Export the module in `lib.rs`**

Add this line with the other public modules:

```rust
pub mod legal;
```

- [ ] **Step 6: Run tests**

```powershell
cargo test -p anno-rag legal::types --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/src/lib.rs crates/anno-rag/src/legal/mod.rs crates/anno-rag/src/legal/types.rs
git commit -m "feat: add legal rag domain types"
```

---

### Task 2: LanceDB Legal Enrichment Store

**Files:**
- Create: `crates/anno-rag/src/legal/store.rs`
- Modify: `crates/anno-rag/src/legal/mod.rs`

- [ ] **Step 1: Write failing schema tests**

Add this test module to `legal/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_contains_filterable_legal_columns() {
        let schema = legal_enrichment_schema();
        let names: Vec<_> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        for expected in [
            "chunk_id",
            "doc_id",
            "doc_type",
            "legal_domain",
            "jurisdiction",
            "document_date",
            "parties",
            "legal_entities",
            "legal_refs",
            "risk_flags",
        ] {
            assert!(names.contains(&expected), "missing {expected}: {names:?}");
        }
    }

    #[test]
    fn sql_string_literals_escape_quotes() {
        assert_eq!(sql_string_lit("l'avocat"), "'l''avocat'");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::store --lib
```

Expected: FAIL because `legal/store.rs` is empty or missing.

- [ ] **Step 3: Implement schema helpers**

```rust
//! LanceDB storage for legal chunk enrichment.

use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use crate::legal::types::{LegalChunkEnrichment, LegalSearchFilters};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use lancedb::{Connection, Table};
use std::sync::Arc;
use uuid::Uuid;

/// Name of the LanceDB table carrying Phase 1 legal metadata.
pub const LEGAL_ENRICHMENT_TABLE: &str = "legal_chunk_enrichment";

/// Arrow schema for `legal_chunk_enrichment`.
#[must_use]
pub fn legal_enrichment_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_id", DataType::FixedSizeBinary(16), false),
        Field::new("doc_type", DataType::Utf8, true),
        Field::new("legal_domain", DataType::Utf8, true),
        Field::new("jurisdiction", DataType::Utf8, true),
        Field::new("document_date", DataType::Timestamp(TimeUnit::Microsecond, None), true),
        Field::new("parties", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("legal_entities", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("legal_refs", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("amounts", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("deadlines", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("risk_flags", DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))), false),
        Field::new("confidence_min", DataType::Float32, false),
        Field::new("confidence_avg", DataType::Float32, false),
        Field::new("extractor_version", DataType::Utf8, false),
        Field::new("model_id", DataType::Utf8, false),
    ]))
}

pub(crate) fn sql_string_lit(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn uuid_hex(u: Uuid) -> String {
    hex::encode(u.as_bytes())
}
```

- [ ] **Step 4: Add the `LegalStore` shell**

Append:

```rust
/// LanceDB handle for legal enrichment metadata.
#[derive(Clone)]
pub struct LegalStore {
    table: Table,
}

impl LegalStore {
    /// Open or create `legal_chunk_enrichment` under the configured LanceDB directory.
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let uri = cfg
            .index_path()
            .to_str()
            .ok_or_else(|| Error::Store(format!("non-utf8 index path: {}", cfg.index_path().display())))?
            .to_string();
        let conn: Connection = lancedb::connect(&uri).execute().await?;
        let names = conn.table_names().execute().await?;
        let table = if names.iter().any(|n| n == LEGAL_ENRICHMENT_TABLE) {
            conn.open_table(LEGAL_ENRICHMENT_TABLE).execute().await?
        } else {
            conn.create_empty_table(LEGAL_ENRICHMENT_TABLE, legal_enrichment_schema())
                .execute()
                .await?
        };
        Ok(Self { table })
    }

    /// Return a SQL predicate matching candidate rows for the supplied filters.
    #[must_use]
    pub fn filter_sql(filters: &LegalSearchFilters) -> Option<String> {
        let mut clauses = Vec::new();
        if let Some(v) = &filters.doc_type {
            clauses.push(format!("doc_type = {}", sql_string_lit(v)));
        }
        if let Some(v) = &filters.legal_domain {
            clauses.push(format!("legal_domain = {}", sql_string_lit(v)));
        }
        if let Some(v) = &filters.jurisdiction {
            clauses.push(format!("jurisdiction = {}", sql_string_lit(v)));
        }
        for v in &filters.parties {
            clauses.push(format!("array_contains(parties, {})", sql_string_lit(v)));
        }
        for v in &filters.legal_refs {
            clauses.push(format!("array_contains(legal_refs, {})", sql_string_lit(v)));
        }
        for v in &filters.risk_flags {
            clauses.push(format!("array_contains(risk_flags, {})", sql_string_lit(v)));
        }
        if let Some(t) = filters.date_from {
            clauses.push(format!("document_date >= CAST({} AS TIMESTAMP)", t.timestamp_micros()));
        }
        if let Some(t) = filters.date_to {
            clauses.push(format!("document_date <= CAST({} AS TIMESTAMP)", t.timestamp_micros()));
        }
        if clauses.is_empty() {
            None
        } else {
            Some(clauses.join(" AND "))
        }
    }
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag legal::store --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/legal/store.rs crates/anno-rag/src/legal/mod.rs
git commit -m "feat: add legal enrichment store schema"
```

---

### Task 3: Chunk-Filtered Hybrid Search

**Files:**
- Modify: `crates/anno-rag/src/store.rs`

- [ ] **Step 1: Write unit tests for chunk filter SQL**

Add near the existing `store.rs` tests:

```rust
#[test]
fn chunk_id_filter_uses_fixed_binary_literals() {
    let a = uuid::Uuid::parse_str("018f0000-0000-7000-8000-000000000001").unwrap();
    let b = uuid::Uuid::parse_str("018f0000-0000-7000-8000-000000000002").unwrap();
    let sql = chunk_id_filter_sql(&[a, b]).expect("filter");
    assert!(sql.contains("chunk_id IN"));
    assert!(sql.contains("X'018f0000000070008000000000000001'"));
    assert!(sql.contains("X'018f0000000070008000000000000002'"));
}

#[test]
fn empty_chunk_id_filter_is_none() {
    assert!(chunk_id_filter_sql(&[]).is_none());
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag chunk_id_filter --lib
```

Expected: FAIL because `chunk_id_filter_sql` does not exist.

- [ ] **Step 3: Add filter helper in `store.rs`**

Place near the existing UUID helpers:

```rust
fn fixed_binary_uuid_lit(id: Uuid) -> String {
    format!("X'{}'", hex::encode(id.as_bytes()))
}

pub(crate) fn chunk_id_filter_sql(ids: &[Uuid]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let literals: Vec<String> = ids.iter().copied().map(fixed_binary_uuid_lit).collect();
    Some(format!("chunk_id IN ({})", literals.join(", ")))
}
```

- [ ] **Step 4: Add filtered search method**

Add below `Store::search`:

```rust
/// Hybrid search constrained to a set of chunk ids.
pub async fn search_filtered_to_chunks(
    &self,
    query_text: &str,
    query_vec: &[f32],
    k: usize,
    allowed_chunk_ids: &[Uuid],
) -> Result<Vec<SearchHit>> {
    use lance_index::scalar::FullTextSearchQuery;
    use lancedb::query::QueryBase;
    use lancedb::rerankers::rrf::RRFReranker;
    use std::sync::Arc;

    if allowed_chunk_ids.is_empty() {
        return Ok(Vec::new());
    }

    let filter = chunk_id_filter_sql(allowed_chunk_ids)
        .expect("non-empty chunk id slice always builds a filter");
    let stream = self
        .tbl
        .query()
        .nearest_to(query_vec.to_vec())?
        .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
        .only_if(filter)
        .rerank(Arc::new(RRFReranker::default()))
        .limit(k)
        .execute()
        .await?;
    let batches: Vec<RecordBatch> = stream.try_collect().await?;
    let mut hits = Vec::new();
    for batch in &batches {
        for i in 0..batch.num_rows() {
            hits.push(batch_to_hit(batch, i)?);
        }
    }
    hits.truncate(k);
    Ok(hits)
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p anno-rag chunk_id_filter --lib
cargo test -p anno-rag store --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/store.rs
git commit -m "feat: add chunk-filtered hybrid search"
```

---

### Task 4: LegalEnricher with Model-Free Tests

**Files:**
- Create: `crates/anno-rag/src/legal/enricher.rs`

- [ ] **Step 1: Write fake-extractor tests**

Add to `legal/enricher.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct FakeExtractor;

    impl LegalEntityExtractor for FakeExtractor {
        fn extract(&self, text: &str, _labels: &[LegalLabel]) -> Result<Vec<LegalEntity>> {
            let start = text.find("paiement").expect("fixture contains paiement") as u32;
            Ok(vec![LegalEntity {
                label: "obligation".to_string(),
                text: "paiement".to_string(),
                byte_start: start,
                byte_end: start + "paiement".len() as u32,
                confidence: 0.91,
            }])
        }

        fn model_id(&self) -> &'static str {
            "fake"
        }
    }

    #[test]
    fn enrich_chunk_carries_obligation_refs_and_confidence() {
        let enricher = LegalEnricher::new(Box::new(FakeExtractor));
        let chunk_id = uuid::Uuid::now_v7();
        let doc_id = uuid::Uuid::now_v7();
        let out = enricher
            .enrich_chunk(chunk_id, doc_id, "Le paiement intervient sous 30 jours.")
            .expect("enrich");
        assert_eq!(out.chunk_id, chunk_id);
        assert_eq!(out.doc_id, doc_id);
        assert!(out.legal_entities.contains(&"obligation:paiement".to_string()));
        assert_eq!(out.confidence_min, 0.91);
        assert_eq!(out.model_id, "fake");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

```powershell
cargo test -p anno-rag legal::enricher --lib
```

Expected: FAIL because trait and struct do not exist.

- [ ] **Step 3: Implement model-free `LegalEnricher`**

```rust
//! Legal enrichment over pseudonymized chunks.

use crate::error::Result;
use crate::legal::types::{
    default_legal_labels, LegalChunkEnrichment, LegalEntity, LegalLabel,
};
use uuid::Uuid;

/// Abstraction over GLiNER2 so unit tests do not load model weights.
pub trait LegalEntityExtractor: Send + Sync {
    /// Extract legal entities from one pseudonymized chunk.
    fn extract(&self, text: &str, labels: &[LegalLabel]) -> Result<Vec<LegalEntity>>;
    /// Model identifier for audit and stored enrichment rows.
    fn model_id(&self) -> &'static str;
}

/// Deterministic legal enricher that converts extracted spans into metadata.
pub struct LegalEnricher {
    extractor: Box<dyn LegalEntityExtractor>,
    labels: Vec<LegalLabel>,
}

impl LegalEnricher {
    /// Create an enricher with the default French legal label catalog.
    #[must_use]
    pub fn new(extractor: Box<dyn LegalEntityExtractor>) -> Self {
        Self {
            extractor,
            labels: default_legal_labels(),
        }
    }

    /// Enrich one pseudonymized chunk.
    pub fn enrich_chunk(
        &self,
        chunk_id: Uuid,
        doc_id: Uuid,
        text_pseudo: &str,
    ) -> Result<LegalChunkEnrichment> {
        let entities = self.extractor.extract(text_pseudo, &self.labels)?;
        let mut legal_entities = Vec::new();
        let mut legal_refs = Vec::new();
        let mut risk_flags = Vec::new();
        let mut parties = Vec::new();
        let mut confidences = Vec::new();

        for entity in entities {
            confidences.push(entity.confidence);
            let normalized = normalize_ref(&entity.label, &entity.text);
            match entity.label.as_str() {
                "contract_party" | "organization" => parties.push(normalized.clone()),
                "legal_reference" | "article" => legal_refs.push(normalized.clone()),
                "risk_indicator" | "sanction" => risk_flags.push(normalized.clone()),
                _ => {}
            }
            legal_entities.push(normalized);
        }

        let confidence_min = confidences.iter().copied().reduce(f32::min).unwrap_or(0.0);
        let confidence_avg = if confidences.is_empty() {
            0.0
        } else {
            confidences.iter().sum::<f32>() / confidences.len() as f32
        };

        Ok(LegalChunkEnrichment {
            chunk_id,
            doc_id,
            doc_type: None,
            legal_domain: None,
            jurisdiction: None,
            document_date: None,
            parties,
            legal_entities,
            legal_refs,
            amounts: Vec::new(),
            deadlines: Vec::new(),
            risk_flags,
            confidence_min,
            confidence_avg,
            extractor_version: env!("CARGO_PKG_VERSION").to_string(),
            model_id: self.extractor.model_id().to_string(),
        })
    }
}

fn normalize_ref(label: &str, text: &str) -> String {
    let body = text
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!("{label}:{body}")
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
git commit -m "feat: add model-free legal enricher"
```

---

### Task 5: Pipeline Legal Search and Citation Rehydration

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/legal/store.rs`

- [ ] **Step 1: Add `LegalStore` to `Pipeline`**

In `pipeline.rs`, update imports and struct:

```rust
use crate::legal::store::LegalStore;
use crate::legal::types::{LegalSearchFilters, LegalSearchHit};
```

Add field:

```rust
legal_store: LegalStore,
```

In `Pipeline::new`:

```rust
let legal_store = LegalStore::open(&cfg).await?;
```

In returned struct:

```rust
legal_store,
```

- [ ] **Step 2: Add filtered chunk-id lookup to `LegalStore`**

Implement in `legal/store.rs`:

```rust
impl LegalStore {
    /// Return candidate chunk ids matching legal metadata filters.
    pub async fn filter_chunk_ids(&self, filters: &LegalSearchFilters, limit: usize) -> Result<Vec<Uuid>> {
        use arrow_array::FixedSizeBinaryArray;
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        if !filters.has_any_filter() {
            return Ok(Vec::new());
        }
        let filter = Self::filter_sql(filters)
            .expect("has_any_filter true should produce a SQL predicate");
        let stream = self
            .table
            .query()
            .select(lancedb::query::Select::columns(&["chunk_id"]))
            .only_if(filter)
            .limit(limit)
            .execute()
            .await?;
        let batches: Vec<arrow_array::RecordBatch> = stream.try_collect().await?;
        let mut out = Vec::new();
        for batch in batches {
            let arr = batch
                .column_by_name("chunk_id")
                .ok_or_else(|| Error::Store("legal filter missing chunk_id column".to_string()))?
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .ok_or_else(|| Error::Store("legal filter chunk_id column has wrong type".to_string()))?;
            for i in 0..arr.len() {
                out.push(Uuid::from_slice(arr.value(i)).map_err(|e| Error::Store(format!("chunk uuid: {e}")))?);
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 3: Add `Pipeline::legal_search`**

```rust
impl Pipeline {
    /// Legal-aware search: pseudonymize query, apply legal metadata filters, then run hybrid search.
    pub async fn legal_search(
        &self,
        query: &str,
        top_k: usize,
        filters: LegalSearchFilters,
    ) -> Result<Vec<LegalSearchHit>> {
        let entities = self.detector_get_or_init()?.detect(query)?;
        let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
        let qv = self.embedder().await?.embed_query(&pseudo_q)?;

        let chunk_hits = if filters.has_any_filter() {
            let allowed = self
                .legal_store
                .filter_chunk_ids(&filters, top_k.saturating_mul(20).max(100))
                .await?;
            self.store
                .search_filtered_to_chunks(&pseudo_q, &qv, top_k, &allowed)
                .await?
        } else {
            self.store.search(&pseudo_q, &qv, top_k).await?
        };

        Ok(chunk_hits
            .into_iter()
            .map(|h| LegalSearchHit {
                chunk_id: h.chunk_id,
                doc_id: h.doc_id,
                text_pseudo: h.text_pseudo,
                score: h.score,
                enrichment: None,
            })
            .collect())
    }
}
```

- [ ] **Step 4: Run compile checks**

```powershell
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/legal/store.rs
git commit -m "feat: add legal search pipeline"
```

---

### Task 6: Chunk Point Lookup and Citation-Scoped Rehydration

**Files:**
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add `Store::chunk_by_id`**

```rust
impl Store {
    /// Fetch one chunk by id for citation verification and rehydration.
    pub async fn chunk_by_id(&self, chunk_id: Uuid) -> Result<Option<SearchHit>> {
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let filter = chunk_id_filter_sql(&[chunk_id]).expect("one chunk id builds a filter");
        let stream = self.tbl.query().only_if(filter).limit(1).execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(batch_to_hit(batch, 0)?));
            }
        }
        Ok(None)
    }
}
```

- [ ] **Step 2: Add `Pipeline::legal_rehydrate_citation`**

```rust
pub async fn legal_rehydrate_citation(
    &self,
    chunk_id: Uuid,
    byte_start: u32,
    byte_end: u32,
) -> Result<RehydratedText> {
    if byte_start >= byte_end {
        return Err(Error::Store("citation byte_start must be smaller than byte_end".to_string()));
    }
    let hit = self
        .store
        .chunk_by_id(chunk_id)
        .await?
        .ok_or_else(|| Error::Store(format!("unknown citation chunk_id: {chunk_id}")))?;
    let start = usize::try_from(byte_start)
        .map_err(|e| Error::Store(format!("byte_start conversion: {e}")))?;
    let end = usize::try_from(byte_end)
        .map_err(|e| Error::Store(format!("byte_end conversion: {e}")))?;
    let span = hit
        .text_pseudo
        .get(start..end)
        .ok_or_else(|| Error::Store("citation offsets are not valid UTF-8 boundaries".to_string()))?;
    self.rehydrate(span).await
}
```

- [ ] **Step 3: Run tests**

```powershell
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat: add citation-scoped rehydration"
```

---

### Task 7: MCP Legal Tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add parameter/result structs**

Add near existing MCP param structs:

```rust
/// Parameters for the `legal_search` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalSearchParams {
    /// User legal query. PII is pseudonymized before retrieval.
    pub query: String,
    /// Number of results to return.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional document type filter.
    #[serde(default)]
    pub doc_type: Option<String>,
    /// Optional legal domain filter.
    #[serde(default)]
    pub legal_domain: Option<String>,
    /// Optional jurisdiction filter.
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Party refs to require.
    #[serde(default)]
    pub parties: Vec<String>,
    /// Legal refs to require.
    #[serde(default)]
    pub legal_refs: Vec<String>,
    /// Risk flags to require.
    #[serde(default)]
    pub risk_flags: Vec<String>,
}

/// Parameters for `legal_rehydrate_citation`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalRehydrateCitationParams {
    /// Chunk UUID.
    pub chunk_id: String,
    /// Byte start offset inside the pseudonymized chunk.
    pub byte_start: u32,
    /// Byte end offset inside the pseudonymized chunk.
    pub byte_end: u32,
}
```

- [ ] **Step 2: Add MCP tool handlers**

Add inside `impl AnnoRagServer`:

```rust
#[tool(
    description = "Legal-aware search over pseudonymized documents. Applies legal metadata filters before LanceDB hybrid retrieval when filters are supplied."
)]
async fn legal_search(&self, Parameters(params): Parameters<LegalSearchParams>) -> String {
    let filters = anno_rag::legal::LegalSearchFilters {
        doc_type: params.doc_type,
        legal_domain: params.legal_domain,
        jurisdiction: params.jurisdiction,
        parties: params.parties,
        legal_refs: params.legal_refs,
        risk_flags: params.risk_flags,
        date_from: None,
        date_to: None,
    };
    match self.pipeline.legal_search(&params.query, params.top_k, filters).await {
        Ok(hits) => serde_json::to_string_pretty(&hits)
            .unwrap_or_else(|e| format!(r#"{{"error":"serialize legal_search: {e}"}}"#)),
        Err(e) => format!(r#"{{"error":"{e}"}}"#),
    }
}

#[tool(
    description = "Rehydrate only one citation span from a pseudonymized chunk. Requires chunk_id and byte offsets."
)]
async fn legal_rehydrate_citation(
    &self,
    Parameters(params): Parameters<LegalRehydrateCitationParams>,
) -> String {
    let chunk_id = match uuid::Uuid::parse_str(&params.chunk_id) {
        Ok(v) => v,
        Err(e) => return format!(r#"{{"error":"invalid chunk_id: {e}"}}"#),
    };
    match self
        .pipeline
        .legal_rehydrate_citation(chunk_id, params.byte_start, params.byte_end)
        .await
    {
        Ok(out) => serde_json::to_string_pretty(&RehydrateResult {
            text: out.text,
            tokens_rehydrated: out.tokens_rehydrated,
        })
        .unwrap_or_else(|e| format!(r#"{{"error":"serialize rehydrate: {e}"}}"#)),
        Err(e) => format!(r#"{{"error":"{e}"}}"#),
    }
}
```

- [ ] **Step 3: Run MCP compile tests**

```powershell
cargo test -p anno-rag-mcp --lib
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat: expose legal search mcp tools"
```

---

### Task 8: Index Maintenance and Validation Hooks

**Files:**
- Modify: `crates/anno-rag/src/legal/store.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add scalar index setup skeleton**

In `LegalStore`:

```rust
/// Create scalar indexes used by legal metadata filters.
pub async fn setup_indexes(&self) -> Result<()> {
    use lancedb::index::scalar::{BTreeIndexBuilder, BitmapIndexBuilder, LabelListIndexBuilder};
    use lancedb::index::Index;

    let existing = self
        .table
        .list_indices()
        .await
        .map_err(|e| Error::Store(format!("legal list_indices: {e}")))?;
    let has_index_on = |col: &str| existing.iter().any(|i| i.columns.iter().any(|c| c == col));

    if !has_index_on("chunk_id") {
        self.table.create_index(&["chunk_id"], Index::BTree(BTreeIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("doc_id") {
        self.table.create_index(&["doc_id"], Index::BTree(BTreeIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("doc_type") {
        self.table.create_index(&["doc_type"], Index::Bitmap(BitmapIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("legal_domain") {
        self.table.create_index(&["legal_domain"], Index::Bitmap(BitmapIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("document_date") {
        self.table.create_index(&["document_date"], Index::BTree(BTreeIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("parties") {
        self.table.create_index(&["parties"], Index::LabelList(LabelListIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("legal_refs") {
        self.table.create_index(&["legal_refs"], Index::LabelList(LabelListIndexBuilder::default())).execute().await?;
    }
    if !has_index_on("risk_flags") {
        self.table.create_index(&["risk_flags"], Index::LabelList(LabelListIndexBuilder::default())).execute().await?;
    }
    Ok(())
}
```

- [ ] **Step 2: Call index setup from `Pipeline::new`**

After opening `legal_store`:

```rust
legal_store.setup_indexes().await?;
```

- [ ] **Step 3: Add maintenance method**

```rust
impl Pipeline {
    /// Maintain legal and chunk indexes after ingest/enrichment batches.
    pub async fn optimize_after_ingest(&self) -> Result<()> {
        self.store.maybe_build_index(self.cfg.vector_index_threshold).await?;
        self.store.maybe_build_fts_index().await?;
        self.legal_store.setup_indexes().await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test -p anno-rag --lib
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/store.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat: add legal index maintenance"
```

---

### Task 9: GLiNER2 Adapter Behind the Legal Extractor Trait

**Files:**
- Modify: `crates/anno-rag/src/legal/enricher.rs`

- [ ] **Step 1: Add ignored model-loading test**

```rust
#[test]
#[ignore = "requires warm HF GLiNER2 ONNX cache"]
fn gliner_legal_extractor_loads_from_cache() {
    let extractor = GlinerLegalExtractor::new().expect("load GLiNER2 legal extractor");
    assert!(extractor.model_id().contains("gliner2"));
}
```

- [ ] **Step 2: Implement GLiNER2 adapter**

```rust
/// GLiNER2-backed legal extractor.
pub struct GlinerLegalExtractor {
    model: anno::backends::gliner2_fastino::GLiNER2Fastino,
}

impl GlinerLegalExtractor {
    /// Load the existing multilingual ONNX GLiNER2 model.
    pub fn new() -> Result<Self> {
        let model = anno::backends::gliner2_fastino::GLiNER2Fastino::from_pretrained(
            crate::detect::NER_MODEL_ID,
        )
        .map_err(|e| crate::error::Error::Detect(format!("legal gliner load: {e}")))?;
        Ok(Self { model })
    }
}

impl LegalEntityExtractor for GlinerLegalExtractor {
    fn extract(&self, text: &str, labels: &[LegalLabel]) -> Result<Vec<LegalEntity>> {
        let label_names: Vec<&str> = labels.iter().map(|l| l.name).collect();
        let entities = self
            .model
            .extract_with_types(text, &label_names, 0.5)
            .map_err(|e| crate::error::Error::Detect(format!("legal gliner extract: {e}")))?;
        Ok(entities
            .into_iter()
            .map(|e| LegalEntity {
                label: e.entity_type.clone(),
                text: e.text.clone(),
                byte_start: e.start() as u32,
                byte_end: e.end() as u32,
                confidence: e.confidence,
            })
            .collect())
    }

    fn model_id(&self) -> &'static str {
        crate::detect::NER_MODEL_ID
    }
}
```

- [ ] **Step 3: Run non-ignored tests**

```powershell
cargo test -p anno-rag legal::enricher --lib
```

Expected: PASS without loading the model.

- [ ] **Step 4: Run ignored test only on a warm cache machine**

```powershell
cargo test -p anno-rag gliner_legal_extractor_loads_from_cache --lib -- --ignored
```

Expected: PASS when the GLiNER2 cache is warm; skip with a note when cache is absent.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/legal/enricher.rs
git commit -m "feat: add gliner legal extractor adapter"
```

---

### Task 10: Verification, GitNexus, and Handoff

**Files:**
- No new files unless tests reveal required fixes.

- [ ] **Step 1: Run focused test suite**

```powershell
cargo test -p anno-rag --lib
cargo test -p anno-rag-mcp --lib
cargo test -p anno-rag-tabular --lib
```

Expected: PASS.

- [ ] **Step 2: Run formatting and lint checks**

```powershell
cargo fmt --check
cargo clippy -p anno-rag -p anno-rag-mcp --all-targets -- -D warnings
```

Expected: PASS. If clippy reports pre-existing warnings outside changed modules, capture the warning and run a narrower clippy command for changed packages.

- [ ] **Step 3: Run GitNexus scope check**

If the MCP `detect_changes` tool is available, run it for the staged scope. If only the CLI is available, run:

```powershell
npx gitnexus query --repo anno "legal rag gliner lancedb changed store pipeline mcp"
npx gitnexus impact --repo anno Pipeline --direction upstream
npx gitnexus impact --repo anno Store --direction upstream
```

Expected: changed scope is limited to `anno-rag`, `anno-rag-mcp`, and planned legal module files.

- [ ] **Step 4: Review diff**

```powershell
git diff --stat origin/main...HEAD
git diff origin/main...HEAD -- crates/anno-rag crates/anno-rag-mcp
```

Expected: no unrelated files, no raw secrets, no accidental generated artifacts.

- [ ] **Step 5: Final commit if needed**

```powershell
git status --short
git add crates/anno-rag crates/anno-rag-mcp
git commit -m "feat: add legal rag search foundation"
```

Expected: only runs if there are uncommitted implementation changes after prior task commits.

---

## Spec Coverage Check

- LegalEnricher separated from Detector: Task 4 and Task 9.
- `legal_chunk_enrichment` instead of chunk schema migration: Task 2.
- LanceDB scalar indexes: Task 8.
- Metadata prefilter path: Task 3 and Task 5.
- Citation-scoped rehydration: Task 6 and Task 7.
- Claude Desktop MCP tools: Task 7.
- GLiNER2 adapter without forcing model startup in unit tests: Task 9.
- Tabular extraction reuse: File structure and architecture preserve `anno-rag-tabular`; a second implementation plan should add `legal_extract_contract`, `legal_extract_case_file`, `legal_timeline`, and validation workflows on top of the foundation built here.

## Execution Notes

- Keep commits small. The first five tasks should compile without requiring model downloads.
- Do not load GLiNER2 in normal tests. Use ignored tests for warm-cache validation.
- Do not expose free-form rehydration through legal tools.
- Keep `Detector` focused on pseudonymization. Legal labels belong to `LegalEnricher`.
- Use LanceDB filters before hybrid retrieval when a legal filter is part of the user request.
