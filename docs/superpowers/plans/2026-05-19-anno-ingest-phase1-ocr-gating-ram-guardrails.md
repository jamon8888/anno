# Phase 1 — OCR Gating + RAM Guardrails — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace the implicit "empty PDF = OCR, one synthetic chunk, silent-empty on no-OCR" behavior with explicit per-document/page classification, embedded-Kreuzberg OCR behind a default-off Cargo feature + runtime `ocr_mode`, a per-batch OCR time budget with non-fatal **deferral**, a returned ingest outcome summary, and test-enforced RAM guardrails.

**Architecture:** `crates/anno-rag` only. `ingest::extract` does a native (OCR-disabled) Kreuzberg pass, classifies the doc (`DocClass`), and — only for scanned/mixed PDFs, only if `embedded-ocr` is compiled and `ocr_mode = auto_embedded` and the batch OCR budget remains — does a second Kreuzberg pass with `force_ocr`/`force_ocr_pages`, consuming Kreuzberg's real chunks (no synthetic chunk). Deferred/unavailable OCR is a typed non-fatal outcome; `ingest_one`/`ingest_folder` return an `IngestOutcome`. RAM invariants (single NER engine, per-doc buffer drop, peak-RSS ceiling) become tests.

**Tech Stack:** Rust, kreuzberg 4.9.7 (`ocr` feature = `dep:kreuzberg-tesseract`; `ExtractionConfig{ocr,force_ocr,force_ocr_pages,disable_ocr}`; `kreuzberg::core::config::{OcrConfig, ExtractionConfig, ChunkingConfig, ChunkerType}`), serde, tokio.

**Spec:** `docs/superpowers/specs/2026-05-19-anno-ingest-phase1-ocr-gating-ram-guardrails-design.md` (revised; `0fbcd180`+user edits) — parent research `2026-05-19-anno-local-ingest-architecture-research.md`.

