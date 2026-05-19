# Design — Phase 1: OCR Gating + RAM Guardrails

**Date**: 2026-05-19
**Status**: Draft for review
**Parent**: `docs/superpowers/specs/2026-05-19-anno-local-ingest-architecture-research.md` (§3, §5, §8 Phase 1)
**Scope**: `crates/anno-rag` only. Pure-codebase + Cargo feature work, fully testable. Installer/model-distribution work stays out of scope, but the OCR engine path is now **embedded Kreuzberg OCR** behind build/runtime gates.

## 1. Problem

The corpus is **mixed/unknown** — digital-text and scanned PDFs. Today (`crates/anno-rag/src/ingest.rs:50` `extract`):

- "Scanned" is an *implicit* heuristic: `is_pdf && content.trim().is_empty()`.
- If `cfg.enable_ocr` → `crate::ocr::ocr_pdf` forks a system `tesseract` binary and the entire OCR'd document is collapsed into **one synthetic chunk** (ingest.rs:~96 — explicitly flagged in-code as a v0.5 shortcut "for larger scanned docs we [should do better]"). One chunk for a 40-page scan destroys retrieval granularity and citation offsets.
- If a PDF has no text layer and OCR is disabled/unavailable, the pipeline can still produce an empty-document false success instead of a typed, resumable OCR outcome.
- There is no per-document/per-page classification, no budget, no deferral: a scan-heavy week either produces useless output or silently spends unbounded OCR time (1–5 s/page).

RAM: the design intent (single NER engine, bounded per-doc buffers, no parallel engines — the reverted A″ anti-pattern) is **not enforced or tested**; nothing prevents regressing it, and the packaged RSS is unmeasured.

## 2. Goals

1. **Explicit PDF classification** at extraction: `TextLayer` vs `ScannedPdf` vs `MixedPdf { ocr_pages }` vs `Unsupported/Empty`, returned as data — not an implicit empty-string check buried in `extract`.
2. **Embedded, disableable, gated OCR**: ship Kreuzberg OCR behind `embedded-ocr`, expose runtime `ocr_mode = off | auto_embedded`, and run OCR only for scanned PDFs/pages after a native no-OCR pass.
3. **Two-lane, budgeted OCR**: text-layer docs unchanged (fast lane); scanned docs/pages OCR'd **and properly chunked** (not one synthetic chunk), under a per-batch OCR time budget; docs over budget are **deferred** (recorded, resumable via PR #14 idempotency), never a hard failure and never an unbounded stall.
4. **RAM guardrails made explicit + enforced by test**: single NER engine invariant, bounded per-doc working set, a measured-RSS regression test, and a compile/contract guard that the reverted detector-pool/fan-out cannot return silently.

## 3. Design

### 3.1 Build + runtime gates

- Add `embedded-ocr = ["kreuzberg/ocr"]` to `anno-rag`.
- Do **not** enable Kreuzberg `liter-llm`, VLM OCR, PaddleOCR, or EasyOCR in this phase. Target backend is embedded Tesseract via Kreuzberg OCR only.
- Add runtime `ocr_mode`:
  - `off`: OCR is disabled even if the binary was built with `embedded-ocr`.
  - `auto_embedded`: OCR is allowed, but only after classification marks a PDF/page as scanned.
- In builds without `embedded-ocr`, `auto_embedded` produces a typed `OcrUnavailable` deferral for scanned pages/docs instead of falling back to an external binary.
- The current external `ocr::ocr_pdf` path can remain temporarily for compatibility, but it is not the acceptance path for this spec.

### 3.2 Classification (replace the implicit heuristic)

Add to `ingest.rs`:

```rust
pub enum DocClass {
    TextLayer,
    ScannedPdf,
    MixedPdf { ocr_pages: Vec<usize> },
    Empty,
}
```

`extract` first runs Kreuzberg with OCR explicitly disabled (`disable_ocr = true`) and page extraction/chunking enabled. Classification uses the native result:

- `TextLayer`: non-PDF text docs, or PDFs whose native text/page chunks pass quality thresholds.
- `ScannedPdf`: PDF with no usable native text across the document.
- `MixedPdf { ocr_pages }`: PDF with usable native text on some pages and weak/empty native text on others.
- `Empty`: unsupported/genuinely empty non-PDF.

The page-level decision should mirror Kreuzberg's own native-text quality ideas where possible: empty/near-empty page text, garbage ratio, low alphanumeric ratio, fragmented words, or font-encoding failure marks that page for OCR. Do not OCR the whole PDF just because one page is weak.

### 3.3 OCR lane — embedded, page-gated, chunked

- When `ocr_mode == off`: any `ScannedPdf` or `MixedPdf` OCR requirement becomes a typed, non-fatal deferral.
- When `ocr_mode == auto_embedded` and `embedded-ocr` is compiled:
  - `ScannedPdf`: run a second Kreuzberg extraction with `ocr: Some(OcrConfig { backend: "tesseract", language: "fra+eng", ..Default::default() })`, `force_ocr = true`, same chunking config.
  - `MixedPdf { ocr_pages }`: run a second Kreuzberg extraction with the same OCR config and `force_ocr_pages = Some(ocr_pages)`, preserving native text on good pages and replacing only scanned/weak pages.
