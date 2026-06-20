# VLM-OCR Implementation + Kreuzberg Licensing Containment

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the two-spec effort:
- **Spec A** ([kreuzberg-licensing-containment](../specs/2026-06-20-kreuzberg-licensing-containment-design.md)) — its one remaining code change: tighten the `deny.toml` ELv2 comment into a pointer to the spec (Task 1).
- **Spec B** ([vlm-ocr](../specs/2026-06-20-vlm-ocr-design.md)) — add a within-trust-boundary VLM-OCR path (default model **`lightonai/LightOnOCR-2-1B`**, Apache-2.0) behind a new `VlmOcrClient` trait, with a co-located **vLLM** backend for the on-prem GPU profile and a **GGUF** backend for desktop. Third-party hosted is **dropped** (Spec B §4.3). Tasks 2–7.

**Prerequisites:**
- On `main` (Spec A `6c56d7b5`, Spec B `a1a1d66e` already committed).
- Local Rust loop per CLAUDE.md: `CARGO_TARGET_DIR=E:\cargo-target`, use `scripts/test-local.ps1` / `scripts/dev-fast.ps1`. Never `cargo build --workspace`.
- For the on-prem GPU backend test: a reachable vLLM server (local dev box or appliance) serving `lightonai/LightOnOCR-2-1B`.
- Branch: `feat/vlm-ocr`.

---

## Deployment tiers (drives the backend design — Spec B "Deployment tiers")

| Tier | Backend built here | Image leaves box? | ELv2 |
|------|--------------------|-------------------|------|
| Desktop / CPU | `LocalVlmClient` — LightOnOCR GGUF via llama.cpp | No | Not triggered |
| **On-prem GPU** (primary) | `VllmServerClient` — co-located vLLM, LightOnOCR-2-1B | **No** | **Not triggered** |
| Third-party SaaS | **NOT BUILT** — dropped | Yes | Triggered |

Both built backends are within the customer's trust boundary, so **no image-PII gate is
needed**. The third-party tier is dropped, not deferred — see Spec B §4.3.

---

## File Map

| File | Change |
|------|--------|
| `deny.toml` | Task 1 — rewrite ELv2 comment into a pointer to Spec A's trigger + escape plan |
| `crates/anno-rag-tabular/src/llm/vlm/mod.rs` | NEW — `VlmOcrClient` trait, `PageImage`, `Transcription` |
| `crates/anno-rag-tabular/src/llm/vlm/vllm_server.rs` | NEW — `VllmServerClient` (on-prem GPU; OpenAI-compatible HTTP to co-located vLLM) |
| `crates/anno-rag-tabular/src/llm/vlm/local_gguf.rs` | NEW — `LocalVlmClient` (desktop; LightOnOCR GGUF via llama.cpp) |
| `crates/anno-rag-tabular/src/llm/vlm/routing.rs` | NEW — `RoutingVlmClient` (selects within-boundary backend; third-party slot `None`) |
| `crates/anno-rag-tabular/src/llm/mod.rs` | Add `pub mod vlm;` |
| `crates/anno-rag-tabular/Cargo.toml` | Add `vlm-ocr` feature |
| `crates/anno-rag/src/ingest.rs` | OCR branch: route `ScannedPdf`/`MixedPdf` page images through VLM; flip `extract_images` for the OCR path only ([ingest.rs:262](../../../crates/anno-rag/src/ingest.rs)) |
| `crates/anno-rag/src/config.rs` | `vlm_backend` (off/gguf/vllm), `vlm_vllm_url`, `vlm_confidence_threshold` |
| `crates/anno-rag/Cargo.toml` | Add `vlm-ocr` passthrough feature |

---

### Task 1: Spec A — tighten the deny.toml ELv2 comment

The current comment ([deny.toml](../../../deny.toml)) still reads "REVIEW BEFORE any SaaS/hosted offering" — vague, with no pointer to the trigger or escape plan. Spec A's Acceptance requires it point at the spec.

- [ ] **Step 1: Rewrite the comment block**

