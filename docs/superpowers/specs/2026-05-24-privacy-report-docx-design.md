# Privacy Report (.docx) — Implementation Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** After every document ingest, auto-generate a local `.docx` report showing the original text alongside the pseudonymized version so the user can audit exactly what PII was found and replaced — without the content ever crossing the LLM boundary.

**Architecture:** A new `privacy_report` module in `anno-rag` owns all report generation. It is called non-fatally at the end of `ingest_one` and exposed on-demand via a new `anno_privacy_report` MCP tool. A JSON sidecar persisted alongside the `.docx` enables on-demand regeneration without re-running NER.

**Tech Stack:** `docx-rs` (pure-Rust .docx writer), `serde_json` (sidecar), existing `anno-rag` pipeline

---

## 1. Components

### 1.1 `crates/anno-rag/src/privacy_report.rs` (new)

Public API:

```rust
/// All data needed to produce one privacy report.
pub struct PrivacyReportInput {
    /// Original filename stem, e.g. "contrat_dupont" (no extension).
    pub doc_name: String,
    /// UTC timestamp of ingest.
    pub ingested_at: chrono::DateTime<chrono::Utc>,
    /// One entry per chunk: (original_text, pseudonymized_text, entity_map).
    /// entity_map: surface_form → token (e.g. "Jean Dupont" → "PERSON_1").
    pub chunks: Vec<ChunkReport>,
}

pub struct ChunkReport {
    pub original: String,
    pub pseudonymized: String,
    /// Ordered by first occurrence in the chunk.
    pub entities: Vec<EntityEntry>,
}

pub struct EntityEntry {
    pub surface: String,   // "Jean Dupont"
    pub token: String,     // "PERSON_1"
    pub category: String,  // "PER"
}

/// Generate the .docx report and JSON sidecar.
/// Returns the path to the .docx file.
/// Non-fatal: logs a warning and returns Err on write failure.
pub fn generate(input: &PrivacyReportInput, outputs_dir: &Path) -> Result<PathBuf>;

/// Load a previously saved sidecar and regenerate the .docx.
pub fn regenerate_from_sidecar(doc_name: &str, outputs_dir: &Path) -> Result<PathBuf>;
```

### 1.2 Sidecar: `<doc_name>_privacy_map.json`

Saved to `~/.anno-rag/outputs/`. Contains the `PrivacyReportInput` serialized as JSON (where `ingested_at` is an ISO 8601 string via `serde`). Used by on-demand regeneration so the original file does not need to be on disk.

### 1.3 Report: `<doc_name>_privacy_report.docx`

Saved to `~/.anno-rag/outputs/`. Overwritten on re-ingest or on-demand call.

### 1.4 `ingest_one` change (`pipeline.rs`)

After `vault.pseudonymize_with_map()` returns, collect `ChunkReport` entries. After the LanceDB write, call `privacy_report::generate(...)`. Failure is non-fatal:

```rust
if let Err(e) = privacy_report::generate(&report_input, &self.cfg.outputs_dir()) {
    tracing::warn!(error = %e, doc = %doc_name, "privacy report generation failed (non-fatal)");
}
```

### 1.5 `anno_privacy_report` MCP tool (`anno-rag-mcp/src/lib.rs`)

```
Tool: anno_privacy_report
Input: { "doc_name": "contrat_dupont" }
Output: { "report_path": "C:\\Users\\...\\contrat_dupont_privacy_report.docx" }
        or { "error": "no privacy map found for contrat_dupont" }
```

The LLM receives **only the file path string** — never the report contents.

---

## 2. Data Flow

### Ingest path (automatic)

```
ingest_one(file_path)
  → chunk text
  → detect PII entities per chunk        (GLiNER2, in memory)
  → vault.pseudonymize_with_map()        → (pseudo_text, entity_map) per chunk
  → embed pseudo_text → LanceDB
  → privacy_report::generate({
        doc_name, ingested_at, chunks: [(orig, pseudo, entities), ...]
    })
  → ~/.anno-rag/outputs/contrat_dupont_privacy_report.docx   ← local only
  → ~/.anno-rag/outputs/contrat_dupont_privacy_map.json      ← local only
```

