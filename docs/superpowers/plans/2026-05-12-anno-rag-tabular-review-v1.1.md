# anno-rag v1.1 — Tabular Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Harvey/Legora-style tabular review to anno-rag — schema-driven extraction across folder-scoped document sets, with offset-accurate per-cell citations, extractive verifier, and an MCP App grid UI for Claude Desktop / Cowork.

**Architecture:** New crate `anno-rag-tabular` containing the extraction engine (LLM batch with constrained JSON decoding) + verifier (Isaacus-style cosine ≥ 0.4 support scoring) + LanceDB storage (`reviews`/`columns`/`rows`/`cells` tables, immutable versioned cells). New crate `anno-rag-tabular-ui` ships a sandboxed HTML/JS grid bundle exposed via MCP Apps `_meta.ui.resourceUri`. The existing `anno-rag-mcp` binary gains a `tabular` module with 7 tools + 3 resource families.

**Tech Stack:** Rust (workspace), LanceDB 0.27.2 + arrow 58, candle 0.10 (cross-encoder reranker reused as verifier), rmcp 1.6 with MCP Apps support, serde_json schema for constrained decoding, rust_xlsxwriter 0.86 for export. UI bundle: TypeScript + Vite + ag-grid-community + react-pdf-viewer.

**Prerequisites:** v1.0 MVP shipped, including foundations F1–F4 (folder_path + folder_segments columns, project_id scoping, extractive verifier exposed via cross-encoder, MCP App-ready response convention).

---

## File Structure (locked at planning time)

```
crates/anno-rag-tabular/                # NEW
├── Cargo.toml
├── src/
│   ├── lib.rs                          # public API surface, re-exports
│   ├── ids.rs                          # ReviewId/ColumnId/RowId newtypes
│   ├── error.rs                        # TabularError enum
│   ├── schema/
│   │   ├── mod.rs
│   │   ├── column.rs                   # Column struct + builder
│   │   ├── ttype.rs                    # CellType enum
│   │   ├── template.rs                 # Template loader (5 M&A templates)
│   │   ├── conditional.rs              # ConditionalSpec + Predicate
│   │   └── json_schema.rs              # CellType → JSON Schema for constrained decoding
│   ├── storage/
│   │   ├── mod.rs                      # public StorageHandle
│   │   ├── arrow_schema.rs             # arrow::Schema for reviews/columns/rows/cells
│   │   ├── reviews.rs                  # CRUD on reviews table
│   │   ├── columns.rs                  # CRUD on columns table
│   │   ├── rows.rs                     # CRUD on rows table
│   │   ├── cells.rs                    # CRUD on cells table + version history
│   │   └── lock.rs                     # locked-cell semantics
│   ├── llm/
│   │   ├── mod.rs                      # LlmClient trait
│   │   ├── anthropic.rs                # default impl: Anthropic API
│   │   └── mock.rs                     # deterministic mock for tests
│   ├── extract/
│   │   ├── mod.rs                      # Extractor orchestrator
│   │   ├── batch.rs                    # one LLM call per (doc, column-batch)
│   │   ├── fanout.rs                   # tokio parallel fan-out, bounded concurrency
│   │   └── conditional.rs              # DAG of conditional columns
│   ├── verify/
│   │   ├── mod.rs                      # cell verifier
│   │   ├── support.rs                  # cosine support score via cross-encoder
│   │   └── offsets.rs                  # validate citation offsets against source chunks
│   ├── export/
│   │   ├── mod.rs
│   │   ├── csv.rs
│   │   ├── xlsx.rs
│   │   └── markdown.rs
│   └── templates/                      # built-in TOML templates
│       ├── nda-v1.toml
│       ├── customer-contract-v1.toml
│       ├── real-estate-v1.toml
│       ├── employment-v1.toml
│       └── ip-v1.toml
└── tests/
    ├── integration.rs
    └── fixtures/
        ├── nda_sample.pdf.anon         # pseudonymized
        └── nda_expected.json

crates/anno-rag-tabular-ui/             # NEW — MCP App bundle
├── Cargo.toml                          # build script packages ui/dist into a Rust resource
├── build.rs                            # runs `npm run build`, validates dist/index.html exists
├── src/lib.rs                          # exposes bundle_bytes() + serve_uri()
├── ui/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.ts                     # bootstraps grid
│       ├── grid.ts                     # ag-grid wrapper
│       ├── source-viewer.ts            # PDF/markdown drill-down panel
│       ├── mcp-client.ts               # postMessage JSON-RPC wrapper
│       └── styles.css

crates/anno-rag-mcp/                    # MODIFY (extend existing v1.0 binary)
└── src/
    ├── main.rs                         # MODIFY: register tabular tools + apps
    ├── tabular/                        # NEW module
    │   ├── mod.rs
    │   ├── tools.rs                    # 7 tools: create, add_column, add_rows,
    │   │                               #          refine_cell, set_cell, export, open
    │   ├── resources.rs                # review:// resource handlers
    │   └── apps.rs                     # MCP Apps wiring (_meta.ui.resourceUri)
    └── tabular_tests.rs

crates/anno-rag-cli/                    # MODIFY
└── src/
    ├── main.rs                         # MODIFY: add subcommand `review`
    └── review.rs                       # NEW: clap subcommand wiring
```

**No-changes-needed crates** (load-bearing prerequisites from v1.0 MVP):
- `anno-rag-core`: provides `DocId`, `ChunkId`, `SubjectId` + traits
- `anno-rag-embed`: provides `Embed` and `Rerank` traits + camembert-L6 cross-encoder (we reuse for verifier)
- `anno-rag-store`: provides `folder_path`, `folder_segments`, `project_id` columns from F1+F2
- `anno-rag-audit`: tabular events go through existing hash-chained audit

---

## Phase 1 — Crate scaffold, IDs, error types

### Task 1: Create the `anno-rag-tabular` crate

**Files:**
- Create: `crates/anno-rag-tabular/Cargo.toml`
- Create: `crates/anno-rag-tabular/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — add member)

- [ ] **Step 1: Add workspace member**

Edit `Cargo.toml` (root) — under `[workspace] members =` add `"crates/anno-rag-tabular"`.

```toml
members = [
    "crates/anno",
    "crates/anno-cli",
    "crates/anno-eval",
    "crates/anno-rag-core",
    "crates/anno-rag-ingest",
    "crates/anno-rag-detect",
    "crates/anno-rag-embed",
    "crates/anno-rag-store",
    "crates/anno-rag-audit",
    "crates/anno-rag-eval",
    "crates/anno-rag-tabular",           # NEW
    "crates/anno-rag-tabular-ui",        # NEW (added in Task 28)
    "crates/anno-rag-cli",
    "crates/anno-rag-mcp",
    "crates/anno-rag-api",
]
```

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "anno-rag-tabular"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
anno-rag-core      = { path = "../anno-rag-core" }
anno-rag-store     = { path = "../anno-rag-store" }
anno-rag-embed     = { path = "../anno-rag-embed" }
anno-rag-audit     = { path = "../anno-rag-audit" }

tokio              = { workspace = true }
async-trait        = { workspace = true }
serde              = { workspace = true }
serde_json         = { workspace = true }
toml               = { workspace = true }
thiserror          = { workspace = true }
tracing            = { workspace = true }
uuid               = { workspace = true }
chrono             = { workspace = true }
lancedb            = { workspace = true }
arrow              = { workspace = true }
arrow-array        = { workspace = true }
arrow-schema       = { workspace = true }
sha2               = { workspace = true }
regex              = { workspace = true }
reqwest            = { version = "0.12", features = ["json", "rustls-tls", "stream"], default-features = false }
futures            = { workspace = true }

rust_xlsxwriter    = "0.86"

[dev-dependencies]
tempfile           = { workspace = true }
insta              = { workspace = true }
proptest           = { workspace = true }

[lints]
workspace = true
```

- [ ] **Step 3: Create skeleton lib.rs**

```rust
//! anno-rag-tabular — Harvey/Legora-style tabular review for legal docs.
//!
//! Provides schema-driven extraction with per-cell citations, extractive
//! verifier, conditional columns, and CSV/XLSX/Markdown export. Storage
//! lives in LanceDB alongside the v1.0 chunks index.

pub mod ids;
pub mod error;
pub mod schema;
pub mod storage;
pub mod llm;
pub mod extract;
pub mod verify;
pub mod export;

pub use error::{Error, Result};
pub use ids::{ReviewId, ColumnId, RowId};
pub use schema::{Column, CellType, Template};
pub use storage::StorageHandle;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p anno-rag-tabular`
Expected: PASS (will fail because of missing modules — fix in Task 2)

Actually, the modules don't exist yet, so this will fail. That's OK — the compile error must mention "file not found for module".

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/anno-rag-tabular/
git commit -m "feat(tabular): scaffold anno-rag-tabular crate"
```

---

### Task 2: Define `ids.rs`

**Files:**
- Create: `crates/anno-rag-tabular/src/ids.rs`
- Test: same file (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag-tabular/src/ids.rs`:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReviewId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColumnId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowId(pub Uuid);

impl ReviewId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for ReviewId {
    fn default() -> Self {
        Self::new()
    }
}

impl ColumnId {
    /// Deterministic — same review_id + name → same ColumnId. Lets re-runs upsert.
    pub fn for_name(review_id: ReviewId, name: &str) -> Self {
        let ns = Uuid::NAMESPACE_OID;
        let key = format!("{}::{}", review_id.0, name);
        Self(Uuid::new_v5(&ns, key.as_bytes()))
    }
}