Replace the trailing "REVIEW BEFORE any SaaS/hosted offering of…" lines above the `kreuzberg` allow entry with:

```toml
    # Elastic-2.0 via `kreuzberg` (direct anno-rag dependency — document
    # extraction backend). Source-available, NOT OSI-approved: ELv2 §1 forbids
    # offering the software as a managed service. anno consumes kreuzberg as a
    # library in a locally-run desktop / on-prem appliance — not triggered.
    #
    # TRIGGER + ESCAPE PLAN: docs/superpowers/specs/2026-06-20-kreuzberg-licensing-containment-design.md
    # Before ANY third-party hosted/multi-tenant offering exposing kreuzberg-backed
    # extraction, execute the permissive-stack escape plan (A3) in that spec.
    { crate = "kreuzberg", allow = ["Elastic-2.0"] },
```

- [ ] **Step 2: Verify deny still passes**

```powershell
cargo deny check licenses 2>&1 | Select-String -Pattern "kreuzberg|error|advisories" | Select-Object -First 10
```

Expected: no new license errors; `kreuzberg` Elastic-2.0 still allowed.

- [ ] **Step 3: Commit**

```powershell
git add deny.toml
git commit -m "docs(license): point kreuzberg ELv2 deny.toml entry at containment spec (Spec A)"
```

---

### Task 2: FR eval gate (entry criterion — do this BEFORE wiring a default)

Spec B makes the model choice conditional on real French legal pages. Resolve it first.

- [ ] **Step 1: Assemble a fixture set** of 10–20 representative French legal pages
  (scanned contracts, stamped/signed pages, handwritten margins, table-heavy pages).
  Keep them under test fixtures; do **not** commit real client PII — use synthetic or
  consented samples (privacy rules).

- [ ] **Step 2: Run each candidate** — `lightonai/LightOnOCR-2-1B`, `allenai/olmOCR-*`,
  `PaddlePaddle/PaddleOCR-VL-1.6` — over the set (a throwaway Python/vLLM harness is fine
  here; this is a selection eval, not shipped code).

- [ ] **Step 3: Score** per class (printed / handwritten / tables / stamps) on CER/WER and
  table-cell F1. Record the winner + per-class scores **in this file** under "Eval result"
  below.

- [ ] **Step 4: Confirm the default.** If LightOnOCR-2-1B wins or ties, keep it as default
  (expected). If PaddleOCR-VL wins a class decisively, note it as a per-class override.

> **Eval result:** _(fill in: model, date, per-class scores, decision)_

---

### Task 3: `VlmOcrClient` trait + value types

Sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs) — NOT an overload of `generate_structured` (a vision call needs image bytes + an OCR instruction → text). Mirror the trait's `Send + Sync` + `model_id()` shape so the same `Arc<dyn …>` fan-out and routing wrapper apply.

- [ ] **Step 1: Create the module dir + trait**

`crates/anno-rag-tabular/src/llm/vlm/mod.rs`:

```rust
//! Vision-OCR client trait — transcribes page images to text. Sibling to
//! [`LlmClient`](crate::llm::LlmClient): that trait is text→JSON; this one is
//! image→text. Backends in [`vllm_server`] (on-prem GPU) and [`local_gguf`]
//! (desktop); routing in [`routing`]. Both backends run inside the customer's
//! trust boundary — no third-party egress (Spec B §4.3).

use async_trait::async_trait;

pub mod local_gguf;
pub mod routing;
pub mod vllm_server;

/// A decoded page image plus provenance, so every transcription is
/// attributable in audit logs (mirrors `Author::System { extractor_version }`).
#[derive(Debug, Clone)]
pub struct PageImage {
    /// Decoded RGB image bytes (caller decodes from the source doc).
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Source document id this page came from.
    pub doc_id: String,
    /// Zero-based page index within the source document.
    pub page: usize,
}

/// Result of transcribing one page image.
#[derive(Debug, Clone)]
pub struct Transcription {
    /// Layout-aware transcribed text.
    pub text: String,
    /// Confidence in [0.0, 1.0]; drives the Tesseract fallback (Task 6).
    pub confidence: f32,
}

/// One vision-OCR call. `Send + Sync` so ingest can fan pages across tokio tasks.
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance, e.g. "French legal contract; preserve table structure".
    ///
    /// # Errors
    /// Returns [`crate::error::Error`] on backend or inference failure.
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription>;

    /// Stable model identifier for audit logs.
    fn model_id(&self) -> &str;
}
```

