# VLM-OCR Implementation + Kreuzberg Licensing Containment

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the two-spec effort:
- **Spec A** ([kreuzberg-licensing-containment](../specs/2026-06-20-kreuzberg-licensing-containment-design.md)) — its one remaining code change: tighten the `deny.toml` ELv2 comment into a pointer to the spec (Task 1).
- **Spec B** ([vlm-ocr](../specs/2026-06-20-vlm-ocr-design.md)) — add a local-default VLM-OCR path behind a new `VlmOcrClient` trait, wired into the OCR ingest branch, hosted path deferred (Tasks 2–6).

**Prerequisites:**
- On `main` (Spec A `6c56d7b5`, Spec B `a1a1d66e` already committed).
- Local Rust loop per CLAUDE.md: `CARGO_TARGET_DIR=E:\cargo-target`, use `scripts/test-local.ps1` / `scripts/dev-fast.ps1`. Never `cargo build --workspace`.
- Branch: `feat/vlm-ocr`.

---

## Scope boundary (what this plan does NOT do)

| Deferred | Why | Where |
|---|---|---|
| Hosted VLM-OCR fallback | Needs the image-PII gate, which has no cheap reliable design yet | Spec B §4.3, phase 3 |
| Image embeddings / multimodal retrieval | VLM-OCR emits *text* into the existing pipeline; embedding pixels is a separate effort | Spec B "Out of scope" |
| Adopting kreuzberg's ELv2 VLM layer | Reintroduces the licensing posture Spec A contains | Spec A |

Phase 1 (Tasks 2–5) ships a **local-only, safe-by-construction** VLM-OCR: no network at inference, so no image leaves the device and no image gate is required. Phase 2 (Task 6) adds the Tesseract confidence fallback + chunk integration.

---

## File Map

| File | Change |
|------|--------|
| `deny.toml` | Task 1 — rewrite ELv2 comment (lines ~70–80) into a pointer to Spec A's trigger + escape plan |
| `crates/anno-rag-tabular/src/llm/vlm/mod.rs` | NEW — `VlmOcrClient` trait, `PageImage`, `Transcription` |
| `crates/anno-rag-tabular/src/llm/vlm/local.rs` | NEW — local VLM runtime (ONNX/Candle), `download_models` registration |
| `crates/anno-rag-tabular/src/llm/vlm/routing.rs` | NEW — `RoutingVlmClient` (local-first; hosted slot stays `None` in phase 1) |
| `crates/anno-rag-tabular/src/llm/mod.rs` | Add `pub mod vlm;` |
| `crates/anno-rag-tabular/Cargo.toml` | Add `vlm-ocr` feature (mirrors `gliner2`) |
| `crates/anno-rag/src/ingest.rs` | OCR branch: route `ScannedPdf`/`MixedPdf` page images through VLM; flip `extract_images` for the OCR path only ([ingest.rs:262](../../../crates/anno-rag/src/ingest.rs)) |
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
    # library in a locally-run desktop binary — not triggered by that use.
    #
    # TRIGGER + ESCAPE PLAN: docs/superpowers/specs/2026-06-20-kreuzberg-licensing-containment-design.md
    # Before ANY hosted/multi-tenant offering exposing kreuzberg-backed
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

### Task 2: `VlmOcrClient` trait + value types

Sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs) — NOT an overload of `generate_structured` (a vision call needs image bytes + an OCR instruction → text). Mirror the trait's `Send + Sync` + `model_id()` shape so the same `Arc<dyn …>` fan-out and routing wrapper apply.

- [ ] **Step 1: Create the module dir + trait**

`crates/anno-rag-tabular/src/llm/vlm/mod.rs`:

```rust
//! Vision-OCR client trait — transcribes page images to text. Sibling to
//! [`LlmClient`](crate::llm::LlmClient): that trait is text→JSON; this one is
//! image→text. Local backend in [`local`]; routing in [`routing`].

use async_trait::async_trait;

pub mod local;
pub mod routing;

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
    /// Model self-reported / heuristic confidence in [0.0, 1.0]; drives the
    /// Tesseract fallback decision in [`routing::RoutingVlmClient`] (Task 6).
    pub confidence: f32,
}

/// One vision-OCR call. `Send + Sync` so ingest can fan pages across tokio tasks.
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance, e.g. "French legal contract; preserve table structure".
    ///
    /// # Errors
    /// Returns [`crate::error::Error`] on model-load or inference failure.
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription>;

    /// Stable model identifier for audit logs.
    fn model_id(&self) -> &str;
}
```

- [ ] **Step 2: Register the module**

In [`llm/mod.rs`](../../../crates/anno-rag-tabular/src/llm/mod.rs), add next to the other `pub mod` lines:

```rust
/// Vision-OCR client (local-first image→text transcription).
#[cfg(feature = "vlm-ocr")]
pub mod vlm;
```

