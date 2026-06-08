# Anno OCR Pipeline Optimization

## Goal

Modernize Anno's OCR pipeline by removing dead legacy code, leveraging kreuzberg's built-in caching, and adding PaddleOCR as an automatic quality-based fallback behind Tesseract — all without writing custom OCR infrastructure.

## Architecture

Anno delegates all document extraction and OCR to **kreuzberg 4.9.7**. The current pipeline in `ingest.rs` already has a smart 3-stage flow: native extraction (pdfium) → document classification → embedded OCR for scanned pages only. This spec tightens that pipeline by removing the orphaned legacy `ocr.rs` shell-out path, confirming kreuzberg's extraction cache covers the OCR path, and activating kreuzberg's built-in multi-backend OCR pipeline (Tesseract → PaddleOCR fallback) via a new feature flag.

## Tech Stack

- **kreuzberg 4.9.7** — extraction + OCR orchestration (already a dependency)
- **kreuzberg `ocr` feature** — Tesseract integration (already gated behind `embedded-ocr`)
- **kreuzberg `paddle-ocr` feature** — PaddleOCR via ONNX Runtime (new, opt-in)
- **ort** — ONNX Runtime (already an optional dependency for `rerank` feature)

---

## Phase 1: Legacy Cleanup

### Problem

`crates/anno-rag/src/ocr.rs` defines `ocr_pdf()` which shells out to system `tesseract` with a 60s timeout. This function is **never called** — zero callers in the entire codebase. It is dead code left over from before the kreuzberg embedded OCR path was added.

Additionally, `AnnoRagConfig` carries two legacy fields:
- `enable_ocr: bool` — mapped to `OcrMode::AutoEmbedded` via `effective_ocr_mode()`
- `tesseract_path: Option<PathBuf>` — never used by the embedded OCR path

The empty feature flag `ocr = []` in `anno-rag/Cargo.toml` gates nothing.

### Changes

1. **Delete** `crates/anno-rag/src/ocr.rs`
2. **Remove** `pub(crate) mod ocr;` from `crates/anno-rag/src/lib.rs`
3. **Remove** `ocr = []` feature from `crates/anno-rag/Cargo.toml`
4. **Deprecate** `enable_ocr` and `tesseract_path` in `AnnoRagConfig`:
   - Keep both fields for serde compatibility (existing configs must still parse)
   - `effective_ocr_mode()` logs `tracing::warn!("config field 'enable_ocr' is deprecated; use 'ocr_mode: auto_embedded' instead")` when the legacy mapping fires
   - `tesseract_path` log `tracing::warn!("config field 'tesseract_path' is deprecated and ignored; embedded OCR manages its own Tesseract binary")` in a new `warn_deprecated_fields()` method called at config load
5. **Update CLI** (`anno-rag-bin/src/main.rs`):
   - The `--enable-ocr` flag stays functional but logs the same deprecation warning
   - Add `--ocr-mode <off|auto_embedded>` as the canonical replacement (not yet present — currently only `--enable-ocr` exists as a CLI flag)

### Testing

- Existing config tests (`legacy_enable_ocr_maps_to_auto_embedded`, `ocr_mode_round_trips_as_snake_case`) continue to pass
- New test: `deprecated_fields_still_parse` — deserialize a config JSON with `enable_ocr: true` and `tesseract_path: "/usr/bin/tesseract"`, assert it parses without error
- Verify `cargo build` succeeds without the `ocr` feature (it was empty anyway)

---

## Phase 2: Kreuzberg Extraction Cache for OCR

### Problem

kreuzberg 4.9.7 has a built-in extraction cache (`blake3` file hash + config hash → `rmp_serde` serialized result, 30-day TTL, 2GB max). However, our `embedded_ocr_extract()` in `ingest.rs` does not explicitly confirm that caching is active for OCR calls.

### Current State

kreuzberg's `ExtractionConfig::default()` has `use_cache: true`. Our `embedded_ocr_extract()` creates a fresh `ExtractionConfig` without setting `use_cache` — so it inherits the default `true`. **This means kreuzberg already caches OCR results.**

### Changes