- [ ] **Step 2: Register the module** in [`llm/mod.rs`](../../../crates/anno-rag-tabular/src/llm/mod.rs):

```rust
/// Vision-OCR client (within-trust-boundary image→text transcription).
#[cfg(feature = "vlm-ocr")]
pub mod vlm;
```

- [ ] **Step 3: Compile-check** (stub the two backend modules first):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
```

---

### Task 4: `vlm-ocr` Cargo feature

Mirror the `gliner2` feature ([Cargo.toml](../../../crates/anno-rag-tabular/Cargo.toml) line 13) — off by default so CI never downloads VLM weights.

- [ ] **Step 1: anno-rag-tabular feature**

```toml
# Within-boundary vision-OCR (VLM). Off by default — pulls a vision model at
# runtime. Wires VlmOcrClient + RoutingVlmClient into the OCR path.
# `reqwest` = OpenAI-compatible HTTP to a co-located vLLM (on-prem GPU).
# `image`   = decode PageImage. GGUF/llama.cpp binding added in Task 5 Step 2.
vlm-ocr = ["dep:reqwest", "dep:image"]
```

> ⚠️ The desktop GGUF backend (Task 5) needs a llama.cpp binding (e.g. `llama-cpp-2`) or
> a local llama.cpp server reached over the same HTTP client as vLLM. Decide in Task 5
> Step 1 and adjust this `dep:` list. The on-prem GPU path needs only `reqwest`.

- [ ] **Step 2: anno-rag passthrough feature** in [`anno-rag/Cargo.toml`](../../../crates/anno-rag/Cargo.toml). Align the GPU path with the existing `gpu-cuda` profile:

```toml
# Route OCR-classified pages through a within-boundary VLM (Spec B). Off by default.
vlm-ocr = ["anno-rag-tabular/vlm-ocr"]
```

- [ ] **Step 3: Verify both feature on/off configs compile**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Features vlm-ocr -Mode check
```

- [ ] **Step 4: Commit (trait + feature scaffolding)**

```powershell
git add crates/anno-rag-tabular/src/llm/vlm/mod.rs crates/anno-rag-tabular/src/llm/mod.rs crates/anno-rag-tabular/Cargo.toml crates/anno-rag/Cargo.toml
git commit -m "feat(vlm): VlmOcrClient trait + vlm-ocr feature scaffolding (Spec B)"
```

---

### Task 5: Backends — `VllmServerClient` (on-prem GPU) then `LocalVlmClient` (desktop)

Build the on-prem GPU backend first — it is the primary target for legal workloads.

- [ ] **Step 1: `VllmServerClient`** (`vlm/vllm_server.rs`) — POST the page image to a
  **co-located** vLLM OpenAI-compatible `/v1/chat/completions` endpoint (image as a
  base64 data URL + the OCR `hint` as the text part) serving `lightonai/LightOnOCR-2-1B`.
  URL comes from `config.vlm_vllm_url` (default `http://127.0.0.1:8000`). This is on the
  customer's box — within the trust boundary, no third-party egress.

```rust
use super::{PageImage, Transcription, VlmOcrClient};
use async_trait::async_trait;

pub struct VllmServerClient {
    base_url: String,           // co-located vLLM, e.g. http://127.0.0.1:8000
    model_id: String,           // "lightonai/LightOnOCR-2-1B"
    http: reqwest::Client,
}

#[async_trait]
impl VlmOcrClient for VllmServerClient {
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription> {
        // build chat/completions request: image_url(data:image/png;base64,..) + hint
        // parse text; derive confidence (see ⚠️ in Self-Review)
        todo!("vLLM call")
    }
    fn model_id(&self) -> &str { &self.model_id }
}
```

