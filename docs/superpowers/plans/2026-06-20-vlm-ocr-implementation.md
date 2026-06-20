# VLM-OCR Implementation — Full MIT Codebase

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate every non-MIT dependency and add VLM-OCR as a within-trust-boundary capability:

- **Task 1 (was Spec A):** Downgrade kreuzberg `=4.9.7` (ELv2) → `=4.7.4` (MIT). This **removes** the only ELv2 dep from the codebase — no containment dance, no deny.toml exception, full permissive stack.
- **Tasks 2–7 (Spec B):** Add VLM-OCR (`lightonai/LightOnOCR-2-1B`, Apache-2.0) behind a new `VlmOcrClient` trait. Backends use **`liter-llm`** (MIT, Rust-native, kreuzberg's own universal LLM client) instead of hand-rolled reqwest — both vLLM (on-prem GPU) and llama.cpp server (desktop) are OpenAI-compat endpoints configured via `liter_llm::ClientConfig::base_url`. Third-party hosted is **dropped** (Spec B §4.3).

**Why liter-llm instead of raw reqwest:**
- MIT licensed, Rust core, no Python, no supply chain risk (explicitly built as a response to the 2026 litellm backdoor)
- `ChatCompletionRequest + ContentPart::ImageUrl` handles the vision call — exactly the pattern kreuzberg 4.8.0 used internally before the license change
- `ClientConfig.base_url` routes to any OpenAI-compat endpoint: co-located vLLM, `llama-server` for GGUF, or any future backend — no new backends to own
- Secrets in `secrecy::SecretString` (zeroed on drop, never serialized)
- Retries, OpenTelemetry, rate limiting all built in

**Why kreuzberg 4.7.4:**
- Last MIT release. VLM-OCR was introduced at 4.8.0 — the same commit that flipped the license to ELv2. There is no "MIT kreuzberg with VLM-OCR".
- All features anno uses exist in 4.7.4: `pdf`, `bundled-pdfium`, `office`, `html`, `email`, `excel`, `xml`, `archives`, `tokio-runtime`, `chunking`, `ocr`, `paddle-ocr`.
- Public API is identical: `kreuzberg::extract_file`, `kreuzberg::core::config::ExtractionConfig` — both call sites (`anno-rag/src/ingest.rs`, `anno-privacy-gateway/src/document_extract.rs`) are unaffected.

**Prerequisites:**
- On `main` (specs committed in `6c56d7b5` / `d387d46a`)
- Local Rust loop per CLAUDE.md: `CARGO_TARGET_DIR=E:\cargo-target`, use `scripts/test-local.ps1` / `scripts/dev-fast.ps1`. Never `cargo build --workspace`.
- Branch: `feat/vlm-ocr`

---

## Deployment tiers (drives backend design — Spec B §3)

| Tier | VLM backend | liter-llm target | Image leaves box? | ELv2 |
|------|-------------|-----------------|-------------------|------|
| Desktop / CPU | `LocalVlmClient` → `llama-server` (LightOnOCR GGUF) | `base_url = http://127.0.0.1:8080` | No | Not triggered |
| **On-prem GPU** (primary) | `VllmServerClient` → co-located vLLM | `base_url = http://127.0.0.1:8000` | **No** — stays on customer's box | **Not triggered** |
| Third-party SaaS | **NOT BUILT** — dropped | — | Yes | Triggered |

Both built backends are OpenAI-compat HTTP servers running on the customer's hardware. liter-llm treats them identically — only `base_url` and `model_id` differ.

---

## File Map

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | `kreuzberg = "=4.7.4"` (was `=4.9.7`); add `liter-llm` workspace dep |
| `deny.toml` | Remove the `kreuzberg` Elastic-2.0 allow entry entirely |
| `crates/anno-rag-tabular/src/llm/vlm/mod.rs` | NEW — `VlmOcrClient` trait, `PageImage`, `Transcription` |
| `crates/anno-rag-tabular/src/llm/vlm/vllm_server.rs` | NEW — `VllmServerClient` wrapping `liter_llm::DefaultClient` |
| `crates/anno-rag-tabular/src/llm/vlm/local_gguf.rs` | NEW — `LocalVlmClient` wrapping `liter_llm::DefaultClient` (points to `llama-server`) |
| `crates/anno-rag-tabular/src/llm/vlm/routing.rs` | NEW — `RoutingVlmClient` |
| `crates/anno-rag-tabular/src/llm/mod.rs` | Add `pub mod vlm;` |
| `crates/anno-rag-tabular/Cargo.toml` | Add `vlm-ocr = ["dep:liter-llm"]` feature |
| `crates/anno-rag/src/ingest.rs` | OCR branch: route `ScannedPdf`/`MixedPdf` pages through VLM |
| `crates/anno-rag/src/config.rs` | `vlm_backend`, `vlm_vllm_url`, `vlm_local_url`, `vlm_confidence_threshold` |
| `crates/anno-rag/Cargo.toml` | Add `vlm-ocr = ["anno-rag-tabular/vlm-ocr"]` passthrough |

---

### Task 1: Downgrade kreuzberg to 4.7.4 — eliminate ELv2

This is the entirety of what Spec A required. No comment rewriting, no containment — just remove the ELv2 dep.

- [ ] **Step 1: Bump version in workspace `Cargo.toml`**

  ```toml
  # was: kreuzberg = { version = "=4.9.7", default-features = false, features = [...] }
  kreuzberg = { version = "=4.7.4", default-features = false, features = [
      "pdf", "bundled-pdfium", "office", "html", "email", "excel",
      "xml", "archives", "tokio-runtime", "chunking"
  ] }
  ```

- [ ] **Step 2: Remove the kreuzberg ELv2 entry from `deny.toml`**

  Delete these lines entirely (kreuzberg is now MIT — no exception needed):
  ```toml
  # Elastic-2.0 via `kreuzberg` ...
  { crate = "kreuzberg", allow = ["Elastic-2.0"] },
  ```

  > ⚠️ Keep the `bzip2-1.0.6` entries below it — `sevenz-rust2` (pulled by `archives`) still uses that license regardless of kreuzberg version.

- [ ] **Step 3: Verify compile + license clean**

  ```powershell
  cargo deny check licenses 2>&1 | Select-String "kreuzberg|error" | Select-Object -First 10
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -AllAffected -Mode check
  ```

  Expected: no Elastic-2.0 anywhere; `anno-rag` and `anno-privacy-gateway` compile cleanly.

- [ ] **Step 4: Commit**

  ```powershell
  git add Cargo.toml Cargo.lock deny.toml
  git commit -m "chore(deps): downgrade kreuzberg to 4.7.4 (MIT) — remove last ELv2 dep"
  ```

---

### Task 2: FR eval gate (entry criterion — do before wiring a default)

Spec B makes the model choice conditional on real French legal pages.

- [ ] **Step 1: Assemble fixture set** — 10–20 representative French legal pages (scanned contracts, stamped/signed pages, handwritten margins, table-heavy pages). **Do NOT commit real client PII** — use synthetic or consented samples (privacy rules).

- [ ] **Step 2: Run each candidate** via a throwaway Python/vLLM harness:
  - `lightonai/LightOnOCR-2-1B`
  - `allenai/olmOCR-*`
  - `PaddlePaddle/PaddleOCR-VL-1.6`

- [ ] **Step 3: Score** per class (printed / handwritten / tables / stamps) on CER/WER and table-cell F1. Record winner + per-class scores below under "Eval result".

- [ ] **Step 4: Confirm default.** If LightOnOCR-2-1B wins or ties → keep as default. If PaddleOCR-VL wins a class decisively → note as per-class override.

> **Eval result:** _(fill in: model, date, per-class scores, decision)_

---

### Task 3: `VlmOcrClient` trait + value types

Sibling to [`LlmClient`](../../../crates/anno-rag-tabular/src/llm/mod.rs). The difference: text→JSON vs image→text, so it is a distinct contract.

- [ ] **Step 1: Create `crates/anno-rag-tabular/src/llm/vlm/mod.rs`**

```rust
//! Vision-OCR client — image→text transcription. Sibling to [`crate::llm::LlmClient`]
//! (text→JSON). Backends in [`vllm_server`] (on-prem GPU) and [`local_gguf`]
//! (desktop llama.cpp server); routing in [`routing`]. Both run inside the
//! customer's trust boundary — no third-party egress (Spec B §4.3).

use async_trait::async_trait;

pub mod local_gguf;
pub mod routing;
pub mod vllm_server;

/// Decoded page image + provenance for audit attribution.
#[derive(Debug, Clone)]
pub struct PageImage {
    /// Raw image bytes (PNG or JPEG — caller encodes from the source doc).
    pub bytes: Vec<u8>,
    /// MIME type: `"image/png"` or `"image/jpeg"`.
    pub mime: &'static str,
    /// Source document id.
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

/// Vision-OCR call. `Send + Sync` so ingest can fan pages across tokio tasks.
#[async_trait]
pub trait VlmOcrClient: Send + Sync {
    /// Transcribe text from a page image. `hint` carries layout/language
    /// guidance, e.g. "French legal contract; preserve table structure".
    async fn transcribe(&self, image: &PageImage, hint: &str)
        -> crate::error::Result<Transcription>;
    /// Stable model identifier for audit logs.
    fn model_id(&self) -> &str;
}
```

- [ ] **Step 2: Register in `llm/mod.rs`**

  ```rust
  #[cfg(feature = "vlm-ocr")]
  pub mod vlm;
  ```

- [ ] **Step 3: Stub the two backend modules** (empty `mod.rs` with `todo!` impls) so it compiles, then run check:

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
  ```

---

### Task 4: `vlm-ocr` Cargo feature + `liter-llm` dep

Mirror the `gliner2` feature pattern. Off by default so CI never downloads VLM weights.

- [ ] **Step 1: Add `liter-llm` to workspace deps** in root `Cargo.toml`:

  ```toml
  liter-llm = { version = "1.7", default-features = false, features = ["native-http"] }
  ```

- [ ] **Step 2: `anno-rag-tabular` feature** in `crates/anno-rag-tabular/Cargo.toml`:

  ```toml
  # Within-boundary vision-OCR via liter-llm (MIT, Rust-native). Off by default.
  # liter-llm routes to any OpenAI-compat endpoint: co-located vLLM (on-prem GPU)
  # or llama-server GGUF (desktop) — configured via vlm_vllm_url / vlm_local_url.
  vlm-ocr = ["dep:liter-llm"]
  ```

  And add the dep:
  ```toml
  liter-llm = { workspace = true, optional = true }
  ```

- [ ] **Step 3: `anno-rag` passthrough** in `crates/anno-rag/Cargo.toml`:

  ```toml
  # Route OCR-classified pages through a within-boundary VLM (Spec B). Off by default.
  vlm-ocr = ["anno-rag-tabular/vlm-ocr"]
  ```

- [ ] **Step 4: Verify both configs compile**

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Mode check
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-tabular -Features vlm-ocr -Mode check
  ```

- [ ] **Step 5: Commit scaffolding**

  ```powershell
  git add crates/anno-rag-tabular/src/llm/vlm/ crates/anno-rag-tabular/src/llm/mod.rs `
         crates/anno-rag-tabular/Cargo.toml crates/anno-rag/Cargo.toml Cargo.toml Cargo.lock
  git commit -m "feat(vlm): VlmOcrClient trait + vlm-ocr feature with liter-llm (Spec B)"
  ```

---

### Task 5: Backends — `VllmServerClient` and `LocalVlmClient` via liter-llm

Both backends are thin wrappers around `liter_llm::DefaultClient`. Only `base_url` and `model_id` differ. No separate HTTP code to own.

- [ ] **Step 1: `VllmServerClient`** (`vlm/vllm_server.rs`)

  Co-located vLLM (on-prem GPU) serving `lightonai/LightOnOCR-2-1B` at `http://127.0.0.1:8000` by default. URL and model come from `AnnoRagConfig.vlm_vllm_url`.

  ```rust
  use liter_llm::{
      ChatCompletionRequest, ClientBuilder, ContentPart, ImageUrl,
      Message, UserContent, UserMessage,
  };
  use liter_llm::client::config::ClientConfig;
  use liter_llm::image::encode_data_url;
  use super::{PageImage, Transcription, VlmOcrClient};
  use async_trait::async_trait;
  use secrecy::SecretString;

  pub struct VllmServerClient {
      client: liter_llm::DefaultClient,
      model: String,
  }

  impl VllmServerClient {
      pub fn new(base_url: &str, model: impl Into<String>) -> crate::error::Result<Self> {
          let config = ClientConfig::builder()
              .api_key(SecretString::from(""))   // vLLM on-prem — no key needed
              .base_url(base_url.to_string())
              .build();
          let client = ClientBuilder::new(config).build()?;
          Ok(Self { client, model: model.into() })
      }
  }

  #[async_trait]
  impl VlmOcrClient for VllmServerClient {
      async fn transcribe(&self, image: &PageImage, hint: &str)
          -> crate::error::Result<Transcription> {
          let data_url = encode_data_url(&image.bytes, Some(image.mime));
          let req = ChatCompletionRequest::builder()
              .model(&self.model)
              .messages(vec![Message::User(UserMessage {
                  content: UserContent::Parts(vec![
                      ContentPart::ImageUrl(ImageUrl { url: data_url, detail: None }),
                      ContentPart::Text(hint.to_string()),
                  ]),
                  name: None,
              })])
              .build();
          let resp = self.client.chat_completion(req).await?;
          let text = resp.choices.into_iter()
              .next()
              .and_then(|c| c.message.content)
              .unwrap_or_default();
          // liter-llm does not expose per-token logprobs from vLLM by default;
          // use a length heuristic until a logprob pass is added (see ⚠️ Self-Review).
          let confidence = if text.is_empty() { 0.0 } else { 0.8 };
          Ok(Transcription { text, confidence })
      }
      fn model_id(&self) -> &str { &self.model }
  }
  ```

- [ ] **Step 2: `LocalVlmClient`** (`vlm/local_gguf.rs`)

  Desktop path — same pattern, but points to a `llama-server` instance running `Mungert/LightOnOCR-1B-1025-GGUF` at `http://127.0.0.1:8080`. **No in-process llama.cpp binding needed.** The user runs `llama-server` (pre-built binary) once; anno talks to it over OpenAI-compat HTTP.

  ```rust
  // Identical structure to VllmServerClient — only the default URL differs.
  pub struct LocalVlmClient {
      inner: super::vllm_server::VllmServerClient,
  }

  impl LocalVlmClient {
      pub fn new(base_url: &str, model: impl Into<String>) -> crate::error::Result<Self> {
          Ok(Self { inner: super::vllm_server::VllmServerClient::new(base_url, model)? })
      }
  }

  #[async_trait::async_trait]
  impl super::VlmOcrClient for LocalVlmClient {
      async fn transcribe(&self, image: &super::PageImage, hint: &str)
          -> crate::error::Result<super::Transcription> {
          self.inner.transcribe(image, hint).await
      }
      fn model_id(&self) -> &str { self.inner.model_id() }
  }
  ```

- [ ] **Step 3: `download_models` registration**

  Register `lightonai/LightOnOCR-2-1B` (safetensors, for vLLM) and `Mungert/LightOnOCR-1B-1025-GGUF` (for llama-server) in the existing `download_models` plumbing — same pattern as bge-m3 / GLiNER2.

  > ⚠️ **License gate (Spec A discipline):** LightOnOCR-2-1B is Apache-2.0 per its model card. Re-confirm the GGUF redistribution terms (`Mungert/...`) before shipping — third-party GGUF repacks sometimes add restrictions.

- [ ] **Step 4: `#[ignore]` tests** mirroring the GLiNER2 convention:

  ```rust
  #[tokio::test]
  #[ignore = "requires co-located vLLM serving lightonai/LightOnOCR-2-1B"]
  async fn vllm_server_client_transcribes_fixture() { ... }

  #[tokio::test]
  #[ignore = "requires llama-server with LightOnOCR GGUF on :8080"]
  async fn local_vlm_client_transcribes_fixture() { ... }
  ```

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-tabular -Features vlm-ocr
  ```

- [ ] **Step 5: Commit backends**

  ```powershell
  git add crates/anno-rag-tabular/src/llm/vlm/
  git commit -m "feat(vlm): vLLM + llama-server backends via liter-llm (Spec B)"
  ```

---

### Task 6: `RoutingVlmClient` + ingest wiring + Tesseract fallback

- [ ] **Step 1: `RoutingVlmClient`** (`vlm/routing.rs`) — selects backend from `config.vlm_backend`:

  ```rust
  pub struct RoutingVlmClient {
      backend: Box<dyn super::VlmOcrClient>,
      // NOTE: no third-party slot — hosted VLM-OCR is dropped (Spec B §4.3).
  }

  impl RoutingVlmClient {
      pub fn from_config(cfg: &crate::AnnoRagConfig) -> crate::error::Result<Self> {
          let backend: Box<dyn super::VlmOcrClient> = match cfg.vlm_backend.as_deref() {
              Some("vllm") | None => Box::new(super::vllm_server::VllmServerClient::new(
                  cfg.vlm_vllm_url.as_deref().unwrap_or("http://127.0.0.1:8000"),
                  "lightonai/LightOnOCR-2-1B",
              )?),
              Some("local") => Box::new(super::local_gguf::LocalVlmClient::new(
                  cfg.vlm_local_url.as_deref().unwrap_or("http://127.0.0.1:8080"),
                  "LightOnOCR-1B-1025",
              )?),
              Some(other) => return Err(/* unsupported backend error */),
          };
          Ok(Self { backend })
      }
  }
  ```

- [ ] **Step 2: Config fields** in `crates/anno-rag/src/config.rs`:

  ```rust
  /// VLM backend: "vllm" (on-prem GPU, default) or "local" (llama-server desktop).
  pub vlm_backend: Option<String>,
  /// Base URL for the co-located vLLM server (default: http://127.0.0.1:8000).
  pub vlm_vllm_url: Option<String>,
  /// Base URL for the local llama-server (default: http://127.0.0.1:8080).
  pub vlm_local_url: Option<String>,
  /// Confidence below which VLM output is discarded in favour of Tesseract (default: 0.6).
  pub vlm_confidence_threshold: Option<f32>,
  ```

- [ ] **Step 3: Page image sourcing** — investigate how to get page bitmaps from kreuzberg 4.7.4.

  kreuzberg's `ocr` feature includes `pdfium-render` + `image`. The `embedded_ocr_extract` function calls kreuzberg internally which renders pages to images for Tesseract — but we don't get those intermediate bitmaps back via the current `extract_file` API.

  Options (in priority order):
  1. **Use `pdfium-render` directly** — kreuzberg already pulls it as a transitive dep via `kreuzberg/pdf`; render each PDF page to a `DynamicImage`, encode as PNG bytes, pass to VLM.
  2. **Two-pass extraction** — run kreuzberg OCR first (Tesseract), collect page images via an `ExtractionConfig { extract_images: true }` pass, then run VLM on those images.
  3. **Add a kreuzberg API** — upstream a `render_pages_to_images()` helper if neither above is clean.

  > ⚠️ Resolve this before coding Step 4. Option 1 is preferred — no new dep (`pdfium-render` is already available), and it avoids a double extraction pass.

- [ ] **Step 4: Wire VLM into ingest OCR branch** (`ingest.rs`) — behind `#[cfg(feature = "vlm-ocr")]`:

  In `OcrMode::AutoEmbedded`, for `ScannedPdf`/`MixedPdf` pages:
  1. Render page to `PageImage` bytes (from Step 3)
  2. Call `routing_client.transcribe(&page_image, "French legal document; preserve table structure")`
  3. If `transcription.confidence >= cfg.vlm_confidence_threshold.unwrap_or(0.6)` → use VLM text
  4. Otherwise → fall through to existing `embedded_ocr_extract` (Tesseract)
  5. Emit text through the existing `ElementBased` chunk consumers

  Digital-text documents (`DocClass::TextLayer`) remain untouched — no image extraction, no VLM pass.

- [ ] **Step 5: Integration test**

  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -Features vlm-ocr
  ```

  Expected: scanned-PDF fixture → VLM text; digital-text fixture → unchanged; low-confidence page → Tesseract (no panic, no empty chunks).

- [ ] **Step 6: Commit**

  ```powershell
  git add crates/anno-rag-tabular/src/llm/vlm/routing.rs crates/anno-rag/src/ingest.rs crates/anno-rag/src/config.rs
  git commit -m "feat(vlm): RoutingVlmClient + OCR-branch ingest wiring + Tesseract fallback (Spec B)"
  ```

---

### Task 7: PR

- [ ] **Step 1: fmt + clippy** (per repo convention — commit fmt separately if it changes files)

  ```powershell
  cargo fmt --all
  cargo clippy -p anno-rag-tabular -p anno-rag --features vlm-ocr --jobs 2 2>&1 | Select-String "warning|error" | Select-Object -First 20
  ```

- [ ] **Step 2: Open PR**

  ```powershell
  git push origin feat/vlm-ocr
  gh pr create --title "feat: full MIT codebase — kreuzberg 4.7.4 + VLM-OCR via liter-llm" --body "..."
  ```

  PR body should cover:
  - kreuzberg 4.9.7 (ELv2) → 4.7.4 (MIT): zero ELv2 in the entire dependency graph
  - `liter-llm` (MIT, Rust-native): replaces hand-rolled reqwest; vLLM + llama-server as OpenAI-compat endpoints
  - `VlmOcrClient` trait + two backends (`VllmServerClient`, `LocalVlmClient`)
  - `vlm-ocr` Cargo feature (off by default)
  - OCR-branch ingest wiring + Tesseract confidence fallback
  - Third-party hosted VLM-OCR: dropped, not deferred (Spec B §4.3)

---

## Self-Review

- ✅ **Full MIT stack**: kreuzberg 4.7.4 (MIT) + liter-llm (MIT) + LightOnOCR-2-1B (Apache-2.0). Zero ELv2, zero exceptions in deny.toml.
- ✅ **No hand-rolled HTTP**: liter-llm owns the OpenAI-compat transport, retries, secrets handling. `VllmServerClient` and `LocalVlmClient` are ~30 lines each.
- ✅ **No llama.cpp Rust binding**: desktop GGUF runs as a `llama-server` process; anno talks HTTP. Same trust boundary, zero in-process binding complexity.
- ✅ **`vlm-ocr` feature off by default** — CI never downloads VLM weights; GPU path aligns with existing `gpu-cuda` profile.
- ✅ **VLM scoped to OCR branch only** (`ScannedPdf`/`MixedPdf`); digital-text docs untouched; Tesseract is confidence fallback.
- ✅ **FR eval is Task 2 — an entry gate**, before wiring any default. Synthetic fixtures only (privacy rules).
- ✅ **Third-party tier dropped, not deferred** — `reject_images: true` in gateway stays; no image-PII gate needed for within-boundary tiers.
- ⚠️ **Page image sourcing (Task 6 Step 3)** — must confirm pdfium-render approach before coding ingest wiring. This is the only open architecture question.
- ⚠️ **Confidence heuristic** — liter-llm does not expose per-token logprobs from vLLM by default. The length heuristic in Task 5 is a placeholder; add a logprob pass or re-OCR agreement check before the confidence threshold is relied upon in production.
- ⚠️ **GGUF redistribution** — verify `Mungert/LightOnOCR-1B-1025-GGUF` terms before shipping. Third-party repacks sometimes add restrictions beyond the base model license.
- ⚠️ **Eval fixtures must not contain real client PII** — use synthetic/consented French legal pages (privacy rules).