- [ ] **Step 3: Compile-check (trait only — local/routing are stubs next)**

Comment out the `pub mod local/routing` lines temporarily, then:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
```

---

### Task 3: `vlm-ocr` Cargo feature

Mirror the `gliner2` feature ([Cargo.toml](../../../crates/anno-rag-tabular/Cargo.toml) line 13) — off by default so CI never downloads VLM weights.

- [ ] **Step 1: anno-rag-tabular feature**

```toml
# Local vision-OCR (VLM) backend. Off by default — pulls a vision model
# at runtime. Wires VlmOcrClient + RoutingVlmClient into the OCR path.
vlm-ocr = ["dep:ort", "dep:image"]
```

> ⚠️ Adjust the `dep:` list to the actual runtime chosen in Task 4 (ONNX via `ort`, or Candle). `image` is for decoding `PageImage`.

- [ ] **Step 2: anno-rag passthrough feature**

In [`anno-rag/Cargo.toml`](../../../crates/anno-rag/Cargo.toml) `[features]`:

```toml
# Route OCR-classified pages through a local VLM (Spec B). Off by default.
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
git commit -m "feat(vlm): VlmOcrClient trait + vlm-ocr feature scaffolding (Spec B phase 1)"
```

---

### Task 4: Local VLM runtime

`crates/anno-rag-tabular/src/llm/vlm/local.rs` — the default backend. Mirror the GLiNER2 local pattern ([llm/local/client.rs](../../../crates/anno-rag-tabular/src/llm/local/client.rs)): construct from `download_models`-fetched weights, run on-device, no network.

- [ ] **Step 1: Pick + register the model**

Choose a permissively-licensed, OCR-focused VLM (~1–4B). Register its weights with the existing `download_models` plumbing so first use fetches and offline runs reuse the cache — same path as bge-m3 / GLiNER2.

> ⚠️ **License gate (Spec A discipline):** verify the model weights' SPDX license is permissive AND redistributable via `download_models` BEFORE wiring it in. Record the model id + license in this file once chosen.

- [ ] **Step 2: Implement `LocalVlmClient`**

```rust
use super::{PageImage, Transcription, VlmOcrClient};
use async_trait::async_trait;

pub struct LocalVlmClient {
    model_id: String,
    // session/runtime handle (ort::Session or Candle model) — per Task 4.1 choice
}

impl LocalVlmClient {
    /// Load weights via download_models (cached after first fetch).
    pub fn from_pretrained(model_id: &str) -> crate::error::Result<Self> {
        // resolve weights through the shared model cache, build the session
        todo!("wire to download_models + runtime")
    }
}

#[async_trait]
impl VlmOcrClient for LocalVlmClient {
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription> {
        // preprocess image -> model inputs; run inference; decode text + confidence
        todo!("inference")
    }
    fn model_id(&self) -> &str { &self.model_id }
}
```

- [ ] **Step 3: Unit test with a fixture page image (`#[ignore]` — downloads weights)**

Mirror the GLiNER2 test convention (`#[ignore = "downloads … weights at runtime"]`). Assert non-empty text on a known scanned-legal fixture.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features vlm-ocr
```

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/vlm/local.rs
git commit -m "feat(vlm): local VLM-OCR runtime via download_models (Spec B phase 1)"
```

---

### Task 5: `RoutingVlmClient` (local-only in phase 1)

`crates/anno-rag-tabular/src/llm/vlm/routing.rs` — mirror [`RoutingLlmClient`](../../../crates/anno-rag-tabular/src/llm/routing.rs). Phase 1 keeps the hosted slot `None`, so there is no network call and no image gate is needed.

- [ ] **Step 1: Implement**

```rust
use super::{PageImage, Transcription, VlmOcrClient};
use async_trait::async_trait;

pub struct RoutingVlmClient {
    local: Box<dyn VlmOcrClient>,
    /// Phase 1: always None. Phase 3 attaches a hosted client ONLY behind the
    /// image-PII gate (Spec B §4.3) + an explicit opt-in flag.
    hosted: Option<Box<dyn VlmOcrClient>>,
}

impl RoutingVlmClient {
    pub fn local_only(local: Box<dyn VlmOcrClient>) -> Self {
        Self { local, hosted: None }
    }
}

#[async_trait]
impl VlmOcrClient for RoutingVlmClient {
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription> {
        // Phase 1: local only. Hosted fallback (Task deferred) would require
        // image-PII gating before any remote send — NOT wired here.
        self.local.transcribe(image, hint).await
    }
    fn model_id(&self) -> &str { "routing-local-vlm" }
}
```

- [ ] **Step 2: Test — local-only routing never constructs a hosted client**

Assert `model_id() == "routing-local-vlm"` and that `local_only` compiles without any network/keyring dependency (parallels `routing_factory_local_only_when_remote_denied`).

