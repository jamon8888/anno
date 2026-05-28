# Anno Tabular Local Legal Extraction Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a conservative local-first legal extraction layer to `anno-rag-tabular`, using GLiNER2/Fastino for evidence-backed fields and optional LLM fallback for complex legal interpretation.

**Architecture:** Extend tabular column metadata with extraction modes, expose that metadata through JSON-schema vendor extensions, add a local tabular client that implements the existing `LlmClient` seam, and add a routing client that partitions local-safe, LLM-required, and manual columns. Preserve the current citation envelope and verifier behavior.

**Tech Stack:** Rust workspace, `anno-rag-tabular`, `anno` GLiNER2/Fastino APIs, serde/TOML template loading, LanceDB-backed tabular storage, existing `verify::offsets` and `verify::support`, optimized `scripts/dev-fast.ps1` loop with `profile.dev-fast`, targeted Cargo tests, and nextest profiles when appropriate.

---

## File Structure

Modify:

- `crates/anno-rag-tabular/Cargo.toml`  
  Add a direct dependency on `anno` with the GLiNER2/Fastino feature set required by the workspace.

- `crates/anno-rag-tabular/src/schema/column.rs`  
  Add `ExtractionMode`, `ExtractionSpec`, `ExtractionLabel`, `ExtractionNormalizer`, and builder support.

- `crates/anno-rag-tabular/src/schema/template.rs`  
  Parse optional `[column.extraction]` TOML metadata.

- `crates/anno-rag-tabular/src/schema/json_schema.rs`  
  Emit `x-anno-column` vendor metadata per column so `LlmClient` implementations can route without private Rust access.

- `crates/anno-rag-tabular/src/storage/arrow_schema.rs`  
  Add nullable `extraction_json` to `tabular_columns`.

- `crates/anno-rag-tabular/src/storage/columns.rs`  
  Persist and read `Column.extraction`, defaulting to `Auto` when absent.

Create:

- `crates/anno-rag-tabular/src/llm/local/mod.rs`  
  Public module export for local extraction.

- `crates/anno-rag-tabular/src/llm/local/prompt.rs`  
  Parse `[CHUNK]` and `[COLUMN]` sections from the existing `build_user_prompt()` output.

- `crates/anno-rag-tabular/src/llm/local/offsets.rs`  
  Convert GLiNER character offsets to byte offsets safely.

- `crates/anno-rag-tabular/src/llm/local/normalizers.rs`  
  Normalize dates, currency, numbers, enums, and verbatim clauses.

- `crates/anno-rag-tabular/src/llm/local/client.rs`  
  `LocalTabularClient` implementing `LlmClient`.

- `crates/anno-rag-tabular/src/llm/local/legal_signals.rs`  
  Adapter over Anno/anno-rag legal labels, thresholds, GLiNER2 extraction modes, and deterministic legal facts.

- `crates/anno-rag-tabular/src/llm/routing.rs`  
  `RoutingLlmClient` that merges local and optional LLM outputs, while filtering fallback columns and preventing raw-PII fallback prompts.

- `crates/anno-rag-tabular/src/llm/privacy.rs`  
  Prompt preflight guard for fallback LLM calls.

Tests:

- `crates/anno-rag-tabular/src/schema/column.rs` unit tests.
- `crates/anno-rag-tabular/src/schema/template.rs` unit tests.
- `crates/anno-rag-tabular/src/schema/json_schema.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/local/prompt.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/local/offsets.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/local/normalizers.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/local/client.rs` unit tests with a mock local extractor.
- `crates/anno-rag-tabular/src/llm/local/legal_signals.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/routing.rs` unit tests.
- `crates/anno-rag-tabular/src/llm/privacy.rs` unit tests.

Do not modify MCP tools in this plan. `anno-rag-mcp` still excludes `anno-rag-tabular`; MCP wrapping is a later integration.

---

## Build Strategy for This Plan

Use the optimized build path from the rapid-build commit. Do not use broad workspace builds during this plan.

For compile feedback, prefer:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check -Profile dev-fast -NoSccache -PrintOnly
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check -Profile dev-fast -NoSccache
```

For a narrow unit test, keep the shared target directory on `D:` and use `dev-fast`:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular <test_filter>
```

Use `-AllAffected` only after changing shared crates such as `anno` or `anno-rag`. Use `-NoSccache` unless `sccache` is explicitly confirmed available in the current shell. Reserve `--all-targets`, full package tests, or nextest package sweeps for final verification.

---

## Task 1: Add Extraction Metadata Types

**Files:**
- Modify: `crates/anno-rag-tabular/src/schema/column.rs`

- [ ] **Step 1: Add the failing unit test for default metadata**

Add this test in the existing `#[cfg(test)]` module in `column.rs`:

```rust
#[test]
fn column_defaults_to_auto_extraction() {
    let review = ReviewId(uuid::Uuid::now_v7());
    let col = ColumnBuilder::new(review, "landlord", "Landlord?", CellType::Text).build();

    assert_eq!(col.extraction.mode, ExtractionMode::Auto);
    assert!(col.extraction.labels.is_empty());
    assert!(col.extraction.keywords.is_empty());
    assert_eq!(col.extraction.threshold, None);
    assert_eq!(col.extraction.normalizer, None);
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular column_defaults_to_auto_extraction
```

Expected: fail because `Column.extraction`, `ExtractionMode`, and related types do not exist.

- [ ] **Step 3: Add metadata types and field**

