# Hacienda Tauri Format Ingestion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the Tauri workbench engine from text-only ingestion to normalized working documents for PDF, DOCX, XLSX, PPTX, email, and OCR-backed images/scanned PDFs.

**Architecture:** Add a format extraction boundary inside `hacienda-workbench-core` that converts every source into a normalized `NormalizedDocument` tree before anonymization. Use Kreuzberg first because `anno-rag` already depends on it for document extraction; keep OCR opt-in and record extraction provenance, warnings, page numbers, and source spans.

**Tech Stack:** Rust 1.95, `hacienda-workbench-core`, `anno-rag`, `kreuzberg`, `walkdir`, `serde`, SQLite metadata from the walking skeleton.

---

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

- Modify `crates/hacienda-workbench-core/Cargo.toml`
- Create `crates/hacienda-workbench-core/src/normalized.rs`
- Create `crates/hacienda-workbench-core/src/format/mod.rs`
- Create `crates/hacienda-workbench-core/src/format/text.rs`
- Create `crates/hacienda-workbench-core/src/format/kreuzberg.rs`
- Modify `crates/hacienda-workbench-core/src/ingest.rs`
- Modify `crates/hacienda-workbench-core/src/model.rs`
- Modify `crates/hacienda-workbench-core/src/store.rs`
- Test fixtures under `crates/hacienda-workbench-core/tests/fixtures/formats/`

## Tasks

### Task 1: Normalized Document Model

- [ ] Add `normalized.rs` with:

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

### Task 2: Format Extractor Trait

- [ ] Add `format/mod.rs`:

```rust
pub trait FormatExtractor: Send + Sync {
    fn supports(&self, path: &std::path::Path) -> bool;
    fn extract(&self, path: &std::path::Path) -> crate::Result<crate::normalized::NormalizedDocument>;
}
```

- [ ] Add `TextExtractor` for `.txt`, `.md`, `.markdown`.
- [ ] Add tests that unsupported extensions are rejected and text source remains unchanged.
- [ ] Run `cargo test -p hacienda-workbench-core format --lib`.

### Task 3: Kreuzberg Extractor

- [ ] Add `KreuzbergExtractor` that calls Kreuzberg for PDF/Office/email/HTML/image sources.
- [ ] Convert extractor output to paragraphs and page breaks when page metadata exists.
- [ ] Record warnings when OCR is unavailable or a file is partially extracted.
- [ ] Add tests with small fixtures. Use text fixtures for deterministic CI; mark heavy binary fixtures ignored if needed.
- [ ] Run `cargo test -p hacienda-workbench-core format --lib`.

### Task 4: Ingestion Pipeline Integration

- [ ] Replace `scan_text_folder` with `scan_folder_with_extractors`.
- [ ] Store normalized JSON alongside anonymized text.
- [ ] Keep `SourceDocument.status = Error` for failed extraction instead of aborting whole matter.
- [ ] Add integration test: one supported file, one unsupported file, one extraction error fixture.
- [ ] Run `cargo test -p hacienda-workbench-core ingest --lib`.

### Task 5: UI Surface

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