1. **Make caching explicit** in `embedded_ocr_extract()`:
   - Add `use_cache: true` to the `ExtractionConfig` struct literal (documentation-as-code — makes intent clear even though it's the default)
2. **Expose** `ocr_cache_enabled: bool` in `AnnoRagConfig` (default `true`):
   - When `false`, pass `use_cache: false` to the OCR extraction config
   - Use case: tests that need deterministic behavior, or debugging cache issues
3. **Log cache status** after OCR extraction:
   - `tracing::debug!("OCR extraction completed (cache_enabled={})", cfg.ocr_cache_enabled)`

### Testing

- New test: `ocr_cache_config_propagates_to_kreuzberg` — construct an `AnnoRagConfig` with `ocr_cache_enabled: false`, verify the resulting kreuzberg `ExtractionConfig` has `use_cache: false`
- Existing ingest tests continue to pass

---

## Phase 3: PaddleOCR Multi-Backend Pipeline

### Problem

Some scanned documents (especially older French legal PDFs with poor print quality, handwriting, or rotated pages) produce low-quality OCR with Tesseract alone. kreuzberg 4.9.7 already supports a **multi-backend OCR pipeline** with quality-based fallback: when the `paddle-ocr` feature is compiled in, `OcrConfig::effective_pipeline()` automatically constructs a 2-stage pipeline:

1. Tesseract (priority 100) — tried first
2. PaddleOCR (priority 50) — fallback if Tesseract quality score < 0.5

This is fully automatic — zero code changes needed in the extraction call. We just need to wire the feature flags.

### kreuzberg Pipeline Behavior

```
Document page → Tesseract OCR → quality score ≥ 0.5? → accept
                                                    ↓ no
                                 PaddleOCR OCR → accept (best effort)
```

Quality scoring uses `OcrQualityThresholds` (configurable): minimum non-whitespace characters, alphanumeric ratio, meaningful word count, fragmentation detection, consecutive repetition detection.

### PaddleOCR Model Details

- Models: detection (~5MB) + recognition (~5MB) + classifier (~1MB) — ~11MB total
- Downloaded on first use via `hf-hub` (same pattern as GLiNER2 weights)
- CPU-only by default; GPU via `ort` execution providers
- Supports French (latin script recognition model covers all latin-script languages)

### Changes

1. **Add feature flags**:
   - `anno-rag/Cargo.toml`: `ocr-paddle = ["kreuzberg/paddle-ocr", "kreuzberg/ocr"]`
   - `anno-rag-bin/Cargo.toml`: `ocr-paddle = ["anno-rag/ocr-paddle", "anno-rag-mcp/ocr-paddle"]`
   - `anno-rag-mcp/Cargo.toml`: `ocr-paddle = ["anno-rag/ocr-paddle"]`

2. **Enable `auto_rotate`** in `embedded_ocr_extract()`:
   - Set `auto_rotate: true` in the `OcrConfig` passed to kreuzberg
   - Benefit: handles rotated scanned pages (common in scanned French legal dossiers)
   - No feature gate needed — `auto_rotate` uses Tesseract's built-in orientation detection

3. **Expose `ocr_backend` config** in `AnnoRagConfig`:
   - `ocr_backend: Option<String>` (default `None` → kreuzberg default "tesseract")
   - When set, propagate to `OcrConfig.backend`
   - This lets users force `"paddleocr"` as primary instead of fallback

4. **CI matrix**:
   - Add job: `Embedded OCR (Tesseract + PaddleOCR)` with features `embedded-ocr,ocr-paddle`
   - This job runs the existing OCR tests to verify the pipeline compiles and the feature flags propagate correctly
   - No new test PDFs needed — the pipeline is tested by kreuzberg's own test suite

5. **Documentation**:
   - Update README OCR section with backend options
   - Document `ocr-paddle` feature in `anno-rag/Cargo.toml` feature doc comments

### Testing

- New test: `paddle_ocr_feature_creates_two_stage_pipeline` — when compiled with `ocr-paddle`, verify `OcrConfig::effective_pipeline()` returns 2 stages (this is a kreuzberg API test, not an integration test requiring actual PDFs)
- New test: `auto_rotate_enabled_in_ocr_config` — verify `embedded_ocr_extract` config has `auto_rotate: true`
- Existing embedded OCR tests continue to pass (they don't require `paddle-ocr`)

---

## Non-Goals

- **Custom OCR cache**: kreuzberg handles this natively. No `ocr_cache.rs` module.
- **Per-page parallelization**: kreuzberg handles this internally via its thread budget system.
- **VLM OCR**: kreuzberg supports `backend: "vlm"` for vision LLM OCR but this requires an external LLM API. Out of scope — evaluate in a future spec if Tesseract+PaddleOCR quality proves insufficient.
- **EasyOCR backend**: kreuzberg supports it but EasyOCR requires Python. Breaks local-first zero-dependency constraint.
- **Removing `enable_ocr`/`tesseract_path` entirely**: deferred to next major version (0.3.0). This spec deprecates with warnings only.

## File Impact Summary

| File | Action |
|------|--------|
| `crates/anno-rag/src/ocr.rs` | Delete |
| `crates/anno-rag/src/lib.rs` | Remove `mod ocr` |
| `crates/anno-rag/src/config.rs` | Deprecation warnings, add `ocr_cache_enabled`, `ocr_backend` |
| `crates/anno-rag/src/ingest.rs` | Explicit `use_cache`, `auto_rotate`, propagate `ocr_backend` |
| `crates/anno-rag/Cargo.toml` | Remove `ocr = []`, add `ocr-paddle` feature |
| `crates/anno-rag-bin/Cargo.toml` | Add `ocr-paddle` feature propagation |
| `crates/anno-rag-bin/src/main.rs` | Deprecation warning on `--enable-ocr` |
| `crates/anno-rag-mcp/Cargo.toml` | Add `ocr-paddle` feature propagation |
| `.github/workflows/ci.yml` | Add OCR+PaddleOCR CI job |

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| PaddleOCR models add ~11MB download on first use | Models cached by `hf-hub`; download is lazy (only when OCR actually fires on a scanned page) |
| `ort` version conflict with existing `rerank` feature | Both use workspace `ort`; `paddle-ocr` kreuzberg crate uses the same `ort` version |
| Breaking existing configs with `enable_ocr: true` | Configs continue to work; only log a deprecation warning |
| CI time increase from PaddleOCR job | PaddleOCR tests are compile-check only (no model download in CI) |