- [ ] **Step 2: `LocalVlmClient`** (`vlm/local_gguf.rs`) — load LightOnOCR GGUF
  (`Mungert/LightOnOCR-1B-1025-GGUF`) via the binding chosen in Task 4, in-process.
  Mirror the GLiNER2 `from_pretrained` + `download_models` pattern
  ([llm/local/client.rs](../../../crates/anno-rag-tabular/src/llm/local/client.rs)).

- [ ] **Step 3: Weights via `download_models`.** Register both artifacts so first use
  fetches and offline reuses the cache.

  > ⚠️ **License gate (Spec A discipline):** LightOnOCR-2-1B is Apache-2.0 per its model
  > card — re-confirm the safetensors AND GGUF redistribution terms before shipping.

- [ ] **Step 4: Tests.** `VllmServerClient` against a dev vLLM (`#[ignore]` — needs a
  server); `LocalVlmClient` against a fixture page (`#[ignore]` — downloads weights),
  mirroring the GLiNER2 `#[ignore = "downloads … weights at runtime"]` convention.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features vlm-ocr
```

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/vlm/vllm_server.rs crates/anno-rag-tabular/src/llm/vlm/local_gguf.rs
git commit -m "feat(vlm): vLLM (on-prem GPU) + GGUF (desktop) LightOnOCR backends (Spec B)"
```

---

### Task 6: `RoutingVlmClient` + ingest wiring + Tesseract fallback

`vlm/routing.rs` mirrors [`RoutingLlmClient`](../../../crates/anno-rag-tabular/src/llm/routing.rs): it picks the within-boundary backend for the active profile. The third-party slot stays `None` (Spec B §4.3).

- [ ] **Step 1: `RoutingVlmClient`**

```rust
use super::{PageImage, Transcription, VlmOcrClient};
use async_trait::async_trait;

pub struct RoutingVlmClient {
    /// Active within-boundary backend: VllmServerClient (on-prem GPU) or
    /// LocalVlmClient (desktop), chosen from config.vlm_backend.
    backend: Box<dyn VlmOcrClient>,
    // NOTE: no third-party slot. A hosted backend would require an image-PII
    // gate (Spec B §4.3) and is intentionally not representable here.
}

#[async_trait]
impl VlmOcrClient for RoutingVlmClient {
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription> {
        self.backend.transcribe(image, hint).await
    }
    fn model_id(&self) -> &str { "routing-vlm" }
}
```

- [ ] **Step 2: Enable image extraction for the OCR path only.** [`ingest.rs:262`](../../../crates/anno-rag/src/ingest.rs) currently sets `extract_images: false`. Flip to `true` **only** in the `embedded_ocr_extract` config (the `ScannedPdf`/`MixedPdf` path); leave `native_extraction_config` ([ingest.rs:245](../../../crates/anno-rag/src/ingest.rs)) untouched so digital docs still skip images.

- [ ] **Step 3: Route OCR-classified pages.** In the `OcrMode::AutoEmbedded` arm ([ingest.rs:159](../../../crates/anno-rag/src/ingest.rs)), behind `#[cfg(feature = "vlm-ocr")]`: for each `page_needs_ocr` page, build a `PageImage` from the kreuzberg `ExtractedImage`, call `transcribe`, and emit the result through the existing `ElementBased` chunk consumers.

- [ ] **Step 4: Confidence fallback to Tesseract.** When `Transcription.confidence` is below `config.vlm_confidence_threshold` (default ~0.6), discard the VLM text and keep the Tesseract result for that page. Log the decision with `tracing` (page index + chosen backend), per the Rust rules.

- [ ] **Step 5: Integration test on a mixed scanned/digital fixture**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -Features vlm-ocr
```

Expected: scanned page yields VLM text; digital page unchanged; low-confidence page falls back to Tesseract (no panic, no empty chunks).

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/vlm/routing.rs crates/anno-rag/src/ingest.rs crates/anno-rag/src/config.rs
git commit -m "feat(vlm): RoutingVlmClient + OCR-branch wiring with Tesseract fallback (Spec B)"
```

