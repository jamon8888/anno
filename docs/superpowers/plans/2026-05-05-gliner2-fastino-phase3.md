# gliner2_fastino — Phase 3 (multi-session pipeline) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the multi-session ONNX pipeline that matches fastino-ai's actual GLiNER2 export structure, replacing Phase 1's single-graph load path. Closes the gap between Phase 1's structurally-complete-but-unrunnable backend and end-to-end inference against the canonical `SemplificaAI/gliner2-multi-v1-onnx` pin.

**Architecture:** Replace the single `Session` field on `GLiNER2Fastino` with a typed 8-session container (`encoder`, `token_gather`, `span_rep`, `schema_gather`, `count_pred_argmax`, `count_lstm_fixed`, `scorer`, `classifier`). Add a `pipeline` module that orchestrates the chain. Port from `SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs::extract_standard` (Apache-2.0, ~250 LOC), with mechanical transformations to match anno's `Entity` shape and `Result` type. IOBinding mode is split as a Phase 3.5 follow-up (this plan ships standard mode only). GPU execution providers (CUDA/CoreML) are wired in at the end.

**Tech Stack:** Rust 2021, `ort` rc.12, `tokenizers`, `hf-hub`, `ndarray`, `half` (for fp16 tensors).

**Spec:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5 Phase 3.
**Roadmap:** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md` (Track D).
**Phase 1 architectural finding:** commit `44b158aa` on `feat/gliner2-fastino` — documents the 5-graph layout discovered against `SemplificaAI/gliner2-multi-v1-onnx` (51d4a15c). The actual count is **8 graphs** for the v2 IOBinding-ready export (an 8-session superset).

---

## Pre-flight

- [ ] **Phase 1 PR is open and merged (or near-merge).** Don't start Phase 3 on top of an unmerged Phase 1 — rebase risk grows linearly with Phase 3's commit count. If Phase 1 is still on `feat/gliner2-fastino` locally, push and open the PR first per Track A's M5.
- [ ] **Read `SemplificaAI/gliner2-rs/rust_component/src/lib_v2.rs`.** Specifically:
  - Lines 79–140: 8-session struct + `from_pretrained` download list.
  - Lines 660–905: `extract_standard` — the orchestration we're porting. Apache-2.0; carry attribution into ports.
  - Lines 285–660 (IOBinding mode) — out of scope for this plan; bookmarked for Phase 3.5.
  ```bash
  curl -fsSL https://raw.githubusercontent.com/SemplificaAI/gliner2-rs/main/rust_component/src/lib_v2.rs \
      -o /tmp/gliner2-rs-lib_v2.rs
  wc -l /tmp/gliner2-rs-lib_v2.rs   # expect ~905 lines
  ```
- [ ] **Confirm WSL Ubuntu-C is set up** with cargo + the `--cap-lints allow` workaround (per Phase 1 finalization). All builds and tests in this plan assume that environment.
- [ ] **Cache `SemplificaAI/gliner2-multi-v1-onnx`** in `~/.cache/huggingface` (the integration test in M12 requires it). Per the Phase 1 introspection, the snapshot is ~6 GB across `fp32/`, `fp16/`, `fp32_v2/`, `fp16_v2/` subdirs.
  ```bash
  ~/.venv/anno-tools/bin/python -c "from huggingface_hub import snapshot_download; print(snapshot_download('SemplificaAI/gliner2-multi-v1-onnx'))"
  ```
- [ ] **Create a worktree off Phase 1's tip:**
  ```bash
  git worktree add ../anno-gliner2-phase3 -b feat/gliner2-fastino-phase3 feat/gliner2-fastino
  ```

---

## File structure (locked)

| File | Action | Purpose |
|---|---|---|
| `crates/anno/src/backends/gliner2_fastino/mod.rs` | modify | Struct rewrite (single Session → 8-session container); rewire `from_local`/`from_pretrained`; remove the multi-graph-rejection error from Phase 1; rewire `extract_ner`/`classify` to call the pipeline |
| `crates/anno/src/backends/gliner2_fastino/sessions.rs` | create | Typed multi-session container + IOBinding-aware fp16/fp32 selection logic |
| `crates/anno/src/backends/gliner2_fastino/pipeline.rs` | create | Standard-mode 8-session inference (port of `extract_standard`); per-task entity / classification / relation decoding |
| `crates/anno/src/backends/gliner2_fastino/nms.rs` | create | Greedy NMS for span-level entities (extracted from upstream's inline NMS in `extract_standard`) |
| `crates/anno/src/backends/gliner2_fastino/session.rs` | delete | Replaced by `sessions.rs`; the single-Session wrapper is no longer the right abstraction |
| `crates/anno/src/backends/gliner2_fastino/decoder.rs` | modify or delete | Phase 1's `decode_spans` is unused once `pipeline::extract_standard` lands. Either remove or repurpose for unit tests of NMS edge cases |
| `crates/anno/src/backends/gliner2_fastino/errors.rs` | modify | Add session-load variants and pipeline error variants |
| `crates/anno/Cargo.toml` | modify | Add `half = { workspace = true }` if not already a dep (needed for fp16 classifier inputs); add `gliner2-fastino-cuda` and `gliner2-fastino-coreml` features |
| `crates/anno/tests/gliner2_fastino_integration.rs` | modify | Un-stub the placeholder integration tests; they now run end-to-end |
| `docs/dev-notes/gliner2-fastino-export.md` | modify | Update with multi-graph requirements, GPU-EP table, performance notes |
| `docs/BACKENDS.md` | modify | Update WIP status note (still WIP until Phase 3.5 IOBinding lands; but functional now) |
| `crates/anno/src/backends/catalog.rs` | modify | Update `gpu_support: true` (was `false` in Phase 1 since Phase 3 wires it) |
| `rust-toolchain.toml` | possibly create | Pin a toolchain that doesn't ICE on `annotate_snippets::renderer::styled_buffer::StyledBuffer::replace` (rustc 1.94.0 panic). Investigation in M15 |

---

## Milestone P3.M1 — Source pull and read (~half day)

Goal: have the upstream port source on disk, understand the orchestration before writing tasks.

### Task M1.1: Pull `lib_v2.rs` and verify line count

- [ ] **Step 1: Download.**

  ```bash
  curl -fsSL https://raw.githubusercontent.com/SemplificaAI/gliner2-rs/main/rust_component/src/lib_v2.rs \
      -o /tmp/gliner2-rs-lib_v2.rs
  wc -l /tmp/gliner2-rs-lib_v2.rs
  ```

  Expected: ~905 lines.

- [ ] **Step 2: Confirm key symbols exist** (so subsequent tasks can reference exact lines).

  ```bash
  grep -n "fn extract_standard\|fn extract_iobinding\|impl Gliner2EngineV2\|MAX_COUNT" /tmp/gliner2-rs-lib_v2.rs
  ```

  Expected output similar to:
  ```
  79:impl Gliner2EngineV2 {
  81:    pub fn from_pretrained(
  285:    fn extract_iobinding(
  660:    pub fn extract_standard(
  903:const MAX_COUNT: usize = 20;
  ```

  If line numbers differ from what this plan references (because upstream commits forward), use the symbols, not the lines.

### Task M1.2: Manual reading

- [ ] **Step 1: Read `extract_standard` end-to-end** (lines ~660–897). Don't skim — every detail matters:
  - Encoder output name fallback chain (`hidden_states`, `last_hidden_state`, `output`)
  - `token_gather` input/output shapes
  - `span_idx` construction loop (note the `if end >= num_words { [0,0] } else { [start, end] }` zero-padding)
  - `schema_gather` returns TWO tensors (`pc_emb` then `field_embs`) via `values().next()` twice
  - `count_pred_argmax` returns i64
  - **Classifier path** vs **scorer path** branch on `task_map.task_type == "classifications"`
  - Classifier expects `padded` shape `[1, num_labels, max_width, hidden_size]` with **fp16** values
  - Scorer output `[MAX_COUNT=20, num_words, max_width, num_labels]` is **already sigmoided**
  - NMS: greedy by score desc; `flat_ner` flag controls cross-label overlap
  - `text[char_start..char_end].trim()` extracts the surface form

- [ ] **Step 2: Note the v1-vs-v2 difference.** Upstream comments (lines 11–18) explain v1 has 5 sessions with Gather/ArgMax/Einsum done in Rust; v2 has 8 sessions with those ops fused in ONNX. We're porting **v2** because the SemplificaAI pin includes the v2 graphs (`token_gather_fp16.onnx`, `schema_gather_fp16.onnx`, `count_pred_argmax_fp16.onnx`, `count_lstm_fixed_fp16.onnx`). The v1 pipeline doesn't exist in our target export.

### Task M1.3: Commit a reading note

**Files:** `crates/anno/src/backends/gliner2_fastino/PORT_NOTES.md` (new)

- [ ] **Step 1: Write port notes.** Capture the architectural decisions for future archaeologists:

  ```markdown
  # Phase 3 port notes (2026-05-05)

  Source: github.com/SemplificaAI/gliner2-rs (Apache-2.0).
  Specifically: `rust_component/src/lib_v2.rs` — the v2 IOBinding-ready engine.

  ## Why v2 not v1

  v1 has 5 sessions (encoder, span_rep, count_lstm, count_pred, classifier)
  with token-gathering, schema-gathering, ArgMax over count logits, and
  Einsum-style scoring done in Rust. v2 fuses those ops into 3 additional
  ONNX graphs (token_gather, schema_gather, count_pred_argmax,
  count_lstm_fixed) for IOBinding-friendly zero-copy chaining. The
  SemplificaAI pin (`SemplificaAI/gliner2-multi-v1-onnx`, commit
  51d4a15c) ships the v2 graphs in `fp32/` and `fp32_v2/` subdirs. We
  port v2.

  ## Standard mode vs IOBinding mode

  Phase 3 ships standard mode (extract_standard at lib_v2.rs:660-897).
  The pipeline runs the 8 sessions sequentially with `Tensor::from_array`
  + `try_extract_tensor` round trips between Rust ndarray and ort tensors.

  IOBinding mode (extract_iobinding at lib_v2.rs:285-660) keeps tensors
  in a single ort allocator across session boundaries (typically GPU
  device memory). 2-3× speedup, no functional difference. Phase 3.5
  follow-up — separate plan.

  ## Mechanical transformations applied during port

  | Upstream | This crate |
  |---|---|
  | `anyhow::Result<_>` | `Result<_, super::errors::Error>` (or `crate::Result`) |
  | `ExtractedEntity { score, label, text, start_tok, end_tok }` | `crate::Entity` (char offsets, not token offsets) |
  | `ExtractedClassification { task_name, label, score }` | tuple `(String, f32)` for the internal `classify` API |
  | `ExtractedRelation` | not ported in Phase 3 (relations are a separate Track / Phase 2.5) |
  | `Gliner2Config { max_width, ... }` | hardcoded `max_width: 8` per spec; max_count 20 |
  | `tokenizer.encode(.., false)` panics | propagate as `Error::Tokenizer` |
  | `RwLock<ExecutionMode>` | not needed (we ship standard mode only) |
  ```

- [ ] **Step 2: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/PORT_NOTES.md
  git commit -m "docs(gliner2_fastino): phase 3 port notes from SemplificaAI/gliner2-rs lib_v2.rs"
  ```

---

## Milestone P3.M2 — Multi-session container (~1 day)

Goal: replace `session::Session` (single) with `sessions::Sessions` (8 named sessions). Phase 1's `Session::with_session` closure pattern stays — applied per session.

### Task M2.1: Create `sessions.rs`

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino/sessions.rs`

- [ ] **Step 1: Module skeleton.**

  ```rust
  //! Multi-session ONNX container for `gliner2_fastino`. Phase 3.
  //!
  //! Adapted from SemplificaAI/gliner2-rs (Apache-2.0):
  //! https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/lib_v2.rs
  //! Original: Copyright 2026 Dario Finardi, Semplifica s.r.l.

  use crate::backends::gliner2_fastino::errors::Error;
  use crate::backends::hf_loader;
  use std::path::Path;
  use std::sync::{Arc, Mutex};

  /// Eight ONNX sessions making up the GLiNER2 v2 inference pipeline.
  ///
  /// Each `Session` is wrapped in `Arc<Mutex<>>` so the engine can hand
  /// out `&self` while the closure-style `with_session` API mutates the
  /// underlying ort `Session::run`. This mirrors Phase 1's single-Session
  /// pattern, applied per role.
  pub struct Sessions {
      pub encoder:           SessionSlot,
      pub token_gather:      SessionSlot,
      pub span_rep:          SessionSlot,
      pub schema_gather:     SessionSlot,
      pub count_pred_argmax: SessionSlot,
      pub count_lstm_fixed:  SessionSlot,
      pub scorer:            SessionSlot,
      pub classifier:        SessionSlot,
  }

  /// Single session wrapped in Arc<Mutex<>> with a `with_session` closure
  /// API. Identical to Phase 1's `session::Session` but extracted as a
  /// reusable type.
  #[derive(Debug)]
  pub struct SessionSlot {
      inner: Arc<Mutex<ort::session::Session>>,
  }

  impl SessionSlot {
      pub fn from_path(model_path: &Path) -> Result<Self, Error> {
          let cfg = hf_loader::OnnxSessionConfig::default();
          let session = hf_loader::create_onnx_session(model_path, cfg)
              .map_err(|e| Error::Tokenizer(format!("session {}: {e}", model_path.display())))?;
          Ok(Self {
              inner: Arc::new(Mutex::new(session)),
          })
      }

      pub fn with_session<F, R>(&self, f: F) -> R
      where
          F: FnOnce(&mut ort::session::Session) -> R,
      {
          let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
          f(&mut guard)
      }
  }

  impl std::fmt::Debug for Sessions {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          f.debug_struct("Sessions").field("count", &8usize).finish()
      }
  }
  ```

- [ ] **Step 2: Add `from_dir` to `Sessions`.**

  Append:

  ```rust
  impl Sessions {
      /// Load all 8 sessions from a directory. Tries fp32 names first
      /// (matches `fp32/encoder_fp32.onnx` layout), then fp16. Returns the
      /// first successful resolution path.
      ///
      /// Phase 3 standard mode does NOT use the `_iobinding.onnx` variants
      /// — those are reserved for Phase 3.5 IOBinding mode.
      pub fn from_dir(model_dir: &Path) -> Result<Self, Error> {
          // The 8-session v2 layout lives in `_v2` subdirs only — `fp32/`
          // and `fp16/` are the legacy v1 layout (5 graphs, missing
          // token_gather/schema_gather/count_pred_argmax/count_lstm_fixed).
          // The `all_present` guard below filters incompatible layouts.
          for (subdir, suffix) in [
              ("fp32_v2", "_fp32.onnx"),
              ("fp16_v2", "_fp16.onnx"),
              ("fp32",    "_fp32.onnx"),  // v1 fallback — likely won't match
              ("fp16",    "_fp16.onnx"),  // v1 fallback
          ] {
              let try_dir = model_dir.join(subdir);
              if !try_dir.is_dir() {
                  continue;
              }
              let candidate = |name: &str| try_dir.join(format!("{name}{suffix}"));
              let all_present = [
                  "encoder", "token_gather", "span_rep", "schema_gather",
                  "count_pred_argmax", "count_lstm_fixed", "scorer", "classifier",
              ].iter().all(|n| candidate(n).exists());
              if !all_present {
                  continue;
              }
              return Ok(Self {
                  encoder:           SessionSlot::from_path(&candidate("encoder"))?,
                  token_gather:      SessionSlot::from_path(&candidate("token_gather"))?,
                  span_rep:          SessionSlot::from_path(&candidate("span_rep"))?,
                  schema_gather:     SessionSlot::from_path(&candidate("schema_gather"))?,
                  count_pred_argmax: SessionSlot::from_path(&candidate("count_pred_argmax"))?,
                  count_lstm_fixed:  SessionSlot::from_path(&candidate("count_lstm_fixed"))?,
                  scorer:            SessionSlot::from_path(&candidate("scorer"))?,
                  classifier:        SessionSlot::from_path(&candidate("classifier"))?,
              });
          }
          Err(Error::Tokenizer(format!(
              "no complete v2 session set found under {} (looked in fp32/, fp32_v2/, fp16/, fp16_v2/)",
              model_dir.display()
          )))
      }
  }
  ```

- [ ] **Step 3: Add a unit test that doesn't require a real model.**

  Append:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use tempfile::tempdir;

      #[test]
      fn from_dir_fails_clearly_on_empty_dir() {
          let dir = tempdir().unwrap();
          let err = Sessions::from_dir(dir.path()).unwrap_err();
          let msg = err.to_string();
          assert!(msg.contains("no complete v2 session set"), "got: {msg}");
          assert!(msg.contains("fp32") || msg.contains("fp16"), "got: {msg}");
      }

      #[test]
      fn from_dir_fails_clearly_on_partial_layout() {
          let dir = tempdir().unwrap();
          std::fs::create_dir_all(dir.path().join("fp32")).unwrap();
          // Only encoder present — should not be a "complete set".
          std::fs::write(dir.path().join("fp32/encoder_fp32.onnx"), b"").unwrap();
          let err = Sessions::from_dir(dir.path()).unwrap_err();
          assert!(err.to_string().contains("no complete v2 session set"));
      }
  }
  ```

- [ ] **Step 4: Register the module.**

  In `crates/anno/src/backends/gliner2_fastino/mod.rs`, replace `pub(crate) mod session;` with:

  ```rust
  pub(crate) mod sessions;
  ```

  And update the import (the struct field) — that comes in M5.

- [ ] **Step 5: Compile-check.**

  ```bash
  bash /mnt/c/Users/NMarchitecte/anno-gliner2-phase3/wsl-c-build-test.sh
  ```

  (Reuses the script pattern from Phase 1.) Expected: `cargo check --features gliner2-fastino` and `--tests` both pass. The unit tests in this file pass under `cargo test`.

- [ ] **Step 6: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/sessions.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): multi-session container (Sessions struct, 8 slots, fp32/fp16 dispatch)"
  ```

### Task M2.2: Delete `session.rs`

**Files:**
- Delete: `crates/anno/src/backends/gliner2_fastino/session.rs`
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Confirm no other code references `session::Session`.**

  ```bash
  grep -rn "session::Session\|use.*session;" crates/anno/src/backends/gliner2_fastino/
  ```

  Expected: only `mod.rs` references it. If the `extract_ner` body still refers to `self.session`, that's expected — it'll be rewritten in M5.

- [ ] **Step 2: Delete the file.**

  ```bash
  git rm crates/anno/src/backends/gliner2_fastino/session.rs
  ```

- [ ] **Step 3: Update `mod.rs` to compile against the new struct.** Replace the `session: session::Session` field with a placeholder that M3 will fully wire:

  In `mod.rs`, change the struct field:
  ```rust
  pub(crate) sessions: sessions::Sessions,
  ```
  (was: `pub(crate) session: session::Session`)

  And in `from_local`, replace the line that loads the session with a temporarily-disabled stub that returns the error from `Sessions::from_dir` directly:
  ```rust
  let sessions = sessions::Sessions::from_dir(model_dir)?;
  ```
  (replaces the `let onnx_candidates = [...]; let model_path = ...; let session = session::Session::from_path(&model_path)?;` block — keep the LoRA-rejection guard at the top of `from_local` intact).

  Update the struct construction at the bottom of `from_local` accordingly:
  ```rust
  Ok(Self {
      tokenizer, special, transformer, config,
      sessions,
      model_id: ...,
  })
  ```

- [ ] **Step 4: Update `extract_ner` to a temporary stub** (since the old single-session call won't compile and the real pipeline lands in M5). For now:

  ```rust
  pub(crate) fn extract_ner(
      &self,
      text: &str,
      types: &[&str],
      threshold: f32,
  ) -> crate::Result<Vec<crate::Entity>> {
      if types.is_empty() {
          return Ok(vec![]);
      }
      let _ = (text, types, threshold, &self.sessions);
      Err(crate::Error::Backend(
          "gliner2_fastino: extract_ner is being rewritten in Phase 3 \
           (multi-session pipeline) — not yet wired".into(),
      ))
  }
  ```

  (This unblocks compilation of M2-M4 without losing the `extract_ner` API. M5-M11 fills the body.)

- [ ] **Step 5: Compile + test.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  cargo test  -p anno --features gliner2-fastino backends::gliner2_fastino::sessions
  ```

  All sessions::tests pass. `extract_ner` doesn't run (no caller exercises it in this milestone — the trait impls compile but tests assert error wording, which we'll re-update in M11).

- [ ] **Step 6: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git rm  crates/anno/src/backends/gliner2_fastino/session.rs
  git commit -m "refactor(gliner2_fastino): replace single Session with Sessions container; extract_ner stubbed pending M5"
  ```

---

## Milestone P3.M3 — Discover SemplificaAI multi-graph layout in `from_local` (~half day)

Goal: `Sessions::from_dir` already does the right thing, but `from_local` needs to remove the Phase 1 "is_multi_graph" rejection (since Phase 3 IS the multi-graph implementation).

### Task M3.1: Remove the multi-graph rejection from `from_local`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Locate the rejection block.** It's the `is_multi_graph` boolean and the `if is_multi_graph { ... }` arm of the `ok_or_else` closure introduced by commit `44b158aa`. The whole block became dead-code after M2.2's refactor (since `Sessions::from_dir` is now what handles multi-graph). Remove it.

- [ ] **Step 2: Verify `from_local` is now: load tokenizer → load special → load transformer → load config → `Sessions::from_dir` → return Self.** The body should be ~30 lines.

- [ ] **Step 3: Add an integration-style unit test** that points at an empty directory and confirms the error message no longer suggests "Phase 3" (since we ARE Phase 3 now):

  In `mod.rs::from_local_tests`:

  ```rust
  #[test]
  fn from_local_empty_dir_returns_session_set_error() {
      let dir = tempdir().unwrap();
      // Need at least tokenizer.json to bypass the early return.
      // Stub one out using the project's own fixture.
      let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
          .join("../../testdata/gliner2_fastino/stub_tokenizer.json");
      std::fs::copy(&fixture, dir.path().join("tokenizer.json")).unwrap();
      // And a config.json with hidden_size.
      std::fs::write(
          dir.path().join("config.json"),
          r#"{"hidden_size": 768, "counting_layer": "count_lstm_v2"}"#,
      ).unwrap();

      let err = GLiNER2Fastino::from_local(dir.path()).unwrap_err();
      let msg = err.to_string();
      assert!(
          msg.contains("no complete v2 session set"),
          "Phase 3 should report missing sessions, not 'Phase 3 needed'. Got: {msg}"
      );
  }
  ```

- [ ] **Step 4: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino from_local_empty_dir_returns_session_set_error
  ```

  Expected: PASS.

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): drop Phase 1 multi-graph rejection (Phase 3 implements it)"
  ```

---

## Milestone P3.M4 — `from_pretrained` downloads all 8 ONNX files (~half day)

### Task M4.1: Update `from_pretrained` for multi-graph downloads

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Determine the file list from the SemplificaAI repo structure.** Per the introspection in Phase 1 (commit `44b158aa`'s ARCHITECTURAL FINDING), the `fp32/` subdir contains 5 files (encoder, span_rep, classifier, count_pred, count_lstm) and `fp32_v2/` adds the 3 v2-fused ones (token_gather, schema_gather, count_pred_argmax, count_lstm_fixed, scorer). Wait — re-check: actually the v2 graphs are in `fp32_v2/` only. Some bases live in both subdirs (encoder is in `fp32/encoder_fp32.onnx` AND `fp32_v2/encoder_fp32.onnx`). For Phase 3 standard mode, download just `fp32_v2/` since it has the complete 8-graph set. Verify this against the snapshot in `~/.cache/huggingface`:
  ```bash
  ls ~/.cache/huggingface/hub/models--SemplificaAI--gliner2-multi-v1-onnx/snapshots/*/fp32_v2/
  ```

- [ ] **Step 2: Replace the `from_pretrained` body** to download the v2 file set:

  ```rust
  pub fn from_pretrained(model_id: &str) -> crate::Result<Self> {
      let api = crate::backends::hf_loader::hf_api()
          .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: hf_api: {e}")))?;
      let repo = api.model(model_id.to_string());

      // Download tokenizer + config (anno's standard pattern).
      let tokenizer_path =
          crate::backends::hf_loader::download_model_file(&repo, &["tokenizer.json"])
              .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download tokenizer: {e}")))?;
      let _config_path =
          crate::backends::hf_loader::download_model_file(&repo, &["config.json"])
              .map_err(|e| crate::Error::Backend(format!("gliner2_fastino: download config: {e}")))?;

      // Download the 8 v2 ONNX files. Try fp32_v2 first (smaller payload than
      // fp32 because v2 fuses ops into single graphs, but larger than fp16).
      // Production users override with GLINER2_FASTINO_FP=fp16_v2 etc — see
      // future env-override task.
      let bases = [
          "encoder", "token_gather", "span_rep", "schema_gather",
          "count_pred_argmax", "count_lstm_fixed", "scorer", "classifier",
      ];
      for base in &bases {
          // Try fp32_v2 then fp32 (some single-graph bases live in fp32/ only).
          let candidates = [
              format!("fp32_v2/{base}_fp32.onnx"),
              format!("fp32/{base}_fp32.onnx"),
          ];
          let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
          crate::backends::hf_loader::download_model_file(&repo, &candidate_refs)
              .map_err(|e| crate::Error::Backend(
                  format!("gliner2_fastino: download {base}: {e}")
              ))?;
      }

      // Resolve to the snapshot dir and dispatch.
      let snapshot_dir = tokenizer_path.parent().ok_or_else(|| {
          crate::Error::Backend("gliner2_fastino: tokenizer parent missing".into())
      })?;
      let mut model = Self::from_local(snapshot_dir)?;
      model.model_id = model_id.to_string();
      Ok(model)
  }
  ```

  (Note: `download_model_file` accepts a slice of candidate paths and returns the first that successfully downloads. Reusing Phase 1's pattern.)

- [ ] **Step 3: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

  Expected: clean compile.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): from_pretrained downloads v2 8-graph set (fp32_v2 priority)"
  ```

---

## Milestone P3.M5 — Encoder + token_gather (~1 day)

Goal: first two pipeline steps. Outputs are owned ndarrays handed off to the next session.

### Task M5.1: Create `pipeline.rs` skeleton

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Module skeleton with attribution.**

  ```rust
  //! Standard-mode 8-session inference pipeline for `gliner2_fastino`.
  //!
  //! Adapted from SemplificaAI/gliner2-rs (Apache-2.0):
  //! https://github.com/SemplificaAI/gliner2-rs/blob/main/rust_component/src/lib_v2.rs
  //! Specifically: `Gliner2EngineV2::extract_standard` (lines ~660-897).
  //! Original: Copyright 2026 Dario Finardi, Semplifica s.r.l.
  //!
  //! Phase 3 standard mode (this module) does NOT implement IOBinding.
  //! The IOBinding-mode pipeline (lib_v2.rs:285-660) keeps tensors in
  //! a single ort allocator across session boundaries for 2-3× speedup;
  //! that's a Phase 3.5 follow-up.

  use crate::backends::gliner2_fastino::errors::Error;
  use crate::backends::gliner2_fastino::processor::{
      ProcessedRecord, SchemaTask, SchemaTransformer,
  };
  use crate::backends::gliner2_fastino::sessions::Sessions;
  use ndarray::{Array1, Array2, Array3, Array4};
  use ort::value::Tensor;

  /// Maximum span width baked into the v2 export. Spans wider than this
  /// can't be scored. Hardcoded in `count_lstm_fixed` and `scorer` ONNX
  /// graphs.
  pub const MAX_WIDTH: usize = 8;

  /// Maximum predicted instance count baked into the v2 export. Used by
  /// the scorer's first dimension (struct_proj is `[MAX_COUNT, M, H]`).
  pub const MAX_COUNT: usize = 20;

  /// Output of the encoder step. Owned f32 ndarray of shape `[1, L, H]`.
  pub(crate) struct EncoderOutput {
      pub hidden_states: ndarray::Array3<f32>,
  }
  ```

- [ ] **Step 2: Add `run_encoder` private function.**

  Append:

  ```rust
  /// Run the encoder graph. Tries output names in priority order
  /// (`hidden_states`, `last_hidden_state`, `output`) — different fastino
  /// exports use different names.
  pub(crate) fn run_encoder(
      sessions: &Sessions,
      record: &ProcessedRecord,
  ) -> Result<EncoderOutput, Error> {
      let seq_len = record.input_ids.len();
      let input_ids: Array2<i64> = Array2::from_shape_vec(
          (1, seq_len),
          record.input_ids.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("encoder input_ids reshape: {e}")))?;
      let attn_mask: Array2<i64> = Array2::from_shape_vec(
          (1, seq_len),
          record.attention_mask.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("encoder attn reshape: {e}")))?;

      let input_ids_t = crate::backends::ort_compat::tensor_from_ndarray(input_ids)
          .map_err(|e| Error::Tokenizer(format!("encoder input_ids tensor: {e}")))?;
      let attn_mask_t = crate::backends::ort_compat::tensor_from_ndarray(attn_mask)
          .map_err(|e| Error::Tokenizer(format!("encoder attn tensor: {e}")))?;

      let hs: ndarray::ArrayD<f32> = sessions.encoder.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "input_ids"      => input_ids_t.into_dyn(),
                      "attention_mask" => attn_mask_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("encoder run: {e}")))?;

              for name in ["hidden_states", "last_hidden_state", "output"] {
                  if let Some(v) = outputs.get(name) {
                      let (_shape, cow) = v
                          .try_extract_tensor::<f32>()
                          .map_err(|e| Error::Tokenizer(format!("encoder extract: {e}")))?;
                      // Convert Cow to owned Vec, infer shape from outputs.
                      let arr = cow.into_owned();
                      return Ok(arr);
                  }
              }
              // Fallback: take the first output.
              let first = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("encoder: no outputs".into())
              })?;
              let (_shape, cow) = first
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("encoder extract first: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      // hs is dynamic; convert to fixed [1, L, H] Array3.
      let shape = hs.shape().to_vec();
      if shape.len() != 3 || shape[0] != 1 {
          return Err(Error::Tokenizer(format!(
              "encoder output shape {:?}: expected [1, L, H]",
              shape
          )));
      }
      let hidden_states: Array3<f32> = hs
          .into_dimensionality::<ndarray::Ix3>()
          .map_err(|e| Error::Tokenizer(format!("encoder dim convert: {e}")))?;
      Ok(EncoderOutput { hidden_states })
  }
  ```

- [ ] **Step 3: Register the module.**

  In `mod.rs`: `pub(crate) mod pipeline;`.

- [ ] **Step 4: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): pipeline.rs skeleton with run_encoder (port from lib_v2.rs:673-694)"
  ```

### Task M5.2: Add `run_token_gather`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Append `run_token_gather`.**

  ```rust
  /// Output of token_gather: word-level embeddings extracted from the
  /// encoder's hidden states using `word_to_token_maps`.
  pub(crate) struct TokenGatherOutput {
      /// Shape: `[1, num_words, H]`
      pub text_embs: ndarray::Array3<f32>,
  }

  pub(crate) fn run_token_gather(
      sessions: &Sessions,
      encoder_out: &EncoderOutput,
      record: &ProcessedRecord,
  ) -> Result<TokenGatherOutput, Error> {
      let num_words = record.word_to_token_maps.len();
      if num_words == 0 {
          return Err(Error::Tokenizer("token_gather: 0 words in record".into()));
      }
      let word_starts: Vec<i64> = record
          .word_to_token_maps
          .iter()
          .map(|&(start, _)| start as i64)
          .collect();
      let word_idx_arr: Array1<i64> = Array1::from_vec(word_starts);

      let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
          encoder_out.hidden_states.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("token_gather hs tensor: {e}")))?;
      let word_idx_t = crate::backends::ort_compat::tensor_from_ndarray(word_idx_arr)
          .map_err(|e| Error::Tokenizer(format!("token_gather idx tensor: {e}")))?;

      let result: ndarray::ArrayD<f32> = sessions.token_gather.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "last_hidden_state" => hs_t.into_dyn(),
                      "word_indices"      => word_idx_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("token_gather run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("token_gather: no outputs".into())
              })?;
              let (_shape, cow) = v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("token_gather extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      let text_embs: Array3<f32> = result
          .into_dimensionality::<ndarray::Ix3>()
          .map_err(|e| Error::Tokenizer(format!("token_gather dim: {e}")))?;
      Ok(TokenGatherOutput { text_embs })
  }
  ```

- [ ] **Step 2: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): pipeline::run_token_gather (port from lib_v2.rs:702-712)"
  ```

---

## Milestone P3.M6 — `run_span_rep` + span_idx construction (~half day)

### Task M6.1: Append `run_span_rep` to pipeline.rs

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Append.**

  ```rust
  /// Output of span_rep: span-level embeddings.
  pub(crate) struct SpanRepOutput {
      /// Shape: `[1, num_spans, H]` where num_spans = num_words * MAX_WIDTH
      pub span_embs: ndarray::Array3<f32>,
  }

  /// Build the span-index tensor used by span_rep.
  ///
  /// For each (start_word, width_idx) pair where `width_idx` ∈ 0..MAX_WIDTH,
  /// emit (start, start + width_idx). If end exceeds `num_words`, emit
  /// `[0, 0]` as zero-padding (matches upstream's behavior — those spans
  /// are masked out by the model).
  pub(crate) fn build_span_idx(num_words: usize) -> ndarray::Array3<i64> {
      let num_spans = num_words * MAX_WIDTH;
      let mut data = Vec::with_capacity(num_spans * 2);
      for start in 0..num_words {
          for width in 0..MAX_WIDTH {
              let end = start + width;
              if end >= num_words {
                  data.extend_from_slice(&[0_i64, 0_i64]);
              } else {
                  data.push(start as i64);
                  data.push(end as i64);
              }
          }
      }
      ndarray::Array3::from_shape_vec((1, num_spans, 2), data)
          .expect("span_idx shape consistent by construction")
  }

  pub(crate) fn run_span_rep(
      sessions: &Sessions,
      tg_out: &TokenGatherOutput,
      num_words: usize,
  ) -> Result<SpanRepOutput, Error> {
      let span_idx = build_span_idx(num_words);

      let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
          tg_out.text_embs.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("span_rep hs tensor: {e}")))?;
      let idx_t = crate::backends::ort_compat::tensor_from_ndarray(span_idx)
          .map_err(|e| Error::Tokenizer(format!("span_rep idx tensor: {e}")))?;

      let result: ndarray::ArrayD<f32> = sessions.span_rep.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "hidden_states" => hs_t.into_dyn(),
                      "span_idx"      => idx_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("span_rep run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("span_rep: no outputs".into())
              })?;
              let (_shape, cow) = v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("span_rep extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      let span_embs: Array3<f32> = result
          .into_dimensionality::<ndarray::Ix3>()
          .map_err(|e| Error::Tokenizer(format!("span_rep dim: {e}")))?;
      Ok(SpanRepOutput { span_embs })
  }
  ```

- [ ] **Step 2: Add a unit test for `build_span_idx`** (the only piece testable without a real model):

  Append to `pipeline.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn build_span_idx_basic_shape() {
          let arr = build_span_idx(3);
          assert_eq!(arr.shape(), &[1, 3 * MAX_WIDTH, 2]);
      }

      #[test]
      fn build_span_idx_zero_pads_overflow() {
          // 2 words, MAX_WIDTH=8. Spans (0,0), (0,1), then (0,2..7) → all overflow.
          let arr = build_span_idx(2);
          // (0, 2..7) inclusive has 6 entries that overflow.
          let zero_count = arr
              .axis_iter(ndarray::Axis(1))
              .filter(|row| row[[0, 0]] == 0 && row[[0, 1]] == 0)
              .count();
          // (0,0) is also legit but it's [0, 0]. Count 0-padding by checking width:
          // For start=0: widths 0,1 valid; 2..7 padded → 6 padded
          // For start=1: width 0 valid (1,1); 1..7 padded → 7 padded
          // Total padded = 13. The (0,0) span at start=0 width=0 is also [0,0]
          // but it's a valid span — we test shape and structure separately.
          let _ = zero_count;
          // Stricter check: first row is start=0 width=0 → [0,0].
          assert_eq!(arr[[0, 0, 0]], 0);
          assert_eq!(arr[[0, 0, 1]], 0);
          // Second row is start=0 width=1 → [0,1].
          assert_eq!(arr[[0, 1, 0]], 0);
          assert_eq!(arr[[0, 1, 1]], 1);
          // 9th row is start=1 width=0 → [1,1].
          assert_eq!(arr[[0, MAX_WIDTH, 0]], 1);
          assert_eq!(arr[[0, MAX_WIDTH, 1]], 1);
          // 10th row is start=1 width=1 → would be (1,2) but 2 >= num_words=2,
          // so padded to [0,0].
          assert_eq!(arr[[0, MAX_WIDTH + 1, 0]], 0);
          assert_eq!(arr[[0, MAX_WIDTH + 1, 1]], 0);
      }
  }
  ```

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::pipeline
  ```

  Expected: PASS.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): pipeline::run_span_rep + build_span_idx (port from lib_v2.rs:716-733)"
  ```

---

## Milestone P3.M7 — schema_gather + count_pred_argmax (~1 day)

Per-task: extract prompt-token + field-token embeddings (`pc_emb`, `field_embs`), predict instance count.

### Task M7.1: Append `run_schema_gather` and `run_count_pred_argmax`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Append.**

  ```rust
  /// Output of schema_gather: per-task pc_emb + field_embs.
  pub(crate) struct SchemaGatherOutput {
      /// Shape: `[1, H]` — the [P]-token embedding (prompt context).
      pub pc_emb: ndarray::Array2<f32>,
      /// Shape: `[M, H]` where M = number of fields/labels for this task.
      pub field_embs: ndarray::Array2<f32>,
  }

  pub(crate) fn run_schema_gather(
      sessions: &Sessions,
      encoder_out: &EncoderOutput,
      task: &crate::backends::gliner2_fastino::processor::TaskMapping,
  ) -> Result<SchemaGatherOutput, Error> {
      let mut indices: Vec<i64> = Vec::with_capacity(1 + task.field_tok_indices.len());
      indices.push(task.prompt_tok_idx as i64);
      indices.extend(task.field_tok_indices.iter().map(|&i| i as i64));
      let idx_arr: Array1<i64> = Array1::from_vec(indices);

      let hs_t = crate::backends::ort_compat::tensor_from_ndarray(
          encoder_out.hidden_states.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("schema_gather hs tensor: {e}")))?;
      let idx_t = crate::backends::ort_compat::tensor_from_ndarray(idx_arr)
          .map_err(|e| Error::Tokenizer(format!("schema_gather idx tensor: {e}")))?;

      type SchemaResult = (ndarray::ArrayD<f32>, ndarray::ArrayD<f32>);
      let (pc, fields): SchemaResult = sessions.schema_gather.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "last_hidden_state" => hs_t.into_dyn(),
                      "schema_indices"    => idx_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("schema_gather run: {e}")))?;
              let mut iter = outputs.values();
              let pc_v = iter.next().ok_or_else(|| {
                  Error::Tokenizer("schema_gather: missing pc_emb".into())
              })?;
              let fields_v = iter.next().ok_or_else(|| {
                  Error::Tokenizer("schema_gather: missing field_embs".into())
              })?;
              let (_, pc_cow) = pc_v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("schema_gather pc extract: {e}")))?;
              let (_, fields_cow) = fields_v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("schema_gather fields extract: {e}")))?;
              Ok((pc_cow.into_owned(), fields_cow.into_owned()))
          },
      )?;

      let pc_emb: Array2<f32> = pc
          .into_dimensionality::<ndarray::Ix2>()
          .map_err(|e| Error::Tokenizer(format!("schema_gather pc dim: {e}")))?;
      let field_embs: Array2<f32> = fields
          .into_dimensionality::<ndarray::Ix2>()
          .map_err(|e| Error::Tokenizer(format!("schema_gather fields dim: {e}")))?;
      Ok(SchemaGatherOutput { pc_emb, field_embs })
  }

  /// Run `count_pred_argmax`. Returns the predicted instance count
  /// (already argmaxed in-graph; the i64 output is a scalar).
  pub(crate) fn run_count_pred_argmax(
      sessions: &Sessions,
      sg_out: &SchemaGatherOutput,
  ) -> Result<usize, Error> {
      let pc_t = crate::backends::ort_compat::tensor_from_ndarray(sg_out.pc_emb.clone())
          .map_err(|e| Error::Tokenizer(format!("count_pred pc tensor: {e}")))?;

      let count: ndarray::ArrayD<i64> = sessions.count_pred_argmax.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "pc_emb" => pc_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("count_pred run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("count_pred_argmax: no outputs".into())
              })?;
              let (_, cow) = v
                  .try_extract_tensor::<i64>()
                  .map_err(|e| Error::Tokenizer(format!("count_pred extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      let val = count.iter().next().copied().unwrap_or(0);
      Ok(val.max(0) as usize)
  }
  ```

- [ ] **Step 2: `TaskMapping` re-export.** Schema_gather references `TaskMapping`. The struct is already in `processor.rs` (per Phase 1). Confirm it's `pub` (not `pub(crate)`) — if not, promote to `pub(crate)` minimum.

- [ ] **Step 3: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs \
          crates/anno/src/backends/gliner2_fastino/processor.rs
  git commit -m "feat(gliner2_fastino): pipeline::run_schema_gather + run_count_pred_argmax (port from lib_v2.rs:735-771)"
  ```

---

## Milestone P3.M8 — count_lstm_fixed + scorer (~1 day)

### Task M8.1: Append `run_count_lstm_fixed` and `run_scorer`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Append.**

  ```rust
  /// Output of count_lstm_fixed: struct projection used by scorer.
  /// Shape: `[MAX_COUNT, M, H]`.
  pub(crate) struct CountLstmOutput {
      pub struct_proj: ndarray::Array3<f32>,
  }

  pub(crate) fn run_count_lstm_fixed(
      sessions: &Sessions,
      sg_out: &SchemaGatherOutput,
  ) -> Result<CountLstmOutput, Error> {
      let fields_t = crate::backends::ort_compat::tensor_from_ndarray(
          sg_out.field_embs.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("count_lstm tensor: {e}")))?;

      let proj: ndarray::ArrayD<f32> = sessions.count_lstm_fixed.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "field_embs" => fields_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("count_lstm run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("count_lstm_fixed: no outputs".into())
              })?;
              let (_, cow) = v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("count_lstm extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      let struct_proj: Array3<f32> = proj
          .into_dimensionality::<ndarray::Ix3>()
          .map_err(|e| Error::Tokenizer(format!("count_lstm dim: {e}")))?;
      Ok(CountLstmOutput { struct_proj })
  }

  /// Output of scorer: per-instance per-span per-label entity scores.
  /// Shape: `[MAX_COUNT, num_words, MAX_WIDTH, M]`.
  /// Already-sigmoided per upstream (`extract_standard` line ~825 comment:
  /// "Scorer — restituisce probabilità sigmoid già calcolate").
  pub(crate) struct ScorerOutput {
      pub scores: ndarray::Array4<f32>,
  }

  pub(crate) fn run_scorer(
      sessions: &Sessions,
      sr_out: &SpanRepOutput,
      cl_out: &CountLstmOutput,
  ) -> Result<ScorerOutput, Error> {
      let span_t = crate::backends::ort_compat::tensor_from_ndarray(
          sr_out.span_embs.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("scorer span tensor: {e}")))?;
      let proj_t = crate::backends::ort_compat::tensor_from_ndarray(
          cl_out.struct_proj.clone(),
      )
      .map_err(|e| Error::Tokenizer(format!("scorer proj tensor: {e}")))?;

      let result: ndarray::ArrayD<f32> = sessions.scorer.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "span_embeddings" => span_t.into_dyn(),
                      "struct_proj"     => proj_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("scorer run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("scorer: no outputs".into())
              })?;
              let (_, cow) = v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("scorer extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      let scores: Array4<f32> = result
          .into_dimensionality::<ndarray::Ix4>()
          .map_err(|e| Error::Tokenizer(format!("scorer dim: {e}")))?;
      Ok(ScorerOutput { scores })
  }
  ```

- [ ] **Step 2: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): pipeline::run_count_lstm_fixed + run_scorer (port from lib_v2.rs:818-836)"
  ```

---

## Milestone P3.M9 — NMS + entity decoding (~1 day)

Goal: turn the scorer's `[MAX_COUNT, num_words, MAX_WIDTH, M]` tensor into `Vec<crate::Entity>` with NMS.

### Task M9.1: Create `nms.rs`

**Files:**
- Create: `crates/anno/src/backends/gliner2_fastino/nms.rs`

- [ ] **Step 1: Module skeleton + greedy NMS.**

  ```rust
  //! Greedy NMS for span-level entities. Adapted from
  //! SemplificaAI/gliner2-rs `extract_standard` lines ~870-885.

  use crate::Entity;

  /// Sort entities by confidence descending and drop overlapping ones.
  /// `flat_ner = true`: any token-overlap drops the lower-scored entity
  /// regardless of label. `flat_ner = false`: only same-label overlaps drop.
  pub(crate) fn greedy_nms(mut candidates: Vec<Entity>, flat_ner: bool) -> Vec<Entity> {
      candidates.sort_by(|a, b| {
          let ac: f32 = a.confidence.into();
          let bc: f32 = b.confidence.into();
          bc.partial_cmp(&ac).unwrap_or(std::cmp::Ordering::Equal)
      });
      let mut selected: Vec<Entity> = Vec::with_capacity(candidates.len());
      for c in candidates {
          let overlaps = selected.iter().any(|s| {
              let span_overlap = !(c.end() <= s.start() || c.start() >= s.end());
              span_overlap && (flat_ner || s.entity_type == c.entity_type)
          });
          if !overlaps {
              selected.push(c);
          }
      }
      selected
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::EntityType;

      fn ent(text: &str, ty: EntityType, start: usize, end: usize, score: f32) -> Entity {
          Entity::new(text, ty, start, end, score)
      }

      #[test]
      fn nms_keeps_higher_score_drops_overlap_same_label() {
          let cands = vec![
              ent("Acme", EntityType::Organization, 0, 4, 0.8),
              ent("Acme Corp", EntityType::Organization, 0, 9, 0.95),
          ];
          let kept = greedy_nms(cands, false);
          assert_eq!(kept.len(), 1);
          assert_eq!(kept[0].text, "Acme Corp");
      }

      #[test]
      fn nms_flat_ner_drops_overlap_across_labels() {
          let cands = vec![
              ent("Acme", EntityType::Organization, 0, 4, 0.6),
              ent("Acme", EntityType::Person, 0, 4, 0.95),
          ];
          let kept = greedy_nms(cands, true);
          assert_eq!(kept.len(), 1);
          assert!(matches!(kept[0].entity_type, EntityType::Person));
      }

      #[test]
      fn nms_keeps_disjoint_spans() {
          let cands = vec![
              ent("Acme", EntityType::Organization, 0, 4, 0.9),
              ent("Paris", EntityType::Location, 13, 18, 0.85),
          ];
          let kept = greedy_nms(cands, false);
          assert_eq!(kept.len(), 2);
      }
  }
  ```

- [ ] **Step 2: Register module.** In `mod.rs`: `pub(crate) mod nms;`.

- [ ] **Step 3: Run.**

  ```bash
  cargo test -p anno --features gliner2-fastino backends::gliner2_fastino::nms
  ```

  Expected: 3 PASS.

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/nms.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): greedy NMS for span-level entities"
  ```

### Task M9.2: Add `decode_entities` to pipeline.rs

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Append the decoder.**

  ```rust
  /// Decode the scorer's [MAX_COUNT, num_words, MAX_WIDTH, M] tensor to
  /// `Vec<Entity>` (with character offsets in the original text), apply
  /// threshold, then NMS.
  pub(crate) fn decode_entities(
      text: &str,
      record: &ProcessedRecord,
      task: &crate::backends::gliner2_fastino::processor::TaskMapping,
      scorer_out: &ScorerOutput,
      pred_count: usize,
      threshold: f32,
      flat_ner: bool,
  ) -> Vec<crate::Entity> {
      let num_words = record.word_to_char_maps.len();
      let num_labels = task.labels.len();
      let scores = &scorer_out.scores;

      let mut candidates: Vec<crate::Entity> = Vec::new();
      for c_idx in 0..pred_count.min(MAX_COUNT) {
          for start in 0..num_words {
              for width_idx in 0..MAX_WIDTH {
                  let end_word = (start + width_idx + 1).min(num_words);
                  for m in 0..num_labels {
                      let prob = scores[[c_idx, start, width_idx, m]];
                      if prob <= threshold {
                          continue;
                      }
                      let (char_start, _) = record.word_to_char_maps[start];
                      let (_, char_end) = record.word_to_char_maps[end_word - 1];
                      if char_end > text.len() || char_start > char_end {
                          continue;
                      }
                      let surface = text[char_start..char_end].trim();
                      if surface.is_empty() {
                          continue;
                      }
                      let etype = crate::schema::map_to_canonical(&task.labels[m], None);
                      // Convert byte offsets to char offsets (anno convention).
                      let (cs, ce) = crate::offset::bytes_to_chars(text, char_start, char_end);
                      candidates.push(crate::Entity::new(surface, etype, cs, ce, prob));
                  }
              }
          }
      }
      super::nms::greedy_nms(candidates, flat_ner)
  }
  ```

- [ ] **Step 2: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  ```

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): pipeline::decode_entities (threshold + NMS)"
  ```

---

## Milestone P3.M10 — classifier head (~1 day)

Goal: replace Phase 1's NER-head approximation in `classify` with the real `[L]`-head MLP via `classifier_fp32.onnx`.

### Task M10.1: Append `run_classifier` to pipeline.rs

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/pipeline.rs`

- [ ] **Step 1: Add `half = { workspace = true }` to anno's `Cargo.toml`** if not already present:

  ```bash
  grep '^half' crates/anno/Cargo.toml
  ```

  If missing, add to `[dependencies]`:

  ```toml
  half = { workspace = true, optional = true }
  ```

  And include it in the `gliner2-fastino` feature:

  ```toml
  gliner2-fastino = ["onnx", "dep:half"]
  ```

  Workspace `Cargo.toml` should already have `half = "2"` since other crates likely use it; if not, add to root workspace.

- [ ] **Step 2: Append the classifier wrapper.**

  ```rust
  /// Run the classifier head on a single task's field_embs.
  /// Returns label scores (softmax probabilities, sum to 1).
  ///
  /// Internal mechanics: pad `field_embs` to `[1, num_labels, MAX_WIDTH,
  /// hidden_size]` with first-position-only set, convert to fp16, run,
  /// softmax over the label axis.
  pub(crate) fn run_classifier(
      sessions: &Sessions,
      sg_out: &SchemaGatherOutput,
  ) -> Result<Vec<f32>, Error> {
      let num_labels = sg_out.field_embs.shape()[0];
      let hidden_size = sg_out.field_embs.shape()[1];

      // Pad to [1, num_labels, MAX_WIDTH, hidden_size] in fp16.
      let mut padded: Array4<half::f16> = Array4::from_elem(
          (1, num_labels, MAX_WIDTH, hidden_size),
          half::f16::from_f32(0.0),
      );
      for m in 0..num_labels {
          for d in 0..hidden_size {
              padded[[0, m, 0, d]] =
                  half::f16::from_f32(sg_out.field_embs[[m, d]]);
          }
      }
      let pad_t = crate::backends::ort_compat::tensor_from_ndarray(padded)
          .map_err(|e| Error::Tokenizer(format!("classifier tensor: {e}")))?;

      let logits: ndarray::ArrayD<f32> = sessions.classifier.with_session(
          |s| -> Result<_, Error> {
              let outputs = s
                  .run(ort::inputs![
                      "span_embeddings" => pad_t.into_dyn(),
                  ])
                  .map_err(|e| Error::Tokenizer(format!("classifier run: {e}")))?;
              let v = outputs.values().next().ok_or_else(|| {
                  Error::Tokenizer("classifier: no outputs".into())
              })?;
              let (_, cow) = v
                  .try_extract_tensor::<f32>()
                  .map_err(|e| Error::Tokenizer(format!("classifier extract: {e}")))?;
              Ok(cow.into_owned())
          },
      )?;

      // logits shape is [1, num_labels, MAX_WIDTH, 1]. Take position 0.
      let mut exps = Vec::with_capacity(num_labels);
      let mut exp_sum = 0.0f32;
      for m in 0..num_labels {
          let l = logits[[0, m, 0, 0]];
          let e = l.exp();
          exp_sum += e;
          exps.push(e);
      }
      Ok(exps.into_iter().map(|e| e / exp_sum.max(1e-12)).collect())
  }
  ```

- [ ] **Step 3: Verify compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino
  ```

- [ ] **Step 4: Commit.**

  ```bash
  git add crates/anno/Cargo.toml crates/anno/src/backends/gliner2_fastino/pipeline.rs
  git commit -m "feat(gliner2_fastino): pipeline::run_classifier (real [L]-head, replaces Phase 1 approximation)"
  ```

---

## Milestone P3.M11 — Wire pipeline into public API (~1 day)

Goal: real `extract_ner` and `classify` that call the pipeline. Phase 1's stubs go away.

### Task M11.1: Rewire `extract_ner`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Replace the stub `extract_ner` body** (the one M2.2 left as a Phase 3 placeholder) with:

  ```rust
  pub(crate) fn extract_ner(
      &self,
      text: &str,
      types: &[&str],
      threshold: f32,
  ) -> crate::Result<Vec<crate::Entity>> {
      use pipeline::*;
      if types.is_empty() {
          return Ok(vec![]);
      }
      let labels: Vec<String> = types.iter().map(|s| s.to_string()).collect();
      let task = processor::SchemaTask::Entities(labels.clone());
      let record = self.transformer.transform(text, &[task])?;
      let num_words = record.word_to_char_maps.len();
      if num_words == 0 {
          return Ok(vec![]);
      }

      let enc = run_encoder(&self.sessions, &record)?;
      let tg  = run_token_gather(&self.sessions, &enc, &record)?;
      let sr  = run_span_rep(&self.sessions, &tg, num_words)?;

      let task_map = record.tasks.first().ok_or_else(|| {
          crate::Error::Backend("gliner2_fastino: transformer produced no task mapping".into())
      })?;
      let sg = run_schema_gather(&self.sessions, &enc, task_map)?;
      let pred_count = run_count_pred_argmax(&self.sessions, &sg)?;
      if pred_count == 0 {
          return Ok(vec![]);
      }
      let cl = run_count_lstm_fixed(&self.sessions, &sg)?;
      let scorer_out = run_scorer(&self.sessions, &sr, &cl)?;
      let entities = decode_entities(
          text,
          &record,
          task_map,
          &scorer_out,
          pred_count,
          threshold,
          /* flat_ner = */ false,
      );
      Ok(entities)
  }
  ```

- [ ] **Step 2: Compile.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  ```

  Expected: clean.