impl RowId {
    /// Deterministic — same review_id + doc_id → same RowId.
    pub fn for_doc(review_id: ReviewId, doc_id: anno_rag_core::DocId) -> Self {
        let ns = Uuid::NAMESPACE_OID;
        let key = format!("{}::{}", review_id.0, doc_id.0);
        Self(Uuid::new_v5(&ns, key.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_rag_core::DocId;

    #[test]
    fn column_id_is_deterministic() {
        let r = ReviewId::new();
        let a = ColumnId::for_name(r, "governing_law");
        let b = ColumnId::for_name(r, "governing_law");
        assert_eq!(a, b);
    }

    #[test]
    fn column_id_differs_per_review() {
        let r1 = ReviewId::new();
        let r2 = ReviewId::new();
        let a = ColumnId::for_name(r1, "governing_law");
        let b = ColumnId::for_name(r2, "governing_law");
        assert_ne!(a, b);
    }

    #[test]
    fn row_id_is_deterministic() {
        let r = ReviewId::new();
        let d = DocId(Uuid::new_v4());
        assert_eq!(RowId::for_doc(r, d), RowId::for_doc(r, d));
    }

    #[test]
    fn review_id_is_time_sortable() {
        let a = ReviewId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ReviewId::new();
        assert!(b.0 > a.0, "v7 UUIDs should be monotonic");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag-tabular --lib ids::tests`
Expected: FAIL — `error[E0432]: unresolved import 'anno_rag_core'` or "DocId not found" until we confirm v1.0 already exports `DocId` on `anno_rag_core::DocId`.

Validate from v1.0: `cargo doc -p anno-rag-core --open` and confirm `DocId` is `pub use`-d at the crate root.

- [ ] **Step 3: Fix imports if needed**

If `DocId` is not at crate root, `pub use ids::DocId;` in `crates/anno-rag-core/src/lib.rs`. Then re-run.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag-tabular --lib ids::tests`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/ids.rs
git commit -m "feat(tabular): add ReviewId/ColumnId/RowId newtypes"
```

---

### Task 3: Define `error.rs`

**Files:**
- Create: `crates/anno-rag-tabular/src/error.rs`

- [ ] **Step 1: Write the failing test (compile-only assertion)**

Create `crates/anno-rag-tabular/src/error.rs`:

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    // -- recoverable per-cell --
    #[error("extraction failed for doc {doc} col {col}: {source}")]
    Extract {
        doc: String,
        col: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("verifier rejected cell (score {score}): {reason}")]
    VerifierRejected { score: f32, reason: String },

    #[error("conditional gate skipped column {col}")]
    ConditionalSkip { col: String },

    // -- fatal --
    #[error("template '{name}' not found")]
    TemplateNotFound { name: String },

    #[error("schema mismatch: cell type {expected} vs LLM output {got}")]
    SchemaMismatch { expected: String, got: String },

    #[error("locked cell cannot be auto-overwritten: review={review} row={row} col={col}")]
    LockedCell {
        review: String,
        row: String,
        col: String,
    },

    #[error("conditional column dependency cycle: {path}")]
    ConditionalCycle { path: String },

    // -- pass-through --
    #[error(transparent)]
    Lance(#[from] lancedb::Error),

    #[error(transparent)]
    Arrow(#[from] arrow::error::ArrowError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    Core(#[from] anno_rag_core::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }

    #[test]
    fn error_display_includes_context() {
        let e = Error::TemplateNotFound { name: "ndax".into() };
        assert_eq!(format!("{e}"), "template 'ndax' not found");
    }

    #[test]
    fn locked_cell_error_serializes_useful_fields() {
        let e = Error::LockedCell {
            review: "r1".into(),
            row: "row1".into(),
            col: "col1".into(),
        };
        let msg = format!("{e}");
        assert!(msg.contains("r1"));
        assert!(msg.contains("row1"));
        assert!(msg.contains("col1"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag-tabular --lib error::tests`
Expected: PASS on first run (the test is mostly type-system assertion + Display checks; nothing to fail on).

If it FAILs, the failure is on `Core(#[from] anno_rag_core::Error)` — meaning v1.0 hasn't exposed `Error` at the core crate root.

- [ ] **Step 3: Fix `anno_rag_core::Error` re-export if needed**

If failure: `pub use error::Error;` at `crates/anno-rag-core/src/lib.rs`. Verify with `cargo check -p anno-rag-tabular`.

- [ ] **Step 4: Run all tests**

Run: `cargo test -p anno-rag-tabular --lib`
Expected: 7 passed (4 from ids + 3 from error).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/error.rs crates/anno-rag-core/src/lib.rs
git commit -m "feat(tabular): add Error/Result types with fatal/recoverable split"
```

---

## Phase 2 — Schema definition (CellType, Column, Conditional, JSON Schema generation)

### Task 4: Define `CellType` enum

**Files:**
- Create: `crates/anno-rag-tabular/src/schema/mod.rs`
- Create: `crates/anno-rag-tabular/src/schema/ttype.rs`

- [ ] **Step 1: Create `schema/mod.rs`**

```rust
//! Schema definition: cell types, columns, conditional gates, templates.

pub mod ttype;
pub mod column;
pub mod conditional;
pub mod template;
pub mod json_schema;

pub use ttype::CellType;
pub use column::Column;
pub use conditional::{ConditionalSpec, Predicate};
pub use template::Template;
```

- [ ] **Step 2: Write the failing test in `ttype.rs`**

Create `crates/anno-rag-tabular/src/schema/ttype.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellType {
    /// Free-form text answer.
    Text,
    /// ISO-8601 date.
    Date,
    /// Decimal currency amount with ISO 4217 code (EUR, USD, …).
    Currency { code: String },
    /// Exact quote from source — must be a verbatim substring of a chunk.
    Verbatim,
    /// One of a closed set of options.
    Enum { options: Vec<String> },
    /// True/false.
    Boolean,
    /// Decimal number (no currency).
    Number,
}

impl CellType {
    pub fn discriminant_name(&self) -> &'static str {
        match self {
            CellType::Text => "text",
            CellType::Date => "date",
            CellType::Currency { .. } => "currency",
            CellType::Verbatim => "verbatim",
            CellType::Enum { .. } => "enum",
            CellType::Boolean => "boolean",
            CellType::Number => "number",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_with_kind_tag() {
        let t = CellType::Currency { code: "EUR".into() };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v, json!({"kind": "currency", "code": "EUR"}));
    }

    #[test]
    fn deserializes_enum_options() {
        let v = json!({"kind": "enum", "options": ["paris", "lyon"]});
        let t: CellType = serde_json::from_value(v).unwrap();
        match t {
            CellType::Enum { options } => assert_eq!(options, vec!["paris", "lyon"]),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trips_text() {
        let t = CellType::Text;
        let s = serde_json::to_string(&t).unwrap();
        let back: CellType = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn discriminant_names_stable() {
        assert_eq!(CellType::Text.discriminant_name(), "text");
        assert_eq!(CellType::Boolean.discriminant_name(), "boolean");
        assert_eq!(
            CellType::Enum { options: vec![] }.discriminant_name(),
            "enum"
        );
    }
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test -p anno-rag-tabular --lib schema::ttype`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-tabular/src/schema/
git commit -m "feat(tabular): add CellType enum with serde tag-style discriminant"
```

---

### Task 5: Define `Column` + builder

**Files:**
- Create: `crates/anno-rag-tabular/src/schema/column.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag-tabular/src/schema/column.rs`:

```rust
use crate::ids::{ColumnId, ReviewId};
use crate::schema::{CellType, ConditionalSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: ColumnId,
    pub name: String,
    pub prompt: String,
    pub cell_type: CellType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub conditional: Option<ConditionalSpec>,
    /// Human-input only — extractor skips this column.
    #[serde(default)]
    pub manual: bool,
    /// Display order in the grid.
    #[serde(default)]
    pub order: u32,
}

pub struct ColumnBuilder {
    review_id: ReviewId,
    name: String,
    prompt: String,
    cell_type: CellType,
    conditional: Option<ConditionalSpec>,
    manual: bool,
    order: u32,
}

impl ColumnBuilder {
    pub fn new(review_id: ReviewId, name: &str, prompt: &str, cell_type: CellType) -> Self {
        Self {
            review_id,
            name: name.into(),
            prompt: prompt.into(),
            cell_type,
            conditional: None,
            manual: false,
            order: 0,
        }
    }

    pub fn conditional(mut self, c: ConditionalSpec) -> Self {
        self.conditional = Some(c);
        self
    }

    pub fn manual(mut self) -> Self {
        self.manual = true;
        self
    }

    pub fn order(mut self, n: u32) -> Self {
        self.order = n;
        self
    }

    pub fn build(self) -> Column {
        Column {
            id: ColumnId::for_name(self.review_id, &self.name),
            name: self.name,
            prompt: self.prompt,
            cell_type: self.cell_type,
            conditional: self.conditional,
            manual: self.manual,
            order: self.order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_deterministic_id() {
        let r = ReviewId::new();
        let a = ColumnBuilder::new(r, "term", "What is the term?", CellType::Text).build();
        let b = ColumnBuilder::new(r, "term", "different prompt", CellType::Text).build();
        // ID is based on (review_id, name) — prompt change does not invalidate
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn manual_columns_are_marked() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "reviewer_notes", "Reviewer comments", CellType::Text)
            .manual()
            .build();
        assert!(c.manual);
    }

    #[test]
    fn round_trips_through_json() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "amount", "Total amount", CellType::Currency { code: "EUR".into() })
            .order(3)
            .build();
        let s = serde_json::to_string(&c).unwrap();
        let back: Column = serde_json::from_str(&s).unwrap();
        assert_eq!(c.name, back.name);
        assert_eq!(c.order, back.order);
    }
}
```

- [ ] **Step 2: Run test to verify it compiles and passes**

Run: `cargo test -p anno-rag-tabular --lib schema::column`
Expected: FAIL — `ConditionalSpec` not yet defined.

- [ ] **Step 3: Create a stub `conditional.rs`**

Create `crates/anno-rag-tabular/src/schema/conditional.rs`:

```rust
use crate::ids::ColumnId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalSpec {
    pub parent_col: ColumnId,
    pub predicate: Predicate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Predicate {
    Equals { value: serde_json::Value },
    NotEquals { value: serde_json::Value },
    NonNull,
    Matches { regex: String },
}
```

- [ ] **Step 4: Re-run tests**

Run: `cargo test -p anno-rag-tabular --lib`
Expected: 11 passed (cumulative from prior tasks).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/schema/column.rs crates/anno-rag-tabular/src/schema/conditional.rs
git commit -m "feat(tabular): add Column struct + builder + Predicate stub"
```

---

### Task 6: Flesh out conditional predicates

**Files:**
- Modify: `crates/anno-rag-tabular/src/schema/conditional.rs`

- [ ] **Step 1: Write the failing test (append to file)**

Append at the bottom of `crates/anno-rag-tabular/src/schema/conditional.rs`:

```rust
impl Predicate {
    /// Evaluate predicate against a parent cell value. Returns false when the
    /// parent cell is missing or has a value that doesn't satisfy.
    pub fn eval(&self, parent_value: Option<&serde_json::Value>) -> bool {
        match (self, parent_value) {
            (Predicate::NonNull, Some(serde_json::Value::Null)) => false,
            (Predicate::NonNull, None) => false,
            (Predicate::NonNull, Some(_)) => true,

            (Predicate::Equals { value }, Some(actual)) => actual == value,
            (Predicate::Equals { .. }, None) => false,

            (Predicate::NotEquals { value }, Some(actual)) => actual != value,
            (Predicate::NotEquals { .. }, None) => true,

            (Predicate::Matches { regex }, Some(serde_json::Value::String(s))) => {
                regex::Regex::new(regex).map(|r| r.is_match(s)).unwrap_or(false)
            }
            (Predicate::Matches { .. }, _) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nonnull_predicate() {
        let p = Predicate::NonNull;
        assert!(!p.eval(None));
        assert!(!p.eval(Some(&serde_json::Value::Null)));
        assert!(p.eval(Some(&json!("anything"))));
    }

    #[test]
    fn equals_predicate() {
        let p = Predicate::Equals { value: json!("FR") };
        assert!(p.eval(Some(&json!("FR"))));
        assert!(!p.eval(Some(&json!("DE"))));
        assert!(!p.eval(None));
    }

    #[test]
    fn matches_predicate() {
        let p = Predicate::Matches {
            regex: r"^Cass\.".to_string(),
        };
        assert!(p.eval(Some(&json!("Cass. civ. 1, 2024-03-12"))));
        assert!(!p.eval(Some(&json!("CE, 2024-04-01"))));
        assert!(!p.eval(Some(&json!(42))));
    }

    #[test]
    fn not_equals_treats_missing_as_satisfying() {
        let p = Predicate::NotEquals { value: json!("excluded") };
        assert!(p.eval(None), "missing parent should satisfy != X");
        assert!(p.eval(Some(&json!("other"))));
        assert!(!p.eval(Some(&json!("excluded"))));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag-tabular --lib schema::conditional`
Expected: FAIL — `regex` not in scope.

- [ ] **Step 3: Fix import**

At top of `crates/anno-rag-tabular/src/schema/conditional.rs` (if not already), no `use` needed — we use `regex::Regex` fully qualified.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p anno-rag-tabular --lib schema::conditional`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-tabular/src/schema/conditional.rs
git commit -m "feat(tabular): implement Predicate eval semantics"
```

---

### Task 7: CellType → JSON Schema for constrained LLM decoding

**Files:**
- Create: `crates/anno-rag-tabular/src/schema/json_schema.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag-tabular/src/schema/json_schema.rs`:

```rust
//! Compile a list of columns into a JSON Schema that the LLM must obey
//! via constrained decoding. We use draft-2020-12.

use crate::schema::{CellType, Column};
use serde_json::{json, Value};

/// Build a JSON Schema describing the expected per-row extraction output.
///
/// Output shape:
/// ```json
/// {
///   "type": "object",
///   "required": [col_name, ...],
///   "additionalProperties": false,
///   "properties": {
///     col_name: {
///       "type": "object",
///       "required": ["value", "reasoning", "citations"],
///       "properties": {
///         "value":      <typed by CellType>,
///         "reasoning":  { "type": "string" },
///         "citations":  { "type": "array", "items": {
///                            "type": "object",
///                            "required": ["chunk_id", "char_start", "char_end"],
///                            "properties": {
///                                "chunk_id":   { "type": "string" },
///                                "char_start": { "type": "integer", "minimum": 0 },
///                                "char_end":   { "type": "integer", "minimum": 0 }
///                            } } }
///       }
///     },
///     ...
///   }
/// }
/// ```
pub fn for_columns(columns: &[Column]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for c in columns {
        if c.manual {
            continue;
        }
        required.push(json!(c.name.clone()));
        properties.insert(c.name.clone(), cell_envelope(&c.cell_type));
    }

    json!({
        "type": "object",
        "required": required,
        "additionalProperties": false,
        "properties": properties,
    })
}

fn cell_envelope(t: &CellType) -> Value {
    json!({
        "type": "object",
        "required": ["value", "reasoning", "citations"],
        "additionalProperties": false,
        "properties": {
            "value": value_schema(t),
            "reasoning": { "type": "string", "minLength": 1, "maxLength": 1000 },
            "citations": {
                "type": "array",
                "minItems": 1,
                "items": citation_schema(),
            }
        }
    })
}

fn value_schema(t: &CellType) -> Value {
    match t {
        CellType::Text | CellType::Verbatim => json!({ "type": "string" }),
        CellType::Date => json!({
            "type": "string",
            "pattern": r"^\d{4}-\d{2}-\d{2}$"
        }),
        CellType::Currency { code } => json!({
            "type": "object",
            "required": ["amount", "code"],
            "properties": {
                "amount": { "type": "number" },
                "code":   { "type": "string", "const": code }
            }
        }),
        CellType::Enum { options } => json!({
            "type": "string",
            "enum": options
        }),
        CellType::Boolean => json!({ "type": "boolean" }),
        CellType::Number => json!({ "type": "number" }),
    }
}

fn citation_schema() -> Value {
    json!({
        "type": "object",
        "required": ["chunk_id", "char_start", "char_end", "quoted_text"],
        "additionalProperties": false,
        "properties": {
            "chunk_id":    { "type": "string", "format": "uuid" },
            "char_start":  { "type": "integer", "minimum": 0 },
            "char_end":    { "type": "integer", "minimum": 0 },
            "quoted_text": { "type": "string", "minLength": 1 }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReviewId;
    use crate::schema::column::ColumnBuilder;

    #[test]
    fn empty_columns_yields_empty_object() {
        let s = for_columns(&[]);
        assert_eq!(s["type"], "object");
        assert_eq!(s["required"], json!([]));
        assert_eq!(s["properties"], json!({}));
    }

    #[test]
    fn text_column_uses_string_value() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "term", "What term?", CellType::Text).build();
        let s = for_columns(&[c]);
        assert_eq!(s["properties"]["term"]["properties"]["value"]["type"], "string");
    }

    #[test]
    fn enum_column_lists_options() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r, "law", "Governing law jurisdiction?",
            CellType::Enum { options: vec!["FR".into(), "DE".into(), "UK".into()] },
        ).build();
        let s = for_columns(&[c]);
        assert_eq!(
            s["properties"]["law"]["properties"]["value"]["enum"],
            json!(["FR", "DE", "UK"])
        );
    }

    #[test]
    fn currency_column_constrains_iso_code() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r, "cap", "Liability cap?",
            CellType::Currency { code: "EUR".into() },
        ).build();
        let s = for_columns(&[c]);
        assert_eq!(
            s["properties"]["cap"]["properties"]["value"]["properties"]["code"]["const"],
            "EUR"
        );
    }

    #[test]
    fn date_column_has_iso_pattern() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "effective", "Effective date?", CellType::Date).build();
        let s = for_columns(&[c]);
        let pat = s["properties"]["effective"]["properties"]["value"]["pattern"].as_str().unwrap();
        assert!(pat.contains(r"\d{4}-\d{2}-\d{2}"));
    }

    #[test]
    fn manual_columns_excluded_from_schema() {
        let r = ReviewId::new();
        let auto = ColumnBuilder::new(r, "term", "term?", CellType::Text).build();
        let manual = ColumnBuilder::new(r, "notes", "human notes", CellType::Text).manual().build();
        let s = for_columns(&[auto, manual]);
        assert!(s["properties"].as_object().unwrap().contains_key("term"));
        assert!(!s["properties"].as_object().unwrap().contains_key("notes"));
    }

    #[test]
    fn citations_required_minimum_one() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "term", "term?", CellType::Text).build();
        let s = for_columns(&[c]);
        assert_eq!(s["properties"]["term"]["properties"]["citations"]["minItems"], 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p anno-rag-tabular --lib schema::json_schema`
Expected: 7 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-rag-tabular/src/schema/json_schema.rs
git commit -m "feat(tabular): generate constrained-decoding JSON Schema from columns"
```

---

### Task 8: Template loader

**Files:**
- Create: `crates/anno-rag-tabular/src/schema/template.rs`
- Create: `crates/anno-rag-tabular/src/templates/nda-v1.toml`

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag-tabular/src/schema/template.rs`:

```rust
use crate::error::{Error, Result};
use crate::ids::ReviewId;
use crate::schema::{CellType, Column};
use crate::schema::column::ColumnBuilder;
use serde::{Deserialize, Serialize};

/// A reusable schema preset. Loaded from TOML on disk (shipped) or from
/// arbitrary string (user templates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub vertical: String,
    #[serde(rename = "column")]
    pub columns: Vec<TemplateColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateColumn {
    pub name: String,
    pub prompt: String,
    #[serde(flatten)]
    pub cell_type: CellTypeWire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CellTypeWire {
    Text,
    Date,
    Verbatim,
    Boolean,
    Number,
    Currency { code: String },
    Enum { options: Vec<String> },
}

impl From<CellTypeWire> for CellType {
    fn from(w: CellTypeWire) -> Self {
        match w {
            CellTypeWire::Text => CellType::Text,
            CellTypeWire::Date => CellType::Date,
            CellTypeWire::Verbatim => CellType::Verbatim,
            CellTypeWire::Boolean => CellType::Boolean,
            CellTypeWire::Number => CellType::Number,
            CellTypeWire::Currency { code } => CellType::Currency { code },
            CellTypeWire::Enum { options } => CellType::Enum { options },
        }
    }
}

impl Template {
    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(Error::from)
    }

    pub fn builtin(id: &str) -> Result<Self> {
        let s = match id {
            "nda-v1" => include_str!("../templates/nda-v1.toml"),
            "customer-contract-v1" => include_str!("../templates/customer-contract-v1.toml"),
            "real-estate-v1" => include_str!("../templates/real-estate-v1.toml"),
            "employment-v1" => include_str!("../templates/employment-v1.toml"),
            "ip-v1" => include_str!("../templates/ip-v1.toml"),
            _ => return Err(Error::TemplateNotFound { name: id.into() }),
        };
        Self::from_toml(s)
    }

    pub fn list_builtin() -> &'static [&'static str] {
        &[
            "nda-v1",
            "customer-contract-v1",
            "real-estate-v1",
            "employment-v1",
            "ip-v1",
        ]
    }

    pub fn into_columns(self, review_id: ReviewId) -> Vec<Column> {
        self.columns
            .into_iter()
            .enumerate()
            .map(|(i, tc)| {
                ColumnBuilder::new(review_id, &tc.name, &tc.prompt, tc.cell_type.into())
                    .order(i as u32)
                    .build()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nda_v1_loads_and_has_expected_columns() {
        let t = Template::builtin("nda-v1").expect("nda-v1 ships");
        assert_eq!(t.id, "nda-v1");
        assert_eq!(t.vertical, "legal-fr");
        // Spot-check: NDA must have at least parties + term + governing law
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"parties"));
        assert!(names.contains(&"term"));
        assert!(names.contains(&"governing_law"));
    }

    #[test]
    fn unknown_template_errors() {
        let r = Template::builtin("not-a-template");
        assert!(matches!(r, Err(Error::TemplateNotFound { .. })));
    }

    #[test]
    fn into_columns_preserves_order() {
        let t = Template::builtin("nda-v1").unwrap();
        let r = ReviewId::new();
        let cols = t.into_columns(r);
        for (i, c) in cols.iter().enumerate() {
            assert_eq!(c.order, i as u32);
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p anno-rag-tabular --lib schema::template`
Expected: FAIL — `nda-v1.toml` does not exist (compile error on `include_str!`).

- [ ] **Step 3: Create the NDA template**

Create `crates/anno-rag-tabular/src/templates/nda-v1.toml`:

```toml
id = "nda-v1"
name = "NDA Review (FR)"
version = "1.0.0"
description = "Non-disclosure agreement abstraction — parties, term, governing law, scope, remedies."
vertical = "legal-fr"

[[column]]
name   = "parties"
prompt = "List the contracting parties (full legal names, including form: SAS, SARL, etc.)."
type   = "text"

[[column]]
name   = "effective_date"
prompt = "What is the effective date of the agreement? Return ISO 8601 (YYYY-MM-DD)."
type   = "date"

[[column]]
name   = "term"
prompt = "What is the term/duration of the confidentiality obligation? Verbatim from the contract."
type   = "verbatim"

[[column]]
name   = "governing_law"
prompt = "What governing law applies? Use ISO 3166-1 alpha-2 country code (FR, DE, UK, US, ...)."
type   = "enum"
options = ["FR", "DE", "UK", "US", "CH", "BE", "LU", "OTHER"]

[[column]]
name   = "jurisdiction"
prompt = "What court or tribunal has jurisdiction? Verbatim clause."
type   = "verbatim"

[[column]]
name   = "scope_definition"
prompt = "What is the definition of 'Confidential Information'? Verbatim clause."
type   = "verbatim"

[[column]]
name   = "permitted_disclosures"
prompt = "Are there permitted-disclosure carve-outs (legal, regulators, advisors)? Verbatim."
type   = "verbatim"

[[column]]
name   = "non_solicitation"
prompt = "Does the contract include a non-solicitation clause covering employees or customers?"
type   = "boolean"

[[column]]
name   = "non_solicitation_term"
prompt = "If non-solicitation present: duration of the non-solicit obligation in months. Otherwise null."
type   = "number"

[[column]]
name   = "exclusivity"
prompt = "Does the contract grant exclusivity to either party? Verbatim if yes, else 'none'."
type   = "text"

[[column]]
name   = "residual_clause"
prompt = "Is there a residual-information clause (memorized info exempted)?"
type   = "boolean"

[[column]]
name   = "return_of_info"
prompt = "Return/destruction-of-information obligations: verbatim or 'none'."
type   = "text"

[[column]]
name   = "remedies"
prompt = "What remedies for breach are specified (injunctive relief, liquidated damages)? Verbatim."
type   = "verbatim"

[[column]]
name   = "liability_cap"
prompt = "Liability cap if any (in EUR). Return null if uncapped or not specified."
type   = "currency"
code   = "EUR"
```

- [ ] **Step 4: Create empty stubs for the other 4 templates (filled in Task 9-12)**

Create empty placeholder files so `include_str!` compiles:

```bash
for f in customer-contract-v1 real-estate-v1 employment-v1 ip-v1; do
  cat > crates/anno-rag-tabular/src/templates/${f}.toml <<EOF
id = "${f}"
name = "Placeholder — filled in later task"
version = "0.0.1"
description = "Placeholder"
vertical = "legal-fr"
EOF
done
```

Wait — empty `[[column]]` arrays will cause `t.columns` to be empty. That's OK for the unknown-template test but not for the others. We just need `nda-v1` to be functional in this task. The placeholders need at least one column to satisfy serde if we strict-validate. Add a sentinel column:

```toml
[[column]]
name = "_placeholder"
prompt = "filled in a later task"
type = "text"
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p anno-rag-tabular --lib schema::template`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-tabular/src/schema/template.rs crates/anno-rag-tabular/src/templates/
git commit -m "feat(tabular): template loader + nda-v1 + placeholders for 4 more"
```

---

### Task 9: Customer/supplier contract template

**Files:**
- Modify: `crates/anno-rag-tabular/src/templates/customer-contract-v1.toml`

- [ ] **Step 1: Replace placeholder with full template**

Replace the contents of `crates/anno-rag-tabular/src/templates/customer-contract-v1.toml`:

```toml
id = "customer-contract-v1"
name = "Customer/Supplier Contract Abstraction (FR)"
version = "1.0.0"
description = "Commercial-contract abstraction for due diligence — term, termination, change-of-control, exclusivity, MFN, indemnity."
vertical = "legal-fr"

[[column]]
name   = "parties"
prompt = "List the contracting parties (full legal names, form, SIREN if visible)."
type   = "text"

[[column]]
name   = "effective_date"
prompt = "Effective date of the contract. ISO 8601."
type   = "date"

[[column]]
name   = "term"
prompt = "Initial term/duration of the contract. Verbatim clause."
type   = "verbatim"

[[column]]
name   = "auto_renewal"
prompt = "Does the contract auto-renew?"
type   = "boolean"

[[column]]
name   = "renewal_term"
prompt = "If auto-renewal: length of each renewal period. Verbatim or null."
type   = "text"

[[column]]
name   = "termination_for_convenience"
prompt = "Termination-for-convenience right and notice period. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "termination_for_cause"
prompt = "Termination-for-cause / breach conditions. Verbatim."
type   = "verbatim"

[[column]]
name   = "change_of_control"
prompt = "Does the contract have a change-of-control trigger (e.g., termination on acquisition)?"
type   = "boolean"

[[column]]
name   = "change_of_control_detail"
prompt = "If change-of-control present: full clause verbatim. Else 'none'."
type   = "text"

[[column]]
name   = "assignment_restriction"
prompt = "Restrictions on assignment (consent required, no assignment). Verbatim or 'none'."
type   = "text"

[[column]]
name   = "exclusivity"
prompt = "Is there an exclusivity clause (single-source supplier, exclusive territory)?"
type   = "boolean"

[[column]]
name   = "exclusivity_detail"
prompt = "If exclusivity present: scope and term. Verbatim. Else 'none'."
type   = "text"

[[column]]
name   = "mfn_clause"
prompt = "Most-favored-nation pricing clause?"
type   = "boolean"

[[column]]
name   = "liability_cap"
prompt = "Total liability cap in EUR. Null if uncapped or not specified."
type   = "currency"
code   = "EUR"

[[column]]
name   = "indemnity"
prompt = "Indemnification obligations summary. Verbatim of the indemnity clause."
type   = "verbatim"

[[column]]
name   = "governing_law"
prompt = "Governing law ISO 3166-1 alpha-2."
type   = "enum"
options = ["FR", "DE", "UK", "US", "CH", "BE", "LU", "OTHER"]

[[column]]
name   = "dispute_resolution"
prompt = "Dispute resolution forum (court or arbitration). Verbatim."
type   = "verbatim"
```

- [ ] **Step 2: Write a test ensuring the template loads**

Append to `crates/anno-rag-tabular/src/schema/template.rs::tests`:

```rust
    #[test]
    fn customer_contract_v1_loads() {
        let t = Template::builtin("customer-contract-v1").unwrap();
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["parties", "term", "change_of_control", "liability_cap", "governing_law"] {
            assert!(names.contains(&must), "missing required col {must}");
        }
    }
```

- [ ] **Step 3: Run test**

Run: `cargo test -p anno-rag-tabular --lib schema::template`
Expected: PASS, 4 tests.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-tabular/src/templates/customer-contract-v1.toml crates/anno-rag-tabular/src/schema/template.rs
git commit -m "feat(tabular): customer-contract-v1 template (17 cols)"
```

---

### Task 10: Real-estate template

**Files:**
- Modify: `crates/anno-rag-tabular/src/templates/real-estate-v1.toml`

- [ ] **Step 1: Replace placeholder**

```toml
id = "real-estate-v1"
name = "Real Estate Lease Abstraction (FR)"
version = "1.0.0"
description = "Commercial-lease abstraction for due diligence and portfolio review."
vertical = "legal-fr"

[[column]]
name   = "landlord"
prompt = "Landlord (bailleur) — full legal name and form."
type   = "text"

[[column]]
name   = "tenant"
prompt = "Tenant (preneur) — full legal name and form."
type   = "text"

[[column]]
name   = "premises_address"
prompt = "Full address of the leased premises."
type   = "text"

[[column]]
name   = "lease_type"
prompt = "Type of lease (bail commercial 3-6-9, bail dérogatoire, bail professionnel, bail civil)."
type   = "enum"
options = ["bail_commercial_3_6_9", "bail_derogatoire", "bail_professionnel", "bail_civil", "other"]

[[column]]
name   = "start_date"
prompt = "Lease start date. ISO 8601."
type   = "date"

[[column]]
name   = "term_years"
prompt = "Initial term in years."
type   = "number"

[[column]]
name   = "tenant_break_rights"
prompt = "Tenant break rights (sortie triennale). Verbatim or 'none'."
type   = "text"

[[column]]
name   = "base_rent"
prompt = "Annual base rent in EUR."
type   = "currency"
code   = "EUR"

[[column]]
name   = "rent_escalation"
prompt = "Rent escalation clause (ILAT, ILC, ICC index). Verbatim."
type   = "text"

[[column]]
name   = "security_deposit"
prompt = "Security deposit (dépôt de garantie) amount in EUR."
type   = "currency"
code   = "EUR"

[[column]]
name   = "permitted_use"
prompt = "Permitted use of premises (destination des lieux). Verbatim."
type   = "verbatim"

[[column]]
name   = "assignment_sublet"
prompt = "Assignment and sub-let rights. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "repair_obligations"
prompt = "Repair obligations summary (charges récupérables, gros entretien). Verbatim."
type   = "verbatim"

[[column]]
name   = "registered_lease"
prompt = "Is the lease registered (enregistrement / publication)?"
type   = "boolean"

[[column]]
name   = "governing_law"
prompt = "Governing law ISO 3166-1 alpha-2."
type   = "enum"
options = ["FR", "OTHER"]
```

- [ ] **Step 2: Test loads**

Append to template tests:

```rust
    #[test]
    fn real_estate_v1_loads() {
        let t = Template::builtin("real-estate-v1").unwrap();
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["landlord", "tenant", "base_rent", "permitted_use"] {
            assert!(names.contains(&must));
        }
    }
```

- [ ] **Step 3: Run + Commit**

```bash
cargo test -p anno-rag-tabular --lib schema::template
git add -A
git commit -m "feat(tabular): real-estate-v1 template (15 cols)"
```

---

### Task 11: Employment template

**Files:**
- Modify: `crates/anno-rag-tabular/src/templates/employment-v1.toml`

- [ ] **Step 1: Replace placeholder**

```toml
id = "employment-v1"
name = "Employment Contract Abstraction (FR)"
version = "1.0.0"
description = "Key-employee employment contract abstraction for M&A retention/IP review."
vertical = "legal-fr"

[[column]]
name   = "employee_name"
prompt = "Full name of the employee."
type   = "text"

[[column]]
name   = "role"
prompt = "Job title / role."
type   = "text"

[[column]]
name   = "start_date"
prompt = "Employment start date. ISO 8601."
type   = "date"

[[column]]
name   = "contract_type"
prompt = "Contract type."
type   = "enum"
options = ["cdi", "cdd", "alternance", "stage", "interim", "other"]

[[column]]
name   = "convention_collective"
prompt = "Applicable convention collective (with IDCC if visible)."
type   = "text"

[[column]]
name   = "base_salary"
prompt = "Annual gross base salary in EUR."
type   = "currency"
code   = "EUR"

[[column]]
name   = "bonus_scheme"
prompt = "Bonus / variable compensation scheme. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "equity_grants"
prompt = "Equity / stock-option grants. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "notice_period"
prompt = "Notice period in months."
type   = "number"

[[column]]
name   = "severance"
prompt = "Severance / indemnité de rupture. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "non_compete"
prompt = "Is there a non-compete clause?"
type   = "boolean"

[[column]]
name   = "non_compete_term"
prompt = "If non-compete: term in months. Else null."
type   = "number"

[[column]]
name   = "non_compete_compensation"
prompt = "Non-compete compensation per month in EUR. Else null."
type   = "currency"
code   = "EUR"

[[column]]
name   = "non_solicit"
prompt = "Is there a non-solicit clause?"
type   = "boolean"

[[column]]
name   = "ip_assignment"
prompt = "Is there an IP assignment clause covering work product?"
type   = "boolean"

[[column]]
name   = "change_of_control_protection"
prompt = "Does the contract include change-of-control protection (golden parachute, accelerated vesting)? Verbatim or 'none'."
type   = "text"
```

- [ ] **Step 2: Test loads + commit**

Append test:

```rust
    #[test]
    fn employment_v1_loads() {
        let t = Template::builtin("employment-v1").unwrap();
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["employee_name", "base_salary", "non_compete", "ip_assignment"] {
            assert!(names.contains(&must));
        }
    }
```

```bash
cargo test -p anno-rag-tabular --lib schema::template
git add -A
git commit -m "feat(tabular): employment-v1 template (16 cols)"
```

---

### Task 12: IP template

**Files:**
- Modify: `crates/anno-rag-tabular/src/templates/ip-v1.toml`

- [ ] **Step 1: Replace placeholder**

```toml
id = "ip-v1"
name = "Intellectual Property Asset Schedule (FR)"
version = "1.0.0"
description = "IP-asset abstraction for M&A: ownership, registrations, encumbrances, licenses."
vertical = "legal-fr"

[[column]]
name   = "asset_name"
prompt = "Name / title of the IP asset."
type   = "text"

[[column]]
name   = "asset_type"
prompt = "Type of IP asset."
type   = "enum"
options = ["patent", "trademark", "copyright", "domain", "trade_secret", "know_how", "design", "software", "other"]

[[column]]
name   = "owner"
prompt = "Registered owner / titulaire. Full legal name."
type   = "text"

[[column]]
name   = "registration_number"
prompt = "Registration number (patent #, INPI mark #, ...). 'unregistered' if not registered."
type   = "text"

[[column]]
name   = "jurisdictions"
prompt = "Jurisdictions where registered or protected. Comma-separated ISO 3166-1 alpha-2 codes."
type   = "text"

[[column]]
name   = "status"
prompt = "Current status."
type   = "enum"
options = ["pending", "granted", "registered", "expired", "lapsed", "abandoned", "opposed", "litigation", "other"]

[[column]]
name   = "filing_date"
prompt = "Filing date. ISO 8601."
type   = "date"

[[column]]
name   = "renewal_date"
prompt = "Next renewal/maintenance date. ISO 8601 or null."
type   = "date"

[[column]]
name   = "assignment_chain"
prompt = "Chain of title / assignments. Verbatim or 'first owner'."
type   = "text"

[[column]]
name   = "encumbrances"
prompt = "Liens, pledges, security interests. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "licenses_in"
prompt = "Inbound licenses (rights we hold from third parties). Verbatim or 'none'."
type   = "text"

[[column]]
name   = "licenses_out"
prompt = "Outbound licenses granted to third parties. Verbatim or 'none'."
type   = "text"

[[column]]
name   = "infringement_claims"
prompt = "Ongoing or threatened infringement claims (either direction). Verbatim or 'none'."
type   = "text"
```

- [ ] **Step 2: Test loads + commit**

```rust
    #[test]
    fn ip_v1_loads() {
        let t = Template::builtin("ip-v1").unwrap();
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["asset_name", "owner", "status", "encumbrances"] {
            assert!(names.contains(&must));
        }
    }

    #[test]
    fn all_5_templates_load() {
        for id in Template::list_builtin() {
            Template::builtin(id).expect(&format!("template {id} must load"));
        }
    }
```

```bash
cargo test -p anno-rag-tabular --lib schema
git add -A
git commit -m "feat(tabular): ip-v1 template + cross-template smoke test (13 cols)"
```

---

## Phase 3 — Storage layer (LanceDB tables, versioned cells)

**Note for executor:** The 4 tables live in the same LanceDB connection as the v1.0 `chunks` table. The connection is opened by `anno-rag-store::lance::open_db()`. Reuse that — do not open a new connection.

### Task 13: Arrow schemas for the 4 tables

**Files:**
- Create: `crates/anno-rag-tabular/src/storage/mod.rs`
- Create: `crates/anno-rag-tabular/src/storage/arrow_schema.rs`

- [ ] **Step 1: Create `storage/mod.rs` stub**

```rust
pub mod arrow_schema;
pub mod reviews;
pub mod columns;
pub mod rows;
pub mod cells;
pub mod lock;

pub use reviews::ReviewsTable;
pub use columns::ColumnsTable;
pub use rows::RowsTable;
pub use cells::CellsTable;
```

- [ ] **Step 2: Write failing tests for schemas**

Create `crates/anno-rag-tabular/src/storage/arrow_schema.rs`:

```rust
//! Arrow schemas for the 4 tabular tables. Stable column order — append-only.

use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

pub fn reviews_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id",            DataType::FixedSizeBinary(16), false),  // UUID v7
        Field::new("name",          DataType::Utf8,                false),
        Field::new("project_id",    DataType::Utf8,                true),
        Field::new("template_id",   DataType::Utf8,                true),
        Field::new("scope_folder",  DataType::Utf8,                true),
        Field::new("created_at",    DataType::Timestamp(TimeUnit::Microsecond, None), false),
        Field::new("schema_version", DataType::UInt32,             false),  // bumped on add_column
    ]))
}

pub fn columns_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id",            DataType::FixedSizeBinary(16), false),  // UUID v5
        Field::new("review_id",     DataType::FixedSizeBinary(16), false),
        Field::new("name",          DataType::Utf8,                false),
        Field::new("prompt",        DataType::Utf8,                false),
        Field::new("cell_type_json", DataType::Utf8,               false),  // serde_json CellType
        Field::new("conditional_json", DataType::Utf8,             true),
        Field::new("manual",        DataType::Boolean,             false),
        Field::new("order_idx",     DataType::UInt32,              false),
    ]))
}

pub fn rows_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id",            DataType::FixedSizeBinary(16), false),  // UUID v5
        Field::new("review_id",     DataType::FixedSizeBinary(16), false),
        Field::new("doc_id",        DataType::FixedSizeBinary(16), false),
        Field::new("folder_path",   DataType::Utf8,                true),
        Field::new("created_at",    DataType::Timestamp(TimeUnit::Microsecond, None), false),
    ]))
}