**Grounding (verified — do not re-derive):**
- `crates/anno-rag/src/ingest.rs:50` `pub async fn extract(path:&Path, cfg:&AnnoRagConfig) -> Result<ExtractedDoc>`. `ExtractedDoc{ source_path:String, content:String, chunks:Vec<ExtractedChunk> }`; `ExtractedChunk{ idx:u32, text:String, char_start:u32, char_end:u32, page:Option<u32> }`. It builds `ChunkingConfig{max_characters:cfg.chunk_max_chars, overlap:cfg.chunk_overlap, chunker_type:ChunkerType::Markdown,..}` → `ExtractionConfig{chunking:Some(..),..}` → `kreuzberg::extract_file(path,None,&cfg).await`. Maps `result.chunks` (kreuzberg) → `ExtractedChunk` (`c.metadata.{chunk_index,byte_start,byte_end,first_page}`).
- Current OCR (ingest.rs:84-120): `is_pdf && content.trim().is_empty()` → if `cfg.enable_ocr` then `crate::ocr::ocr_pdf` (external system tesseract, `pub async fn ocr_pdf(&Path, Option<&PathBuf>) -> Result<String>`) collapsed into **one synthetic chunk** (lines 100-106); else `tracing::warn!` and falls through to **`Ok(ExtractedDoc{ empty content, empty chunks })`** — a silent-empty success, NOT an `Err`.
- `crates/anno-rag/src/config.rs`: serde `AnnoRagConfig`, `fn default_*()` helpers, `impl Default` (~174), `enable_ocr:bool`(43), `tesseract_path:Option<PathBuf>`(47). New fields follow `#[serde(default="default_x")]` + helper + Default-literal (mirror the `rerank_*`/`ingest_*` precedents).
- `crates/anno-rag/src/pipeline.rs`: `pub async fn ingest_one(&self, path:&Path, output_dir:&Path) -> Result<()>` (76); `pub async fn ingest_folder(&self, folder:&Path, recursive:bool, output_dir:&Path) -> Result<usize>` (133) — sequential `for path { ingest_one }` then `maybe_build_index`/`maybe_build_fts_index`, returns count. (PR #14: deterministic `doc_id` skip + `delete_doc_rows` already in `ingest_one`.) Callers of these: `crates/anno-rag-mcp/src/lib.rs`, `crates/anno-rag-bin/src/main.rs`.
- kreuzberg 4.9.7: `Cargo.toml` `ocr = ["dep:kreuzberg-tesseract", …]`. `ExtractionConfig` (src/core/config/extraction/core.rs:35) fields incl. `ocr:Option<OcrConfig>`(46), `force_ocr:bool`(50), `force_ocr_pages:Option<Vec<usize>>`(60, **1-indexed**, ignored if `force_ocr`), `disable_ocr:bool`(72, mutually exclusive with `force_ocr`). `OcrConfig`/`OcrQualityThresholds` re-exported from `kreuzberg::core::config`; `OcrConfig::default()` exists (targets Tesseract via the `ocr` feature).
- Cargo: workspace cold build ~10 min; always `CARGO_INCREMENTAL=0`, single cargo process at a time (Windows PDB/rlib races). Single test: `cargo test -p anno-rag --lib <path> -- --exact`. Heavy/`#[ignore]`: `-- --ignored --test-threads=1`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/anno-rag/Cargo.toml` | Modify | `embedded-ocr = ["kreuzberg/ocr"]` feature (default off) |
| `crates/anno-rag/src/config.rs` | Modify | `OcrMode` enum + `ocr_mode` + `ocr_batch_budget_secs` fields/defaults |
| `crates/anno-rag/src/ingest.rs` | Modify | `DocClass`, classifier, native-then-OCR two-pass, real-chunk OCR lane, deferral, `ExtractedDoc.class`/outcome |
| `crates/anno-rag/src/pipeline.rs` | Modify | `IngestOutcome`/`DocOutcome`; thread through `ingest_one`/`ingest_folder`; per-batch OCR budget; guardrail tests |
| `crates/anno-rag-mcp/src/lib.rs`, `crates/anno-rag-bin/src/main.rs` | Modify | Update the 2 callers for the new return types |
| `crates/anno-rag/tests/ingest_phase1.rs` | Create | Heavy integration: scanned multi-chunk, budget-defer+resume |

---

## Task 1: `embedded-ocr` Cargo feature (default off)

**Files:** Modify `crates/anno-rag/Cargo.toml`.

- [ ] **Step 1: Add the feature.** In `[features]` (next to existing features), add:
```toml
# Embedded Tesseract OCR via kreuzberg. Default OFF (the base build must
# not pull kreuzberg-tesseract). Phase-1 scanned-PDF support.
embedded-ocr = ["kreuzberg/ocr"]
```
Confirm `kreuzberg` is a normal (not optional) dep so `kreuzberg/ocr` is a valid feature path; if `kreuzberg` is `optional`, use `"kreuzberg?/ocr"` instead.

- [ ] **Step 2: Default build excludes tesseract.** Run: `CARGO_INCREMENTAL=0 cargo tree -p anno-rag -e normal 2>&1 | grep -c kreuzberg-tesseract`
Expected: `0` (no tesseract in the default tree).

- [ ] **Step 3: Feature build includes it + compiles.** Run: `CARGO_INCREMENTAL=0 cargo tree -p anno-rag --features embedded-ocr -e normal 2>&1 | grep -c kreuzberg-tesseract` → `≥1`; then `CARGO_INCREMENTAL=0 cargo check -p anno-rag --features embedded-ocr` → clean.

- [ ] **Step 4: Commit.**
```bash
git add crates/anno-rag/Cargo.toml Cargo.lock
git commit -m "feat(ingest): embedded-ocr Cargo feature (kreuzberg/ocr, default off)"
```

---

## Task 2: `OcrMode` + `ocr_batch_budget_secs` config

**Files:** Modify `crates/anno-rag/src/config.rs`. Test: its `#[cfg(test)] mod tests`.

- [ ] **Step 1: Failing test.** Add to `mod tests`:
```rust
#[test]
fn ocr_phase1_config_defaults() {
    let c = AnnoRagConfig::default();
    assert_eq!(c.ocr_mode, OcrMode::Off);
    assert_eq!(c.ocr_batch_budget_secs, None);
}
```

- [ ] **Step 2: Run, expect FAIL.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib config::tests::ocr_phase1_config_defaults -- --exact` → `cannot find type 'OcrMode'`.

- [ ] **Step 3: Implement.** Near the top of config.rs (with other public types):
```rust
/// How the ingest pipeline may use OCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrMode {
    /// Never OCR. Scanned PDFs/pages become a typed deferral.
    Off,
    /// OCR scanned PDFs/pages, only if the binary was built with
    /// `embedded-ocr`. Without that feature, scanned content defers.
    AutoEmbedded,
}
```
Add to `AnnoRagConfig` (after `tesseract_path`):
```rust
    /// OCR policy. Default: `Off` (no OCR; scanned content is deferred).
    #[serde(default = "default_ocr_mode")]
    pub ocr_mode: OcrMode,
    /// Cumulative OCR wall-time budget per `ingest_folder` run, in
    /// seconds. `None` = unbounded. Over-budget scanned docs defer.
    #[serde(default)]
    pub ocr_batch_budget_secs: Option<u64>,
