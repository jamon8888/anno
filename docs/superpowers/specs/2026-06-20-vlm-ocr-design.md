# VLM-OCR: Vision-Model Document Transcription — Design

**Date:** 2026-06-20
**Status:** Design — scoped, not yet scheduled
**Spec ID:** Spec B (of a two-spec effort; Spec A = [Kreuzberg Licensing Containment](2026-06-20-kreuzberg-licensing-containment-design.md))

## Summary

Kreuzberg 4.8.5 added a VLM-OCR capability (vision-model transcription of page
images) behind its Elastic-2.0 LLM layer. Spec A keeps anno on `kreuzberg = "=4.9.7"`
but deliberately **does not** adopt that VLM-OCR path: it is a hosted/LLM feature,
and anno's posture is local-first and sovereignty-preserving. This spec defines how
anno gains VLM-OCR **on its own terms** — a permissively-licensed vision model running
inside the customer's trust boundary, across two deployment profiles (desktop and
on-premise GPU), with no document image ever sent to a third party.

The motivating gap is quality, not parity. anno's current OCR is Tesseract/Paddle
([ingest.rs:380](../../../crates/anno-rag/src/ingest.rs) `embedded_ocr_extract`),
which is strong on clean printed text but weak on the inputs French legal work
actually produces: stamped/signed pages, handwritten annotations, multi-column
scans, tables-as-images, and low-quality faxes. A VLM transcribes layout-aware text
from those pages where line-based OCR degrades.

## Chosen model: LightOnOCR-2-1B

Default backend is **`lightonai/LightOnOCR-2-1B`** (HuggingFace):

- **Apache-2.0** — clears Spec A's permissive-license gate.
- **French-native.** Built by LightOn (FR); trained on a bilingual **French-English**
  OCR corpus (tokenizer pruning decided on FR-EN data). French is a first-class target,
  not an "other language" — the decisive factor over PaddleOCR-VL (ZH/EN-leaning).
- **1B**, ViT (Pixtral) encoder + Qwen3-based decoder, end-to-end (no external OCR
  pipeline). Handles tables, forms, multi-column, math.
- **Runs in both profiles:** vLLM-servable on GPU (on-prem profile) and available as
  **GGUF** (`Mungert/LightOnOCR-1B-1025-GGUF`) for a desktop/CPU path via llama.cpp.

Alternate (if a FR eval favours it on specific document classes):
`PaddlePaddle/PaddleOCR-VL-1.6` (Apache-2.0) — strong on seals/tables, but PaddlePaddle
framework + no ONNX, and ZH/EN-leaning. Kept as a documented fallback, not the default.

> **FR eval gate (entry criterion).** Before this is wired as default, run a small
> internal eval on 10–20 pages of real French legal documents (LightOnOCR-2-1B vs
> OlmOCR vs PaddleOCR-VL). The 2026-02 French PDF-to-Markdown benchmark (arXiv 2602.11960)
> ranks these closely; the choice must be confirmed on anno's actual corpus, not on a
> public benchmark. Record the winner + per-class scores in the implementation plan.

## Assumption (revisit if false)

anno ships across a spectrum from a locally-run desktop binary to an **on-premise GPU
appliance** (single-tenant, customer-controlled hardware). VLM-OCR must fit that
spectrum — weights fetched by the same `download_models` plumbing, inference staying
inside the customer's trust boundary in every profile. If anno's default ever becomes a
third-party hosted service, the egress posture in §4.3 changes; the trait and tier model
below do not.

## Deployment tiers (the key distinction)

VLM-OCR is **not** a local-vs-hosted binary. There are three tiers, and the privacy
boundary — not the process architecture — is what separates them:

| Tier | VLM backend | Image leaves trust boundary? | ELv2 (Spec A)? |
|---|---|---|---|
| **Desktop / CPU** | LightOnOCR GGUF in-process (llama.cpp), or none | No | Not triggered |
| **On-prem GPU** | LightOnOCR-2-1B via a **co-located vLLM** server | **No** — stays on the customer's box | **Not triggered** (single-tenant / on-prem) |
| **Third-party SaaS** | external vision API | **Yes** | **Triggered** |

The on-prem GPU tier is a client→server architecture (anno → `localhost` vLLM) but is
**privacy-equivalent to local**: the page image never leaves the customer's hardware.
This is the full-quality VLM-OCR path and needs no image-egress gate. Only the
third-party SaaS tier raises the unsolved image-PII problem — and that tier is **dropped**,
not deferred (§4.3, "Out of scope").

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

**Add VLM-OCR (LightOnOCR-2-1B) as a within-trust-boundary capability across the desktop
and on-prem GPU tiers, behind a new anno-owned trait. Do not adopt kreuzberg's ELv2 VLM
path, and do not build a third-party hosted tier.**

- A new `VlmOcrClient` trait, sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs)
  — **not** an overload of `generate_structured`. The text trait takes `(system, user,
  json_schema)`; a vision call needs image bytes + an OCR/transcription instruction and
  returns text, so it is a distinct contract.
- Two concrete backends behind that trait, both within the trust boundary:
  `LocalVlmClient` (GGUF in-process, desktop) and `VllmServerClient` (co-located vLLM,
  on-prem GPU). The third-party slot in routing stays `None`.
- Weights fetched via the existing `download_models` plumbing — no new download logic.
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

### 4.2 Backends (both within the trust boundary)