Add near the top of `column.rs`, after imports:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    Auto,
    LocalSpan,
    LocalClause,
    LocalClassifier,
    LlmRequired,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionLabel {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionNormalizer {
    LegalName,
    DateIso,
    EurCurrency,
    Number,
    Enum,
    VerbatimClause,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionSpec {
    #[serde(default = "ExtractionSpec::default_mode")]
    pub mode: ExtractionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ExtractionLabel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalizer: Option<ExtractionNormalizer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_before_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_after_chars: Option<usize>,
}

impl ExtractionSpec {
    fn default_mode() -> ExtractionMode {
        ExtractionMode::Auto
    }
}

impl Default for ExtractionSpec {
    fn default() -> Self {
        Self {
            mode: ExtractionMode::Auto,
            labels: Vec::new(),
            keywords: Vec::new(),
            threshold: None,
            normalizer: None,
            window_before_chars: None,
            window_after_chars: None,
        }
    }
}
```

Add this field to `Column`:

```rust
#[serde(default)]
pub extraction: ExtractionSpec,
```

Add this field to `ColumnBuilder`:

```rust
extraction: ExtractionSpec,
```

Initialize it in `ColumnBuilder::new()`:

```rust
extraction: ExtractionSpec::default(),
```

Add a builder method:

```rust
#[must_use]
pub fn extraction(mut self, extraction: ExtractionSpec) -> Self {
    self.extraction = extraction;
    self
}
```

Set it in `build()`:

```rust
extraction: self.extraction,
```

- [ ] **Step 4: Run the metadata test**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular column_defaults_to_auto_extraction
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-tabular/src/schema/column.rs
git commit -m "feat(tabular): add extraction metadata to columns"
```

---

## Task 2: Parse Extraction Metadata from Templates

**Files:**
- Modify: `crates/anno-rag-tabular/src/schema/template.rs`
- Modify: `crates/anno-rag-tabular/src/templates/real-estate-v1.toml`

- [ ] **Step 1: Add the failing template parser test**

Add to the template tests:

```rust
#[test]
fn template_parses_extraction_metadata() {
    let raw = r#"
id = "test-template"
name = "Test"
version = "1.0.0"
description = "Test"
vertical = "legal-fr"

[[column]]
name = "landlord"
prompt = "Landlord legal name."
type = "text"

[column.extraction]
mode = "local_span"
normalizer = "legal_name"
threshold = 0.45
keywords = ["bailleur", "entre les soussignes"]
labels = [
  { name = "bailleur", description = "Nom complet du bailleur" }
]
"#;

    let t: TemplateWire = toml::from_str(raw).expect("template wire parses");
    let review = ReviewId(uuid::Uuid::now_v7());
    let cols = t.into_columns(review);

    assert_eq!(cols[0].extraction.mode, ExtractionMode::LocalSpan);
    assert_eq!(cols[0].extraction.normalizer, Some(ExtractionNormalizer::LegalName));
    assert_eq!(cols[0].extraction.threshold, Some(0.45));
    assert_eq!(cols[0].extraction.keywords, vec!["bailleur", "entre les soussignes"]);
    assert_eq!(cols[0].extraction.labels[0].name, "bailleur");
}
```

- [ ] **Step 2: Run the parser test and confirm it fails**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular template_parses_extraction_metadata
```

Expected: fail because `TemplateColumnWire` has no `extraction` field.

- [ ] **Step 3: Add wire support**

Import the new types in `template.rs`:

```rust
use crate::schema::{
    CellType, Column, ExtractionLabel, ExtractionMode, ExtractionNormalizer, ExtractionSpec,
};
```

Add to `TemplateColumnWire`:

```rust
#[serde(default)]
pub extraction: ExtractionSpec,
```

When building the column, call:

```rust
ColumnBuilder::new(review_id, &c.name, &c.prompt, c.cell_type.into())
    .extraction(c.extraction)
    .order(order)
    .build()
```

Keep `#[serde(default)]` so all existing templates remain valid.

- [ ] **Step 4: Annotate only the safest real-estate fields**

Add this metadata to `landlord`, `tenant`, `premises_address`, `start_date`, `base_rent`, and `security_deposit` in `real-estate-v1.toml`:

```toml
[column.extraction]
mode = "local_span"
normalizer = "legal_name"
threshold = 0.45
keywords = ["bailleur"]
labels = [
  { name = "bailleur", description = "Nom complet et forme juridique du bailleur" }
]
```

For non-name fields, set the appropriate normalizer:

```toml
normalizer = "date_iso"
```

or:

```toml
normalizer = "eur_currency"
```

Do not annotate `tenant_break_rights`, `assignment_sublet`, or `repair_obligations` as local-safe in this task.

- [ ] **Step 5: Run template tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular template_
```

Expected: built-in templates still load; the new metadata test passes.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-tabular/src/schema/template.rs crates/anno-rag-tabular/src/templates/real-estate-v1.toml
git commit -m "feat(tabular): parse local extraction metadata from templates"
```

---

## Task 3: Persist Extraction Metadata

**Files:**
- Modify: `crates/anno-rag-tabular/src/storage/arrow_schema.rs`
- Modify: `crates/anno-rag-tabular/src/storage/columns.rs`

- [ ] **Step 1: Add failing storage round-trip test**

Add a test in `columns.rs`:

```rust
#[tokio::test]
async fn columns_round_trip_extraction_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let conn = lancedb::connect(tmp.path().to_str().unwrap())
        .execute()
        .await
        .expect("connect");
    let store = ColumnsStore::open(&conn).await.expect("open");
    let review = ReviewId(uuid::Uuid::now_v7());

    let extraction = ExtractionSpec {
        mode: ExtractionMode::LocalSpan,
        labels: vec![ExtractionLabel {
            name: "bailleur".into(),
            description: "Nom complet du bailleur".into(),
        }],
        keywords: vec!["bailleur".into()],
        threshold: Some(0.45),
        normalizer: Some(ExtractionNormalizer::LegalName),
        window_before_chars: None,
        window_after_chars: None,
    };

    let col = ColumnBuilder::new(review, "landlord", "Landlord?", CellType::Text)
        .extraction(extraction.clone())
        .build();

    store.add(review, &col).await.expect("add");
    let listed = store.list(review).await.expect("list");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].extraction, extraction);
}
```

- [ ] **Step 2: Run and confirm failure**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular columns_round_trip_extraction_metadata
```

Expected: fail because storage schema has no `extraction_json`.

- [ ] **Step 3: Add nullable `extraction_json`**

In `arrow_schema.rs`, add after `conditional_json`:

```rust
Field::new("extraction_json", DataType::Utf8, true),
```

In `columns.rs::add`, serialize:

```rust
let extraction_json = serde_json::to_string(&col.extraction)?;
let extraction_a = StringArray::from(vec![Some(extraction_json)]);
```

Insert `Arc::new(extraction_a)` in the same position as the schema field.

In the read path, parse:

```rust
let extraction = extraction_arr
    .and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) })
    .map(serde_json::from_str)
    .transpose()?
    .unwrap_or_default();
