# anno-rag Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased] — v0.5 performance budget

### Added
- **`anno-rag bench --corpus <dir>`** — reproducible SLO measurement subcommand emitting a markdown report (cold-start, idle/peak RSS, ingest throughput, search p50/p95).
- **Criterion bench harness** under `benches/` (cold-start, idle RSS, search, ingest, recall) with per-OS CI gates (idle RSS <200 MB, recall@10 ≥95%).
- **Optional OCR** via `--enable-ocr` runtime flag (or `enable_ocr = true` in config). Forks system `tesseract` for PDFs without a text layer. Install: `apt install tesseract-ocr tesseract-ocr-fra` / `brew install tesseract tesseract-lang` / `winget install UB-Mannheim.TesseractOCR`.
- **Expanded format support** via kreuzberg features `email`, `excel`, `xml`, `archives`, `tree-sitter`:
  - **Email**: `.eml` (RFC 5322), `.msg` (Outlook)
  - **Spreadsheets**: `.xlsx`, `.xls`, `.xlsb`, `.xlsm`, `.csv`, `.tsv`
  - **Data / markup**: `.xml`, `.json`, `.yaml`, `.toml`, `.rst`
  - **Archives**: `.zip`, `.tar`, `.gz`, `.bz2`, `.xz`, `.7z` (kreuzberg recurses into the archive)
  - **Code source** (texte brut, sans parsing sémantique): `.rs`, `.py`, `.js`, `.ts`, `.java`, `.c`, `.cpp`, `.cs`, `.go`, `.rb`, `.php`, `.swift`, `.kt`, `.scala`, `.sql`
  - Bundled (sans feature): `.rtf`, `.epub`, `.htm`

### Changed
- **Lazy-init `Embedder` and `Detector`** via `tokio::sync::OnceCell`. `Pipeline::new` now constructs in <200 MB RSS; models load on first `ingest`/`search`/`detect` call. (#023)
- **Embedder default dtype: `F16`** (was `F32`). Override with `embedder_dtype = "f32"` in `~/.anno-rag/config.toml` if recall regresses. (#026)
- **Dropped bundled Tesseract** (~500 MB resident reduction). `kreuzberg` feature set no longer includes `ocr`. (#024)
- **Single-backend NER**: `StackedNER` chain replaced with `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` (multi-v1 ONNX). Drops bert-base-NER + gliner_small + NuNER_Zero from RAM (~900 MB saved). E2e name-scrub coverage restored to 12/12 (was ≥50% in v0.2). (#025)

### Performance budget (8 GB / 4 cores laptop target)
- Peak RSS goal: 3.95 GB → < 1.5 GB
- Cold-start goal: ~3s → < 2s
- Search p95 goal: < 200ms (warm path)
- Recall@10 gate: ≥ 95% of v0.4 fp32 baseline (CI-enforced)
- Peak RSS cumulative (T1+T2+T3+T4+T5+T6): 3.95 GB → ~850 MB estimated (-78%)

### Migration notes
- Users who relied on auto-OCR for scanned PDFs must install system tesseract and pass `--enable-ocr`. See README.
- Users who see recall regression from F16 quantization can set `embedder_dtype = "f32"` in `~/.anno-rag/config.toml`.

## [0.2.0] — 2026-05-13

See `docs/superpowers/specs/2026-05-13-anno-rag-v0.2-cowork-minimum.md`. Cowork plugin minimum (MCP stdio + vector index threshold).

## [0.1.0] — 2026-05-12

Initial walking skeleton. See `docs/superpowers/specs/2026-05-12-anno-rag-v0.1-walking-skeleton.md`.
