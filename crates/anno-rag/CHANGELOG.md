# anno-rag Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased] — v0.6 hybrid retrieval + legal eval harness

### Added
- **Legal RAG eval harness** — ~60-document synthetic French legal corpus
  (`tests/fixtures/eval_corpus/`), ~40 graded concept queries
  (`queries.toml`), and `recall@10` / `nDCG@10` metrics in `anno_rag::eval`.
  `bench_eval` replaces `bench_recall`; CI gates both metrics against a
  committed `eval_baseline.toml` (98% tolerance).
- **Optional real eval corpus** via `ANNO_RAG_EVAL_CORPUS` — points the
  harness at an out-of-repo `{*.txt, queries.toml}` directory.

### Changed
- **Hybrid retrieval** — `search()` is now a hybrid vector + full-text
  query: a French-tokenized LanceDB FTS index (stemming + stop-words +
  lowercase) on `text_pseudo`, fused with the dense vector results via
  `RRFReranker`. Legal French is saturated with exact-match terms that
  dense embeddings retrieve poorly.
- **e5 prefix fix** — search queries are embedded with the e5 `"query: "`
  prefix (was `"passage: "`, the passage prefix). `Embedder::embed_query`
  is the new query path; `embed_batch` stays the passage path.
- `SearchHit.distance` (L2, lower = closer) is replaced by
  `SearchHit.score` (RRF relevance, higher = better). Minor breaking
  change; anno-rag is pre-1.0.

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
- **Embedder dtype configurable** via `embedder_dtype` (`"f32"` default, `"f16"` experimental opt-in). F16 halves embedder RSS (~236 MB) but the e5-small BERT forward pass can produce degenerate (NaN) vectors on CPU — which collapsed recall@10 to 0 — so F16 is opt-in until numerically stable. (#026)
- **Dropped bundled Tesseract** (~500 MB resident reduction). `kreuzberg` feature set no longer includes `ocr`. (#024)
- **Single-backend NER**: `StackedNER` chain replaced with `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` (multi-v1 ONNX). Drops bert-base-NER + gliner_small + NuNER_Zero from RAM (~900 MB saved). E2e name-scrub coverage restored to 12/12 (was ≥50% in v0.2). (#025)

### Performance budget (8 GB / 4 cores laptop target)
- Peak RSS goal: 3.95 GB → < 1.5 GB
- Cold-start goal: ~3s → < 2s
- Search p95 goal: < 200ms (warm path)
- Recall@10 gate: ≥ 95% of v0.4 fp32 baseline (CI-enforced)
- Peak RSS cumulative (T1+T2+T3+T4+T6, F32 embedder): 3.95 GB → ~1.08 GB estimated (-73%), well under the 1.5 GB hard cap. F16 (T5, opt-in) would shave a further ~236 MB once numerically stable.

### Migration notes
- Users who relied on auto-OCR for scanned PDFs must install system tesseract and pass `--enable-ocr`. See README.
- Users who see recall regression from F16 quantization can set `embedder_dtype = "f32"` in `~/.anno-rag/config.toml`.

## [0.2.0] — 2026-05-13

See `docs/superpowers/specs/2026-05-13-anno-rag-v0.2-cowork-minimum.md`. Cowork plugin minimum (MCP stdio + vector index threshold).

## [0.1.0] — 2026-05-12

Initial walking skeleton. See `docs/superpowers/specs/2026-05-12-anno-rag-v0.1-walking-skeleton.md`.