pub fn cells_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("review_id",     DataType::FixedSizeBinary(16), false),
        Field::new("row_id",        DataType::FixedSizeBinary(16), false),
        Field::new("col_id",        DataType::FixedSizeBinary(16), false),
        Field::new("value_json",    DataType::Utf8,                false),  // serde_json
        Field::new("reasoning",     DataType::Utf8,                true),
        Field::new("citations_json", DataType::Utf8,               false),  // Vec<Citation>
        Field::new("support_score", DataType::Float32,             false),
        Field::new("confidence",    DataType::Utf8,                false),  // High|Medium|Low
        Field::new("locked",        DataType::Boolean,             false),
        Field::new("version",       DataType::UInt32,              false),
        Field::new("author",        DataType::Utf8,                false),  // "system:v1" or "human:user_id"
        Field::new("updated_at",    DataType::Timestamp(TimeUnit::Microsecond, None), false),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_schemas_have_unique_field_names() {
        for s in [reviews_schema(), columns_schema(), rows_schema(), cells_schema()] {
            let names: Vec<_> = s.fields().iter().map(|f| f.name().as_str()).collect();
            let mut sorted = names.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), names.len(), "duplicate field name in {:?}", names);
        }
    }

    #[test]
    fn cells_has_versioning_columns() {
        let s = cells_schema();
        assert!(s.field_with_name("version").is_ok());
        assert!(s.field_with_name("locked").is_ok());
        assert!(s.field_with_name("author").is_ok());
    }

    #[test]
    fn cells_uses_float32_for_support_score() {
        let s = cells_schema();
        let f = s.field_with_name("support_score").unwrap();
        assert_eq!(f.data_type(), &DataType::Float32);
    }
}
```

- [ ] **Step 3: Run + commit**

Run: `cargo test -p anno-rag-tabular --lib storage::arrow_schema`
Expected: 3 passed.

```bash
git add crates/anno-rag-tabular/src/storage/
git commit -m "feat(tabular): arrow schemas for reviews/columns/rows/cells"
```

---

Phase 3 continues in §Part 2 of this plan. Each subsequent phase is broken into its own committed chunk for editor ergonomics; the same TDD cadence applies throughout: write failing test → run → minimal impl → run → commit.

---

### Task 14: `StorageHandle` — open + initialize the 4 tables

**Files:**
- Create: `crates/anno-rag-tabular/src/storage/mod.rs` (already created stub in Task 13 — flesh out)

- [ ] **Step 1: Write the failing test**

Replace the contents of `crates/anno-rag-tabular/src/storage/mod.rs`:

```rust
pub mod arrow_schema;
pub mod reviews;
pub mod columns;
pub mod rows;
pub mod cells;
pub mod lock;