```
Add helper near other `default_*`:
```rust
fn default_ocr_mode() -> OcrMode { OcrMode::Off }
```
Add to `impl Default for AnnoRagConfig` `Self{..}` literal:
```rust
            ocr_mode: default_ocr_mode(),
            ocr_batch_budget_secs: None,
```

- [ ] **Step 4: Run, expect PASS.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib config:: -- --test-threads=1` → all config tests green (incl. existing serde round-trip / v0.1-compat).

- [ ] **Step 5: Commit.**
```bash
git add crates/anno-rag/src/config.rs
git commit -m "feat(ingest): OcrMode + ocr_batch_budget_secs config"
```

---

## Task 3: `DocClass` + native-pass classifier

**Files:** Modify `crates/anno-rag/src/ingest.rs`. Test: its `#[cfg(test)] mod tests`.

- [ ] **Step 1: Failing test.** Add to `ingest.rs` `mod tests`:
```rust
#[test]
fn classify_native_text_and_empty() {
    use super::{classify, DocClass};
    // non-empty text, non-pdf → TextLayer
    assert!(matches!(classify("Some real content here.", false), DocClass::TextLayer));
    // empty content, pdf → ScannedPdf
    assert!(matches!(classify("   \n  ", true), DocClass::ScannedPdf));
    // empty content, non-pdf → Empty
    assert!(matches!(classify("", false), DocClass::Empty));
    // non-empty pdf → TextLayer
    assert!(matches!(classify("Article 1. Responsabilité.", true), DocClass::TextLayer));
}
```

- [ ] **Step 2: Run, expect FAIL.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib ingest::tests::classify_native_text_and_empty -- --exact` → `cannot find function 'classify'`.

- [ ] **Step 3: Implement.** Add to ingest.rs (module scope):
```rust
/// Classification of a document after a native (OCR-disabled) pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocClass {
    /// Usable native text — fast lane, no OCR.
    TextLayer,
    /// PDF with no usable native text — whole-doc OCR candidate.
    ScannedPdf,
    /// PDF with usable text on some pages, weak/empty on others
    /// (1-indexed page numbers needing OCR). Populated only when
    /// per-page native text is available; otherwise a weak PDF is
    /// `ScannedPdf`.
    MixedPdf { ocr_pages: Vec<usize> },
    /// Unsupported / genuinely empty non-PDF.
    Empty,
}

/// Classify from native-pass content. `is_pdf` = source has a `.pdf`
/// extension. v1 heuristic mirrors Kreuzberg's "empty/near-empty"
/// idea at the document level; per-page Mixed detection is added in
/// Task 4 where per-page text is available.
#[must_use]
pub fn classify(content: &str, is_pdf: bool) -> DocClass {
    if !content.trim().is_empty() {
        return DocClass::TextLayer;
    }
    if is_pdf {
        DocClass::ScannedPdf
    } else {
        DocClass::Empty
    }
}
```
Add `pub class: DocClass` to `ExtractedDoc`. In `extract`, make the native pass **deterministic** per spec §3.2: add `disable_ocr: true` to the existing native `ExtractionConfig` (so classification never accidentally OCRs):
```rust
    let extraction_config = ExtractionConfig {
        chunking: Some(chunking),
        disable_ocr: true, // native classification pass — never OCR here
        ..Default::default()
    };
```
After the native pass + chunk mapping, compute `let is_pdf = …(existing extension logic)…; let class = classify(&content, is_pdf);` and set it on the returned `ExtractedDoc`. **Remove** the old OCR block (lines ~84-120) — OCR moves to Task 4. `extract` returns `ExtractedDoc{ source_path, content, chunks, class }` with no OCR (a scanned PDF yields `class: ScannedPdf` + empty chunks, handled in Task 4/5).

> **Phase-1 scope note (spec §3.2 fidelity):** the v1 `classify` is **doc-level only** — it emits `TextLayer | ScannedPdf | Empty`. `MixedPdf { ocr_pages }` is *defined* (so the type + `should_ocr`/`force_ocr_pages` path are in place) but **not yet produced**: a partially-weak PDF classifies as `ScannedPdf` (whole-doc OCR). Per-page native-text quality scoring (kreuzberg per-page text + the `OcrQualityThresholds` heuristics) that would emit `MixedPdf` is an explicit **Phase-1.x follow-up**, not a silent omission. The spec permits this ("otherwise a weak PDF is `ScannedPdf`"); the plan states it so the gap is intentional and tracked.

- [ ] **Step 4: Run + compile.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib ingest::tests -- --exact` PASS; `CARGO_INCREMENTAL=0 cargo check -p anno-rag` clean (fix any `ExtractedDoc` construction sites flagged — there is one in `extract`; if tests construct it, add `class`).

