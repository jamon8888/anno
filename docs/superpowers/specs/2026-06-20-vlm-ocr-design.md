# VLM-OCR: Vision-Model Document Transcription — Design

**Date:** 2026-06-20
**Status:** Design — scoped, not yet scheduled
**Spec ID:** Spec B (of a two-spec effort; Spec A = [Kreuzberg Licensing Containment](2026-06-20-kreuzberg-licensing-containment-design.md))

## Summary

Kreuzberg 4.8.5 added a VLM-OCR capability (vision-model transcription of page
images) behind its Elastic-2.0 LLM layer. Spec A keeps anno on `kreuzberg = "=4.9.7"`
but deliberately **does not** adopt that VLM-OCR path: it is a hosted/LLM feature,
and anno's posture is local-first and sovereignty-preserving. This spec defines how
anno gains VLM-OCR **on its own terms** — a local-default vision runtime, an opt-in
hosted fallback, and an image-level PII gate — rather than inheriting kreuzberg's
provider-coupled implementation.

The motivating gap is quality, not parity. anno's current OCR is Tesseract/Paddle
([ingest.rs:380](../../../crates/anno-rag/src/ingest.rs) `embedded_ocr_extract`),
which is strong on clean printed text but weak on the inputs French legal work
actually produces: stamped/signed pages, handwritten annotations, multi-column
scans, tables-as-images, and low-quality faxes. A VLM transcribes layout-aware text
from those pages where line-based OCR degrades.

## Assumption (revisit if false)

The local-first runtime work (`feat/zero-config-runtime`: local bge-m3 embeddings,
`download_models`) is the baseline anno ships on. VLM-OCR must fit that model — a
weights download managed by the same plumbing, running on-device by default, with no
mandatory network call. If anno's default shifts to hosted inference, the routing
default in §4.4 flips, but the trait and gating design below are unchanged.

## Motivation: where today's OCR fails

| Input | Tesseract/Paddle today | VLM-OCR |
|---|---|---|
| Handwritten margin notes on a contract | Dropped or garbled | Transcribed with layout context |
| Stamp/signature block overlapping text | Line detection breaks | Read as a region, text recovered |
| Table rendered as a scanned image | Cells lost (no structure) | Structure-aware transcription |
| Multi-column / rotated scan | Reading order scrambled | Reading order inferred |
| Accented French at low DPI | Char-level substitution errors | Context corrects to valid tokens |

This overlaps with — but does not replace — the table-extraction gains Spec A keeps by
staying on 4.9.7 (digital PDFs). VLM-OCR targets the **image/scanned** path, where
those gains do not apply.

## Decision

**Add VLM-OCR as a local-default, opt-in-hosted capability behind a new anno-owned
trait — do not enable kreuzberg's ELv2 VLM path.**

- A new `VlmOcrClient` trait, sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs)
  — **not** an overload of `generate_structured`. The text trait takes `(system, user,
  json_schema)`; a vision call needs image bytes + an OCR/transcription instruction and
  returns text, so it is a distinct contract.
- A local VLM runtime is the default backend, weights fetched via the existing
  `download_models` plumbing — no network at inference time.
- A hosted VLM is an opt-in fallback only, gated exactly like the text path
  ([routing.rs:41](../../../crates/anno-rag-tabular/src/llm/routing.rs)), with the
  added image-PII gate from §4.3.
- VLM-OCR runs **only on pages classified as needing OCR** (`page_needs_ocr` /
  `DocClass::ScannedPdf | MixedPdf` in [ingest.rs](../../../crates/anno-rag/src/ingest.rs)),
  never on already-digital text. It is an upgrade to the OCR branch, not a new pass over
  every document.

Rationale: this reuses three patterns anno already proved in the tabular engine
(local-first trait, routing fallback, prompt safety gate) instead of importing a
fourth provider abstraction from kreuzberg, and it keeps the licensing posture of
Spec A intact (no reliance on the ELv2 LLM layer).

## Design

### 4.1 The `VlmOcrClient` trait

A minimal, `Send + Sync` trait mirroring `LlmClient`'s shape so the same `Arc<dyn …>`
fan-out and the same routing wrapper apply:

```rust
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance (e.g. "French legal contract, preserve table structure").
    async fn transcribe(&self, image: &PageImage, hint: &str) -> Result<Transcription>;
    fn model_id(&self) -> &str;
}
```

`PageImage` wraps the decoded image bytes + provenance (source doc id, page index)
so audit logs can attribute every transcription to a page, matching the
`Author::System { extractor_version }` pattern the text path already uses.
`Transcription` returns text plus a confidence/coverage signal the OCR-mode logic can
use to decide whether to keep VLM output or fall back to Tesseract.

### 4.2 Local VLM runtime (default backend)

- Backend: a small open-weights vision model in the runtime anno already ships
  (ONNX/Candle, consistent with the GLiNER2 local extractor at
  [llm/local](../../../crates/anno-rag-tabular/src/llm/local/)). Model choice is an
  execution-time decision (a permissively-licensed OCR-focused VLM in the 1–4B range);
  the SPDX license must be verified the same way Spec A requires for its escape-plan
  crates.