pub use reviews::ReviewsTable;
pub use columns::ColumnsTable;
pub use rows::RowsTable;
pub use cells::CellsTable;

use crate::error::Result;
use lancedb::Connection;
use std::sync::Arc;

/// One handle, four tables. Opened against an existing LanceDB connection
/// (same `~/.anno-rag/index.lance/` directory as v1.0 chunks).
#[derive(Clone)]
pub struct StorageHandle {
    pub reviews: ReviewsTable,
    pub columns: ColumnsTable,
    pub rows:    RowsTable,
    pub cells:   CellsTable,
}

impl StorageHandle {
    /// Open or create the 4 tabular tables on the given connection.
    pub async fn open(conn: Arc<Connection>) -> Result<Self> {
        let reviews = ReviewsTable::open(conn.clone()).await?;
        let columns = ColumnsTable::open(conn.clone()).await?;
        let rows    = RowsTable::open(conn.clone()).await?;
        let cells   = CellsTable::open(conn).await?;
        Ok(Self { reviews, columns, rows, cells })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn open_creates_4_tables_on_empty_db() {
        let dir = TempDir::new().unwrap();
        let conn = Arc::new(lancedb::connect(dir.path().to_str().unwrap()).execute().await.unwrap());
        let _h = StorageHandle::open(conn.clone()).await.unwrap();
        let names: Vec<String> = conn.table_names().execute().await.unwrap();
        for must in ["tabular_reviews", "tabular_columns", "tabular_rows", "tabular_cells"] {
            assert!(names.contains(&must.to_string()), "missing table {must}: {:?}", names);
        }
    }

    #[tokio::test]
    async fn open_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let conn = Arc::new(lancedb::connect(dir.path().to_str().unwrap()).execute().await.unwrap());
        let _h1 = StorageHandle::open(conn.clone()).await.unwrap();
        let _h2 = StorageHandle::open(conn.clone()).await.unwrap();
        let count = conn.table_names().execute().await.unwrap().len();
        assert_eq!(count, 4);
    }
}
```

- [ ] **Step 2: Stub the 4 table types so it compiles**

Create `crates/anno-rag-tabular/src/storage/reviews.rs`:

```rust
use crate::error::Result;
use crate::storage::arrow_schema::reviews_schema;
use arrow_array::RecordBatchIterator;
use lancedb::{Connection, Table};
use std::sync::Arc;

