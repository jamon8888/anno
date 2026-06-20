# VLM-OCR: Vision-Model Document Transcription — Design

**Date:** 2026-06-20
**Revised:** 2026-06-20
**Status:** Design — scoped, implementation in progress
**Spec ID:** Spec B (of a two-spec effort; Spec A = [Kreuzberg License Migration](2026-06-20-kreuzberg-licensing-containment-design.md))

## Summary

anno gains VLM-OCR on its own terms: a permissively-licensed vision model running
inside the customer's trust boundary, routed through **`liter-llm`** (MIT, Rust-native),
across two deployment profiles (desktop and on-premise GPU). No document image ever
leaves the customer's hardware. No dependency on kreuzberg's VLM-OCR path (introduced
at 4.8.0 under ELv2 — the same release that changed the license; Spec A migrates anno
to kreuzberg 4.7.4 MIT which has no built-in VLM-OCR).

The motivating gap is quality, not parity. anno's current OCR is Tesseract/Paddle
([ingest.rs:380](../../../crates/anno-rag/src/ingest.rs) `embedded_ocr_extract`),
which is strong on clean printed text but weak on the inputs French legal work
produces: stamped/signed pages, handwritten annotations, multi-column scans,
tables-as-images, and low-quality faxes. A VLM transcribes layout-aware text from
those pages where line-based OCR degrades.

## Chosen model: LightOnOCR-2-1B

Default backend: **`lightonai/LightOnOCR-2-1B`** (HuggingFace):

- **Apache-2.0** — clears the full-MIT-stack requirement.
- **French-native.** Built by LightOn (FR); trained on a bilingual **French-English**
  OCR corpus. French is a first-class target, not an "other language" — the decisive
  factor over PaddleOCR-VL (ZH/EN-leaning).
- **1B**, ViT (Pixtral) encoder + Qwen3-based decoder. Handles tables, forms,
  multi-column, math.
- **Runs in both deployment profiles:**
  - vLLM-servable on GPU (on-prem profile)
  - Available as GGUF (`Mungert/LightOnOCR-1B-1025-GGUF`) for desktop; served by
    `llama-server` (pre-built binary, no Rust binding needed)

Alternate if FR eval favours it on specific document classes:
`PaddlePaddle/PaddleOCR-VL-1.6` (Apache-2.0) — strong on seals/tables, ZH/EN-leaning.
Kept as a documented per-class fallback, not the default.

> **FR eval gate (entry criterion).** Before this is wired as default, run a small
> internal eval on 10–20 pages of real French legal documents (LightOnOCR-2-1B vs
> OlmOCR vs PaddleOCR-VL). Record the winner + per-class scores in the implementation
> plan. Fixtures must not contain real client PII (privacy rules).

## Deployment tiers

| Tier | VLM backend | liter-llm target | Image leaves box? | ELv2 |
|---|---|---|---|---|
| **Desktop / CPU** | `LocalVlmClient` → `llama-server` (LightOnOCR GGUF) | `base_url = http://127.0.0.1:8080` | No | Not triggered |
| **On-prem GPU** (primary) | `VllmServerClient` → co-located vLLM | `base_url = http://127.0.0.1:8000` | **No** — stays on customer's box | **Not triggered** |
| **Third-party SaaS** | **NOT BUILT** — dropped | — | Yes | Triggered |

The on-prem GPU tier is client→server (anno → `localhost` vLLM) but
**privacy-equivalent to local**: the page image never leaves the customer's hardware.
Only the third-party SaaS tier raises the unsolved image-PII problem — and that tier
is **dropped, not deferred** (§4.3).

## Motivation: where today's OCR fails

| Input | Tesseract/Paddle today | VLM-OCR |
|---|---|---|
| Handwritten margin notes | Dropped or garbled | Transcribed with layout context |
| Stamp/signature block over text | Line detection breaks | Read as a region, text recovered |
| Table rendered as scanned image | Cells lost | Structure-aware transcription |
| Multi-column / rotated scan | Reading order scrambled | Reading order inferred |
| Accented French at low DPI | Char-level substitution | Context corrects to valid tokens |

## Decision

**Add VLM-OCR (LightOnOCR-2-1B) as a within-trust-boundary capability across the
desktop and on-prem GPU tiers, behind a new anno-owned trait, routed through
`liter-llm` (MIT).**

- A new `VlmOcrClient` trait, sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs)
  — **not** an overload of `generate_structured`. A vision call needs image bytes + an
  OCR instruction and returns text; that is a distinct contract from text→JSON.