- [ ] **Step 3: Commit**

```powershell
git add crates/anno-rag-tabular/src/llm/vlm/routing.rs
git commit -m "feat(vlm): RoutingVlmClient (local-only, hosted path deferred) (Spec B phase 1)"
```

---

### Task 6: Ingest wiring + Tesseract confidence fallback (phase 2)

Wire VLM into the OCR branch of [`ingest.rs`](../../../crates/anno-rag/src/ingest.rs). VLM-OCR is **additive**: Tesseract remains the fallback when the feature is off or VLM confidence is low.

- [ ] **Step 1: Enable image extraction for the OCR path only**

[`ingest.rs:262`](../../../crates/anno-rag/src/ingest.rs) currently sets `extract_images: false`. Flip it to `true` **only** in the `embedded_ocr_extract` config (the `ScannedPdf`/`MixedPdf` path), leaving the native digital-text config ([ingest.rs:245](../../../crates/anno-rag/src/ingest.rs) `native_extraction_config`) untouched so digital docs still skip images.

- [ ] **Step 2: Route OCR-classified page images through `RoutingVlmClient`**

In the `OcrMode::AutoEmbedded` arm ([ingest.rs:159](../../../crates/anno-rag/src/ingest.rs)), behind `#[cfg(feature = "vlm-ocr")]`: for each page classified by `page_needs_ocr`, build a `PageImage` from the kreuzberg `ExtractedImage` and call `transcribe`. Emit the result as the page's chunks through the existing `ElementBased` consumers.

- [ ] **Step 3: Confidence fallback to Tesseract**

When `Transcription.confidence` is below a configurable threshold (add `vlm_confidence_threshold` to `AnnoRagConfig`, default ~0.6), discard the VLM text and keep the Tesseract result for that page. Log the decision with `tracing` (page index + chosen backend), per the Rust rules.

- [ ] **Step 4: Integration test on a mixed scanned/digital fixture**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -Features vlm-ocr
```

Expected: scanned page yields VLM text; digital page unchanged; low-confidence page falls back to Tesseract (no panic, no empty chunks).

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/ingest.rs crates/anno-rag/src/config.rs
git commit -m "feat(vlm): route OCR pages through local VLM with Tesseract fallback (Spec B phase 2)"
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
gh pr create --title "feat: VLM-OCR (local-first) + kreuzberg ELv2 containment" --body "Implements Spec A (deny.toml pointer) + Spec B phases 1–2.

## Changes
- deny.toml: ELv2 comment points at containment spec (Spec A)
- VlmOcrClient trait + PageImage/Transcription (anno-rag-tabular/src/llm/vlm)
- Local VLM runtime via download_models; RoutingVlmClient (local-only)
- vlm-ocr Cargo feature (off by default) in anno-rag-tabular + anno-rag
- ingest.rs: OCR-classified pages route through local VLM, Tesseract fallback

## Out of scope (deferred)
- Hosted VLM-OCR + image-PII gate (Spec B §4.3, phase 3)

## Test plan
- [ ] cargo deny check licenses — kreuzberg Elastic-2.0 still allowed
- [ ] check passes with vlm-ocr on AND off
- [ ] local VLM transcribes a scanned-legal fixture (non-empty)
- [ ] digital-text doc unchanged (no image extraction)
- [ ] low-confidence page falls back to Tesseract"
```

---

## Self-Review

- ✅ New `VlmOcrClient` trait is a sibling to `LlmClient`, not an overload — image→text vs text→JSON are distinct contracts (Spec B §4.1)
- ✅ Local-default, `vlm-ocr` feature **off by default** — mirrors `gliner2`; CI never downloads VLM weights
- ✅ Phase 1 is safe-by-construction (no network → no image leaves device → no image gate needed)
- ✅ Hosted VLM + image-PII gate explicitly **deferred** — keeps gateway `reject_images: true` (Spec B §4.3); not built until the gate is designed
- ✅ VLM-OCR scoped to the OCR branch (`ScannedPdf`/`MixedPdf`); digital docs untouched (`extract_images` flip is OCR-config-only)
- ✅ Additive: Tesseract stays as confidence fallback — VLM is never a hard dependency
- ✅ Spec A's deny.toml acceptance item folded in as Task 1 (currently still the vague "REVIEW BEFORE" note)
- ⚠️ **Model choice open** — pick a permissive OCR VLM in Task 4 and verify its license (Spec A discipline) before wiring. Record id + license here once chosen.
- ⚠️ **Runtime dep list** in the `vlm-ocr` feature (Task 3) is a placeholder (`ort` + `image`); adjust to the actual ONNX/Candle path chosen in Task 4.
- ⚠️ `Transcription.confidence` source — if the chosen model has no native confidence, derive a heuristic (e.g. per-token logprob mean) before relying on the Task 6 threshold.