```

When reading old tables without the column, default to `ExtractionSpec::default()`.

- [ ] **Step 4: Run storage tests**

Run:

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular columns_
```

Expected: storage column tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-tabular/src/storage/arrow_schema.rs crates/anno-rag-tabular/src/storage/columns.rs
git commit -m "feat(tabular): persist extraction metadata"
```

---

## Task 4: Expose Column Metadata in JSON Schema

**Files:**
- Modify: `crates/anno-rag-tabular/src/schema/json_schema.rs`

- [ ] **Step 1: Add failing JSON schema test**

Add:

```rust
#[test]
fn schema_includes_anno_column_metadata() {
    let review = ReviewId(uuid::Uuid::now_v7());
    let extraction = ExtractionSpec {
        mode: ExtractionMode::LocalSpan,
        labels: vec![ExtractionLabel {
            name: "bailleur".into(),
            description: "Nom complet du bailleur".into(),
        }],
        keywords: vec!["bailleur".into()],
        threshold: Some(0.45),
        normalizer: Some(ExtractionNormalizer::LegalName),
        window_before_chars: None,
        window_after_chars: None,
    };
    let col = ColumnBuilder::new(review, "landlord", "Landlord?", CellType::Text)
        .extraction(extraction)
        .build();

    let schema = for_columns(&[col]);
    let meta = &schema["properties"]["landlord"]["x-anno-column"];

    assert_eq!(meta["name"], "landlord");
    assert_eq!(meta["prompt"], "Landlord?");
    assert_eq!(meta["cell_type"]["kind"], "text");
    assert_eq!(meta["extraction"]["mode"], "local_span");
    assert_eq!(meta["extraction"]["labels"][0]["name"], "bailleur");
}
```

- [ ] **Step 2: Run and confirm failure**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular schema_includes_anno_column_metadata
```

Expected: fail because `x-anno-column` does not exist.

- [ ] **Step 3: Add vendor metadata**

In the place where each column property is built, merge this extension into the cell envelope object:

```rust
let mut envelope = cell_envelope(&col.cell_type);
if let Some(obj) = envelope.as_object_mut() {
    obj.insert(
        "x-anno-column".to_string(),
        json!({
            "name": col.name,
            "prompt": col.prompt,
            "cell_type": col.cell_type,
            "extraction": col.extraction,
        }),
    );
}
```

This metadata is part of the schema passed to `LlmClient`; it is not part of the model output, because `additionalProperties: false` still applies to cell envelopes.

- [ ] **Step 4: Run JSON schema tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular json_schema
```

Expected: all JSON schema tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-tabular/src/schema/json_schema.rs
git commit -m "feat(tabular): expose extraction metadata in schema"
```

---