- [ ] **Step 5: Commit.**
```bash
git add crates/anno-rag/src/ingest.rs
git commit -m "feat(ingest): DocClass + native-pass classify(); drop implicit OCR heuristic"
```

---

## Task 4: Embedded-OCR lane (real chunks, feature+mode gated, page-aware)

**Files:** Modify `crates/anno-rag/src/ingest.rs`.

- [ ] **Step 1: Failing test.** Add to `mod tests`:
```rust
#[test]
fn ocr_disabled_modes_yield_no_ocr_request() {
    use super::{should_ocr, OcrDecision};
    use crate::config::{AnnoRagConfig, OcrMode};
    let mut cfg = AnnoRagConfig::default(); // ocr_mode = Off
    assert!(matches!(should_ocr(&super::DocClass::ScannedPdf, &cfg), OcrDecision::Defer));
    cfg.ocr_mode = OcrMode::AutoEmbedded;
    // AutoEmbedded but the `embedded-ocr` feature is not compiled in the
    // default test build → still Defer (no external fallback).
    #[cfg(not(feature = "embedded-ocr"))]
    assert!(matches!(should_ocr(&super::DocClass::ScannedPdf, &cfg), OcrDecision::Defer));
    // TextLayer never OCRs.
    assert!(matches!(should_ocr(&super::DocClass::TextLayer, &cfg), OcrDecision::Skip));
}
```

- [ ] **Step 2: Run, expect FAIL.** `… ingest::tests::ocr_disabled_modes_yield_no_ocr_request --exact` → `cannot find function 'should_ocr'`.

- [ ] **Step 3: Implement the decision + OCR pass.** Add to ingest.rs:
```rust
/// What to do about OCR for a classified doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrDecision {
    /// Not a scanned/mixed doc — no OCR needed.
    Skip,
    /// Needs OCR and OCR is available — run it (pages = None ⇒ whole doc).
    Run { pages: Option<Vec<usize>> },
    /// Needs OCR but OCR is off/unavailable/over-budget — defer.
    Defer,
}

/// Decide OCR action from class + config + compiled feature. Budget is
/// applied by the caller (pipeline) which owns the per-batch clock.
#[must_use]
pub fn should_ocr(class: &DocClass, cfg: &AnnoRagConfig) -> OcrDecision {
    use crate::config::OcrMode;
    match class {
        DocClass::TextLayer | DocClass::Empty => OcrDecision::Skip,
        DocClass::ScannedPdf | DocClass::MixedPdf { .. } => {
            if cfg.ocr_mode != OcrMode::AutoEmbedded {
                return OcrDecision::Defer;
            }
            #[cfg(feature = "embedded-ocr")]
            {
                let pages = match class {
                    DocClass::MixedPdf { ocr_pages } if !ocr_pages.is_empty() => {
                        Some(ocr_pages.clone())
                    }
                    _ => None,
                };
                OcrDecision::Run { pages }
            }
            #[cfg(not(feature = "embedded-ocr"))]
            {
                OcrDecision::Defer
            }
        }
    }
}

/// Run a second Kreuzberg extraction with embedded Tesseract OCR and
/// return real chunks. `pages` = Some(1-indexed) → only those pages are
/// OCR'd (native text kept elsewhere); None → whole-doc OCR.
#[cfg(feature = "embedded-ocr")]
async fn extract_with_ocr(
    path: &Path,
    cfg: &AnnoRagConfig,
    pages: Option<Vec<usize>>,
) -> Result<(String, Vec<ExtractedChunk>)> {
    use kreuzberg::core::config::OcrConfig;
    let chunking = ChunkingConfig {
        max_characters: cfg.chunk_max_chars,
        overlap: cfg.chunk_overlap,
        chunker_type: ChunkerType::Markdown,
        ..Default::default()
    };
    let mut ec = ExtractionConfig {
        chunking: Some(chunking),
        ocr: Some(OcrConfig::default()), // Tesseract (the `ocr` feature)
        ..Default::default()
    };
    match pages {
        Some(p) => ec.force_ocr_pages = Some(p), // 1-indexed; native kept elsewhere
        None => ec.force_ocr = true,             // whole-doc OCR
    }
    let r = kreuzberg::extract_file(path, None, &ec)
        .await
        .map_err(|e| Error::Ingest { path: path.display().to_string(), source: Box::new(e) })?;
    let chunks = r
        .chunks
        .unwrap_or_default()
        .into_iter()
        .map(|c| ExtractedChunk {
            idx: c.metadata.chunk_index as u32,
            text: c.content,
            char_start: c.metadata.byte_start as u32,
            char_end: c.metadata.byte_end as u32,
            page: c.metadata.first_page.map(|p| p as u32),
        })
        .collect();
    Ok((r.content, chunks))
}
```
> `OcrConfig` default targets Tesseract (the `kreuzberg/ocr` feature pulls `kreuzberg-tesseract`). If `OcrConfig` exposes `language`/`backend` fields and the implementer wants `fra+eng`, set them here — verify field names against `kreuzberg::core::config::OcrConfig` at implementation time; defaults are acceptable for Phase 1 and the unit/integration tests gate correctness. `force_ocr` and `force_ocr_pages` are mutually exclusive with `disable_ocr` (do not set `disable_ocr` here).