- [ ] **Step 3: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "feat(gliner2_fastino): wire extract_ner to multi-session pipeline"
  ```

### Task M11.2: Rewire `classify`

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Replace the existing `classify` body** (currently the NER-head approximation):

  ```rust
  pub fn classify(
      &self,
      text: &str,
      labels: &[&str],
      _threshold: f32,
  ) -> crate::Result<Vec<(String, f32)>> {
      use pipeline::*;
      if labels.is_empty() {
          return Ok(vec![]);
      }
      let label_strings: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
      let task = processor::SchemaTask::Classifications(
          "classification".to_string(),
          label_strings.clone(),
      );
      let record = self.transformer.transform(text, &[task])?;
      let task_map = record.tasks.first().ok_or_else(|| {
          crate::Error::Backend("gliner2_fastino: transformer produced no task mapping".into())
      })?;

      let enc = run_encoder(&self.sessions, &record)?;
      let sg = run_schema_gather(&self.sessions, &enc, task_map)?;
      let pred_count = run_count_pred_argmax(&self.sessions, &sg)?;
      if pred_count == 0 {
          return Ok(label_strings.into_iter().map(|l| (l, 0.0)).collect());
      }
      let probs = run_classifier(&self.sessions, &sg)?;

      let mut out: Vec<(String, f32)> = label_strings
          .into_iter()
          .zip(probs.into_iter())
          .collect();
      out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
      Ok(out)
  }
  ```

- [ ] **Step 2: Update the rustdoc** above `classify` to remove the "NER-head approximation" caveat (the Phase 1 caveat is no longer accurate). New rustdoc:

  ```rust
  /// Single-label classification using the dedicated `[L]`-head classifier.
  ///
  /// Returns labels sorted by descending probability (softmax). The
  /// `threshold` parameter is reserved for future multi-label use; in
  /// Phase 3 single-label mode it's ignored.
  ///
  /// Not behind a public trait — see spec §3.
  ```

- [ ] **Step 3: Update `processor::SchemaTask`.** The `Classifications(String, Vec<String>)` variant must exist in the prompt-assembly. Phase 1 ports only `Entities`; restoring `Classifications` is part of this task.

  In `processor.rs`'s `SchemaTask` enum, restore (per upstream):

  ```rust
  Classifications(String, Vec<String>),
  ```

  And in `transform`, restore the `Classifications` arm (port from `gliner2-rs/processor.rs` lines ~180–199):

  ```rust
  SchemaTask::Classifications(task_name, cls_labels) => {
      combined_tokens.push("(");
      let prompt_idx = combined_tokens.len();
      combined_tokens.push(P_TOKEN);
      combined_tokens.push(task_name.as_str());
      combined_tokens.push("(");
      for label in cls_labels {
          combined_tokens.push(L_TOKEN);
          field_indices.push(combined_tokens.len());
          combined_tokens.push(label.as_str());
          labels.push(label.clone());
      }
      combined_tokens.push(")");
      combined_tokens.push(")");
      task_mappings_temp.push((
          task_name.clone(),
          "classifications".to_string(),
          labels,
          prompt_idx,
          field_indices,
      ));
  }
  ```

  (The `// TODO Phase 2` comment from Phase 1 goes away.)