- Weights: registered with `download_models` so first use fetches them and offline
  runs reuse the cache — no new download logic, same gating as bge-m3 / GLiNER2.
- Gated behind a Cargo feature (`vlm-ocr`), mirroring `embedded-ocr` and `gliner2`, so
  builds that do not want the weights/runtime cost compile it out cleanly.

### 4.3 Image-level PII gate (the genuinely new safety surface)

The existing gate is **text-only** — `fallback_prompt_is_safe`
([privacy.rs:16](../../../crates/anno-rag-tabular/src/llm/privacy.rs)) regex-matches a
string. An image cannot be regex-checked, and a scanned legal page is dense PII
(names, signatures, ID photos, IBANs printed on letterhead). Therefore:

- **Local VLM is safe by construction** — it makes no network call, so a raw page
  image never leaves the device. This is the default path and needs no image gate.
- **Hosted VLM-OCR is OFF by default.** Sending a raw page image to a remote provider
  defeats the pseudonymisation the text path is built to guarantee. The gateway
  already encodes this stance with `reject_images: true`
  ([config.rs:104](../../../crates/anno-privacy-gateway/src/config.rs)); Spec B keeps it.
- If a hosted VLM is ever explicitly enabled, it is gated by a conservative
  **pre-transcription image check** (no detected face/signature region, no machine-readable
  zone) AND requires an explicit per-run opt-in flag — analogous to `--allow-remote-llm`.
  Until that check exists, hosted VLM-OCR stays unbuildable, not merely disabled.

This is the part with no existing analogue and is the highest-risk item; it should be
spiked before any hosted path is wired.

### 4.4 Routing & ingest integration

- A `RoutingVlmClient` mirrors
  [`RoutingLlmClient`](../../../crates/anno-rag-tabular/src/llm/routing.rs): local VLM
  first; hosted fallback only if allowed, image-gated, and only when the local
  transcription's confidence is below threshold.
- Ingest wiring: in the OCR branch of
  [ingest.rs](../../../crates/anno-rag/src/ingest.rs) (`OcrMode::AutoEmbedded`,
  `embedded_ocr_extract`), when the `vlm-ocr` feature is on and a page is classified
  `ScannedPdf`/`MixedPdf`, route that page's image through `RoutingVlmClient` and emit
  the result as the page's chunks. Tesseract remains the fallback when VLM confidence
  is low or the feature is off — VLM-OCR is additive, never a hard dependency.
- This requires flipping `extract_images` for the OCR path only
  ([ingest.rs:262](../../../crates/anno-rag/src/ingest.rs) currently `false`), scoped so
  digital-text documents still skip image extraction entirely.

## Phasing

1. **Trait + local runtime + feature flag** (`vlm-ocr`), wired into the OCR branch
   behind `RoutingVlmClient` with no hosted fallback. Local-only, safe by construction.
   Validate transcription quality on a scanned-legal fixture set vs Tesseract.
2. **Confidence-driven fallback to Tesseract** and chunk/heading integration so VLM
   output flows through the same `ElementBased` consumers as the rest of ingest.
3. **(Deferred, trigger-gated)** Image-PII gate + opt-in hosted VLM. Not built until
   there is a concrete need and the §4.3 image check is designed.

## Out of scope (and why)

- **Kreuzberg's built-in VLM-OCR** — ELv2-coupled and provider-shaped; adopting it
  reintroduces exactly the licensing/sovereignty posture Spec A contains. Not used.
- **Hosted VLM-OCR in v1** — deferred to phase 3; the image-PII gate is a prerequisite
  and does not exist yet.
- **Image embeddings / multimodal retrieval** — VLM-OCR produces *text* that flows into
  the existing text embedding + retrieval pipeline. Embedding images directly is a
  separate effort (YAGNI here).
- **Structured extraction from images** — once a page is transcribed to text, the
  existing `LlmClient::generate_structured` / tabular path handles structure. No new
  structured-from-pixels path.

## Risks

- **Local VLM cost/latency** — vision models are heavier than Tesseract; per-page
  latency and RAM must stay within the ingest budget. Mitigate by running VLM only on
  OCR-classified pages and keeping Tesseract as the default for clean scans.
- **License verification** — the chosen model's weights + license must be confirmed
  permissive and redistributable via `download_models`, same discipline Spec A applies
  to its escape-plan crates.
- **Image-PII gate is unsolved** — there is no cheap, reliable "is this image safe to
  send" check. Treated as a hard blocker on the hosted path, not a best-effort filter.

## Acceptance

- This document exists and is committed alongside Spec A.
- The design names a single new trait (`VlmOcrClient`) and reuses the existing
  local-first / routing / safety-gate patterns rather than importing kreuzberg's VLM path.
- A reader can see exactly which ingest branch changes
  ([ingest.rs](../../../crates/anno-rag/src/ingest.rs) OCR path), why images stay gated
  by default ([config.rs](../../../crates/anno-privacy-gateway/src/config.rs)
  `reject_images`), and what must be built before any hosted VLM call is allowed (§4.3).
