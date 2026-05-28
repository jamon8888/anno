# Hacienda Tauri Format Ingestion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the Tauri workbench engine from text-only ingestion to normalized working documents for PDF, DOCX, XLSX, PPTX, email, and OCR-backed images/scanned PDFs.

**Architecture:** Reuse `anno_rag::ingest::extract` as the format extraction boundary and convert its `ExtractedDoc` output into the workbench's normalized document view before anonymization. Keep OCR opt-in through existing `anno-rag` configuration and record extraction provenance, warnings, page numbers, and source spans.

**Tech Stack:** Rust 1.95, `hacienda-workbench-core`, `anno-rag`, `kreuzberg`, `walkdir`, `serde`, SQLite metadata from the walking skeleton.

---

## Lean Validation

This plan should be simplified against the existing implementation. `anno-rag` already has Kreuzberg-based extraction in `anno_rag::ingest::extract`, with chunk provenance, OCR state, and supported file filtering in `Pipeline::ingest_folder`.

Apply these reductions before implementing:

- Do not create a new `format/` extractor subsystem for the first pass.
- Do not duplicate Kreuzberg wiring in `hacienda-workbench-core`.
- Add only the minimal workbench-facing normalized view needed by the UI. Prefer putting `NormalizedDocument`, `NormalizedBlock`, and source-span types in `model.rs` first; extract to `normalized.rs` only if `model.rs` becomes too large.
- Implement conversion from `anno_rag::ingest::ExtractedDoc` to the workbench view in `ingest.rs`.
- Add test fixtures only for behavior not already covered by `anno-rag` tests: per-file error isolation, UI-visible status/warnings, and read-only source preservation.

## Scope

In scope:

- Recognize PDF, DOCX, XLSX, PPTX, EML/MSG when supported by the extractor, HTML, TXT, MD, PNG/JPG/TIFF.
- Convert sources into normalized sections, paragraphs, tables, pages, attachments, and source spans.
- Feed normalized text into existing anonymization and working document storage.
- Preserve source file read-only behavior.
- Record extraction warnings and extraction engine version.

Out of scope:

- Faithful native editing of Office/PDF internals.
- SharePoint/OneDrive fetching.
- Full visual source viewer with page images.
- Legal workflow extraction.

## Files

- Modify `crates/hacienda-workbench-core/Cargo.toml` only if an existing `anno-rag` dependency feature is missing.
- Modify `crates/hacienda-workbench-core/src/model.rs`
- Modify `crates/hacienda-workbench-core/src/ingest.rs`
- Modify `crates/hacienda-workbench-core/src/store.rs`
- Optionally create `crates/hacienda-workbench-core/src/normalized.rs` only if normalized view types make `model.rs` too large.
- Test fixtures under `crates/hacienda-workbench-core/tests/fixtures/formats/`

## Tasks

### Task 1: Normalized Document Model

- [ ] Add these types to `model.rs` first. Move them to `normalized.rs` only if `model.rs` becomes too large:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NormalizedDocument {
    pub title: String,
    pub blocks: Vec<NormalizedBlock>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum NormalizedBlock {
    Paragraph { text: String, source: SourceSpan },
    Table { rows: Vec<Vec<String>>, source: SourceSpan },
    PageBreak { page: u32 },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub page: Option<u32>,
    pub byte_start: u32,
    pub byte_end: u32,
}
```

- [ ] Test JSON round-trip and `NormalizedDocument::plain_text()` helper.
- [ ] Run `cargo test -p hacienda-workbench-core normalized --lib`.

### Task 2: Reuse anno-rag Extraction

- [ ] Call `anno_rag::ingest::extract(path, &cfg)` from workbench ingestion.
- [ ] Convert `ExtractedDoc.content`, `ExtractedDoc.chunks`, `DocClass`, and `OcrStatus` into the workbench normalized view.
- [ ] Record OCR-deferred/unsupported/extraction-failed states as document warnings or `SourceDocument.status = Error`.
- [ ] Add tests that unsupported files fail per-file and source files remain unchanged.
- [ ] Run `cargo test -p hacienda-workbench-core ingest --lib`.

### Task 3: Ingestion Pipeline Integration

- [ ] Replace `scan_text_folder` with folder scanning that calls the `anno-rag` extraction adapter per file.
- [ ] Store normalized JSON alongside anonymized text.
- [ ] Keep `SourceDocument.status = Error` for failed extraction instead of aborting whole matter.
- [ ] Add integration test: one supported file, one unsupported file, one extraction error fixture.
- [ ] Run `cargo test -p hacienda-workbench-core ingest --lib`.

### Task 4: UI Surface

- [ ] Add file-type badges in `apps/hacienda-workbench/src/App.tsx`.
- [ ] Add extraction warning count in document list.
- [ ] Keep editor bound to normalized anonymized text only.
- [ ] Run `npm run build` inside `apps/hacienda-workbench`.

## Verification

Run:

```powershell
cargo test -p hacienda-workbench-core
cargo check -p hacienda-workbench-tauri
Set-Location apps\hacienda-workbench; npm run build; Set-Location ..\..
```

Manual smoke:

```text
Add a folder containing TXT, MD, PDF, DOCX, XLSX, PPTX, EML, and PNG.
Supported files appear in the matter document list.
Each supported file has a normalized anonymized working document.
Unsupported or failed files show an error status without blocking others.
Original files are unchanged.
```
