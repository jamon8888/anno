# Research — anno Local Ingest Architecture (exe/dmg + local MCP, 12 GB RAM, ≥500 docs/week)

**Date**: 2026-05-19
**Status**: Research / analysis — produces a phased roadmap; each phase gets its own spec → plan.
**Decision baseline**: Approach **A** (package + harden what we have; OCR first-class; B/A′ documented, not built). Confirmed with the user.

## 1. Executive verdict

The architecture is **fit for purpose at 500–1000 docs/week on a 12 GB machine**. The work is **distribution + robustness + an OCR strategy + RAM discipline — not speed.**

Evidence (this codebase, measured this session):
- ~203 s for 50 text-layer docs sequential ≈ **~4 s/doc**, NER-dominated → 500 docs ≈ **~34 min**, a single unattended weekly batch. Throughput target already met with zero new perf work.
- Document-level parallelism was implemented and **reverted**: it caused a **~2× regression** (ONNX thread oversubscription — N sessions × all-core intra-op threads). *Do not retry parallel ingest without true batched inference (§B).*
- Resumable/idempotent ingest already shipped (PR #14: deterministic `doc_id` + skip-already-ingested + orphan delete).

The real, unsolved risks: (a) models are fetched from HuggingFace at runtime — fatal for a sealed non-dev machine; (b) the mixed corpus includes scanned PDFs (OCR cliff); (c) the 12 GB envelope under a dual ONNX/candle runtime; (d) robustness for a weekly batch the operator drives **only through MCP** (no terminal/GUI).

## 2. Pipeline assessment (per stage)

Flow: `ingest_folder → ingest_one`: kreuzberg extract → per-chunk [gliner2-fastino NER → cloakpipe vault pseudonymize] → batched candle e5 embed → LanceDB `merge_insert` upsert → write `.anon.md`; then `maybe_build_index` (IVF) + `maybe_build_fts_index` once.

| Stage | Cost | RAM | Failure modes | Verdict |
|---|---|---|---|---|
| kreuzberg extract (text-layer) | ~10–100 ms/doc | low | corrupt/encrypted/zero-byte files | **OK** |
| kreuzberg extract (scanned → OCR) | **1–5 s/page** | medium | no text layer, huge scans | **Cliff — §3** |
| gliner2-fastino NER | ~50–300 ms/chunk; **dominant** | gliner2 ONNX resident + ort arena | mutex-serialized, single-text only (§B) | **OK at target; not scalable (§B)** |
| vault pseudonymize | cheap | low | vault lock/corruption (fatal — by design) | **OK** |
| candle e5 embed | batched/doc, ~tens ms | ~470 MB resident | F16 NaN (known; F32 default) | **OK (already batched)** |
| LanceDB upsert + index | amortized; one-time index build | working set, disk-backed | concurrent upsert (proven safe this session) | **OK** |

Headline: NER is the cost center but **structurally single-text** (§B); everything else is sound for the target. No re-architecture warranted (approach C rejected — solves non-problems).

## 3. OCR strategy (mixed/unknown corpus — must handle both)

The corpus is mixed and unknowns must be handled robustly. Design OCR as a **first-class gated path**, not an afterthought:

1. **Detection**: after extraction, classify a doc as "text-layer present" vs "needs OCR" (kreuzberg signals / empty-text heuristic / page-image ratio).
2. **Two-lane pipeline**: text-layer docs take the fast lane unchanged. Scanned docs route to a **separate, bounded OCR lane** with its own per-class time budget.
3. **Graceful degradation under a scan-heavy week**: OCR docs that exceed the batch budget are **deferred to a queue** (resumable via PR #14's idempotency), not allowed to stall the whole run. The MCP status resource (§6) reports `text_done / ocr_done / ocr_deferred`.
4. **OCR engine**: keep the existing tesseract-fork path as baseline; the OCR model/engine is an **optional, on-consent fetch** (per §4 hybrid delivery) — not bundled into the base installer.

This makes a scan-heavy week *slow but correct and resumable* rather than a hang.

## 4. Model distribution & packaging (hybrid delivery, 4 targets)

**Today (the blocker)**: `Detector::new` → `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` and `Embedder::load` → `hf_hub::api::tokio::Api` both fetch from HuggingFace at first run. There is **no packaging config** (no cargo-dist/bundle/wix/tauri, no `[workspace.metadata]`). A non-dev on a sealed/offline/corp machine cannot bootstrap.

**Chosen strategy — Hybrid**:
- **Bundle the mandatory core** (gliner2-multi-v1 NER + e5 embedder) into the app payload (installer-embedded or a paired data archive resolved via a deterministic local path; loader changed to prefer the bundled path, HF only as explicit fallback). Guarantees offline first-run.
- **Fetch optional/heavy models on explicit consent** surfaced through the MCP layer (OCR engine model, any larger reranker). No silent network.
- **License/redistribution gate** (blocking sub-task): confirm gliner2-multi-v1 (SemplificaAI, Apache-2.0 component; verify the *model weights* license) and e5 redistribution terms permit bundling. If a model can't be redistributed → it moves to the on-consent-fetch tier.

**Targets** (all four, per user): Windows x86_64 (.exe/.msi), macOS arm64 (.dmg), macOS x86_64 (.dmg), Linux x86_64 (AppImage/.deb).
- Build reality: `ort` (ONNX Runtime) + `candle` per target; the existing Windows `/MD` CRT + `/NODEFAULTLIB:libcmt` workaround in `.cargo/config.toml` must travel into the packaging build.
- Tooling: evaluate **cargo-dist** (multi-target CI release artifacts, the best fit for a 4-target matrix) vs cargo-bundle/cargo-wix (per-OS) vs hand-rolled. macOS needs **codesign + notarization** (Gatekeeper) — a real, non-optional sub-task for a distributable .dmg.
- The app *is* `anno-rag-mcp` (stdio MCP server, already split out in PR #9) wrapped per-OS; `anno-rag-bin` remains the CLI.

## 5. RAM budget (12 GB floor)

Resident accounting (approximate, to be measured per target as a packaging sub-task): candle e5 (~470 MB F32) + gliner2 ONNX weights + ort arena/activations + LanceDB working set (disk-backed, modest) + cloakpipe vault. The crate's documented default-build cap is < 1.5 GB RSS; the **packaged footprint with NER loaded is higher and must be measured**, not assumed.

Guardrails (mostly already true; make them explicit and enforced):
- Lazy model load (already `OnceCell`), no parallel NER engines (the A″ anti-pattern — keep it reverted), bounded per-doc buffers (chunks+vectors dropped per doc), single ORT session (no oversubscription).
- 12 GB has comfortable headroom for the *single-engine* design; the only way to blow it is re-introducing an engine pool — explicitly forbidden in §B.

## 6. Robustness for MCP-only operation

Operator never touches a terminal/GUI; Claude/Cowork drives everything via the `anno-rag-mcp` stdio server.
- **Background batch trigger**: an MCP tool kicks the weekly ~500-doc `ingest_folder`; returns immediately.
- **Status as an MCP resource**: `review://ingest/status`-style resource exposing `{total, text_done, ocr_done, ocr_deferred, skipped, failed[]}` so Claude can report progress/partial failures without re-emitting tool calls.
- **Resumability**: PR #14 already makes re-runs skip done files and never duplicate — interrupted/weekly re-runs resume the remainder. Crash/restart safe.
- **Partial-failure surfacing**: per-file errors (locked file, corrupt doc, OCR timeout) are collected and returned, not fatal to the batch.

## §B. True batched NER (A′) — documented technical deep-dive

**This section is the requested real, evidence-backed overview. It is NOT in scope for any phase below — it is the documented "only future lever," with the honest reason it is deferred.**

### B.1 What gliner2-fastino actually is (codebase ground truth)
An **8-stage fused ONNX pipeline**, each stage an `Arc<Mutex<ort::session::Session>>`: `encoder → token_gather → span_rep → schema_gather → count_pred_argmax → count_lstm_fixed → scorer → classifier` (`crates/anno/src/backends/gliner2_fastino/sessions.rs`). The single-text assumption is **baked in**: the encoder builds `Array2::from_shape_vec((1, seq_len), …)` for `input_ids`/`attention_mask` and an explicit guard rejects anything but `[1, L, H]` (`shape[0] != 1 → error`, `pipeline.rs`). Both "batch" APIs (`batch_extract_with_schema_mode`, `batch_extract_streaming`, `mod.rs`) are **confirmed sequential loops** over `extract_ner(text)` — documented in-code as *"the 'batch' refers to API ergonomics, not parallel inference."*

### B.2 Provenance & the decisive upstream fact
anno's backend is a faithful vendored **adaptation of `SemplificaAI/gliner2-rs`** (`rust_component/src/lib_v2.rs`, `processor.rs`, Apache-2.0 — cited in-code). Research finding: **upstream `gliner2-rs` is itself single-text-only** — no batch parameter in `Gliner2Config`, benchmarks reported "per sentence," loop-based; the fused-V2 export hardcodes `[batch, num_words, 8, hidden_size]` as *internal* computation, **not user-facing batched inference**. The GLiNER2 model architecture *does* support batching (the Python/Ray-Serve reference stack does dynamic, power-of-two batching), but **the authors' own Rust component never exposed it**, and anno inherited that.

### B.3 The honest option space
- **"Port upstream batching"** — *does not exist upstream; nothing to port.*
- **B1 — DIY true batching in anno.** Re-export `gliner2-multi-v1` ONNX with a dynamic batch axis, then rewrite all 8 stages to `[N,L,*]` with right-padding + per-row attention masks, batch-aware `token_gather`/`schema_gather`, and the **fixed-shape `count_lstm_fixed` LSTM** (variable-length batched LSTM is the hardest piece), removing the `[1,L,H]` guard. Strongest signal it is hard/low-ROI: **the model authors themselves did not attempt it**, and the fused-V2 export is the *hardest possible variant* to hand-batch.
- **B2 — Adopt a maintained batched Rust GLiNER engine** (`fbilhaut/gline-rs`: ground-up Rust, span/token variants, native batched `inference()`). Lower engineering risk, but it targets **classic GLiNER**, not GLiNER2 multi-task — a model + FR-legal-quality swap requiring an eval gate vs. current output.

### B.4 Verdict
True batched NER is a genuine **research project** (new ONNX export + 8-stage batch rewrite incl. an LSTM), explicitly **not attempted by upstream**, and **not justified at 500–1000 docs/week** (throughput already met sequentially). Revisit only at ~**10× volume**; if pursued, **evaluate B2 first** (lower risk than hand-batching the fused 8-session export), gated on a French-legal NER-quality eval.

**Citations**: [fastino-ai/GLiNER2](https://github.com/fastino-ai/GLiNER2) · [GLiNER2 paper arXiv 2507.18546](https://arxiv.org/pdf/2507.18546) · [SemplificaAI/gliner2-rs](https://github.com/SemplificaAI/gliner2-rs) · [urchade/GLiNER (ONNX convert tooling)](https://github.com/urchade/GLiNER) · [fbilhaut/gline-rs](https://github.com/fbilhaut/gline-rs) ([crates.io](https://crates.io/crates/gline-rs) · [docs.rs](https://docs.rs/gline-rs/latest/gliner/)) · [Knowledgator/GLiNER.cpp](https://github.com/Knowledgator/GLiNER.cpp) · [talmago/fast_gliner](https://github.com/talmago/fast_gliner).

## 7. Risks & explicit non-goals

- **Non-goal**: raw throughput speedup / parallel ingest — measured-harmful (2×), reverted, forbidden without §B.
- **Non-goal**: §B (true batched NER) — documented, deferred to 10× volume.
- **Non-goal**: GUI app — MCP-only operator model.
- **Risk**: model redistribution licensing for bundling (§4) — blocking gate before Phase 2.
- **Risk**: scan-heavy weeks exceed a single batch window — mitigated by §3 deferred queue + §6 resumability, not eliminated.
- **Risk**: macOS notarization friction — real Phase 2 sub-task, not optional.

## 8. Phased roadmap (each phase → its own spec → plan)

- **Phase 1 — OCR gating + RAM guardrails** (in-codebase, fully testable; no packaging deps): scanned-vs-text detection, two-lane pipeline, OCR deferred queue, enforced single-engine/bounded-buffer RAM guardrails + a measured RSS test. *Highest value, lowest risk, unblocks correctness for the mixed corpus.*
- **Phase 2 — Bundled-model packaging & 4-target installers**: license gate → loader prefers bundled path → cargo-dist (or chosen tooling) 4-target artifacts → macOS codesign/notarization → on-consent optional-model fetch. *The actual ship blocker.*
- **Phase 3 — MCP-only robustness**: background ingest trigger tool, `ingest/status` MCP resource, partial-failure surfacing, weekly-batch operator runbook. *Makes it operable by a non-dev via Claude alone.*

Phases are ordered by value/independence: Phase 1 is pure-codebase and de-risks the corpus reality; Phase 2 is the distribution blocker; Phase 3 is operability polish. §B stays a documented future lever, revisited only on a 10× volume trigger.
