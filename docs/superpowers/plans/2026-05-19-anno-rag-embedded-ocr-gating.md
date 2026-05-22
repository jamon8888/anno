# Embedded OCR Gating Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add embedded Kreuzberg OCR as a build/runtime-gated path that runs only for scanned PDFs or weak pages in mixed PDFs.

**Architecture:** `ingest.rs` performs a native no-OCR pass, classifies the document/page state, then optionally performs a second embedded OCR pass. `config.rs` owns runtime OCR mode and defaults. `pipeline.rs` keeps the public ingest signatures stable and skips deferred OCR documents without writing empty outputs.

**Tech Stack:** Rust, Cargo features, Kreuzberg 4.9.7 `pdf`/`chunking`/`ocr`, Tokio tests.

---

### Task 1: Cargo Feature And Runtime Config

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag/src/config.rs`
- Modify: `crates/anno-rag/src/main.rs`

- [x] **Step 1: Add failing config tests**

Add tests asserting:

```rust
assert_eq!(AnnoRagConfig::default().ocr_mode, OcrMode::Off);
assert_eq!(AnnoRagConfig { enable_ocr: true, ..Default::default() }.effective_ocr_mode(), OcrMode::AutoEmbedded);
```

- [x] **Step 2: Run config tests**

Run: `cargo test -p anno-rag config::tests --lib`
Expected: FAIL because `OcrMode` does not exist yet.

- [x] **Step 3: Implement config**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    Off,
    AutoEmbedded,
}
```

Add `ocr_mode: OcrMode`, `ocr_batch_budget_secs: Option<u64>`, defaults, and `effective_ocr_mode()`.

- [x] **Step 4: Wire CLI compatibility**

Keep `--enable-ocr`, but set `cfg.ocr_mode = OcrMode::AutoEmbedded` when passed. Update help text from system Tesseract to embedded OCR.

- [x] **Step 5: Run config tests**

Run: `cargo test -p anno-rag config::tests --lib`
Expected: PASS.

### Task 2: Native Classification And Embedded OCR Extraction

**Files:**
- Modify: `crates/anno-rag/src/ingest.rs`
- Modify: `crates/anno-rag/Cargo.toml`

- [x] **Step 1: Add failing classifier tests**

Add tests for `TextLayer`, `ScannedPdf`, and `MixedPdf { ocr_pages }` using synthetic `PageContent` values.

- [x] **Step 2: Run ingest tests**

Run: `cargo test -p anno-rag ingest::tests::classifies --lib`
Expected: FAIL because `DocClass` does not exist yet.

- [x] **Step 3: Implement classification**

Add:

```rust
pub enum DocClass {
    TextLayer,
    ScannedPdf,
    MixedPdf { ocr_pages: Vec<usize> },
    Empty,
}
```

Run Kreuzberg first with `disable_ocr = true`, classify PDFs by `pages` where present, and use content fallback where pages are absent.

- [x] **Step 4: Add embedded OCR branch**

Add `embedded-ocr = ["kreuzberg/ocr"]` to `crates/anno-rag/Cargo.toml`. Under `#[cfg(feature = "embedded-ocr")]`, perform the second Kreuzberg extraction with `ocr: Some(OcrConfig { backend: "tesseract", language: "fra+eng", ..Default::default() })`; use `force_ocr` for `ScannedPdf` and `force_ocr_pages` for `MixedPdf`.

- [x] **Step 5: Run ingest tests**

Run: `cargo test -p anno-rag ingest::tests --lib`
Expected: PASS.

### Task 3: Pipeline Deferral Guard

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`

- [x] **Step 1: Add failing skip test**

Add a focused test or assertion path showing a deferred OCR doc does not write an empty anonymized output.

- [x] **Step 2: Implement skip**

In `ingest_one`, if `ExtractedDoc` reports `ocr_status.is_deferred()`, log and return `Ok(())` before detector/embedder work. In `ingest_folder`, count only files that produce indexed chunks.

- [x] **Step 3: Run focused tests**

Run: `cargo test -p anno-rag pipeline::tests --lib`
Expected: PASS.

### Task 4: Verification

**Files:**
- Verify only.

- [x] **Step 1: Default build**

Run: `cargo check -p anno-rag`
Expected: PASS; `kreuzberg/ocr` must not be required.

- [x] **Step 2: Embedded feature build**

Run: `cargo check -p anno-rag --features embedded-ocr`
Expected: PASS or report native OCR build dependency failure explicitly.

- [x] **Step 3: Full focused tests**

Run: `cargo test -p anno-rag --lib`
Expected: PASS.