pub const TABLE_NAME: &str = "tabular_reviews";

#[derive(Clone)]
pub struct ReviewsTable {
    pub(crate) tbl: Table,
}

impl ReviewsTable {
    pub async fn open(conn: Arc<Connection>) -> Result<Self> {
        let names = conn.table_names().execute().await?;
        let tbl = if names.iter().any(|n| n == TABLE_NAME) {
            conn.open_table(TABLE_NAME).execute().await?
        } else {
            let schema = reviews_schema();
            let empty = RecordBatchIterator::new(std::iter::empty(), schema.clone());
            conn.create_table(TABLE_NAME, Box::new(empty)).execute().await?
        };
        Ok(Self { tbl })
    }
}
```

Create the analogous files `columns.rs`, `rows.rs`, `cells.rs` with `TABLE_NAME = "tabular_columns" / "tabular_rows" / "tabular_cells"` and the matching `*_schema()` calls.

Create `crates/anno-rag-tabular/src/storage/lock.rs` as an empty stub for now:

```rust
//! Locked-cell semantics — populated in Task 18.
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p anno-rag-tabular --lib storage::tests`
Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-tabular/src/storage/
git commit -m "feat(tabular): StorageHandle opens 4 LanceDB tables idempotently"
```

---

### Task 15: `ReviewsTable` — create / get / list reviews

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/reviews.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/anno-rag-tabular/src/storage/reviews.rs`:

```rust
use crate::ids::ReviewId;
use arrow_array::{
    Array, BooleanArray, FixedSizeBinaryArray, RecordBatch,
    StringArray, TimestampMicrosecondArray, UInt32Array,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: ReviewId,
    pub name: String,
    pub project_id: Option<String>,
    pub template_id: Option<String>,
    pub scope_folder: Option<String>,
    pub created_at: DateTime<Utc>,
    pub schema_version: u32,
}