## Task 5: Add Local Prompt Parser and Offset Conversion

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/local/mod.rs`
- Create: `crates/anno-rag-tabular/src/llm/local/prompt.rs`
- Create: `crates/anno-rag-tabular/src/llm/local/offsets.rs`
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Add module export**

In `llm/mod.rs`:

```rust
pub mod local;
```

Create `local/mod.rs`:

```rust
pub mod offsets;
pub mod prompt;
```

- [ ] **Step 2: Add prompt parser test**

In `prompt.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chunks_and_columns_from_user_prompt() {
        let chunk_id = uuid::Uuid::now_v7();
        let user = format!(
            "[CHUNK::{chunk_id}]Le bailleur est ACME SAS.[/CHUNK]\n\
             [COLUMN::landlord]Landlord legal name[/COLUMN]\n"
        );

        let parsed = parse_user_prompt(&user).expect("parse");

        assert_eq!(parsed.chunks.len(), 1);
        assert_eq!(parsed.chunks[0].id, chunk_id);
        assert_eq!(parsed.chunks[0].text, "Le bailleur est ACME SAS.");
        assert_eq!(parsed.columns[0].name, "landlord");
        assert_eq!(parsed.columns[0].prompt, "Landlord legal name");
    }
}
```

- [ ] **Step 3: Implement parser**

In `prompt.rs`:

```rust
use crate::error::{Error, Result};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPrompt {
    pub chunks: Vec<ParsedChunk>,
    pub columns: Vec<ParsedColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedChunk {
    pub id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedColumn {
    pub name: String,
    pub prompt: String,
}

pub fn parse_user_prompt(input: &str) -> Result<ParsedPrompt> {
    let mut chunks = Vec::new();
    let mut columns = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find("[CHUNK::") {
        rest = &rest[start + "[CHUNK::".len()..];
        let Some(end_id) = rest.find(']') else {
            return Err(parse_error("missing chunk id terminator"));
        };
        let id = rest[..end_id].parse::<Uuid>().map_err(|e| parse_error(e.to_string()))?;
        rest = &rest[end_id + 1..];
        let Some(end) = rest.find("[/CHUNK]") else {
            return Err(parse_error("missing chunk close marker"));
        };
        chunks.push(ParsedChunk {
            id,
            text: rest[..end].to_string(),
        });
        rest = &rest[end + "[/CHUNK]".len()..];
    }

    rest = input;
    while let Some(start) = rest.find("[COLUMN::") {
        rest = &rest[start + "[COLUMN::".len()..];
        let Some(end_name) = rest.find(']') else {
            return Err(parse_error("missing column name terminator"));
        };
        let name = rest[..end_name].to_string();
        rest = &rest[end_name + 1..];
        let Some(end) = rest.find("[/COLUMN]") else {
            return Err(parse_error("missing column close marker"));
        };
        columns.push(ParsedColumn {
            name,
            prompt: rest[..end].to_string(),
        });
        rest = &rest[end + "[/COLUMN]".len()..];
    }

    Ok(ParsedPrompt { chunks, columns })
}

fn parse_error(msg: impl Into<String>) -> Error {
    Error::Extract {
        doc: "prompt".into(),
        col: "*".into(),
        source: msg.into().into(),
    }
}
```

- [ ] **Step 4: Add UTF-8 offset test**

In `offsets.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_char_offsets_to_byte_offsets_for_french_text() {
        let text = "Loyer annuel de 12 000 € payé à échéance.";
        let start_char = text.chars().position(|c| c == '1').unwrap();
        let end_char = start_char + "12 000 €".chars().count();

        let span = char_span_to_byte_span(text, start_char, end_char).expect("span");

        assert_eq!(&text[span.start..span.end], "12 000 €");
    }
}
```

- [ ] **Step 5: Implement offset conversion**

In `offsets.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

pub fn char_span_to_byte_span(text: &str, start_char: usize, end_char: usize) -> Option<ByteSpan> {
    if start_char >= end_char {
        return None;
    }
    let mut map: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    map.push(text.len());
    if start_char >= map.len() || end_char > map.len() {
        return None;
    }
    Some(ByteSpan {
        start: map[start_char],
        end: map[end_char],
    })
}
```

- [ ] **Step 6: Run local parser/offset tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular llm::local
```

Expected: tests pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/local
git commit -m "feat(tabular): add local prompt and offset helpers"
```

---

## Task 6: Add Normalizers

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/local/normalizers.rs`
- Modify: `crates/anno-rag-tabular/src/llm/local/mod.rs`

- [ ] **Step 1: Export normalizers**

In `local/mod.rs`:

```rust
pub mod normalizers;
```

- [ ] **Step 2: Add normalizer tests**

In `normalizers.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::CellType;

    #[test]
    fn normalizes_eur_currency() {
        let value = normalize_value("12 000 €", &CellType::Currency { code: "EUR".into() })
            .expect("normalize");
        assert_eq!(value["amount"], 12000.0);
        assert_eq!(value["code"], "EUR");
    }

    #[test]
    fn normalizes_french_date_to_iso() {
        let value = normalize_value("1er janvier 2026", &CellType::Date).expect("normalize");
        assert_eq!(value, serde_json::json!("2026-01-01"));
    }

    #[test]
    fn rejects_invalid_enum() {
        let cell_type = CellType::Enum {
            options: vec!["FR".into(), "OTHER".into()],
        };
        assert!(normalize_value("Allemagne", &cell_type).is_none());
    }
}
```

- [ ] **Step 3: Implement deterministic normalizers**

In `normalizers.rs`:

```rust
use crate::schema::CellType;
use regex::Regex;
use serde_json::{json, Value};

pub fn normalize_value(raw: &str, cell_type: &CellType) -> Option<Value> {
    let cleaned = raw.trim();
    match cell_type {
        CellType::Text | CellType::Verbatim => Some(json!(cleaned)),
        CellType::Number => normalize_number(cleaned).map(|n| json!(n)),
        CellType::Currency { code } => normalize_currency(cleaned, code),
        CellType::Enum { options } => options
            .iter()
            .find(|opt| opt.eq_ignore_ascii_case(cleaned))
            .map(|opt| json!(opt)),
        CellType::Boolean => None,
        CellType::Date => normalize_date(cleaned),
    }
}

fn normalize_number(raw: &str) -> Option<f64> {
    let re = Regex::new(r"(?P<num>\d+(?:[\s.]\d{3})*(?:,\d+)?)").ok()?;
    let cap = re.captures(raw)?;
    cap.name("num")?
        .as_str()
        .replace(' ', "")
        .replace('.', "")
        .replace(',', ".")
        .parse()
        .ok()
}

fn normalize_currency(raw: &str, code: &str) -> Option<Value> {
    let amount = normalize_number(raw)?;
    Some(json!({ "amount": amount, "code": code }))
}

fn normalize_date(raw: &str) -> Option<Value> {
    let lowered = raw.to_lowercase();
    let months = [
        ("janvier", "01"), ("fevrier", "02"), ("février", "02"),
        ("mars", "03"), ("avril", "04"), ("mai", "05"), ("juin", "06"),
        ("juillet", "07"), ("aout", "08"), ("août", "08"),
        ("septembre", "09"), ("octobre", "10"), ("novembre", "11"), ("decembre", "12"),
        ("décembre", "12"),
    ];
    let re = Regex::new(r"(?P<day>\d{1,2}|1er)\s+(?P<month>[\p{L}]+)\s+(?P<year>\d{4})").ok()?;
    let cap = re.captures(&lowered)?;
    let day_raw = cap.name("day")?.as_str();
    let day = if day_raw == "1er" { 1 } else { day_raw.parse::<u32>().ok()? };
    let month_name = cap.name("month")?.as_str();
    let month = months.iter().find(|(name, _)| *name == month_name)?.1;
    let year = cap.name("year")?.as_str();
    Some(json!(format!("{year}-{month}-{day:02}")))
}
```

- [ ] **Step 4: Run normalizer tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular normalizes_
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/local/mod.rs crates/anno-rag-tabular/src/llm/local/normalizers.rs
git commit -m "feat(tabular): add local value normalizers"
```

---

## Task 7: Implement LocalTabularClient with a Test Double

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/local/client.rs`
- Modify: `crates/anno-rag-tabular/src/llm/local/mod.rs`

- [ ] **Step 1: Export client**

In `local/mod.rs`:

```rust
pub mod client;
```

- [ ] **Step 2: Add a trait for model-free tests**

In `client.rs`:

```rust
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct LocalEntity {
    pub text: String,
    pub start_char: usize,
    pub end_char: usize,
    pub confidence: f32,
}

pub trait LocalEntityExtractor: Send + Sync {
    fn extract(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>>;
}
```

- [ ] **Step 3: Add failing client test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmClient;
    use serde_json::json;

    struct MockExtractor;

    impl LocalEntityExtractor for MockExtractor {
        fn extract(
            &self,
            _text: &str,
            _labels: &[(&str, &str)],
            _threshold: f32,
        ) -> Result<Vec<LocalEntity>> {
            Ok(vec![LocalEntity {
                text: "ACME SAS".into(),
                start_char: 17,
                end_char: 25,
                confidence: 0.91,
            }])
        }
    }

    #[tokio::test]
    async fn local_client_emits_cited_cell_for_local_span() {
        let chunk_id = uuid::Uuid::now_v7();
        let user = format!(
            "[CHUNK::{chunk_id}]Le bailleur est ACME SAS.[/CHUNK]\n\
             [COLUMN::landlord]Landlord legal name[/COLUMN]\n"
        );
        let schema = json!({
            "type": "object",
            "properties": {
                "landlord": {
                    "type": "object",
                    "x-anno-column": {
                        "name": "landlord",
                        "prompt": "Landlord legal name",
                        "cell_type": { "kind": "text" },
                        "extraction": {
                            "mode": "local_span",
                            "labels": [{ "name": "bailleur", "description": "Nom du bailleur" }],
                            "threshold": 0.45
                        }
                    }
                }
            }
        });

        let client = LocalTabularClient::new_for_tests(Box::new(MockExtractor));
        let out = client.generate_structured("", &user, &schema).await.expect("extract");

        assert_eq!(out.value["landlord"]["value"], "ACME SAS");
        assert_eq!(out.value["landlord"]["citations"][0]["chunk_id"], chunk_id.to_string());
        assert_eq!(out.value["landlord"]["citations"][0]["quoted_text"], "ACME SAS");
    }
}
```

- [ ] **Step 4: Implement local client minimum**

Implement:

```rust
use super::{normalizers::normalize_value, offsets::char_span_to_byte_span, prompt::parse_user_prompt};
use crate::llm::{LlmClient, StructuredOutput, Usage};
use crate::schema::CellType;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

