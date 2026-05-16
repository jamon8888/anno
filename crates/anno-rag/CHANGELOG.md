# anno-rag Changelog

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### v0.8 anno-memory v0.2 — bi-temporal + entity-graph

### Added
- **`canonicalize` module** (T1) — deterministic entity canonicalisation:
  lowercase → NFD diacritic strip → ASCII-punct strip → whitespace
  collapse → per-tenant alias table lookup. Output format `ent:TAG:value`
  for NER entities, `pii:LABEL:TOKEN` for vault references.
- **`Pipeline::extract_entities`** (T2) — merges the vault TokenRefs
  from `pseudonymize_with_refs` with non-PII NER from `anno::StackedNER`.
  Filters: confidence ≥ 0.6; Person dropped (vault path only). Now
  populates `Memory::entity_refs` on every `save_memory` (was empty in
  v0.1). Failures non-fatal — log via tracing, return vault tokens only.
- **Bi-temporal recall** (T3) — `Pipeline::recall_memory` gains
  `as_of: Option<DateTime<Utc>>` + `graph_expand: bool` parameters.
  Filter: `valid_from <= t AND (valid_to IS NULL OR valid_to > t)`,
  defaulting to "now". `MemoryHitRow` + `MemoryHit` carry
  `valid_from` / `valid_to` / `entity_refs` / `via` (HitProvenance).
  `Pipeline::invalidate_memory(id, at?) -> Result<bool>` sets
  `valid_to`; idempotent via `valid_to IS NULL` guard.
- **`conflict` module** (T4) — `cosine_sim`, `shares_any`,
  `resolves_conflict(new, prior, threshold)`. Five guards: same kind,
  kind in `{Preference, Reference}`, `prior.valid_to IS NULL`, shared
  entity_ref, cosine ≥ threshold. `save_memory` runs the resolver
  before insert; invalidated ids returned in `SavedMemory.invalidated_ids`.
  Fact and Context stay append-only by design. Config:
  `conflict_cosine_threshold` (default 0.85).
- **`Pipeline::graph_recall(seed, max_hops, per_hop_limit, as_of)`** (T5) —
  BFS over `entity_refs` using the LabelList scalar index from v0.1.
  Returns `GraphRecallResult { seed, seed_resolved, nodes, edges, memories }`
  with `HitProvenance::GraphExpand` on every memory. Bi-temporal threading
  via the `as_of` arg. Config: `graph_max_hops` (default 2),
  `graph_per_hop_limit` (default 50).
- **`Vault::lookup_blocking`** — non-async best-effort reverse lookup
  via `tokio::sync::Mutex::try_lock`; returns `None` on contention.
  Used by `graph_recall` to populate node `display` strings without
  threading `.await` through the BFS.
- **MCP wiring** (T5/T6/T7):
  - `memory_recall` gains `as_of` + `graph_expand`.
  - `memory_graph_recall(entity, max_hops, per_hop_limit, as_of)` —
    new tool returning the full subgraph.
  - `memory_invalidate(id, at?)` — new tool; reports `invalidated`
    + the `valid_to` set; emits `result='noop'` on idempotent re-call.
  - All three tools audit at `target = "anno_rag::memory::audit"`
    with structured fields.