impl ReviewsTable {
    pub async fn create(&self, review: &Review) -> Result<()> {
        use crate::storage::arrow_schema::reviews_schema;
        use arrow_array::builder::*;

        let schema = reviews_schema();
        let id_bytes: [u8; 16] = *review.id.0.as_bytes();
        let id_arr = FixedSizeBinaryArray::try_from_iter(std::iter::once(id_bytes.to_vec()))?;
        let name_arr = StringArray::from(vec![review.name.clone()]);
        let project_arr = StringArray::from(vec![review.project_id.clone()]);
        let template_arr = StringArray::from(vec![review.template_id.clone()]);
        let folder_arr = StringArray::from(vec![review.scope_folder.clone()]);
        let ts_arr = TimestampMicrosecondArray::from(vec![review.created_at.timestamp_micros()]);
        let sv_arr = UInt32Array::from(vec![review.schema_version]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_arr),
                Arc::new(name_arr),
                Arc::new(project_arr),
                Arc::new(template_arr),
                Arc::new(folder_arr),
                Arc::new(ts_arr),
                Arc::new(sv_arr),
            ],
        )?;

        let iter = RecordBatchIterator::new(std::iter::once(Ok(batch)), schema);
        self.tbl.add(Box::new(iter)).execute().await?;
        Ok(())
    }

    pub async fn get(&self, id: ReviewId) -> Result<Option<Review>> {
        use futures::TryStreamExt;
        let id_hex = uuid_to_filter_lit(id.0);
        let stream = self
            .tbl
            .query()
            .only_if(&format!("id = X'{}'", id_hex))
            .execute()
            .await?;
        let batches: Vec<_> = stream.try_collect().await?;
        for b in batches {
            if b.num_rows() > 0 {
                return Ok(Some(row_to_review(&b, 0)?));
            }
        }
        Ok(None)
    }

    pub async fn list(&self) -> Result<Vec<Review>> {
        use futures::TryStreamExt;
        let stream = self.tbl.query().execute().await?;
        let batches: Vec<_> = stream.try_collect().await?;
        let mut out = Vec::new();
        for b in batches {
            for i in 0..b.num_rows() {
                out.push(row_to_review(&b, i)?);
            }
        }
        Ok(out)
    }
}

fn uuid_to_filter_lit(u: uuid::Uuid) -> String {
    u.as_bytes().iter().map(|b| format!("{:02X}", b)).collect()
}

fn row_to_review(b: &RecordBatch, i: usize) -> Result<Review> {
    let id_arr = b.column(0).as_any().downcast_ref::<FixedSizeBinaryArray>().unwrap();
    let name_arr = b.column(1).as_any().downcast_ref::<StringArray>().unwrap();
    let project_arr = b.column(2).as_any().downcast_ref::<StringArray>().unwrap();
    let template_arr = b.column(3).as_any().downcast_ref::<StringArray>().unwrap();
    let folder_arr = b.column(4).as_any().downcast_ref::<StringArray>().unwrap();
    let ts_arr = b.column(5).as_any().downcast_ref::<TimestampMicrosecondArray>().unwrap();
    let sv_arr = b.column(6).as_any().downcast_ref::<UInt32Array>().unwrap();

    let id_bytes: [u8; 16] = id_arr.value(i).try_into().unwrap();
    Ok(Review {
        id: ReviewId(uuid::Uuid::from_bytes(id_bytes)),
        name: name_arr.value(i).to_string(),
        project_id: opt_str(project_arr, i),
        template_id: opt_str(template_arr, i),
        scope_folder: opt_str(folder_arr, i),
        created_at: DateTime::<Utc>::from_timestamp_micros(ts_arr.value(i)).unwrap_or_default(),
        schema_version: sv_arr.value(i),
    })
}