Do NOT call `extract_with_ocr` from `extract` yet — Task 5 wires it under the budget in the pipeline. `extract` still returns the native result + `class`.

- [ ] **Step 4: Verify.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib ingest::tests -- --exact` PASS; `CARGO_INCREMENTAL=0 cargo check -p anno-rag` clean; `CARGO_INCREMENTAL=0 cargo check -p anno-rag --features embedded-ocr` clean.

- [ ] **Step 5: Commit.**
```bash
git add crates/anno-rag/src/ingest.rs
git commit -m "feat(ingest): embedded-OCR lane (should_ocr decision + extract_with_ocr, gated)"
```

---

## Task 5: `IngestOutcome`, per-batch OCR budget, deferral wiring

**Files:** Modify `crates/anno-rag/src/pipeline.rs`.

- [ ] **Step 1: Failing test.** Add to pipeline.rs `#[cfg(test)] mod tests`:
```rust
#[test]
fn ingest_outcome_accumulates() {
    use super::IngestOutcome;
    let mut o = IngestOutcome::default();
    o.text_done += 2;
    o.ocr_deferred += 1;
    o.skipped += 3;
    o.failed.push("a.pdf: boom".into());
    assert_eq!(o.total_seen(), 2 + 1 + 3 + 1);
    assert_eq!(o.text_done, 2);
}
```

- [ ] **Step 2: Run, expect FAIL.** `… pipeline::tests::ingest_outcome_accumulates --exact` → `cannot find type 'IngestOutcome'`.

