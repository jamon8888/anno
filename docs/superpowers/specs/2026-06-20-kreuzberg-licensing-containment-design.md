# Kreuzberg License Migration — MIT Codebase

**Date:** 2026-06-20
**Revised:** 2026-06-20
**Status:** RESOLVED — migrated to kreuzberg 4.7.4 (MIT); zero ELv2 in dependency graph
**Spec ID:** Spec A (of a two-spec effort; Spec B = VLM-OCR, separate doc)

## Decision

**Downgrade `kreuzberg = "=4.9.7"` (Elastic-2.0) → `"=4.7.4"` (MIT).**

Anno's codebase is and must remain fully permissively licensed. 4.7.4 is the last
MIT release; 4.8.0 introduced ELv2 alongside a new LLM layer. The downgrade removes
the only non-permissive dependency in the graph — no deny.toml exception, no
containment plan, no trigger condition to monitor.

The previous version of this spec (before 2026-06-20 revision) took the opposite
position: contain the ELv2 risk and stay on 4.9.7. That analysis documented several
regressions as blockers for downgrading. Investigation revealed those blockers were
overstated or mitigatable (see §"Re-evaluation" below). The full-MIT requirement
takes precedence.

## What anno uses from kreuzberg

Two call sites:

| File | API used |
|---|---|
| `crates/anno-rag/src/ingest.rs` | `kreuzberg::extract_file`, `ExtractionConfig`, `OcrConfig`, `OutputFormat::ElementBased`, `ExtractionResult`, `PageContent`, `Chunk`, `ChunkType`, `HeadingContext` |
| `crates/anno-privacy-gateway/src/document_extract.rs` | `kreuzberg::core::config::ExtractionConfig`, `kreuzberg::extract_file` |

All of these exist identically in 4.7.4. `OutputFormat::ElementBased` was present
before 4.8.0 — the prior "likely compile break" claim was not verified and is false
(confirmed against `crates/kreuzberg/src/core/config/extraction/core.rs@v4.7.4`).

The features anno enables (`pdf`, `bundled-pdfium`, `office`, `html`, `email`,
`excel`, `xml`, `archives`, `tokio-runtime`, `chunking`) all exist in 4.7.4.

## Re-evaluation of the previous blocking regressions

The prior spec listed these as blockers. Here is the reassessment:

| Regression | Version | Reassessment |
|---|---|---|
| PDF table extraction SF1 15.5% → 53.7% | 4.8.0 | Real quality drop. Mitigated by VLM-OCR (Spec B), which targets exactly the scanned/complex-layout pages where tables regress most. Tracked as known acceptable trade-off. |
| Multi-byte UTF-8 panic on French text | 4.8.1, 4.8.4 | **Must test.** Add a fixture test with accented French content before shipping. If panics occur, apply upstream patch or add a UTF-8 sanitisation pass in ingest. |
| ~1000× slowdown on Ghostscript PDFs | 4.9.0 | Performance regression, not a correctness one. Add a per-document timeout in `extract_file` calls if this manifests in practice. |
| Tesseract C++ exception crash (FFI unwind) | 4.8.0 | **Must test.** Exercise the OCR path on a fixture set; catch panics at the ingest boundary. |
| Image-decode 64MP pixel cap (DoS protection) | 4.9.6 | **Security — requires mitigation.** `document_extract.rs` in `anno-privacy-gateway` processes untrusted uploads without this cap. Mitigation: add an explicit file-size limit and image-dimension cap in `extract_uploaded_document` before passing bytes to kreuzberg. Do not ship without this. |
| PDF heading detection 40.7% → 43.7% | 4.8.0 | Minor; acceptable. |
| Email PST + DOCX fixes | 4.9.x | Correctness regressions in edge cases. Test against anno's email fixture set. |
| `ElementBased` compile break | 4.8.0 | **Not real.** `ElementBased` exists in 4.7.4 source. |

## Required mitigations (must ship with the downgrade)

These are not optional — they address a security regression in the 4.7.4 downgrade.

### M1 — Upload file-size and image-dimension cap (SECURITY)

`anno-privacy-gateway/src/document_extract.rs:extract_uploaded_document` passes
untrusted bytes to `kreuzberg::extract_file` with no size guard. Kreuzberg 4.9.6
added an internal 64MP pixel cap and a decompression-bomb limit; 4.7.4 has neither.

**Add before calling `extract_with_kreuzberg`:**

```rust
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;   // 50 MB
if bytes.len() > MAX_UPLOAD_BYTES {
    return Err(Error::Privacy(format!(
        "uploaded document exceeds {} MB limit", MAX_UPLOAD_BYTES / 1_048_576
    )));
}
```

Image-dimension capping (against image-file uploads) requires inspecting the
image header before decode — use the `image` crate's `image::io::Reader` in
`guess_format` + `into_dimensions()` (no full decode) to reject files where
`width * height > 64_000_000` pixels.

### M2 — UTF-8 and OCR crash fixtures

Run the existing French legal fixture set through `embedded_ocr_extract` after
the version bump. Any panic = must fix before merge (upstream patch or input sanitisation).

### M3 — Per-document extraction timeout

Guard `extract_file` calls with a `tokio::time::timeout` (suggested: 120 s for OCR,
30 s for native extraction) to prevent Ghostscript-PDF-induced hangs from blocking
the ingest queue.

## Escape plan (A3) — retained for reference, no longer the primary plan

If a future kreuzberg version fixes the regressions above under a permissive license,
upgrading is preferable to maintaining the mitigations indefinitely. Until then, the
permissive sub-crate stack documented in the original Spec A remains the fallback if
4.7.4 proves insufficient:

- PDF: `pdfium-render` (MIT) directly
- OCR: `kreuzberg-tesseract` (MIT fork) directly
- Tables: `kreuzberg-paddle-ocr` (MIT fork) directly

These sub-crates are already transitive deps of kreuzberg 4.7.4 and are present in
the workspace-hack.

## Acceptance

- `kreuzberg` in `Cargo.toml` (workspace) reads `= "=4.7.4"`.
- `deny.toml` contains no `Elastic-2.0` allow entry.
- Mitigations M1, M2, M3 are implemented and tested.
- `cargo deny check licenses` passes clean.