- [ ] **Step 4: Compile + test.**

  ```bash
  cargo check -p anno --features gliner2-fastino --tests
  cargo test  -p anno --features gliner2-fastino backends::gliner2_fastino
  ```

  Expected: all unit tests still pass.

- [ ] **Step 5: Commit.**

  ```bash
  git add crates/anno/src/backends/gliner2_fastino/mod.rs \
          crates/anno/src/backends/gliner2_fastino/processor.rs
  git commit -m "feat(gliner2_fastino): real [L]-head classify; restore Classifications task arm"
  ```

---

## Milestone P3.M12 — Integration test against real model (~1 day)

Goal: the Tier-2 integration tests (currently `#[ignore]` placeholders) actually run end-to-end.

### Task M12.1: Update integration tests

**Files:**
- Modify: `crates/anno/tests/gliner2_fastino_integration.rs`

- [ ] **Step 1: Update header.** Drop the "Phase-3-dependent" caveat — Phase 3 is now this work.

  Replace:

  ```rust
  //! Tier-2 integration tests for `gliner2_fastino`. `#[ignore]`-gated.
  //!
  //! **Status (2026-05-05): these tests will FAIL...
  ```

  with:

  ```rust
  //! Tier-2 integration tests for `gliner2_fastino`. `#[ignore]`-gated since
  //! they download the SemplificaAI/gliner2-multi-v1-onnx model (~6 GB) on
  //! first run and require a working multi-session pipeline (Phase 3).
  //!
  //! Run locally with:
  //!
  //!     cargo test -p anno --features gliner2-fastino \
  //!         --test gliner2_fastino_integration -- --ignored
  ```

- [ ] **Step 2: Tighten the assertion in `fastino_multi_v1_extracts_org_and_loc`.** With the real pipeline running, we can assert specific entities:

  ```rust
  #[test]
  #[ignore]
  fn fastino_multi_v1_extracts_org_and_loc() {
      let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load gliner2-multi-v1");
      let ents = model
          .extract_with_types(FIXTURE, &["organization", "location"], 0.5)
          .expect("extract");

      eprintln!("entities: {ents:#?}");

      // Loose assertions — the model's exact tokenization-driven output
      // varies, but Acme Corp + Paris are clearly correct labels.
      let acme = ents.iter().find(|e| e.text.contains("Acme"));
      let paris = ents.iter().find(|e| e.text == "Paris" || e.text.contains("Paris"));
      assert!(acme.is_some(), "expected an Acme org entity, got {ents:#?}");
      assert!(paris.is_some(), "expected a Paris entity, got {ents:#?}");
  }
  ```

- [ ] **Step 3: Tighten classify smoke test.**

  ```rust
  #[test]
  #[ignore]
  fn fastino_classify_smoke() {
      let model = GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
          .expect("load");
      let scores = model
          .classify(
              "This product is wonderful, I love it.",
              &["positive", "negative", "neutral"],
              0.0,
          )
          .expect("classify");
      assert_eq!(scores.len(), 3);
      eprintln!("classify scores: {scores:?}");
      // Top-ranked should be 'positive' for this clearly-positive text.
      assert_eq!(scores[0].0, "positive", "expected 'positive' top-ranked, got {scores:?}");
  }
  ```

- [ ] **Step 4: Drop the deprecated `semplifica_external_pin_loads` test** (it was a placeholder; the other two now exercise the same path more meaningfully).

- [ ] **Step 5: Run.**

  ```bash
  bash /mnt/c/Users/NMarchitecte/anno-gliner2-phase3/wsl-c-test-integration.sh
  ```

  (Reuse the script pattern; invokes `cargo test ... -- --ignored`.) ETA: 2-5 minutes (model is cached, but inference is unaccelerated CPU).

  Expected: both tests PASS.

- [ ] **Step 6: Commit.**

  ```bash
  git add crates/anno/tests/gliner2_fastino_integration.rs
  git commit -m "test(gliner2_fastino): integration tests now run end-to-end against SemplificaAI pin"
  ```

---

## Milestone P3.M13 — GPU EP wiring (~1 day, optional but recommended)

Goal: opt-in CUDA + CoreML execution providers. Reuses anno's existing `OnnxSessionConfig::prefer_cuda`/`prefer_coreml` plumbing.

### Task M13.1: Plumb GPU prefs through Sessions

**Files:**
- Modify: `crates/anno/src/backends/gliner2_fastino/sessions.rs`
- Modify: `crates/anno/src/backends/gliner2_fastino/mod.rs`

- [ ] **Step 1: Extend `Sessions::from_dir` to accept an `OnnxSessionConfig`.**

  Replace the signature:

  ```rust
  pub fn from_dir(
      model_dir: &Path,
      cfg: hf_loader::OnnxSessionConfig,
  ) -> Result<Self, Error> {
  ```

  And inside, replace `SessionSlot::from_path(...)` calls with `SessionSlot::from_path_with_cfg(...)`:

  ```rust
  pub fn from_path_with_cfg(
      model_path: &Path,
      cfg: hf_loader::OnnxSessionConfig,
  ) -> Result<Self, Error> {
      let session = hf_loader::create_onnx_session(model_path, cfg)
          .map_err(|e| Error::Tokenizer(format!("session {}: {e}", model_path.display())))?;
      Ok(Self { inner: Arc::new(Mutex::new(session)) })
  }
  ```

  Keep the original `from_path` (with `Default::default()`) for backwards-compat.

- [ ] **Step 2: Update `from_local`** to take a config:

  ```rust
  pub fn from_local_with_options(
      model_dir: &Path,
      cfg: hf_loader::OnnxSessionConfig,
  ) -> crate::Result<Self> {
      // ... same body as from_local but passing cfg into Sessions::from_dir
  }

  pub fn from_local(model_dir: &Path) -> crate::Result<Self> {
      Self::from_local_with_options(model_dir, hf_loader::OnnxSessionConfig::default())
  }
  ```

- [ ] **Step 3: Add Cargo features.**

  In `crates/anno/Cargo.toml`:

  ```toml
  gliner2-fastino-cuda    = ["gliner2-fastino", "onnx-cuda"]
  gliner2-fastino-coreml  = ["gliner2-fastino", "onnx-coreml"]
  ```

  These transitively enable the existing `onnx-cuda` / `onnx-coreml` features that `hf_loader::OnnxSessionConfig` already understands.

- [ ] **Step 4: Update catalog row.** In `crates/anno/src/backends/catalog.rs`, change:

  ```rust
  gpu_support: false, // CPU only in Phase 1; GPU EP wiring lands in Phase 3
  ```

  to:

  ```rust
  gpu_support: true,
  ```

- [ ] **Step 5: Compile + test (no GPU host needed for compile).**

  ```bash
  cargo check -p anno --features gliner2-fastino
  cargo check -p anno --features gliner2-fastino-cuda
  cargo check -p anno --features gliner2-fastino-coreml
  cargo test  -p anno --features gliner2-fastino
  ```

- [ ] **Step 6: Commit.**

  ```bash
  git add crates/anno/Cargo.toml \
          crates/anno/src/backends/gliner2_fastino/sessions.rs \
          crates/anno/src/backends/gliner2_fastino/mod.rs \
          crates/anno/src/backends/catalog.rs
  git commit -m "feat(gliner2_fastino): GPU EP wiring (cuda + coreml feature flags)"
  ```

---

## Milestone P3.M14 — Toolchain pin for rustc 1.94 ICE workaround (~half day)

The Phase 1 finalization session discovered `rustc 1.94.0` ICEs in `annotate_snippets::renderer::styled_buffer::StyledBuffer::replace` while rendering lint diagnostics. Workaround: `RUSTFLAGS="--cap-lints allow"`. Phase 3 should make this less embarrassing.

### Task M14.1: Investigate and pin

**Files:**
- Possibly create: `rust-toolchain.toml`
- Modify: `.github/workflows/*.yml` if any (set RUSTFLAGS or pinned toolchain)

- [ ] **Step 1: Test on a recent stable.** From WSL Ubuntu-C:

  ```bash
  rustup install 1.95
  rustup default 1.95
  cargo clean
  cargo check -p anno --features gliner2-fastino   # without RUSTFLAGS=--cap-lints
  ```

  If it compiles cleanly: pin 1.95+ in `rust-toolchain.toml`.

- [ ] **Step 2: Test current beta.**

  ```bash
  rustup install beta
  cargo +beta check -p anno --features gliner2-fastino
  ```

- [ ] **Step 3: Pick the lowest version that doesn't ICE.** Create `rust-toolchain.toml` at the workspace root:

  ```toml
  [toolchain]
  channel = "1.95"
  profile = "default"
  ```

  (Or whichever version was confirmed.)

- [ ] **Step 4: Document the finding.** Add to `docs/dev-notes/windows-msvc-build-notes.md` (create if missing):

  ```markdown
  # Build environment notes for anno

  ## rustc 1.94.0 ICE in lint rendering

  Discovered 2026-05-05 during Phase 3 work on gliner2_fastino.
  rustc 1.94.0 (4a4ef493e 2026-03-02) panics in
  `annotate_snippets::renderer::styled_buffer::StyledBuffer::replace`
  with "slice index starts at 9 but ends at 8" while rendering lint
  diagnostics during compilation of anno's gliner2_fastino backend.

  The panic happens BEFORE compile errors would be emitted, masking
  real issues. Pin `rustc 1.95+` (or whichever version this plan
  confirms) via `rust-toolchain.toml` to avoid.

  Workaround on hosts that can't upgrade: `RUSTFLAGS="--cap-lints allow"`
  caps lints below the rendering threshold and sidesteps the ICE.
  ```

- [ ] **Step 5: Commit.**

  ```bash
  git add rust-toolchain.toml docs/dev-notes/windows-msvc-build-notes.md
  git commit -m "chore(toolchain): pin rust 1.95+ to avoid annotate_snippets ICE on 1.94"
  ```

---

## Milestone P3.M15 — Documentation updates (~half day)

### Task M15.1: Update spec, plan, BACKENDS, export doc

- [ ] **Step 1: Update `docs/superpowers/specs/2026-05-04-gliner2-fastino-design.md` §5** — mark Phase 3 as shipped.

  ```markdown
  | **3** | Multi-session pipeline (encoder + token_gather + span_rep + schema_gather + count_pred_argmax + count_lstm_fixed + scorer + classifier). 8-session standard mode. GPU EP wiring (CUDA, CoreML). | Issue #18 Phase-3 acceptance: end-to-end inference against `SemplificaAI/gliner2-multi-v1-onnx` produces correct entities + classifications on fixture text. ✅ shipped 2026-05-XX | ~1.5 wk |
  ```

  Note: Phase 3.5 (IOBinding mode) becomes a follow-up.

- [ ] **Step 2: Update roadmap** `docs/superpowers/specs/2026-05-04-gliner2-fastino-roadmap.md`:
  - Track D status: `started → shipped` (or whatever post-merge state)
  - Add Track D' (IOBinding mode) as a new optional track per the architectural finding.

- [ ] **Step 3: Update `docs/dev-notes/gliner2-fastino-export.md`.** Drop the "Phase 3 needed for SemplificaAI pin" caveat. Add a section:

  ```markdown
  ## ONNX layout

  The backend expects 8 ONNX files in either `fp32_v2/` or `fp16_v2/`
  subdirectories (or `fp32/`, `fp16/` for compatibility):

  - `encoder_<dtype>.onnx`
  - `token_gather_<dtype>.onnx`
  - `span_rep_<dtype>.onnx`
  - `schema_gather_<dtype>.onnx`
  - `count_pred_argmax_<dtype>.onnx`
  - `count_lstm_fixed_<dtype>.onnx`
  - `scorer_<dtype>.onnx`
  - `classifier_<dtype>.onnx`

  Plus `tokenizer.json` and `config.json` in the snapshot root.

  Compatible exports include `SemplificaAI/gliner2-multi-v1-onnx`.
  ```

- [ ] **Step 4: Update `docs/BACKENDS.md`** — change the gliner2_fastino description from "experimental, issue #18" to "experimental — multi-session pipeline. NER + classification. Issue #18".

- [ ] **Step 5: Update `crates/anno/src/backends/gliner2_fastino/mod.rs` rustdoc**:
  - Drop the LoRA-redirect prominence (LoRA hot-swap is still Phase 4; just demote, don't remove).
  - Add a "# Architecture" section documenting the 8-session pipeline.
  - Drop the "Phase 1 stub" comments throughout `extract_ner` (they're stale).

- [ ] **Step 6: Commit.**

  ```bash
  git add docs/ crates/anno/src/backends/gliner2_fastino/mod.rs
  git commit -m "docs(gliner2_fastino): mark phase 3 (multi-session pipeline) shipped"
  ```

---

## Milestone P3.M16 — Final sweep + PR (~half day)

### Task M16.1: Full sweep

- [ ] **Step 1: Cargo matrix.**

  ```bash
  bash /mnt/c/Users/NMarchitecte/anno-gliner2-phase3/wsl-c-final-sweep.sh
  ```

  Expected:
  - `cargo check --no-default-features` ✅
  - `cargo check --features gliner2-fastino` ✅
  - `cargo check --features gliner2-fastino --tests` ✅
  - `cargo test --lib` (gliner2_fastino + catalog + dispatch) ✅ all pass
  - `cargo test --test gliner2_fastino_integration -- --ignored` ✅ both pass
  - `cargo clippy --features gliner2-fastino -- -D warnings` ✅ clean

- [ ] **Step 2: Cap-lints recheck.** Confirm `RUSTFLAGS=--cap-lints allow` is no longer needed (after the toolchain pin in M14):

  ```bash
  unset RUSTFLAGS
  cargo test -p anno --features gliner2-fastino --lib
  ```

  Expected: clean (no `--cap-lints` workaround required on the pinned toolchain).

- [ ] **Step 3: Doc build.**

  ```bash
  cargo doc -p anno --features gliner2-fastino --no-deps
  ```

  Expected: clean.

### Task M16.2: PR

- [ ] **Step 1: Push and open PR.**

  ```bash
  git push -u fork feat/gliner2-fastino-phase3
  gh pr create \
      --repo arclabs561/anno \
      --head jamon8888:feat/gliner2-fastino-phase3 \
      --base main \
      --title "feat(gliner2_fastino): Phase 3 — multi-session pipeline (issue #18)" \
      --body-file docs/superpowers/plans/2026-05-05-gliner2-fastino-phase3.md
  ```

- [ ] **Step 2: Update issue #18.** Comment with PR link, summary of what shipped, and note that Phase 3.5 (IOBinding mode) is now the remaining performance follow-up.

---

## Out of scope (tracked, not implemented here)

- **Phase 3.5 (IOBinding mode):** port `extract_iobinding` from `lib_v2.rs:285-660`. 2-3× CPU/GPU speedup via zero-copy session chaining. ~1 week. Separate plan.
- **Phase 2 (structure extraction):** the 8-session pipeline in this plan handles the `Entities` and `Classifications` arms of `SchemaTask`. The `Structures` arm (`extract_structure(text, schema) -> serde_json::Value`) needs an output-assembly pass that uses the per-instance `[MAX_COUNT, ...]` axis. Separate plan: `docs/superpowers/plans/2026-05-04-gliner2-fastino-phase2.md`.
- **Phase 4 (Candle path + LoRA hot-swap):** see roadmap.
- **Per-label thresholds / label descriptions / streaming batch:** Phase 1.5 polish.
- **Relations extraction:** the upstream `extract_standard` handles `task_map.task_type == "relations"` by pairing `head` and `tail` entities. Phase 3 doesn't expose this through the public API — gated as a future expansion of `RelationExtractor` impl.

---

## Acceptance for Phase 3

- [ ] `extract_with_types("Acme Corp signed a deal in Paris", &["organization", "location"], 0.5)` against `SemplificaAI/gliner2-multi-v1-onnx` returns at least one Organization entity matching "Acme" and one Location entity matching "Paris".
- [ ] `classify("This is wonderful", &["positive", "negative"], 0.0)` against the same model ranks "positive" first.
- [ ] All Phase 1 unit tests still pass (regression check).
- [ ] Two `#[ignore]` integration tests pass when run with `--ignored`.
- [ ] `cargo check --features gliner2-fastino-cuda` and `--features gliner2-fastino-coreml` compile (runtime GPU validation is host-dependent and not part of the cargo gate).
- [ ] `cargo test` passes WITHOUT the `RUSTFLAGS="--cap-lints allow"` workaround (toolchain pin in M14).
- [ ] Documentation reflects shipped state.