- Consume Kreuzberg's returned chunks directly. Remove the single synthetic OCR chunk path for embedded OCR.
- Preserve page metadata from Kreuzberg chunk metadata (`first_page`/`last_page`) where available; this is why embedded OCR is preferred over the external stdout path.
- **Per-batch OCR budget**: new `cfg.ocr_batch_budget_secs: Option<u64>` (default `None` = unlimited; opt-in bound). `ingest_folder` tracks cumulative OCR wall-time; once exceeded, further `ScannedPdf`/`MixedPdf` OCR work is **deferred** (not extracted) and recorded.
- **Deferral is not failure**: a deferred or OCR-unavailable scanned doc returns a typed, non-fatal outcome the caller records (counted, resumable next run via deterministic `doc_id` skip from PR #14) instead of `Err` or `Ok(empty)`.
- `ingest_one`/`ingest_folder` return an outcome summary: `{ text_done, ocr_done, ocr_deferred, skipped, failed[] }` (consumed by Phase 3's MCP status resource; for Phase 1 it is logged + returned).

### 3.4 RAM guardrails (explicit + tested)

- **Single-engine invariant**: a documented module contract + a unit test asserting `Pipeline` exposes exactly one lazily-built NER `Detector` (no pool type, no `ingest_ner_pool` re-introduction, no `buffer_unordered` over `ingest_one`). This is a regression guard for the proven-harmful A″/B (research §1/§7).
- **Bounded per-doc working set**: assert (test) `ingest_one` drops a document's chunk/vector buffers before the next document (no cross-doc accumulation in `ingest_folder`).
- **Measured RSS test** (`#[ignore]`, heavy): ingest a fixed small corpus, sample peak RSS, assert it stays under a documented ceiling (e.g. ≤ 3 GB with NER+embedder loaded — number set from the first measured run, then locked as a regression gate). This is the only way the 12 GB-floor claim is kept honest.

## 4. Non-goals (deferred to later phases / §B)

- Installer UX and release packaging polish (Phase 2). This phase may add the Cargo feature/dependency path, but not installers or model-delivery UX.
- MCP status resource + background trigger (Phase 3) — Phase 1 only *returns/logs* the outcome summary.
- True batched NER / parallelism (research §B — deferred to 10× volume; the guardrails here actively prevent re-introducing the harmful variant).
- VLM OCR, PaddleOCR, EasyOCR, or external system-Tesseract as the primary path. Embedded Kreuzberg/Tesseract is the only target OCR engine for this phase.

## 5. Testing

- Unit: classifier — text-layer file → `TextLayer`; fully scanned PDF → `ScannedPdf`; mixed PDF → `MixedPdf { ocr_pages }`; empty non-pdf/unsupported → `Empty`.
- Unit: `ocr_mode = off` with scanned PDF → non-fatal deferral outcome (not `Err`, not `Ok(empty)`), counted in the summary.
- Compile/contract: default/minimal build does not include `kreuzberg/ocr`; `embedded-ocr` build includes it; neither build includes `liter-llm`.
- Integration (`#[ignore]`, heavy, `embedded-ocr`): a scanned-PDF fixture with `ocr_mode = auto_embedded` → multiple chunks (not one), page metadata present where Kreuzberg provides it, retrievable.
- Integration (`#[ignore]`, heavy, `embedded-ocr`): a mixed-PDF fixture → only weak/scanned pages are OCR'd via `force_ocr_pages`; good native pages remain native.
- Integration (`#[ignore]`, heavy): with `ocr_batch_budget_secs` tiny → doc deferred, recorded, and a re-run (PR #14 skip) resumes it.
- Guardrail unit: single-engine contract test; per-doc buffer-drop test.
- Guardrail heavy (`#[ignore]`): measured peak RSS ≤ documented ceiling.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Embedded OCR increases binary size/RSS | Keep `embedded-ocr` feature-gated, keep runtime `ocr_mode = off`, add cargo-tree and measured-RSS gates |
| OCR'd text chunker assumes markdown structure scans lack | Use Kreuzberg's OCR result + same chunking config; verify multi-chunk on the fixture test |
| Mixed-PDF classification over-OCRs a mostly text PDF | Native no-OCR pass first, page-level thresholds, and `force_ocr_pages` instead of whole-document OCR |
| Kreuzberg docs drift from shipped artifacts (e.g. kreuzberg-dev/kreuzberg#965 C# binding mismatch) | Treat pinned Rust source/API as authoritative; acceptance tests compile the exact Rust feature set we ship |
| RSS ceiling number is environment-dependent | Set from first measured run on the dev target, document it as "regression gate, not absolute"; tolerance factor like the eval baselines |
| Behavior change for OCR-disabled scanned PDFs | Intended + documented; covered by the deferral unit test; callers already tolerate per-doc non-fatal outcomes (PR #14 skip/failed pattern) |
| Scope creep into Phase 3 status surfacing | Phase 1 only returns/logs the summary struct; no MCP wiring |

## 7. Expected outcome

A mixed corpus ingests **correctly and predictably**: text-layer docs unchanged; scanned pages/docs OCR through embedded Kreuzberg only when needed; OCR can be disabled at runtime; scanned output is properly chunked/retrievable with page metadata where available; a scan-heavy week degrades to *slower + deferred + resumable*, never a hang or empty-doc false success; and the 12 GB / single-engine envelope is enforced by tests so it cannot silently regress.