pub struct LocalTabularClient {
    extractor: Box<dyn LocalEntityExtractor>,
}

impl LocalTabularClient {
    #[cfg(test)]
    pub fn new_for_tests(extractor: Box<dyn LocalEntityExtractor>) -> Self {
        Self { extractor }
    }
}

#[derive(Debug, Deserialize)]
struct ColumnMeta {
    name: String,
    prompt: String,
    cell_type: CellType,
    extraction: crate::schema::ExtractionSpec,
}

#[async_trait]
impl LlmClient for LocalTabularClient {
    async fn generate_structured(
        &self,
        _system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        let parsed = parse_user_prompt(user)?;
        let mut result = serde_json::Map::new();
        let props = json_schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        for (name, prop) in props {
            let Some(meta_val) = prop.get("x-anno-column") else { continue; };
            let meta: ColumnMeta = serde_json::from_value(meta_val.clone())?;
            if !matches!(
                meta.extraction.mode,
                crate::schema::ExtractionMode::LocalSpan | crate::schema::ExtractionMode::LocalClause
            ) {
                continue;
            }
            let labels: Vec<(&str, &str)> = meta
                .extraction
                .labels
                .iter()
                .map(|l| (l.name.as_str(), l.description.as_str()))
                .collect();
            let threshold = meta.extraction.threshold.unwrap_or(0.45);

            let mut best: Option<(uuid::Uuid, String, usize, usize, f32)> = None;
            for chunk in &parsed.chunks {
                let entities = self.extractor.extract(&chunk.text, &labels, threshold)?;
                for ent in entities {
                    let Some(span) = char_span_to_byte_span(&chunk.text, ent.start_char, ent.end_char) else {
                        continue;
                    };
                    let Some(quote) = chunk.text.get(span.start..span.end) else {
                        continue;
                    };
                    if quote != ent.text {
                        continue;
                    }
                    if best.as_ref().map(|b| ent.confidence > b.4).unwrap_or(true) {
                        best = Some((chunk.id, quote.to_string(), span.start, span.end, ent.confidence));
                    }
                }
            }

            if let Some((chunk_id, quote, start, end, confidence)) = best {
                if let Some(value) = normalize_value(&quote, &meta.cell_type) {
                    result.insert(name, json!({
                        "value": value,
                        "reasoning": format!("Local GLiNER extraction with confidence {:.2}", confidence),
                        "citations": [{
                            "chunk_id": chunk_id.to_string(),
                            "byte_start": start,
                            "byte_end": end,
                            "quoted_text": quote
                        }]
                    }));
                }
            }
        }

        Ok(StructuredOutput {
            value: Value::Object(result),
            usage: Usage::default(),
        })
    }

    fn model_id(&self) -> &str {
        "local-tabular-gliner2"
    }
}
```

- [ ] **Step 5: Run local client tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular local_client_emits_cited_cell_for_local_span
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/local
git commit -m "feat(tabular): add local tabular extraction client"
```

---

## Task 8: Add Full Anno GLiNER2/Fastino Legal Signal Adapter

**Files:**
- Modify: `crates/anno-rag-tabular/Cargo.toml`
- Modify: `crates/anno-rag-tabular/src/llm/local/client.rs`
- Create: `crates/anno-rag-tabular/src/llm/local/legal_signals.rs`
- Modify: `crates/anno-rag-tabular/src/llm/local/mod.rs`

- [ ] **Step 1: Add dependency**

In `crates/anno-rag-tabular/Cargo.toml`, add:

```toml
anno = { path = "../anno", features = ["gliner2-fastino"] }
```

The feature name is validated against `crates/anno/Cargo.toml`, where `gliner2-fastino` enables the ONNX-backed Fastino GLiNER2 backend.

- [ ] **Step 2: Export legal signal module**

In `local/mod.rs`:

```rust
pub mod legal_signals;
```

- [ ] **Step 3: Add legal signal provider test**