- **`liter-llm`** (kreuzberg's own MIT Rust-native universal LLM client, 143 providers)
  handles the OpenAI-compatible HTTP transport for both backends. Built as a response
  to the 2026 litellm Python backdoor: compiled Rust core, no pip, no supply chain
  risk, secrets in `secrecy::SecretString`.
- Both backends are OpenAI-compat HTTP endpoints on the customer's hardware, reached
  via `liter_llm::ClientConfig::base_url`. Only the URL and model differ.
- VLM-OCR runs **only on pages classified as needing OCR** (`page_needs_ocr` /
  `DocClass::ScannedPdf | MixedPdf`), never on digital text.

## Design

### 4.1 The `VlmOcrClient` trait

```rust
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance, e.g. "French legal contract; preserve table structure".
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription>;
    fn model_id(&self) -> &str;
}
```

`PageImage` wraps raw image bytes + provenance (doc id, page index) for audit logs.
`Transcription` returns text plus a confidence/coverage signal for the Tesseract
fallback. Both types live in `crates/anno-rag-tabular/src/llm/vlm/mod.rs`.

### 4.2 Backends — both via `liter-llm`

Both backends call `liter_llm::DefaultClient::chat_completion` with a
`ChatCompletionRequest` containing `ContentPart::ImageUrl` (image as a base64 data
URL, encoded via `liter_llm::image::encode_data_url`) plus the OCR hint as the text
part. This is the exact pattern kreuzberg uses internally in `kreuzberg/src/llm/vlm_ocr.rs@4.8.0` — we
replicate it with the MIT liter-llm crate directly, without taking the ELv2 kreuzberg dependency.

**`VllmServerClient` — on-prem GPU:**
- `ClientConfig { base_url: "http://127.0.0.1:8000", api_key: "" }` (vLLM on-prem needs no key)
- Model: `lightonai/LightOnOCR-2-1B`
- The co-located vLLM server is part of the on-prem appliance — within the trust boundary.

**`LocalVlmClient` — desktop / CPU:**
- `ClientConfig { base_url: "http://127.0.0.1:8080", api_key: "" }`
- Model: `LightOnOCR-1B-1025` (GGUF via `llama-server` — pre-built binary, no Rust binding)
- Same liter-llm call path as `VllmServerClient`; `LocalVlmClient` delegates to it.

Weights registered with `download_models` — same gating as bge-m3 / GLiNER2.
Gated behind a `vlm-ocr` Cargo feature (`dep:liter-llm`), off by default.

### 4.3 Image egress & PII (why third-party hosted is dropped, not deferred)

- **Desktop and on-prem GPU need no image gate** — the image never leaves the
  customer's trust boundary. Safe by construction.
- **Third-party SaaS VLM-OCR is dropped.** Sending a raw page image to an external
  provider defeats pseudonymisation, there is no cheap "is this image safe to send"
  check, and it re-triggers ELv2 (Spec A). The gateway keeps `reject_images: true`
  ([config.rs:104](../../../crates/anno-privacy-gateway/src/config.rs)).
- Were a third-party path ever revisited, it would require a real image-PII gate
  (face / signature / MRZ detection) AND explicit opt-in. Out of scope here.

### 4.4 Routing & ingest integration

- `RoutingVlmClient` mirrors [`RoutingLlmClient`](../../../crates/anno-rag-tabular/src/llm/routing.rs):
  selects the within-boundary backend from `config.vlm_backend`. No third-party slot.
- In the OCR branch of [ingest.rs](../../../crates/anno-rag/src/ingest.rs)
  (`OcrMode::AutoEmbedded`, `embedded_ocr_extract`), when `vlm-ocr` is on and a page
  is `ScannedPdf`/`MixedPdf`: render the page to a `PageImage`, call
  `RoutingVlmClient::transcribe`, emit via the existing `ElementBased` consumers.
  Tesseract is the fallback when `Transcription.confidence` is below threshold (default 0.6).
- Page image sourcing: `pdfium-render` (already a transitive dep via `kreuzberg/pdf`)
  renders PDF pages to bitmaps. This is the preferred approach — no new dep, no
  second extraction pass.

## Phasing

1. **FR eval gate** — confirm LightOnOCR-2-1B on real French legal pages (vs OlmOCR /
   PaddleOCR-VL). Entry criterion before anything is wired as default.
2. **Trait + on-prem GPU backend** (`VllmServerClient` via liter-llm) + `vlm-ocr`
   feature, wired into the OCR branch behind `RoutingVlmClient`.
3. **Desktop backend** (`LocalVlmClient` → `llama-server`) + confidence-driven Tesseract
   fallback.

## Out of scope (and why)

- **kreuzberg's built-in VLM-OCR** — ELv2-coupled (introduced at 4.8.0). Not used.
- **Third-party hosted VLM-OCR** — dropped (§4.3).
- **Image embeddings / multimodal retrieval** — VLM-OCR produces *text*; image
  embedding is a separate effort (YAGNI).
- **Structured extraction from images** — once transcribed to text, the existing
  `LlmClient::generate_structured` / tabular path handles structure.

## Risks

- **FR quality unconfirmed on anno's corpus** — phase-1 eval gate exists to resolve this.
- **Page bitmap sourcing** — pdfium-render is preferred; confirm API at implementation
  time (Task 6 Step 3 in the plan).
- **Confidence heuristic** — liter-llm does not expose per-token logprobs from vLLM
  by default; the initial implementation uses a length heuristic. Add a logprob pass
  or re-OCR agreement check before relying on the threshold in production.
- **llama-server ops dependency** — the desktop path requires the user to run a
  `llama-server` process. Document in the user-facing setup guide; fallback to Tesseract
  if the server is unreachable.
- **License verification** — LightOnOCR-2-1B is Apache-2.0; `Mungert/LightOnOCR-1B-1025-GGUF`
  redistribution terms must be re-confirmed at execution time (third-party GGUF repacks
  sometimes add restrictions).

## Acceptance

- This document and Spec A are committed.
- The design names the default model (LightOnOCR-2-1B, Apache-2.0), liter-llm as the
  transport layer, the three deployment tiers, and the two within-boundary backends.
- A reader can see why third-party hosted is dropped (§4.3), why kreuzberg's built-in
  VLM-OCR is not used (ELv2), and why liter-llm was chosen over hand-rolled reqwest
  (MIT, Rust-native, kreuzberg's own client, exact same API pattern).
- Implementation plan: [2026-06-20-vlm-ocr-implementation.md](../plans/2026-06-20-vlm-ocr-implementation.md)