### On-demand path (MCP tool)

```
anno_privacy_report({ doc_name: "contrat_dupont" })
  → read ~/.anno-rag/outputs/contrat_dupont_privacy_map.json
  → privacy_report::generate(input from sidecar)
  → overwrite contrat_dupont_privacy_report.docx
  → return { "report_path": "/absolute/path/to/contrat_dupont_privacy_report.docx" }
```

---

## 3. Report Structure (.docx)

```
Title page
──────────
  ANNO-RAG PRIVACY REPORT
  Document : contrat_dupont.pdf
  Ingested : 2026-05-24 17:42 UTC
  Entities : 23 found across 5 chunks

Summary table
─────────────
  Category      | Count | Tokens
  PERSON        |   8   | PERSON_1 … PERSON_8
  ORGANIZATION  |   4   | ORG_1 … ORG_4
  DATE          |   6   | DATE_1 … DATE_6
  LOCATION      |   3   | LOC_1 … LOC_3
  AMOUNT        |   2   | AMOUNT_1 … AMOUNT_2

Per-chunk diff (one section per chunk)
──────────────────────────────────────
  CHUNK 1 / 5
  ┌─────────────────────┬─────────────────────────┐
  │ ORIGINAL            │ PSEUDONYMIZED           │
  │ **Jean Dupont** a   │ PERSON_1 a signé le     │
  │ signé le contrat    │ contrat avec ORG_1 le   │
  │ avec **Acme SA** le │ DATE_1 pour AMOUNT_1.   │
  │ **12 mars 2024**    │                         │
  │ pour **50 000 €**.  │                         │
  └─────────────────────┴─────────────────────────┘
  (entities bolded in ORIGINAL column)

Footer (every page)
───────────────────
  ⚠ Generated locally by anno-rag. Never transmitted to any AI model.
```

---

## 4. File Naming

| Input | .docx | sidecar |
|---|---|---|
| `contrat_dupont.pdf` | `contrat_dupont_privacy_report.docx` | `contrat_dupont_privacy_map.json` |
| `Invoice 2024-Q1.pdf` | `Invoice_2024-Q1_privacy_report.docx` | `Invoice_2024-Q1_privacy_map.json` |

Spaces in filenames are replaced with `_`. Re-ingest overwrites both files (no history accumulation).

---

## 5. Error Handling

| Situation | Behaviour |
|---|---|
| `docx-rs` write fails (disk full, permissions) | `tracing::warn!`, ingest continues, no `.docx` written |
| Sidecar missing on on-demand call | MCP tool returns `{"error": "no privacy map found for <doc_name>"}` |
| Sidecar JSON malformed | MCP tool returns `{"error": "privacy map corrupted for <doc_name>"}` |
| `outputs_dir` does not exist | `std::fs::create_dir_all` before write, fail only if that fails |

---

## 6. Privacy Guarantee

- The `.docx` and `.json` files are written only to `~/.anno-rag/outputs/` on the user's local machine.
- The `anno_privacy_report` MCP tool returns a file **path** string only — the contents are never included in any tool response.
- No network call is made during report generation.
- The LLM can tell the user *"your report is at X"* but cannot read it.

---

## 7. New Dependency

```toml
# crates/anno-rag/Cargo.toml
[dependencies]
docx-rs = "0.4"
```

`docx-rs` is pure Rust (no system libraries), MIT licensed, ~500 KB compiled.

---

## 8. Tests

| Test | What it checks |
|---|---|
| `generate_writes_docx_and_sidecar` | 2 synthetic chunks → both files exist, JSON round-trips |
| `regenerate_from_sidecar_matches` | sidecar → regenerated `.docx` identical to original |
| `privacy_boundary` | MCP response contains only a path string, not entity text |
| `ingest_produces_report` | Integration: ingest sample file → `outputs/<name>_privacy_report.docx` exists |
| `generate_non_fatal_on_bad_dir` | write to non-writable dir → `Err(...)`, no panic |