- [ ] **Step 3: Implement.** Add to pipeline.rs (module scope):
```rust
/// Summary of an `ingest_folder` run (Phase 1: returned + logged;
/// Phase 3 surfaces it as an MCP resource).
#[derive(Debug, Default, Clone)]
pub struct IngestOutcome {
    /// Text-layer docs ingested.
    pub text_done: usize,
    /// Scanned/mixed docs ingested via OCR.
    pub ocr_done: usize,
    /// Scanned/mixed docs deferred (OCR off/unavailable/over-budget).
    pub ocr_deferred: usize,
    /// Files skipped (already ingested — PR #14 idempotency).
    pub skipped: usize,
    /// Per-file non-fatal failures (`"<path>: <err>"`).
    pub failed: Vec<String>,
}
impl IngestOutcome {
    /// Total documents observed this run.
    #[must_use]
    pub fn total_seen(&self) -> usize {
        self.text_done + self.ocr_done + self.ocr_deferred + self.skipped + self.failed.len()
    }
}
```
Change `ingest_one` to return `Result<DocOutcome>` where:
```rust
/// Per-document result `ingest_one` reports to `ingest_folder`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocOutcome { Text, Ocr, Deferred, Skipped }
```
In `ingest_one`: after `extract`, branch on `extracted.class` via `ingest::should_ocr`:
- `Skip` + non-empty chunks → existing index path → `Ok(DocOutcome::Text)`.
- `Skip` + `Empty` class → `Ok(DocOutcome::Deferred)` (nothing to index; not a failure).
- `Defer` → log `tracing::info!(path, "scanned doc deferred (ocr off/unavailable/over-budget)")`, do NOT index, `Ok(DocOutcome::Deferred)`.
- `Run{pages}` → `let (content, chunks) = ingest::extract_with_ocr(path, &self.cfg, pages).await?;` then run the **same** detect→pseudonymize→embed→upsert path on those chunks → `Ok(DocOutcome::Ocr)`. (The existing PR #14 deterministic-`doc_id` skip + `delete_doc_rows` stay ahead of this; the skip path returns `Ok(DocOutcome::Skipped)`.)

Per-batch budget lives in `ingest_folder`: maintain `let mut ocr_spent = std::time::Duration::ZERO;` and a budget `self.cfg.ocr_batch_budget_secs.map(Duration::from_secs)`. Before doing the `Run` branch for a doc, if `budget` is `Some(b)` and `ocr_spent >= b` → treat as `Deferred` (do not OCR). Time the `extract_with_ocr` call and add to `ocr_spent`. `ingest_folder` returns `Result<IngestOutcome>` (was `Result<usize>`); accumulate per-doc `DocOutcome` into the summary, push per-file errors into `failed` instead of only `tracing::warn`, keep the post-loop `maybe_build_index`/`maybe_build_fts_index`, and `tracing::info!` the final summary.

> Budget enforcement is in `ingest_folder` (it owns the per-run clock); `should_ocr` stays budget-agnostic. Keep `ingest_one`'s `&self`/sequential nature (no fan-out — see Task 6/7/8).

- [ ] **Step 4: Update the 2 callers.** `crates/anno-rag-mcp/src/lib.rs` and `crates/anno-rag-bin/src/main.rs` call `ingest_folder` (expecting `usize`) and possibly `ingest_one`. Update them: use `outcome.text_done + outcome.ocr_done` where the count was used, surface `ocr_deferred`/`failed` in the user-facing string/log. Keep changes minimal and behavior-equivalent for the success path.

- [ ] **Step 5: Verify.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib pipeline::tests::ingest_outcome_accumulates -- --exact` PASS; `CARGO_INCREMENTAL=0 cargo check -p anno-rag` + `--features embedded-ocr` clean; `CARGO_INCREMENTAL=0 cargo check -p anno-rag-mcp` + `cargo check -p anno-rag-bin` clean (callers updated).

- [ ] **Step 6: Commit.**
```bash
git add crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-bin/src/main.rs
git commit -m "feat(ingest): IngestOutcome + per-batch OCR budget + non-fatal deferral"
```

---

## Task 6: RAM guardrail — single-NER-engine contract test

**Files:** Modify `crates/anno-rag/src/pipeline.rs` (`#[cfg(test)] mod tests`).

- [ ] **Step 1: Write the test.**
```rust
#[test]
fn pipeline_has_exactly_one_ner_engine_field() {
    // Contract guard: the proven-harmful A″ NER engine *pool* / B
    // fan-out must not silently return. Pipeline must hold a single
    // lazily-built detector and NOT a pool type, and ingest must not
    // import buffer_unordered. Enforced by source inspection.
    let src = include_str!("pipeline.rs");
    assert!(src.contains("detector:"), "single detector field expected");
    assert!(!src.contains("detector_pool"), "NER engine pool reintroduced (forbidden — research §1/§7)");
    assert!(!src.contains("buffer_unordered"), "fan-out reintroduced over ingest (forbidden)");
    assert!(!src.contains("ingest_ner_pool"), "ingest_ner_pool config reintroduced (forbidden)");
}
```

- [ ] **Step 2: Run, expect PASS** (current code already satisfies it — this is a *regression guard*): `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib pipeline::tests::pipeline_has_exactly_one_ner_engine_field -- --exact` → PASS. If it FAILS, a pool/fan-out leaked back in — stop and report (do not weaken the assertions).

- [ ] **Step 3: Commit.**
```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "test(ingest): single-NER-engine contract guard (anti-A″/B regression)"
```

---

## Task 7: RAM guardrail — per-doc buffer-drop test

**Files:** Modify `crates/anno-rag/src/pipeline.rs` (`#[cfg(test)] mod tests`).

- [ ] **Step 1: Write the test** (source-contract: `ingest_folder` must not collect per-doc chunk/vector buffers across docs — each `ingest_one` owns and drops its own):
```rust
#[test]
fn ingest_folder_does_not_accumulate_per_doc_buffers() {
    let src = include_str!("pipeline.rs");
    // The fan-out experiment collected futures/results; the sequential
    // design must consume each doc fully inside the loop body. Guard
    // that no Vec of per-doc chunk/vector buffers is hoisted out of the
    // ingest_folder loop.
    assert!(!src.contains("let mut all_chunks"), "per-doc chunks hoisted out of loop (RAM regression)");
    assert!(!src.contains("Vec<ExtractedChunk>> = Vec::new()"), "cross-doc chunk accumulation (RAM regression)");
    assert!(src.contains("ingest_one"), "ingest_folder must delegate per-doc work to ingest_one");
}
```

- [ ] **Step 2: Run, expect PASS:** `… pipeline::tests::ingest_folder_does_not_accumulate_per_doc_buffers -- --exact` → PASS. If FAIL, real cross-doc accumulation exists — fix the loop to consume per-doc inside the body; do not weaken the test.

- [ ] **Step 3: Commit.**
```bash
git add crates/anno-rag/src/pipeline.rs
git commit -m "test(ingest): per-doc buffer-drop contract guard"
```

---

## Task 8: RAM guardrail — measured peak-RSS regression gate

**Files:** Create `crates/anno-rag/tests/ingest_phase1.rs`.

- [ ] **Step 1: Write the heavy tests.** Create `crates/anno-rag/tests/ingest_phase1.rs`:
```rust
//! Phase 1 heavy integration: peak-RSS ceiling + scanned-OCR multi-chunk
//! + budget deferral/resume. Ignored by default (LanceDB + NER model;
//! OCR tests need --features embedded-ocr). Run:
//! `cargo test -p anno-rag --test ingest_phase1 -- --ignored --test-threads=1`
#![allow(clippy::unwrap_used)]

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig { data_dir: dir.to_path_buf(), ..Default::default() }
}

/// Peak resident memory while ingesting a small text corpus must stay
/// under the documented ceiling. The number is set from the first
/// measured run on the dev target and locked as a regression gate
/// (NOT an absolute cross-machine guarantee — research §5).
#[tokio::test]
#[ignore = "heavy: loads NER+embedder, measures RSS"]
async fn peak_rss_under_ceiling_on_text_corpus() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    std::fs::create_dir_all(&corpus).unwrap();
    for i in 0..20 {
        std::fs::write(corpus.join(format!("d{i}.txt")),
            format!("Contrat {i}. Responsabilité contractuelle et obligation de moyen.")).unwrap();
    }
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32]).await.unwrap();
    let _ = p.ingest_folder(&corpus, false, &tmp.path().join("o")).await.unwrap();
    let rss_mb = peak_rss_mb();
    eprintln!("PEAK_RSS_MB={rss_mb}");
    // Ceiling: set from the first measured run, then commit the number.
    // Placeholder gate value 6144 MB (6 GB) — REPLACE with measured*1.15
    // on first green run and re-commit (see Step 3).
    assert!(rss_mb <= 6144, "peak RSS {rss_mb} MB exceeded ceiling 6144 MB");
}

#[cfg(target_os = "windows")]
fn peak_rss_mb() -> u64 {
    // GetProcessMemoryInfo PeakWorkingSetSize via `wmic`-free std: use
    // the `peak_alloc`-free approach — read from the OS.
    use std::process::Command;
    let pid = std::process::id();
    let out = Command::new("powershell").args(["-NoProfile","-Command",
        &format!("(Get-Process -Id {pid}).PeakWorkingSet64")]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / (1024*1024)
}
#[cfg(not(target_os = "windows"))]
fn peak_rss_mb() -> u64 {
    // ru_maxrss: KB on Linux, bytes on macOS.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    let v = u.ru_maxrss as u64;
    if cfg!(target_os = "macos") { v / (1024*1024) } else { v / 1024 }
}
```
Add `libc = "0.2"` to `crates/anno-rag/[dev-dependencies]` (non-Windows rusage). If `libc` is already a (dev-)dep, skip.

- [ ] **Step 2: Compile.** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --test ingest_phase1 --no-run` → SUCCESS. Also `--features embedded-ocr --no-run` → SUCCESS.

- [ ] **Step 3: Measure + lock the ceiling.** Run: `CARGO_INCREMENTAL=0 cargo test -p anno-rag --test ingest_phase1 peak_rss_under_ceiling_on_text_corpus -- --ignored --nocapture`. Read the printed `PEAK_RSS_MB=`. Set the assertion ceiling to `ceil(measured * 1.15)` (15% headroom), edit the literal, re-run → PASS, and record the measured number in the commit message. (If 6144 already passes comfortably, still tighten to measured*1.15 so it's a real gate.)

- [ ] **Step 4: Scanned-OCR integration (feature-gated, ignored).** Append to the same file:
```rust
#[cfg(feature = "embedded-ocr")]
#[tokio::test]
#[ignore = "heavy: embedded OCR on a scanned fixture"]
async fn scanned_pdf_ocrs_into_multiple_chunks_and_budget_defers() {
    // Requires a committed scanned-PDF fixture with no text layer.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scanned_sample.pdf");
    assert!(fixture.exists(), "add a no-text-layer scanned PDF at tests/fixtures/scanned_sample.pdf");
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    std::fs::create_dir_all(&corpus).unwrap();
    std::fs::copy(&fixture, corpus.join("scanned_sample.pdf")).unwrap();

    // OCR on, generous budget → ingested as OCR with >1 chunk.
    let mut c = cfg(tmp.path());
    c.ocr_mode = anno_rag::config::OcrMode::AutoEmbedded;
    let p = Pipeline::new(c, [0u8; 32]).await.unwrap();
    let out = p.ingest_folder(&corpus, false, &tmp.path().join("o")).await.unwrap();
    assert_eq!(out.ocr_done, 1, "scanned doc ingested via OCR");
    let hits = p.search("the", 50).await.unwrap();
    assert!(hits.iter().filter(|h| h.source_path.ends_with("scanned_sample.pdf")).count() > 1,
        "OCR'd doc must yield multiple chunks, not one synthetic chunk");

    // Tiny budget → deferred (non-fatal), and a re-run resumes it.
    let tmp2 = tempfile::tempdir().unwrap();
    let c2corpus = tmp2.path().join("c"); std::fs::create_dir_all(&c2corpus).unwrap();
    std::fs::copy(&fixture, c2corpus.join("scanned_sample.pdf")).unwrap();
    let mut c2 = cfg(tmp2.path());
    c2.ocr_mode = anno_rag::config::OcrMode::AutoEmbedded;
    c2.ocr_batch_budget_secs = Some(0); // immediately over budget
    let p2 = Pipeline::new(c2, [0u8; 32]).await.unwrap();
    let out2 = p2.ingest_folder(&c2corpus, false, &tmp2.path().join("o")).await.unwrap();
    assert_eq!(out2.ocr_deferred, 1, "over-budget scanned doc deferred, not failed");
}
```
(The fixture is a small, content-free scanned PDF the engineer adds at `crates/anno-rag/tests/fixtures/scanned_sample.pdf`; if none is available, generate one by rasterizing a 2-page text PDF to images and re-wrapping — note this in the commit.)

- [ ] **Step 5: Run feature-gated heavy (one machine):** `CARGO_INCREMENTAL=0 cargo test -p anno-rag --features embedded-ocr --test ingest_phase1 -- --ignored --test-threads=1` → both pass. Record `PEAK_RSS_MB` in the commit.

- [ ] **Step 6: Commit.**
```bash
git add crates/anno-rag/tests/ingest_phase1.rs crates/anno-rag/Cargo.toml crates/anno-rag/tests/fixtures/scanned_sample.pdf
git commit -m "test(ingest): peak-RSS gate + scanned-OCR multi-chunk/budget-defer integration"
```

---

## Final verification

- [ ] `CARGO_INCREMENTAL=0 cargo check -p anno-rag` + `--features embedded-ocr` + `-p anno-rag-mcp` + `-p anno-rag-bin` — all clean
- [ ] `CARGO_INCREMENTAL=0 cargo clippy -p anno-rag --all-targets -- -D warnings` and `--features embedded-ocr` — clean
- [ ] `cargo fmt --all -- --check` — clean (else `cargo fmt --all` + `style:` commit)
- [ ] `CARGO_INCREMENTAL=0 cargo test -p anno-rag --lib -- --test-threads=1` — fast unit (config, classify, should_ocr, IngestOutcome, guardrail contracts) green
- [ ] Default `cargo tree` shows **no** `kreuzberg-tesseract` (feature truly off by default)
- [ ] Heavy, one machine: `... --features embedded-ocr --test ingest_phase1 -- --ignored --test-threads=1` — RSS gate + scanned-OCR pass; measured `PEAK_RSS_MB` recorded
- [ ] Spec non-goals respected: no installer/packaging, no MCP status wiring, no batched-NER/parallelism, no VLM/Paddle/Easy/external-tesseract-as-primary