- **Property test** (T8) — `graph_recall_is_monotonic_in_hops`
  (#[ignore]'d, 25 cases). 2 hops never returns fewer memories or
  nodes than 1 hop over the same planted graph.

### Changed
- `MemoryHitRow` gains `session_id`, `valid_from_us`, `valid_to_us`,
  `entity_refs` fields. `MemoryHit` gains `valid_from`, `valid_to`,
  `entity_refs`, `via`.
- `SavedMemory` gains `entity_refs`, `invalidated_ids`.
- `AnnoRagConfig` gains `entity_aliases`, `conflict_cosine_threshold`,
  `graph_max_hops`, `graph_per_hop_limit`.

### Deferred
- **Task 9 LoCoMo multi-hop accuracy gate** — depends on the still-
  deferred v0.1 LoCoMo subset (T13). v0.2 ships without the eval gate
  per the same no-CI-gate carve-out v0.1 took.

### v0.7 anno-memory v0.1 — PII-safe session memory

### Added
- **`memory` module** (`crates/anno-rag/src/memory.rs`) — types: `Memory`,
  `MemoryId` (v7 UUID, time-sortable), `MemoryKind` (Fact/Preference/
  Reference/Context), `TokenRef`, `MemoryHit`, `MemoryHitRow`.
- **`memories` LanceDB collection** with 11 columns — 7 active v0.1
  (id, session_id, kind, text, created_at, accessed_at, embedding,
  token_refs) + 3 forward-compat for v0.2 (valid_from, valid_to,
  entity_refs).
- **`Store::memory_insert / memory_get / memory_delete_by_id`** — CRUD over
  the second LanceDB collection. Arrow batch helpers handle the
  `List<Struct{label, token}>` for `token_refs`.
- **Scalar indexes** — `setup_memory_indexes` creates BTree on
  `created_at` + `session_id`, Bitmap on `kind`, LabelList on
  `token_refs` + `entity_refs`. Idempotent; requires ≥1 row.
- **Hybrid search over memories** — `Store::build_memories_fts_index`
  (French-tokenized FTS on `text`) + `Store::memories_hybrid_search`
  (dense vector + FTS via `Arc<RRFReranker>`), mirroring the chunks
  retrieval pattern v0.6 landed.
- **`Pipeline::save_memory(text, kind?, session_id?)`** — detect →
  pseudonymize → embed → persist. The on-disk text is ALWAYS the
  tokenized form. Returns `SavedMemory { id, redacted_text, token_refs }`.
- **`Pipeline::recall_memory(query, top_k, session_id?, kinds?)`** —
  hybrid search with 2× oversample, kind+session filter, vault
  rehydrate at the boundary.
- **`Pipeline::forget_memory(id | query, limit, dry_run)`** —
  RGPD Art. 17 erasure with vault-token cascade. Reuses the v0.4
  `Vault::forget` primitive; vault entries are purged only when the
  token's `Store::token_reference_count` drops to zero.
- **`Pipeline::list_memories(session_id?, kind?, limit, cursor?)`** —
  cursor-paginated list ordered by `created_at` DESC.
- **`Pipeline::compact_now` + `spawn_compaction_task`** — daily
  `Table::optimize(All)` to satisfy the 24h physical-erasure SLO.
- **MCP tools** — `memory_save / memory_recall / memory_forget /
  memory_list` on `AnnoRagServer`, each emitting a structured
  `tracing::info!` audit event at `target = "anno_rag::memory::audit"`
  with `tool`, `result`, `duration_ms`, and (where applicable) row
  counts. Deployers can pipe that target to the Art. 30 audit sink.
- **Property test** — `tests/memory_proptest.rs` (50 cases,
  `#[ignore]`) verifies the save/forget invariant: ≤1 row per forget,
  no panic, no store corruption.
- **Vault::pseudonymize_with_refs** — returns the
  `(text, Vec<TokenRef>)` pair needed by `save_memory` for the
  cascade payload.
- **`Error::Memory`** variant for bad-argument paths in the memory
  layer.

### Changed
- `AnnoRagConfig` gains `memory_collection_name`, `memory_embedding_dim`,
  `compaction_interval_secs`, `compaction_min_age_secs`.
- `Store::open` opens both the `chunks` and `memories` tables via
  the new `open_or_create_table` helper.

### Deferred
- **LoCoMo subset eval baseline** (Task 13 of the plan) — `bench_locomo`
  + 50-item conversation/question fixture + recorded `accuracy@1` /
  `latency_p95_ms` baseline. The v0.1 surface ships without the eval
  gate; baseline lands in a follow-up PR.

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
