# Design — Phase 1: OCR Gating + RAM Guardrails

**Date**: 2026-05-19
**Status**: Draft for review
**Parent**: `docs/superpowers/specs/2026-05-19-anno-local-ingest-architecture-research.md` (§3, §5, §8 Phase 1)
**Scope**: `crates/anno-rag` only. Pure-codebase, fully testable, no packaging dependency. Highest value / lowest risk phase.

## 1. Problem

The corpus is **mixed/unknown** — digital-text and scanned PDFs. Today (`crates/anno-rag/src/ingest.rs:50` `extract`):

- "Scanned" is an *implicit* heuristic: `is_pdf && content.trim().is_empty()`.
- If `cfg.enable_ocr` → `crate::ocr::ocr_pdf` and the entire OCR'd document is collapsed into **one synthetic chunk** (ingest.rs:~96 — explicitly flagged in-code as a v0.5 shortcut "for larger scanned docs we [should do better]"). One chunk for a 40-page scan destroys retrieval granularity and citation offsets.
- If a PDF has no text layer and `!enable_ocr` → hard `Error` ("enable --enable-ocr…"). In an unattended MCP batch this **fails the document with no graceful path**.
- There is no per-document classification, no budget, no deferral: a scan-heavy week either errors out doc-by-doc or silently produces useless single-chunk docs, and OCR time (1–5 s/page) is unbounded.

RAM: the design intent (single NER engine, bounded per-doc buffers, no parallel engines — the reverted A″ anti-pattern) is **not enforced or tested**; nothing prevents regressing it, and the packaged RSS is unmeasured.

## 2. Goals

1. **Explicit doc classification** at extraction: `TextLayer` vs `NeedsOcr` vs `Unsupported/Empty`, returned as data — not an implicit empty-string check buried in `extract`.
2. **Two-lane, budgeted OCR**: text-layer docs unchanged (fast lane); scanned docs OCR'd **and properly chunked** (not one synthetic chunk), under a per-batch OCR time budget; docs over budget are **deferred** (recorded, resumable via PR #14 idempotency), never a hard failure and never an unbounded stall.
3. **RAM guardrails made explicit + enforced by test**: single NER engine invariant, bounded per-doc working set, a measured-RSS regression test, and a compile/contract guard that the reverted detector-pool/fan-out cannot return silently.

## 3. Design

### 3.1 Classification (replace the implicit heuristic)

Add to `ingest.rs`:

```rust
pub enum DocClass { TextLayer, NeedsOcr, Empty }
```

`extract` computes `DocClass` after raw extraction (text-layer present = non-empty trimmed content; `NeedsOcr` = pdf/image with empty content; `Empty` = unsupported/genuinely empty non-pdf) and returns it on `ExtractedDoc` (`pub class: DocClass`). The existing `is_pdf && content.trim().is_empty()` logic moves into this classifier; no behavior change for text-layer docs.

### 3.2 OCR lane — proper chunking + budget + deferral

- When `class == NeedsOcr` and OCR is available (`cfg.enable_ocr` + tesseract resolvable): run `ocr::ocr_pdf`, then **chunk the OCR'd text through the same markdown/length chunker text-layer docs use** (remove the single-synthetic-chunk shortcut) so retrieval granularity + offsets are consistent across lanes.
- **Per-batch OCR budget**: new `cfg.ocr_batch_budget_secs: Option<u64>` (default `None` = unlimited; opt-in bound). `ingest_folder` tracks cumulative OCR wall-time; once exceeded, further `NeedsOcr` docs are **deferred** (not extracted) and recorded.
- **Deferral is not failure**: a deferred or OCR-unavailable scanned doc returns a typed, non-fatal outcome the caller records (counted, resumable next run via deterministic `doc_id` skip from PR #14) instead of `Err`. The hard-error branch (`!enable_ocr` → `Error`) is replaced by this deferral outcome.
- `ingest_one`/`ingest_folder` return an outcome summary: `{ text_done, ocr_done, ocr_deferred, skipped, failed[] }` (consumed by Phase 3's MCP status resource; for Phase 1 it is logged + returned).

### 3.3 RAM guardrails (explicit + tested)

- **Single-engine invariant**: a documented module contract + a unit test asserting `Pipeline` exposes exactly one lazily-built NER `Detector` (no pool type, no `ingest_ner_pool` re-introduction, no `buffer_unordered` over `ingest_one`). This is a regression guard for the proven-harmful A″/B (research §1/§7).
- **Bounded per-doc working set**: assert (test) `ingest_one` drops a document's chunk/vector buffers before the next document (no cross-doc accumulation in `ingest_folder`).
- **Measured RSS test** (`#[ignore]`, heavy): ingest a fixed small corpus, sample peak RSS, assert it stays under a documented ceiling (e.g. ≤ 3 GB with NER+embedder loaded — number set from the first measured run, then locked as a regression gate). This is the only way the 12 GB-floor claim is kept honest.

## 4. Non-goals (deferred to later phases / §B)

- Packaging / bundled models / installers (Phase 2).
- MCP status resource + background trigger (Phase 3) — Phase 1 only *returns/logs* the outcome summary.
- True batched NER / parallelism (research §B — deferred to 10× volume; the guardrails here actively prevent re-introducing the harmful variant).
- A new OCR engine — keep the existing tesseract-fork path; engine/model delivery is Phase 2's on-consent fetch.

## 5. Testing

- Unit: classifier — text-layer file → `TextLayer`; empty PDF → `NeedsOcr`; empty non-pdf/unsupported → `Empty`.
- Unit: OCR-unavailable scanned doc → non-fatal deferral outcome (not `Err`), counted in the summary.
- Integration (`#[ignore]`, heavy): a scanned-PDF fixture with OCR enabled → multiple chunks (not one), retrievable; with `ocr_batch_budget_secs` tiny → doc deferred, recorded, and a re-run (PR #14 skip) resumes it.
- Guardrail unit: single-engine contract test; per-doc buffer-drop test.
- Guardrail heavy (`#[ignore]`): measured peak RSS ≤ documented ceiling.

## 6. Risks

| Risk | Mitigation |
|---|---|
| OCR'd text chunker assumes markdown structure scans lack | Chunker already length-based fallback; feed OCR text through the same path, verify multi-chunk on the fixture test |
| RSS ceiling number is environment-dependent | Set from first measured run on the dev target, document it as "regression gate, not absolute"; tolerance factor like the eval baselines |
| Behavior change for the `!enable_ocr` empty-PDF case (was `Err`, now deferral) | Intended + documented; covered by the deferral unit test; callers already tolerate per-doc non-fatal outcomes (PR #14 skip/failed pattern) |
| Scope creep into Phase 3 status surfacing | Phase 1 only returns/logs the summary struct; no MCP wiring |

## 7. Expected outcome

A mixed corpus ingests **correctly and predictably**: text-layer docs unchanged; scanned docs properly chunked and retrievable; a scan-heavy week degrades to *slower + deferred + resumable*, never a hang or doc-by-doc hard failure; and the 12 GB / single-engine envelope is enforced by tests so it cannot silently regress. No packaging or perf rework — pure correctness + safety, fully in-codebase.
