# Kreuzberg Licensing Containment & Escape Plan

**Date:** 2026-06-20
**Status:** Design — approved for documentation
**Spec ID:** Spec A (of a two-spec effort; Spec B = VLM-OCR, separate doc)

## Summary

anno depends on `kreuzberg = "=4.9.7"` ([Cargo.toml:113](../../../Cargo.toml)) for
core document extraction. Kreuzberg changed its license from MIT to **Elastic
License 2.0 (ELv2)** at version **4.8.0** (2026-04-08); 4.7.4 was the last MIT
release. anno's [deny.toml:65-72](../../../deny.toml) already allows `Elastic-2.0`
for this crate with the rationale that local-desktop library use does not trigger
ELv2's restrictions.

This spec does **not** change the dependency. It **contains** the licensing risk:
it records the posture, defines the exact condition that would activate the risk,
and pre-scopes a permissive-stack escape plan so it is ready to execute the moment
that condition is met. The deliverable is this document plus a tightening of the
deny.toml comment to point at it.

## Assumption (revisit if false)

A hosted / multi-tenant / SaaS offering of anno is **not imminent**. The project is
currently a locally-run desktop binary, actively developing a local-first runtime
(`feat/zero-config-runtime`: local bge-m3 embeddings, `download_models`). If hosting
becomes a near-term goal, the decision below changes — jump straight to the escape
plan (A3).

## Background: why "just downgrade to 4.7.4" was rejected

The initial idea was to pin `kreuzberg = "=4.7.4"` (MIT) and delete the ELv2
allowance — assumed to be a near-free one-line change because everything anno calls
(`extract_file` + the `ExtractionConfig` / `OcrConfig` / result types) predates the
LLM features. Verification against the 4.8.0–4.9.7 changelog disproved this. The LLM
layer (VLM-OCR, structured extraction, hosted embeddings) actually landed in **4.8.5**;
**4.8.0** was a large architectural release. Reverting to 4.7.4 would discard:

| Lost by reverting to 4.7.4 | Version | Impact on anno |
|---|---|---|
| PDF table extraction quality, SF1 15.5% → **53.7%** | 4.8.0 | ~3.5× worse table cells; legal docs are table-heavy. |
| Multi-byte UTF-8 char-boundary **panic fixes** (PPTX/DOCX/comrak) | 4.8.1, 4.8.4 | Crashes on accented French text. |
| ~**1000× slowdown fix** on Ghostscript PDFs (O(N²)→O(1)) | 4.9.0 (#752) | Ingestion hangs on real-world PDFs. |
| Tesseract C++ exception crash fix (FFI unwind) | 4.8.0 | Hard crash on OCR. |
| Image-decode 64MP pixel cap + decompression size limits | 4.9.6 | **DoS protection on uploaded files** — [document_extract.rs](../../../crates/anno-privacy-gateway/src/document_extract.rs) processes untrusted uploads. |
| PDF structure / heading detection 40.7% → 43.7% | 4.8.0 | anno uses `HeadingContext` / `HeadingLevel`. |
| Email PST attachments + EML HTML-body fallback | 4.9.x | anno enables the `email` feature. |
| DOCX page extraction + DOCX-OCR fixes | 4.9.3, 4.9.5 | Correctness. |

Two additional disqualifiers:

- **Likely compile break.** anno sets `result_format = OutputFormat::ElementBased`
  ([ingest.rs:254](../../../crates/anno-rag/src/ingest.rs),
  [ingest.rs:488](../../../crates/anno-rag/src/ingest.rs)). The element-based model is
  the 4.8.0 "unified `InternalDocument`" rework; `ElementBased` may not exist in 4.7.4,
  in which case ingest does not compile without rework.
- **Dead branch.** Kreuzberg is already on `5.0.0-rc`; 4.7.4 receives no future
  security backports.

A1 (downgrade now) therefore takes regressions **and** a less-secure, unmaintained
base, to neutralize a risk that is dormant until anno is hosted. Rejected.

## Decision

**A2 — contain and document now; hold A3 as a pre-scoped, ready-to-execute escape.**

- Keep `kreuzberg = "=4.9.7"`. No product regression; retain all quality/security fixes.
- Make the dormant ELv2 risk *managed* rather than *forgotten*: an explicit trigger
  condition and a ready escape plan.
- Execute A3 (permissive replacement) only when the trigger fires.

A3 is the correct end-state for a hosted product but is a multi-step project
(reimplement format dispatch + the element/table/image output model anno depends on);
building it speculatively, before hosting is confirmed, is premature and competes with
in-flight local-runtime work.

## Trigger condition (when this risk activates)

Activate the escape plan **before** the first deployment where anno is offered to third
parties as a hosted or managed service that exposes kreuzberg-backed extraction
functionality to those users — i.e. any multi-tenant SaaS, hosted API, or managed
appliance where users other than the operator obtain access to document-extraction
features. ELv2 §1 prohibits providing "the software" as a managed service exposing a
substantial set of its functionality; a hosted anno whose extraction surface is
kreuzberg falls within that prohibition. Single-tenant, on-premise, or end-user-desktop
distribution does **not** trigger it.

Concretely, treat any of these as the trigger: a `serve`/HTTP deployment intended for
external tenants, a cloud control-plane that runs extraction on uploaded files for
multiple customers, or a contract/RFP requiring SaaS delivery.

## Escape plan (A3) — pre-scoped, not yet executed

When triggered, replace the Elastic `kreuzberg` orchestration crate with a permissive
(MIT/Apache) stack. The MIT OCR/pdfium **sub-crates** kreuzberg forked upstream remain
usable directly.

**Candidate crate mapping (permissive):**

| Capability | Permissive replacement | License |
|---|---|---|
| PDF text/layout | `kreuzberg-pdfium-render` (MIT fork) over bundled pdfium | MIT / BSD-3 |
| OCR (Tesseract) | `kreuzberg-tesseract` (MIT fork) | MIT |
| OCR (Paddle) | `kreuzberg-paddle-ocr` (MIT fork) | MIT |
| Excel | `calamine` | MIT |
| Email (EML/MSG) | `mail-parser` | MIT/Apache-2.0 |
| DOCX/PPTX/ODT | `docx-rs` / direct zip + quick-xml | MIT/Apache-2.0 |
| HTML | `scraper` / `html2text` | MIT/Apache-2.0 |
| Archives | `zip`, `sevenz-rust2` | MIT/Apache-2.0 |

**API surface to preserve** (so call sites in
[ingest.rs](../../../crates/anno-rag/src/ingest.rs) and
[document_extract.rs](../../../crates/anno-privacy-gateway/src/document_extract.rs)
change minimally):

- entry point: `extract_file(path, password, &config) -> Result<ExtractionResult>`
- config: `ExtractionConfig { disable_ocr, output_format, result_format, .. }`, `OcrConfig`
- result/types: `ExtractionResult`, `PageContent`, `Chunk`, `ChunkType`, `ChunkMetadata`,
  `HeadingContext`, `HeadingLevel`, `Table`, `ExtractedImage`, both `OutputFormat` enums

**Primary open risk to resolve at execution time:** the permissive stack must reproduce
the `ElementBased` / `InternalDocument` element-tree output anno consumes, **or** anno's
ingest must adapt to a simpler output model. This is the largest unknown and should be
spiked first when A3 begins. The replacement should be introduced behind a thin
anno-owned `DocumentExtractor` trait so the cutover is incremental and testable against
the existing fixtures.

## Changes in this spec

1. **New doc:** this file (the source of truth for posture, trigger, and escape plan).
2. **deny.toml:** rewrite the [deny.toml:65-72](../../../deny.toml) comment from the
   vague "REVIEW BEFORE any SaaS/hosted offering" note into a pointer to this spec's
   trigger condition and escape plan, so the gate is discoverable from the dependency
   policy itself.

No code, dependency, or build changes.

## Out of scope (and why)

- **Structured extraction** — already shipped in
  [`anno-rag-tabular/src/llm`](../../../crates/anno-rag-tabular/src/llm/mod.rs)
  (`LlmClient::generate_structured` + `RoutingLlmClient`: local-first, hosted-opt-in,
  PII-gated). Kreuzberg's 4.8.5 structured extraction is redundant. Nothing to build.
- **LLM-provider embeddings** — dropped (YAGNI). Conflicts with the in-progress local
  bge-m3 embedding pipeline.
- **VLM-OCR** — genuinely new; deferred to **Spec B**. Requires extending the text-only
  `LlmClient` trait with a vision-capable call, a local VLM runtime for the local-default
  path, and image-level PII gating (the current safety gate checks text, not images).

## Acceptance

- This document exists and is committed.
- The deny.toml ELv2 comment references this spec's trigger + escape plan.
- A future reader hitting the deny.toml entry can reach the trigger condition and a
  ready execution plan without re-deriving any of the analysis above.