- **`LocalVlmClient` — desktop / CPU.** Runs LightOnOCR GGUF in-process via a llama.cpp
  binding. Smaller/quantized; the OCR path may stay Tesseract-only on low-end hardware.
- **`VllmServerClient` — on-prem GPU.** Talks to a **co-located** vLLM server
  (OpenAI-compatible HTTP on `localhost` / the customer's private network) serving
  `lightonai/LightOnOCR-2-1B` at full quality. This is the recommended profile for legal
  workloads. The server is part of the on-prem appliance, not a third party.
- Both register weights with `download_models` so first use fetches them and offline runs
  reuse the cache — same gating as bge-m3 / GLiNER2.
- Gated behind a Cargo feature (`vlm-ocr`), with the GPU/vLLM path aligning with anno's
  existing `gpu-cuda` build profile ([anno-rag/Cargo.toml](../../../crates/anno-rag/Cargo.toml)),
  so desktop builds compile out the runtime cost cleanly.

### 4.3 Image egress & PII (why third-party hosted is dropped, not deferred)

The existing prompt gate is **text-only** — `fallback_prompt_is_safe`
([privacy.rs:16](../../../crates/anno-rag-tabular/src/llm/privacy.rs)) regex-matches a
string. An image cannot be regex-checked, and a scanned legal page is dense PII
(names, signatures, ID photos, IBANs printed on letterhead).

- **Desktop and on-prem GPU tiers need no image gate** — the image never leaves the
  customer's trust boundary. In-process (desktop) and co-located vLLM (on-prem) are both
  on the customer's hardware. This is safe by construction.
- **Third-party SaaS VLM-OCR is dropped.** Sending a raw page image to an external
  provider defeats the pseudonymisation the text path is built to guarantee, and there is
  no cheap, reliable "is this image safe to send" check. The gateway already encodes this
  stance with `reject_images: true`
  ([config.rs:104](../../../crates/anno-privacy-gateway/src/config.rs)); Spec B keeps it.
- Were a third-party path ever revisited, it would require a real image-PII gate (face /
  signature / MRZ detection) AND an explicit opt-in — and would re-trigger Spec A's ELv2
  analysis. Out of scope here.

### 4.4 Routing & ingest integration

- A `RoutingVlmClient` mirrors
  [`RoutingLlmClient`](../../../crates/anno-rag-tabular/src/llm/routing.rs): it selects the
  within-boundary backend for the active profile (`LocalVlmClient` desktop /
  `VllmServerClient` on-prem GPU). The third-party slot is `None`.
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

1. **FR eval gate** — confirm LightOnOCR-2-1B (vs OlmOCR / PaddleOCR-VL) on real French
   legal pages before wiring anything as default. Entry criterion, not an afterthought.
2. **Trait + on-prem GPU backend** (`VllmServerClient`) + feature flag (`vlm-ocr`), wired
   into the OCR branch behind `RoutingVlmClient`. This is the primary target (legal =
   GPU appliance). Validate transcription quality vs Tesseract on the eval set.
3. **Desktop GGUF backend** (`LocalVlmClient`) + confidence-driven fallback to Tesseract,
   with chunk/heading integration so VLM output flows through the same `ElementBased`
   consumers as the rest of ingest.

## Out of scope (and why)

- **Third-party hosted VLM-OCR** — dropped. No image-PII gate exists, it breaks the
  privacy posture, and it would re-trigger Spec A's ELv2 prohibition. The on-prem GPU
  tier delivers full VLM quality without any of that.
- **Kreuzberg's built-in VLM-OCR** — ELv2-coupled and provider-shaped; adopting it
  reintroduces the licensing/sovereignty posture Spec A contains. Not used.
- **Image embeddings / multimodal retrieval** — VLM-OCR produces *text* that flows into
  the existing text embedding + retrieval pipeline. Embedding images directly is a
  separate effort (YAGNI here).
- **Structured extraction from images** — once a page is transcribed to text, the
  existing `LlmClient::generate_structured` / tabular path handles structure. No new
  structured-from-pixels path.

## Risks

- **FR quality unconfirmed on anno's corpus** — public benchmarks rank LightOnOCR,
  OlmOCR, and PaddleOCR-VL closely for French; the phase-1 eval gate exists to resolve
  this on real documents before commitment.
- **On-prem GPU ops dependency** — the vLLM sidecar adds a server process + GPU drivers
  to the appliance. Acceptable for a GPU deployment; mitigated by the desktop GGUF path
  for non-GPU installs.
- **Latency/RAM** — vision models are heavier than Tesseract; run VLM only on
  OCR-classified pages and keep Tesseract as the default for clean scans.
- **License verification** — LightOnOCR-2-1B is Apache-2.0 per its model card; re-confirm
  at execution time (Spec A discipline), including any GGUF redistribution terms.

## Acceptance

- This document exists and is committed alongside Spec A.
- The design names the default model (LightOnOCR-2-1B, Apache-2.0), the three deployment
  tiers, and the two within-boundary backends, and reuses the existing local-first /
  routing / safety-gate patterns rather than importing kreuzberg's VLM path.
- A reader can see exactly which ingest branch changes
  ([ingest.rs](../../../crates/anno-rag/src/ingest.rs) OCR path), why images stay gated
  against third parties by default ([config.rs](../../../crates/anno-privacy-gateway/src/config.rs)
  `reject_images`), and that the third-party hosted tier is dropped — not deferred (§4.3).