---

### Task 7: PR

- [ ] **Step 1: fmt + clippy before pushing** (per repo convention — commit fmt separately if it changes anything)

```powershell
cargo fmt --all
cargo clippy -p anno-rag-tabular -p anno-rag --features vlm-ocr --jobs 2 2>&1 | Select-String -Pattern "warning|error" | Select-Object -First 20
```

- [ ] **Step 2: Open PR**

```powershell
git push origin feat/vlm-ocr
gh pr create --title "feat: VLM-OCR (LightOnOCR, on-prem GPU + desktop) + kreuzberg ELv2 containment" --body "Implements Spec A (deny.toml pointer) + Spec B.

## Model
- Default: lightonai/LightOnOCR-2-1B (Apache-2.0, French-native) — confirmed by FR eval (Task 2)

## Changes
- deny.toml: ELv2 comment points at containment spec (Spec A)
- VlmOcrClient trait + PageImage/Transcription (anno-rag-tabular/src/llm/vlm)
- VllmServerClient (on-prem GPU, co-located vLLM) + LocalVlmClient (desktop GGUF)
- RoutingVlmClient (within-boundary only; no third-party slot)
- vlm-ocr Cargo feature (off by default) in anno-rag-tabular + anno-rag
- ingest.rs: OCR-classified pages route through VLM, Tesseract fallback

## Out of scope (dropped, not deferred)
- Third-party hosted VLM-OCR — no image-PII gate; re-triggers ELv2 (Spec B §4.3)

## Test plan
- [ ] FR eval recorded in plan Task 2 (LightOnOCR vs OlmOCR vs PaddleOCR-VL)
- [ ] cargo deny check licenses — kreuzberg Elastic-2.0 still allowed
- [ ] check passes with vlm-ocr on AND off
- [ ] VllmServerClient transcribes a scanned-legal fixture via co-located vLLM
- [ ] LocalVlmClient (GGUF) transcribes the same fixture
- [ ] digital-text doc unchanged (no image extraction)
- [ ] low-confidence page falls back to Tesseract"
```

---

## Self-Review

- ✅ Default model is **LightOnOCR-2-1B** — Apache-2.0, French-native (LightOn, FR-EN training), beats PaddleOCR-VL's ZH/EN lean for legal FR
- ✅ Three-tier model: desktop GGUF + on-prem GPU vLLM are **within the trust boundary** → no image-PII gate needed; third-party SaaS **dropped, not deferred**
- ✅ "No ONNX" non-issue — LightOnOCR runs via vLLM (GPU) and GGUF (CPU), both native runtimes; no conversion spike
- ✅ FR eval is **Task 2 — an entry gate**, before any default is wired (resolves the public-benchmark uncertainty on anno's real corpus)
- ✅ New `VlmOcrClient` trait is a sibling to `LlmClient`, not an overload (image→text vs text→JSON)
- ✅ `vlm-ocr` feature **off by default** — mirrors `gliner2`; GPU path aligns with existing `gpu-cuda`
- ✅ VLM scoped to the OCR branch (`ScannedPdf`/`MixedPdf`); digital docs untouched (`extract_images` flip is OCR-config-only); Tesseract stays as confidence fallback
- ✅ Spec A's deny.toml acceptance item folded in as Task 1 (currently still the vague "REVIEW BEFORE" note)
- ⚠️ **Runtime dep list** (Task 4) — `reqwest` covers vLLM; the GGUF backend needs a llama.cpp binding decided in Task 5 Step 1; adjust the feature `dep:` list then.
- ⚠️ **Confidence source** — if LightOnOCR exposes no native confidence, derive a heuristic (mean token logprob, or a re-OCR agreement check) before relying on the Task 6 threshold.
- ⚠️ **Eval fixtures must not contain real client PII** — use synthetic/consented French legal pages (privacy rules).