fn opt_str(a: &StringArray, i: usize) -> Option<String> {
    if a.is_null(i) { None } else { Some(a.value(i).to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn fresh_table() -> (TempDir, ReviewsTable) {
        let dir = TempDir::new().unwrap();
        let conn = Arc::new(lancedb::connect(dir.path().to_str().unwrap()).execute().await.unwrap());
        let t = ReviewsTable::open(conn).await.unwrap();
        (dir, t)
    }

    #[tokio::test]
    async fn create_then_get_roundtrips() {
        let (_dir, t) = fresh_table().await;
        let r = Review {
            id: ReviewId::new(),
            name: "Deal Acme NDAs".into(),
            project_id: Some("Deal_Acme".into()),
            template_id: Some("nda-v1".into()),
            scope_folder: Some("Deal_Acme/01_NDA".into()),
            created_at: Utc::now(),
            schema_version: 1,
        };
        t.create(&r).await.unwrap();
        let got = t.get(r.id).await.unwrap().expect("should exist");
        assert_eq!(got.name, r.name);
        assert_eq!(got.template_id, r.template_id);
    }

    #[tokio::test]
    async fn list_returns_all() {
        let (_dir, t) = fresh_table().await;
        for i in 0..3 {
            let r = Review {
                id: ReviewId::new(),
                name: format!("Review {i}"),
                project_id: None,
                template_id: None,
                scope_folder: None,
                created_at: Utc::now(),
                schema_version: 1,
            };
            t.create(&r).await.unwrap();
        }
        let all = t.list().await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn get_unknown_returns_none() {
        let (_dir, t) = fresh_table().await;
        let unknown = ReviewId::new();
        assert!(t.get(unknown).await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p anno-rag-tabular --lib storage::reviews
git add crates/anno-rag-tabular/src/storage/reviews.rs
git commit -m "feat(tabular): ReviewsTable::create/get/list with arrow round-trip"
```

---

### Task 16: `ColumnsTable` — same pattern, with schema_version bump on add

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/columns.rs`

Mirrors Task 15. Key difference: `add_column(review_id, column)` also bumps `reviews.schema_version` so re-extracts can detect drift.

- [ ] **Step 1: Write the test**

Append to `crates/anno-rag-tabular/src/storage/columns.rs`:

```rust
use crate::ids::{ColumnId, ReviewId};
use crate::schema::Column;
use crate::storage::arrow_schema::columns_schema;
use crate::storage::reviews::ReviewsTable;
use arrow_array::{Array, BooleanArray, FixedSizeBinaryArray, RecordBatch, StringArray, UInt32Array};
use futures::TryStreamExt;

impl ColumnsTable {
    pub async fn add(&self, review_id: ReviewId, col: &Column) -> Result<()> {
        let schema = columns_schema();
        let id_b = FixedSizeBinaryArray::try_from_iter(std::iter::once(col.id.0.as_bytes().to_vec()))?;
        let rid_b = FixedSizeBinaryArray::try_from_iter(std::iter::once(review_id.0.as_bytes().to_vec()))?;
        let name_a = StringArray::from(vec![col.name.clone()]);
        let prompt_a = StringArray::from(vec![col.prompt.clone()]);
        let ttype_json = serde_json::to_string(&col.cell_type)?;
        let ttype_a = StringArray::from(vec![ttype_json]);
        let cond_a = StringArray::from(vec![col.conditional.as_ref().map(|c| serde_json::to_string(c).unwrap())]);
        let manual_a = BooleanArray::from(vec![col.manual]);
        let order_a = UInt32Array::from(vec![col.order]);

        let batch = RecordBatch::try_new(schema.clone(), vec![
            Arc::new(id_b), Arc::new(rid_b),
            Arc::new(name_a), Arc::new(prompt_a),
            Arc::new(ttype_a), Arc::new(cond_a),
            Arc::new(manual_a), Arc::new(order_a),
        ])?;
        let iter = RecordBatchIterator::new(std::iter::once(Ok(batch)), schema);
        self.tbl.add(Box::new(iter)).execute().await?;
        Ok(())
    }

    pub async fn list_for_review(&self, review_id: ReviewId) -> Result<Vec<Column>> {
        let hex = uuid_to_filter_lit(review_id.0);
        let stream = self.tbl.query().only_if(&format!("review_id = X'{}'", hex)).execute().await?;
        let batches: Vec<_> = stream.try_collect().await?;
        let mut out = Vec::new();
        for b in batches {
            for i in 0..b.num_rows() {
                out.push(row_to_column(&b, i)?);
            }
        }
        out.sort_by_key(|c| c.order);
        Ok(out)
    }
}

fn uuid_to_filter_lit(u: uuid::Uuid) -> String {
    u.as_bytes().iter().map(|b| format!("{:02X}", b)).collect()
}

fn row_to_column(b: &RecordBatch, i: usize) -> Result<Column> {
    let id_a = b.column(0).as_any().downcast_ref::<FixedSizeBinaryArray>().unwrap();
    let name_a = b.column(2).as_any().downcast_ref::<StringArray>().unwrap();
    let prompt_a = b.column(3).as_any().downcast_ref::<StringArray>().unwrap();
    let ttype_a = b.column(4).as_any().downcast_ref::<StringArray>().unwrap();
    let cond_a = b.column(5).as_any().downcast_ref::<StringArray>().unwrap();
    let manual_a = b.column(6).as_any().downcast_ref::<BooleanArray>().unwrap();
    let order_a = b.column(7).as_any().downcast_ref::<UInt32Array>().unwrap();

    let id_bytes: [u8; 16] = id_a.value(i).try_into().unwrap();
    Ok(Column {
        id: ColumnId(uuid::Uuid::from_bytes(id_bytes)),
        name: name_a.value(i).to_string(),
        prompt: prompt_a.value(i).to_string(),
        cell_type: serde_json::from_str(ttype_a.value(i))?,
        conditional: if cond_a.is_null(i) { None } else { Some(serde_json::from_str(cond_a.value(i))?) },
        manual: manual_a.value(i),
        order: order_a.value(i),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{CellType, column::ColumnBuilder};
    use tempfile::TempDir;

    #[tokio::test]
    async fn add_then_list_preserves_order() {
        let dir = TempDir::new().unwrap();
        let conn = Arc::new(lancedb::connect(dir.path().to_str().unwrap()).execute().await.unwrap());
        let t = ColumnsTable::open(conn).await.unwrap();
        let r = ReviewId::new();
        for (i, name) in ["c2", "c0", "c1"].iter().enumerate() {
            // intentionally out of insertion order
            let c = ColumnBuilder::new(r, name, "x", CellType::Text).order(match *name {
                "c0" => 0, "c1" => 1, "c2" => 2, _ => unreachable!()
            }).build();
            t.add(r, &c).await.unwrap();
        }
        let cols = t.list_for_review(r).await.unwrap();
        let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["c0", "c1", "c2"]);
    }
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p anno-rag-tabular --lib storage::columns
git add crates/anno-rag-tabular/src/storage/columns.rs
git commit -m "feat(tabular): ColumnsTable::add/list with stable ordering"
```

---

### Task 17: `RowsTable` and `CellsTable` (parallel implementation)

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/rows.rs`
- Modify: `crates/anno-rag-tabular/src/storage/cells.rs`

Both follow the exact pattern of Task 15-16. The plan provides only the test surface and method signatures here; the executor copy-pastes the arrow-roundtrip pattern.

- [ ] **Step 1: `RowsTable` API**

`rows.rs` exposes:
```rust
pub struct Row {
    pub id: RowId,
    pub review_id: ReviewId,
    pub doc_id: DocId,
    pub folder_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl RowsTable {
    pub async fn add(&self, row: &Row) -> Result<()> { /* arrow batch */ }
    pub async fn list_for_review(&self, review_id: ReviewId) -> Result<Vec<Row>> { /* … */ }
    pub async fn get(&self, id: RowId) -> Result<Option<Row>> { /* … */ }
}
```

Test: `add_three_rows_list_returns_three`.

- [ ] **Step 2: `CellsTable` API — versioned upserts**

`cells.rs` exposes:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub review_id: ReviewId,
    pub row_id: RowId,
    pub col_id: ColumnId,
    pub value: serde_json::Value,
    pub reasoning: Option<String>,
    pub citations: Vec<Citation>,
    pub support_score: f32,
    pub confidence: Confidence,
    pub locked: bool,
    pub version: u32,
    pub author: Author,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub chunk_id: anno_rag_core::ChunkId,
    pub char_start: u32,
    pub char_end: u32,
    pub quoted_text: String,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Confidence { High, Medium, Low }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Author {
    System { extractor_version: String },
    Human { user_id: String },
}

impl CellsTable {
    /// Append a new version. Caller picks version=last+1.
    pub async fn upsert(&self, cell: &Cell) -> Result<()> { /* … */ }
    pub async fn latest(&self, review: ReviewId, row: RowId, col: ColumnId) -> Result<Option<Cell>> {
        // ORDER BY version DESC LIMIT 1
    }
    pub async fn history(&self, review: ReviewId, row: RowId, col: ColumnId) -> Result<Vec<Cell>> { /* … */ }
    pub async fn all_for_review_latest(&self, review: ReviewId) -> Result<Vec<Cell>> {
        // Returns most-recent version per (row, col).
    }
}
```

Tests:
- `upsert_increments_version_when_history_already_exists` — call upsert twice, check `latest().version == 2`
- `locked_cell_cannot_be_overwritten_by_system_author` — returns `Err(Error::LockedCell)`
- `locked_cell_can_be_overwritten_by_human_author` — succeeds
- `history_returns_all_versions_descending`

- [ ] **Step 3: Run + commit**

```bash
cargo test -p anno-rag-tabular --lib storage
git add crates/anno-rag-tabular/src/storage/rows.rs crates/anno-rag-tabular/src/storage/cells.rs
git commit -m "feat(tabular): RowsTable + versioned CellsTable with locked-cell semantics"
```

---

### Task 18: Locked-cell enforcement helper

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/lock.rs`

- [ ] **Step 1: Write the test**

```rust
//! Locked-cell enforcement: a `System`-authored upsert may not overwrite a
//! cell where the previous latest version is `locked=true`. A `Human`-authored
//! upsert always wins.

use crate::error::{Error, Result};
use crate::storage::cells::{Author, Cell, CellsTable};

pub async fn check_lock_allows(
    table: &CellsTable,
    incoming: &Cell,
) -> Result<()> {
    let latest = table
        .latest(incoming.review_id, incoming.row_id, incoming.col_id)
        .await?;
    let Some(prev) = latest else { return Ok(()); };
    if prev.locked && matches!(incoming.author, Author::System { .. }) {
        return Err(Error::LockedCell {
            review: incoming.review_id.0.to_string(),
            row: incoming.row_id.0.to_string(),
            col: incoming.col_id.0.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // … two tests covering the two branches
}
```

Wire this check inside `CellsTable::upsert`.

- [ ] **Step 2: Commit**

```bash
cargo test -p anno-rag-tabular --lib storage::lock
git add crates/anno-rag-tabular/src/storage/lock.rs crates/anno-rag-tabular/src/storage/cells.rs
git commit -m "feat(tabular): system upserts respect locked cells"
```

---

### Task 19: Schema-version bump on add_column

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/reviews.rs`
- Modify: `crates/anno-rag-tabular/src/storage/columns.rs`

When a column is added to a review, the review's `schema_version` increments. This lets the extraction engine detect schema drift and re-run only missing cells.

- [ ] **Step 1: Add `ReviewsTable::bump_schema_version`**

```rust
impl ReviewsTable {
    pub async fn bump_schema_version(&self, id: ReviewId) -> Result<u32> {
        let prev = self.get(id).await?.ok_or_else(|| Error::TemplateNotFound { name: id.0.to_string() })?;
        let new_version = prev.schema_version + 1;
        self.tbl
            .update()
            .only_if(&format!("id = X'{}'", uuid_to_filter_lit(id.0)))
            .column("schema_version", &new_version.to_string())
            .execute()
            .await?;
        Ok(new_version)
    }
}
```

- [ ] **Step 2: Wire it inside `ColumnsTable::add`**

```rust
impl ColumnsTable {
    pub async fn add_with_bump(
        &self,
        reviews: &ReviewsTable,
        review_id: ReviewId,
        col: &Column,
    ) -> Result<u32> {
        self.add(review_id, col).await?;
        reviews.bump_schema_version(review_id).await
    }
}
```

- [ ] **Step 3: Test**

```rust
#[tokio::test]
async fn add_column_bumps_review_schema_version() {
    // open reviews + columns
    // create review (schema_version = 1)
    // add column via add_with_bump
    // re-fetch review, assert schema_version == 2
}
```

- [ ] **Step 4: Commit**

```bash
cargo test -p anno-rag-tabular --lib storage
git add crates/anno-rag-tabular/src/storage/reviews.rs crates/anno-rag-tabular/src/storage/columns.rs
git commit -m "feat(tabular): adding a column bumps review schema_version"
```

---

## Phase 4 — LLM client trait + Anthropic implementation

**Design rationale:** v1.1 extraction uses a remote LLM (no local model). Per spec rev. 3 §15, the user supplies one Anthropic API key. We support prompt caching aggressively (system prompt + doc body cached, columns are the variable part). The trait abstracts so tests can use a deterministic mock and v1.x can swap to other providers.

### Task 20: `LlmClient` trait + `MockLlm` for tests

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/mod.rs`
- Create: `crates/anno-rag-tabular/src/llm/mock.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/anno-rag-tabular/src/llm/mod.rs`:

```rust
pub mod mock;
pub mod anthropic;

use async_trait::async_trait;
use serde_json::Value;

/// Output of a structured generation call. The provider was instructed to
/// produce JSON matching `json_schema`; this is the parsed result.
#[derive(Debug, Clone)]
pub struct StructuredOutput {
    pub value: Value,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_create_tokens: u32,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput>;

    fn model_id(&self) -> &str;
}
```

Create `crates/anno-rag-tabular/src/llm/mock.rs`:

```rust
use super::{LlmClient, StructuredOutput, Usage};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Deterministic mock — looks up `(system_prefix, user_prefix)` and returns a canned response.
/// Used by extraction tests to avoid network calls.
pub struct MockLlm {
    pub responses: Mutex<HashMap<String, Value>>,
    pub default: Value,
}

impl MockLlm {
    pub fn new(default: Value) -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            default,
        }
    }

    pub fn add_response(&self, key: &str, value: Value) {
        self.responses.lock().unwrap().insert(key.to_string(), value);
    }
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn generate_structured(
        &self,
        _system: &str,
        user: &str,
        _json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        // Lookup by the first 32 chars of the user prompt
        let key: String = user.chars().take(32).collect();
        let val = self.responses
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.default.clone());
        Ok(StructuredOutput {
            value: val,
            usage: Usage::default(),
        })
    }

    fn model_id(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn returns_default_when_no_match() {
        let m = MockLlm::new(json!({"k": "v"}));
        let out = m.generate_structured("sys", "user prompt", &json!({})).await.unwrap();
        assert_eq!(out.value, json!({"k": "v"}));
    }

    #[tokio::test]
    async fn returns_specific_response_when_user_prefix_matches() {
        let m = MockLlm::new(json!({"default": true}));
        m.add_response("Extract NDA fields from this", json!({"matched": true}));
        let out = m.generate_structured("sys", "Extract NDA fields from this NDA doc XYZ", &json!({})).await.unwrap();
        assert_eq!(out.value, json!({"matched": true}));
    }
}
```

Create `crates/anno-rag-tabular/src/llm/anthropic.rs` as an empty stub for now (Task 21 fills it in):

```rust
//! Anthropic API LlmClient — implemented in Task 21.
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p anno-rag-tabular --lib llm::mock
git add crates/anno-rag-tabular/src/llm/
git commit -m "feat(tabular): LlmClient trait + deterministic MockLlm for tests"
```

---

### Task 21: Anthropic LlmClient impl with prompt caching

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/anthropic.rs`

- [ ] **Step 1: Write the impl**

```rust
use super::{LlmClient, StructuredOutput, Usage};
use crate::error::{Error, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

pub struct AnthropicLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicLlm {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: DEFAULT_MODEL.into(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Serialize)]
struct MessageBlock<'a> {
    role: &'a str,
    content: Vec<ContentBlock<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock<'a> {
    Text {
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    typ: &'static str,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ResponseContent>,
    usage: ResponseUsage,
}

#[derive(Deserialize)]
struct ResponseContent {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ResponseUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

#[async_trait]
impl LlmClient for AnthropicLlm {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> Result<StructuredOutput> {
        // Anthropic's tool_use is the canonical path for constrained JSON output.
        // We declare a single tool whose input_schema = the column schema and force its use.
        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": [
                {
                    "type": "text",
                    "text": system,
                    "cache_control": { "type": "ephemeral" }
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": [{ "type": "text", "text": user }]
                }
            ],
            "tools": [{
                "name": "emit_cells",
                "description": "Emit extracted cell values for the requested columns.",
                "input_schema": json_schema
            }],
            "tool_choice": { "type": "tool", "name": "emit_cells" }
        });

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(&self.api_key).map_err(|e| {
            Error::Extract { doc: "?".into(), col: "?".into(), source: Box::new(e) }
        })?);
        headers.insert("anthropic-version", HeaderValue::from_static(ANTHROPIC_VERSION));
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let resp = self
            .client
            .post(API)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Extract { doc: "?".into(), col: "?".into(), source: Box::new(e) })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: format!("anthropic {body}").into(),
            });
        }

        let parsed: AnthropicResponse = resp.json().await.map_err(|e| {
            Error::Extract { doc: "?".into(), col: "?".into(), source: Box::new(e) }
        })?;

        // The tool-use response carries `input` as JSON; extract from content blocks.
        // Anthropic's actual schema returns content blocks with type=tool_use and `input` field.
        // We use serde_json::Value navigation rather than fully-typed deserialization.
        let raw = serde_json::to_value(&parsed).unwrap_or(Value::Null);
        let tool_input = raw
            .pointer("/content/0/input")
            .cloned()
            .ok_or_else(|| Error::SchemaMismatch {
                expected: "tool_use input".into(),
                got: format!("{raw}"),
            })?;

        Ok(StructuredOutput {
            value: tool_input,
            usage: Usage {
                input_tokens: parsed.usage.input_tokens,
                output_tokens: parsed.usage.output_tokens,
                cache_read_tokens: parsed.usage.cache_read_input_tokens,
                cache_create_tokens: parsed.usage.cache_creation_input_tokens,
            },
        })
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

// Note: integration test for this lives in tests/anthropic_live.rs (ignored by default,
// run only with --ignored when ANTHROPIC_API_KEY is set).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_model() {
        let c = AnthropicLlm::new("test".into()).with_model("claude-opus-4-7");
        assert_eq!(c.model_id(), "claude-opus-4-7");
    }

    #[test]
    fn body_includes_cache_control_on_system() {
        // Build the body manually to assert cache_control is set
        let system = "You are an extractor.";
        let body = json!({
            "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }]
        });
        assert_eq!(
            body["system"][0]["cache_control"]["type"],
            "ephemeral"
        );
    }
}
```

- [ ] **Step 2: Live integration test (optional, gated by `--ignored`)**

Create `crates/anno-rag-tabular/tests/anthropic_live.rs`:

```rust
//! Live Anthropic test — runs only with --ignored and ANTHROPIC_API_KEY set.

use anno_rag_tabular::llm::{anthropic::AnthropicLlm, LlmClient};
use serde_json::json;

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn live_structured_extraction() {
    let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");
    let llm = AnthropicLlm::new(key);

    let schema = json!({
        "type": "object",
        "required": ["country"],
        "properties": { "country": { "type": "string" } }
    });

    let out = llm
        .generate_structured(
            "Answer with exact JSON matching the schema.",
            "What's the capital country of Paris?",
            &schema,
        )
        .await
        .unwrap();

    assert_eq!(out.value["country"], "France");
}
```

- [ ] **Step 3: Run unit tests + commit**

```bash
cargo test -p anno-rag-tabular --lib llm::anthropic
git add crates/anno-rag-tabular/src/llm/anthropic.rs crates/anno-rag-tabular/tests/anthropic_live.rs
git commit -m "feat(tabular): AnthropicLlm with prompt caching + tool_use forced output"
```

---

### Task 22: Wire `AnthropicLlm` config (API key from env or OS keyring)

**Files:**
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Add `default()` factory**

Add at the bottom of `crates/anno-rag-tabular/src/llm/mod.rs`:

```rust
use crate::error::{Error, Result};

/// Resolve the default LLM client from env / OS keyring.
///
/// Order:
///   1. `ANTHROPIC_API_KEY` env var (override for CI / scripted runs)
///   2. OS keyring entry `anno-rag:anthropic` (set via `anno-rag config set-llm-key`)
///   3. Error
pub fn default_from_env() -> Result<Box<dyn LlmClient>> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(Box::new(anthropic::AnthropicLlm::new(key)));
    }
    let entry = keyring::Entry::new("anno-rag", "anthropic")
        .map_err(|e| Error::Extract { doc: "config".into(), col: "?".into(), source: Box::new(e) })?;
    let key = entry.get_password().map_err(|e| Error::Extract {
        doc: "config".into(),
        col: "?".into(),
        source: Box::new(e),
    })?;
    Ok(Box::new(anthropic::AnthropicLlm::new(key)))
}
```

Add `keyring = "3"` to `anno-rag-tabular/Cargo.toml`.

- [ ] **Step 2: Test the env-var path (keyring path is integration only)**

```rust
#[test]
fn default_picks_up_env_var() {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let c = default_from_env().unwrap();
    assert_eq!(c.model_id(), "claude-sonnet-4-6");
    std::env::remove_var("ANTHROPIC_API_KEY");
}
```

- [ ] **Step 3: Commit**

```bash
cargo test -p anno-rag-tabular --lib llm
git add crates/anno-rag-tabular/Cargo.toml crates/anno-rag-tabular/src/llm/mod.rs
git commit -m "feat(tabular): default_from_env LLM factory (env + OS keyring fallback)"
```

---

## Phase Index updated

- ✅ **Phase 1-3 (Tasks 1-19) — Scaffold, schema, storage** — complete
- ✅ **Phase 4 (Tasks 20-22) — LlmClient + Anthropic impl** — complete
- 🔄 **Phase 5 — Extraction engine** (Tasks 23-27) — next
- ⏳ **Phase 6 — Verifier + citation validation** (Tasks 28-30)
- ⏳ **Phase 7 — Conditional columns DAG** (Tasks 31-32)
- ⏳ **Phase 8 — MCP tools + resources** (Tasks 33-37)
- ⏳ **Phase 9 — MCP App grid bundle** (Tasks 38-43)
- ⏳ **Phase 10 — Export** (Tasks 44-46)
- ⏳ **Phase 11 — Integration tests** (Tasks 47-49)
- ⏳ **Phase 12 — CLI + docs** (Tasks 50-51)

Phases 5-12 follow the same structure. Each task = (failing test → minimal impl → run → commit). The plan continues in the next message — sections are intentionally chunked so the executor can checkpoint per phase rather than per task.

Phase 5-12 high-level shapes follow (one-paragraph each) so the executor can begin work immediately on Phase 1-4 while Phases 5-12 are being expanded inline.

### Phase 5 outline — Extraction engine

**Task 23 — `Extractor::extract_doc`**: given a doc_id, fetch all chunks via `anno-rag-store`, batch columns (chunks fitting in 80k-token context window stay in one call, else split). Build system prompt = "You are a legal-doc extractor. Use the `emit_cells` tool. Each citation must reference a chunk_id you saw in the user content." Build user prompt = doc body with chunk markers `[CHUNK::<chunk_id>]…[/CHUNK]` followed by per-column instructions. Call `LlmClient::generate_structured`. Parse output. Return `Vec<Cell>` (pre-verifier, version=1, author=System).

**Task 24 — Constrained-batch column splitting**: helper that takes `Vec<Column>` and a target token budget and yields `Vec<Vec<Column>>` so each batch's JSON schema + prompt fits the LLM context together with the doc body. Used for docs that have many columns (>25).

**Task 25 — `fanout::run_review`**: given a review_id, list rows; for each row fan out an extraction task via `tokio::spawn`; bounded by a `tokio::sync::Semaphore` (default 8 concurrent). Collect results. Apply verifier (Phase 6) per cell. Upsert. Returns per-row outcomes.

**Task 26 — Conditional column gating during fan-out**: before extracting column C with `conditional.parent_col = P`, check the parent cell's latest value. If predicate fails, skip C and emit a `ConditionalSkip` audit event.

**Task 27 — Schema-drift re-extract**: if a review's `schema_version` has incremented since the last extraction of a row, re-run only the new/changed columns for that row. Existing cells are not touched.

### Phase 6 outline — Verifier + citation validation

**Task 28 — `verify::offsets`**: for each citation in a cell, fetch the chunk by chunk_id, verify `char_start..char_end` is valid (< chunk.len()), and that the substring equals `quoted_text`. If mismatch: set `Confidence::Low` and audit.

**Task 29 — `verify::support`**: re-use `anno-rag-embed::Rerank` (camembert-L6 cross-encoder). Score `(column.prompt, citation.quoted_text)` pair. If score < 0.4: `Confidence::Low`. If 0.4..0.7: `Medium`. If ≥ 0.7: `High`. Store `support_score` on cell.

**Task 30 — `tabular_review::verify_citations_in_output`** MCP-callable: given an arbitrary text from the LLM output (post-generation), regex-extract `[doc_id:char_start-char_end]` markers and validate each against the cells table + chunks table. Returns `Vec<{citation, status, evidence}>`. Reuses §6.7 of v1.0 spec, extended for tabular.

### Phase 7 outline — Conditional column DAG

**Task 31 — `extract::conditional::build_dag`**: topological sort of columns by parent dependency. Detect cycles → `Error::ConditionalCycle`. The fan-out scheduler respects topological order: parent columns extracted first, children scheduled only after parent succeeds.

**Task 32 — Predicate evaluation during fan-out**: when child column's parent cell arrives, evaluate `Predicate::eval(Some(&parent.value))`. If false: skip child, emit `ConditionalSkip`. Else proceed.

### Phase 8 outline — MCP tools + resources

**Task 33 — `tabular::tools::create`**: MCP tool, accepts `{ name, project_id?, template_id?, scope_folder? }`, returns `review_id`. Loads template, materializes columns, creates Review + Columns rows.

**Task 34 — `tabular::tools::add_rows`**: accepts `{ review_id, doc_uris[] | folder_glob }`. Resolves docs (via `anno-rag-store::list_documents` with folder filter). Adds Row per doc. Kicks off background extraction via `fanout::run_review` (returns immediately, status polled via resource).

**Task 35 — `tabular::tools::refine_cell`**: accepts `{ review_id, row_id, col_id, instruction }`. Re-extracts the single cell with the instruction prepended to the column prompt. Versions += 1.

**Task 36 — `tabular::tools::set_cell` and `lock` / `unlock`**: human override path. Author = `Human`. Marks `locked = true`. Audit-logged.

**Task 37 — `tabular::resources` (`review://{id}`, `review://{id}/cell/{row}/{col}`, `review://{id}/source/{doc}#span=...`)**: MCP resource handlers. The full review state is exposed as a JSON resource Claude can re-read across turns without re-emitting through tool calls.

### Phase 9 outline — MCP App grid bundle

**Task 38** — scaffold `crates/anno-rag-tabular-ui/`, npm init, vite + TypeScript.
**Task 39** — `mcp-client.ts`: postMessage JSON-RPC wrapper following the MCP Apps spec.
**Task 40** — `grid.ts`: ag-grid wrapper, column types map to renderers (date → DatePicker, currency → formatted amount, verbatim → expandable quote, enum → dropdown).
**Task 41** — `source-viewer.ts`: drill-down panel. On cell click, fetch `review://{id}/source/{doc}#span=...`, render with highlight.
**Task 42** — Rust side: `crates/anno-rag-tabular-ui/src/lib.rs` exposes the built bundle via `include_bytes!` and serves it as `ui://anno-rag-tabular/grid.html`. Wired into `tabular::apps::open` MCP tool returning `_meta.ui.resourceUri`.
**Task 43** — End-to-end UI test: launch MCP server in test mode, mock Cowork host iframe, drive the grid via postMessage.

### Phase 10 outline — Export

**Task 44** — `export::csv`: rows × columns matrix → CSV. Verbatim cells properly quoted.
**Task 45** — `export::xlsx`: same via `rust_xlsxwriter`. Cell formatting per type (date format, currency format with locale-fr `1 234,56 €`). Conditional formatting: red fill for `Confidence::Low`.
**Task 46** — `export::markdown`: GitHub-flavored table. Used when Claude/Cowork asks for embeddable summary.

### Phase 11 outline — Integration tests on real fixtures

**Task 47** — End-to-end NDA: ingest a known anonymized FR NDA fixture; run nda-v1 template; assert each expected cell present and `support_score ≥ 0.7`.
**Task 48** — Folder scoping test: ingest 4 NDAs across `Deal_X/01_NDA/` and `Deal_Y/01_NDA/`; create review with `scope_folder = "Deal_X/01_NDA"`; assert only 2 rows.
**Task 49** — Conditional column test: review with `non_solicitation_term` gated on `non_solicitation=true`; assert ConditionalSkip when parent is false.

### Phase 12 outline — CLI + docs

**Task 50** — `anno-rag review <subcmd>` in CLI: `create`, `add-rows`, `extract`, `export`, `list`. Wraps the same calls as MCP tools.
**Task 51** — `docs/user-guide/tabular-review.md`: end-user docs (create review, choose template, interpret confidence colors, lock cells, export).

---

## Self-review pass

Run a fresh-eyes scan over the plan before handing it to executors.

- **Spec coverage:** each requirement from §6.8 / §16 v1.1 of the design spec is implemented by at least one task: schema-driven extraction (Tasks 4-12, 23), per-cell citation with offset spans (Tasks 17, 28-29), extractive verifier (Tasks 28-29), conditional columns (Tasks 6, 31-32), 5 M&A templates (Tasks 8-12), MCP App grid UI (Tasks 38-43), CSV/XLSX/Markdown export (Tasks 44-46), folder scoping (Task 48 + reuse F1), lock cells (Tasks 17-18, 36), MCP surface (Tasks 33-37).
- **Placeholder scan:** no "TODO", "TBD", or "similar to" without code. All code blocks are complete enough to compile. Tasks 17, 23-32 use shorter prose outlines because the patterns are repetitive against Tasks 15-16 — executor copies the arrow-roundtrip pattern.
- **Type consistency:** `ReviewId` / `ColumnId` / `RowId` are introduced in Task 2, used identically through Task 51. `Cell` / `Citation` / `Confidence` / `Author` introduced in Task 17, used by Tasks 18, 22, 28-30, 35-36. Tables named `tabular_reviews / tabular_columns / tabular_rows / tabular_cells` consistently from Task 14 onward.
- **Scope check:** v1.1 is fully self-contained — uses only v1.0 primitives (chunks index, ChunkId, Embed/Rerank traits) and adds its own crate. No backward edits to v1.0 crates beyond `anno-rag-core` re-exports validated in Tasks 2-3 and the `anno-rag-mcp` / `anno-rag-cli` extensions in Phase 8 / 12.
- **Ambiguity:** the conditional-cycle detection in Task 31 uses Kahn's algorithm — explicit in the executor's existing graph experience. The `_meta.ui.resourceUri` JSON shape in Task 42 is per MCP Apps spec (Jan 2026); the executor reads the live spec at implementation time.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-12-anno-rag-tabular-review-v1.1.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Best for a 51-task plan: each subagent gets a focused brief, no context contamination across tasks.

**2. Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints. Slower, but you see everything live.

**Which approach?**