Create `legal_signals.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_default_legal_catalog_to_labels_and_thresholds() {
        let catalog = LegalSignalCatalog::default();

        assert!(catalog.label("contract_party").is_some());
        assert!(catalog.label("amount").is_some());
        assert_eq!(catalog.threshold("company_identifier"), Some(0.90));
        assert_eq!(catalog.threshold("obligation"), Some(0.55));
    }

    #[test]
    fn chooses_label_thresholds_for_catalog_backed_column() {
        let catalog = LegalSignalCatalog::default();
        let plan = catalog.plan_for_labels(&["contract_party", "amount"]);

        assert_eq!(plan.label_thresholds, vec![("contract_party", 0.65), ("amount", 0.80)]);
        assert!(plan.label_descriptions.iter().any(|(name, _)| *name == "contract_party"));
    }
}
```

- [ ] **Step 4: Implement legal signal catalog**

In `legal_signals.rs`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LegalSignalCatalog {
    labels: Vec<anno_rag::legal::LegalLabel>,
    thresholds: HashMap<&'static str, f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegalSignalPlan {
    pub label_descriptions: Vec<(&'static str, &'static str)>,
    pub label_thresholds: Vec<(&'static str, f32)>,
}

impl Default for LegalSignalCatalog {
    fn default() -> Self {
        Self {
            labels: anno_rag::legal::default_legal_labels(),
            thresholds: anno_rag::legal::default_thresholds(),
        }
    }
}

impl LegalSignalCatalog {
    pub fn label(&self, name: &str) -> Option<anno_rag::legal::LegalLabel> {
        self.labels.iter().copied().find(|label| label.name == name)
    }

    pub fn threshold(&self, name: &str) -> Option<f32> {
        self.thresholds.get(name).copied()
    }

    pub fn plan_for_labels(&self, names: &[&'static str]) -> LegalSignalPlan {
        let mut label_descriptions = Vec::new();
        let mut label_thresholds = Vec::new();
        for name in names {
            if let Some(label) = self.label(name) {
                label_descriptions.push((label.name, label.description));
                if let Some(threshold) = self.threshold(label.name) {
                    label_thresholds.push((label.name, threshold));
                }
            }
        }
        LegalSignalPlan {
            label_descriptions,
            label_thresholds,
        }
    }
}
```

- [ ] **Step 5: Add GLiNER2 adapter**

In `client.rs`:

```rust
pub struct Gliner2EntityExtractor {
    model: std::sync::Arc<anno::backends::gliner2_fastino::GLiNER2Fastino>,
}

impl Gliner2EntityExtractor {
    pub fn new(model: anno::backends::gliner2_fastino::GLiNER2Fastino) -> Self {
        Self {
            model: std::sync::Arc::new(model),
        }
    }
}

impl LocalEntityExtractor for Gliner2EntityExtractor {
    fn extract(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>> {
        let entities = self
            .model
            .extract_with_label_descriptions(text, labels, threshold)
            .map_err(|e| crate::error::Error::Extract {
                doc: "local".into(),
                col: "*".into(),
                source: e.to_string().into(),
            })?;

        Ok(entities
            .into_iter()
            .map(|e| LocalEntity {
                text: e.text,
                start_char: e.start(),
                end_char: e.end(),
                confidence: f64::from(e.confidence) as f32,
            })
            .collect())
    }
}
```

- [ ] **Step 6: Add richer GLiNER2 operations**

Extend `LocalEntityExtractor` or add a sibling trait so the adapter can use more than one GLiNER2 operation:

```rust
pub trait LocalLegalSignalExtractor: Send + Sync {
    fn extract_with_descriptions(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>>;

    fn extract_with_thresholds(
        &self,
        text: &str,
        label_thresholds: &[(&str, f32)],
    ) -> Result<Vec<LocalEntity>>;

    fn classify(&self, text: &str, labels: &[&str]) -> Result<Vec<(String, f32)>>;
}
```

Implement it for `Gliner2EntityExtractor` using:

- `GLiNER2Fastino::extract_with_label_descriptions`
- `GLiNER2Fastino::extract_with_label_thresholds`
- `GLiNER2Fastino::classify`

Keep `extract_structure` behind a smaller helper for grouped fields:

```rust
fn extract_grouped_structure(
    model: &anno::backends::gliner2_fastino::GLiNER2Fastino,
    text: &str,
    schema: &anno::backends::gliner2_fastino::schema::TaskSchema,
    threshold: f32,
) -> Result<Vec<anno::backends::gliner2_fastino::schema::ExtractedStructure>> {
    model.extract_structure(text, schema, threshold).map_err(|e| crate::error::Error::Extract {
        doc: "local".into(),
        col: "*".into(),
        source: e.to_string().into(),
    })
}
```

- [ ] **Step 7: Add ignored live test**

```rust
#[tokio::test]
#[ignore = "loads local GLiNER2 model weights"]
async fn local_gliner2_adapter_extracts_party_name() {
    let model = anno::backends::gliner2_fastino::GLiNER2Fastino::from_pretrained(
        "SemplificaAI/gliner2-multi-v1-onnx",
    )
    .expect("model");
    let extractor = Gliner2EntityExtractor::new(model);
    let out = extractor
        .extract(
            "Entre les soussignes, ACME SAS agit comme bailleur.",
            &[("bailleur", "Nom complet et forme juridique du bailleur")],
            0.3,
        )
        .expect("extract");

    assert!(out.iter().any(|e| e.text.contains("ACME")));
}
```

- [ ] **Step 8: Add ignored IoBinding smoke test**

```rust
#[tokio::test]
#[ignore = "loads local GLiNER2 model weights and IoBinding session variants"]
async fn local_gliner2_iobinding_smoke() {
    use anno::backends::gliner2_fastino::{
        ExecutionMode, GLiNER2Fastino, GLiNER2FastinoConfig,
    };

    let model = GLiNER2Fastino::from_pretrained_with_config(
        "SemplificaAI/gliner2-multi-v1-onnx",
        GLiNER2FastinoConfig::default().with_execution_mode(ExecutionMode::IoBinding),
    )
    .expect("model");

    let out = model
        .classify("Le contrat est soumis au droit français.", &["FR", "OTHER"], 0.0)
        .expect("classify");

    assert!(!out.is_empty());
}
```

- [ ] **Step 9: Run non-ignored local tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular llm::local
```

Expected: pass without downloading model weights.

- [ ] **Step 10: Commit**

```powershell
git add crates/anno-rag-tabular/Cargo.toml crates/anno-rag-tabular/src/llm/local
git commit -m "feat(tabular): reuse anno gliner2 legal signal stack"
```

---

## Task 9: Add RoutingLlmClient

**Files:**
- Create: `crates/anno-rag-tabular/src/llm/routing.rs`
- Create: `crates/anno-rag-tabular/src/llm/privacy.rs`
- Modify: `crates/anno-rag-tabular/src/llm/mod.rs`

- [ ] **Step 1: Export routing module**

In `llm/mod.rs`:

```rust
pub mod privacy;
pub mod routing;
```

- [ ] **Step 2: Add routing test**

In `routing.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmClient, StructuredOutput, Usage};
    use async_trait::async_trait;
    use serde_json::{json, Value};

    struct StaticClient {
        id: &'static str,
        value: Value,
    }

    #[async_trait]
    impl LlmClient for StaticClient {
        async fn generate_structured(
            &self,
            _system: &str,
            _user: &str,
            _json_schema: &Value,
        ) -> crate::error::Result<StructuredOutput> {
            Ok(StructuredOutput {
                value: self.value.clone(),
                usage: Usage::default(),
            })
        }

        fn model_id(&self) -> &str {
            self.id
        }
    }

    #[tokio::test]
    async fn routing_merges_local_and_llm_outputs_without_raw_pii() {
        let local = Box::new(StaticClient {
            id: "local",
            value: json!({ "landlord": { "value": "ACME SAS", "reasoning": "local", "citations": [] } }),
        });
        let llm = Box::new(StaticClient {
            id: "llm",
            value: json!({ "repair_obligations": { "value": "gross repairs", "reasoning": "llm", "citations": [] } }),
        });
        let router = RoutingLlmClient::new(local, Some(llm));

        let user = "[CHUNK::018f0000-0000-7000-8000-000000000001]ORG_1 signe le contrat.[/CHUNK]\n";
        let out = router.generate_structured("", user, &json!({ "type": "object" })).await.expect("route");

        assert_eq!(out.value["landlord"]["value"], "ACME SAS");
        assert_eq!(out.value["repair_obligations"]["value"], "gross repairs");
    }

    #[tokio::test]
    async fn routing_aborts_fallback_when_prompt_contains_clear_pii() {
        let local = Box::new(StaticClient {
            id: "local",
            value: json!({}),
        });
        let llm = Box::new(StaticClient {
            id: "llm",
            value: json!({ "unsafe": { "value": "should not happen", "reasoning": "llm", "citations": [] } }),
        });
        let router = RoutingLlmClient::new(local, Some(llm));

        let user = "[CHUNK::018f0000-0000-7000-8000-000000000001]Contact: marie.dupont@example.com[/CHUNK]\n";
        let out = router.generate_structured("", user, &json!({ "type": "object" })).await.expect("route");

        assert!(out.value.as_object().unwrap().is_empty());
    }
}
```

- [ ] **Step 3: Implement output merge**

```rust
use crate::llm::{privacy::fallback_prompt_is_safe, LlmClient, StructuredOutput, Usage};
use async_trait::async_trait;
use serde_json::Value;

pub struct RoutingLlmClient {
    local: Box<dyn LlmClient>,
    fallback: Option<Box<dyn LlmClient>>,
}

impl RoutingLlmClient {
    pub fn new(local: Box<dyn LlmClient>, fallback: Option<Box<dyn LlmClient>>) -> Self {
        Self { local, fallback }
    }
}

#[async_trait]
impl LlmClient for RoutingLlmClient {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        let mut local = self.local.generate_structured(system, user, json_schema).await?;

        if let Some(fallback) = &self.fallback {
            if !fallback_prompt_is_safe(user) {
                return Ok(local);
            }
            let llm = fallback.generate_structured(system, user, json_schema).await?;
            merge_objects(&mut local.value, llm.value);
            local.usage.input_tokens += llm.usage.input_tokens;
            local.usage.output_tokens += llm.usage.output_tokens;
            local.usage.cache_read_tokens += llm.usage.cache_read_tokens;
            local.usage.cache_create_tokens += llm.usage.cache_create_tokens;
        }

        Ok(local)
    }

    fn model_id(&self) -> &str {
        "routing-local-tabular"
    }
}

fn merge_objects(dst: &mut Value, src: Value) {
    let Some(dst_obj) = dst.as_object_mut() else { return; };
    let Some(src_obj) = src.as_object() else { return; };
    for (key, value) in src_obj {
        dst_obj.entry(key.clone()).or_insert_with(|| value.clone());
    }
}
```

Create `privacy.rs`:

```rust
use regex::Regex;

pub fn fallback_prompt_is_safe(prompt: &str) -> bool {
    obvious_email_absent(prompt)
        && obvious_phone_absent(prompt)
        && obvious_iban_absent(prompt)
        && obvious_siren_absent(prompt)
}

fn obvious_email_absent(text: &str) -> bool {
    let re = Regex::new(r"(?i)[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}").expect("email regex");
    !re.is_match(text)
}

fn obvious_phone_absent(text: &str) -> bool {
    let re = Regex::new(r"(?x)(?:\+33|0)\s*[1-9](?:[\s.\-]?\d{2}){4}").expect("phone regex");
    !re.is_match(text)
}

fn obvious_iban_absent(text: &str) -> bool {
    let re = Regex::new(r"(?i)\bFR\d{2}(?:[\s]?[0-9A-Z]){23}\b").expect("iban regex");
    !re.is_match(text)
}

fn obvious_siren_absent(text: &str) -> bool {
    let re = Regex::new(r"\b\d{3}\s?\d{3}\s?\d{3}\b").expect("siren regex");
    !re.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_pseudonymized_prompt() {
        assert!(fallback_prompt_is_safe("ORG_1 signe avec PERSON_1."));
    }

    #[test]
    fn rejects_clear_email() {
        assert!(!fallback_prompt_is_safe("Contact: marie.dupont@example.com"));
    }

    #[test]
    fn rejects_clear_french_phone() {
        assert!(!fallback_prompt_is_safe("Tel: 06 12 34 56 78"));
    }
}
```

- [ ] **Step 4: Refine routing partition and fallback prompt**

After the merge test passes, add helpers to filter the schema by `x-anno-column.extraction.mode`:

```rust
fn is_local_mode(mode: &str) -> bool {
    matches!(mode, "local_span" | "local_clause" | "local_classifier")
}

fn is_fallback_mode(mode: &str) -> bool {
    matches!(mode, "llm_required" | "auto")
}
```

Add tests that local receives only local-safe columns and fallback receives only `llm_required`/`auto` columns. Keep `manual` out of both.

Also build a fallback user prompt that removes `[COLUMN::...]` sections for local-only/manual columns before calling fallback. The fallback may keep pseudonymized chunks in the first implementation, but it must never call fallback with raw chunks or with a prompt that fails `fallback_prompt_is_safe`.

- [ ] **Step 5: Run routing tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular routing
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular privacy
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/src/llm/routing.rs
git commit -m "feat(tabular): add local-first routing client"
```

---

## Task 10: Add Quality Evaluation Fixtures

**Files:**
- Create: `crates/anno-rag-tabular/tests/local_quality.rs`
- Create: `crates/anno-rag-tabular/tests/fixtures/local_real_estate_chunks.json`

- [ ] **Step 1: Add fixture**

Create `local_real_estate_chunks.json`:

```json
[
  {
    "id": "018f0000-0000-7000-8000-000000000001",
    "text": "Entre les soussignes, ACME SAS, bailleur, et BETA SARL, preneur, il est convenu un bail commercial. Le loyer annuel est fixe a 12 000 EUR hors taxes. Le depot de garantie est de 3 000 EUR."
  }
]
```

- [ ] **Step 2: Add quality test with mock extractor**

In `local_quality.rs`, assert that local-safe cells are emitted and unsupported legal fields are omitted:

```rust
#[tokio::test]
async fn local_quality_extracts_safe_fields_and_abstains_on_clause_reasoning() {
    let template = anno_rag_tabular::schema::Template::builtin("real-estate-v1")
        .expect("template");
    let columns = template.columns;

    assert!(columns.iter().any(|c| c.name == "landlord"));
    assert!(columns.iter().any(|c| c.name == "repair_obligations"));

    let repair = columns.iter().find(|c| c.name == "repair_obligations").unwrap();
    assert_ne!(repair.extraction.mode, anno_rag_tabular::schema::ExtractionMode::LocalSpan);
}
```

- [ ] **Step 3: Run quality tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular --test local_quality
```

Expected: pass.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-tabular/tests/local_quality.rs crates/anno-rag-tabular/tests/fixtures/local_real_estate_chunks.json
git commit -m "test(tabular): add local extraction quality fixtures"
```

---

## Task 11: Final Verification

**Files:** No edits.

- [ ] **Step 1: Run the optimized targeted tabular check**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check -Profile dev-fast -NoSccache -PrintOnly
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check -Profile dev-fast -NoSccache
```

Expected: `cargo check --profile dev-fast -p anno-rag-tabular` completes.

- [ ] **Step 2: Run targeted tabular tests**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" test --profile dev-fast -p anno-rag-tabular
```

Expected: all non-ignored tests pass.

- [ ] **Step 3: Run the final all-targets gate once**

```powershell
$env:CARGO_TARGET_DIR='D:\cargo-shared-target'
cargo --config "build.rustc-wrapper = ''" check --profile dev-fast -p anno-rag-tabular --all-targets
```

Expected: all tabular targets compile. Keep this as the final gate, not the per-step loop.

- [ ] **Step 4: Run GitNexus change detection before final commit or PR**

If the GitNexus MCP tool is available, run:

```text
gitnexus_detect_changes({scope: "all"})
```

If only CLI is available and no detect-changes command exists, record that limitation in the final handoff and use:

```powershell
git diff --stat
git diff --name-only
```

- [ ] **Step 5: Review generated diffs**

```powershell
git diff -- crates/anno-rag-tabular
```

Expected:

- no changes to MCP transport,
- no changes to unrelated crates except direct `anno` dependency if required,
- tests and template metadata included.

- [ ] **Step 5: Final commit**

```powershell
git add crates/anno-rag-tabular
git commit -m "feat(tabular): add local-first legal extraction"
```

---

## Self-Review

Spec coverage:

- Extraction modes are implemented in Tasks 1-2.
- Citation and offset safety is covered in Task 5 and preserved by existing verifier.
- Local candidate generation and full Anno GLiNER2 legal signal reuse are covered in Tasks 7-8.
- Conservative abstention is covered by Task 7 and Task 10.
- Routing to optional LLM and PII preflight guarding are covered in Task 9.
- Template quality metadata is covered in Task 2.

Placeholder scan:

- No task contains unresolved placeholder language or unspecified "add tests" instructions.
- Each test task includes concrete test code or exact expected assertions.
- Each command is explicit and uses `D:\cargo-shared-target`.

Type consistency:

- `ExtractionMode`, `ExtractionSpec`, `ExtractionLabel`, and `ExtractionNormalizer` are introduced in Task 1 and reused consistently.
- `LocalEntityExtractor` is introduced before `LocalTabularClient`.
- `RoutingLlmClient` wraps existing `LlmClient` objects without changing `Extractor`.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-27-anno-tabular-local-legal-extraction-quality.md`.

Two execution options:

1. **Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks, faster iteration.
2. **Inline Execution** - execute tasks in this session using executing-plans, with checkpoints.

The recommended path is Subagent-Driven because schema/storage/local extraction/routing are separable and should be reviewed independently.
