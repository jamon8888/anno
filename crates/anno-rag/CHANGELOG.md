# anno-rag Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### v0.4 GDPR core — rights of data subjects + persistent audit

### Added
- **`Pipeline::forget(subject_ref)`** — Art. 17 erasure. Idempotent;
  returns `ErasureReceipt { subject_ref, mappings_removed, token,
  category, executed_at }`. Persists the vault on every removal.
- **`Pipeline::find_subject(subject_ref)`** — Art. 15 access. Returns
  `SubjectMatches` (currently 0 or 1, vec-shaped for future fuzzy match).
- **`Pipeline::export_subject(subject_ref, ExportFormat::{Json,Csv})`** —
  Art. 20 portability. Returns the matches as machine-readable bytes.
- **`Vault::forget` / `Vault::find_subject`** — async-locked wrappers
  over the new cloakpipe primitives (`Vault::remove`, `Vault::find` in
  `cloakpipe-core::vault`). Counters stay monotonic on removal so token
  ids never collide with retired ones.
- **`Error::Audit`** variant — surfaces export-serialisation failures.

### v0.7 — anonymization eval + FR honorific Person regex + email detection

### Added
- **Email detection in `detect_patterns`** — pragmatic RFC-5321-ish regex
  on the `FrPatterns` pack, emitted as `EntityCategory::Email`. Contract-
  style addresses; quoted local parts out of scope.
- **`pub fn detect_patterns(text)`** — model-free regex pipeline
  (NIR / SIRET+Luhn / IBAN-FR / phone-FR / email / FR honorific Person)
  extracted from `Detector::detect` so callers that must not pay the
  GLiNER2 model load can score the regex categories alone.
- **FR honorific Person regex** — closes the 14/59 Person gap
  GLiNER2-Fastino exhibits on names introduced by `Monsieur`/`Madame`/
  `Mademoiselle`/`Mme`/`Mlle`/`M.`/`Maître`/`Me`. Capture group 1 is the
  name (2+ capitalised words; accents, hyphens, apostrophes); the
  honorific itself stays unredacted. Lowercase function words
  (`le`, `la`) between honorific and noun block role titles like
  `Monsieur le Président` from being mistaken for a person.
- **`pii_eval` module** — pure `score_detections(detected, truth)` with
  per-category precision/recall/F1 and overlap-span matching (greedy
  left-to-right so one wide detection cannot inflate TP across multiple
  truths). Categories use canonical string keys via `category_key`.
- **PII annotation loader** — `PiiAnnotations`/`PiiDoc`/`PiiEntry` serde
  shapes for `pii_annotations.toml`; `pii_corpus_dir`,
  `load_pii_annotations`, `resolve_span`, `check_pii_corpus`. The
  consistency checker enforces every annotated `text` occurs exactly
  once in its document so search-based offset resolution is unambiguous.
- **35-doc annotated French legal PII corpus** —
  `crates/anno-rag/tests/fixtures/pii_corpus/` covers five contract
  families (prestation, CDI, mise en demeure, bail, statuts) × 7 docs
  each, with 309 annotations across 8 categories. Precision traps left
  un-annotated (`le Prestataire`, `Tribunal de commerce`, form names).
- **Model-free regex eval tier** —
  `crates/anno-rag/tests/pii_regex.rs` aggregates TP/FP/FN over the full
  corpus and hard-gates per-category recall against `pii_baseline.toml`
  at 0.98 tolerance. Runs on every CI build (no model needed).
- **Model-requiring NER eval tier** —
  `crates/anno-rag/tests/pii_ner.rs` (`#[ignore]`'d, opt-in via
  `--ignored`) does a single-pass `Detector::detect` over the corpus
  and gates Person/Organization/Location recall. A sibling
  `diagnose_ner_misses` test prints every FN/FP with 20 chars of context
  for debugging (char-boundary-safe for accented French).
- **CI wiring** in `.github/workflows/bench.yml` runs both gates after
  the HF model cache is warmed by `warmup_model`.

### Changed
- **Widened FR phone regex** — the old
  `\b(?:\+33[\s\.\-]?|0)[1-9](?:[\s\.\-]?\d{2}){4}\b` silently dropped
  9/35 `+33 …` phones because Rust's `regex` has no lookbehind and `\b\+`
  never matches before a `+` at start-of-line or after whitespace. New
  pattern keeps `\b` on the domestic `0` branch and at the tail; the
  `+33` literal itself is the structural left guard for the
  international branch:
  `(?:\+33[\s\.\-]?[1-9]|\b0[1-9])(?:[\s\.\-]?\d{2}){4}\b`
- **Luhn-valid corpus SIRETs** — 32 of 35 synthetic corpus SIRETs failed
  `detect::luhn` and were correctly rejected, pinning SIRET recall at
  0.0857. Regenerated each with a Luhn-valid check digit while keeping
  the first 13 digits and uniqueness across the corpus.

### Measured baselines (35-doc FR legal corpus, 309 annotations)
| Category    | Recall | Precision | Truths |
|-------------|-------:|----------:|-------:|
| NIR         | 1.0000 | 1.0000    | 25     |
| SIRET       | 1.0000 | 1.0000    | 35     |
| IBAN_FR     | 1.0000 | 1.0000    | 35     |
| PhoneNumber | 1.0000 | 1.0000    | 35     |
| Email       | 1.0000 | 1.0000    | 35     |
| Person      | 1.0000 | 1.0000    | 59     |
| Organization| 1.0000 | 1.0000    | 42     |
| Location    | 0.9302 | 0.9756    | 43     |

Location residual: 3 missed (Chambéry, La Baule, Saint-Nazaire) +
1 FP ("location de navires de plaisance" — multilingual NER false
friend reading `location` as English). Locked at 0.93 baseline; v0.8
candidate fix is a FR location pattern or a multilingual-NER post-filter.

### v0.6 — hybrid retrieval + legal eval harness

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

### v0.5 — performance budget

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
