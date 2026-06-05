# anno-rag Backlog

Tracking issues for [anno-rag](crates/anno-rag/) post-v0.2. Issues are disabled on `jamon8888/anno` fork, so this file replaces a GitHub issue tracker. Each entry has a stable `#NNN` identifier — reference them in commit messages.

## v0.3 — French-aware NER + Tabular Review unlock

### #001 — Replace `StackedNER::default()` with `GLiNER2Fastino` multi-v1 ONNX (priority #1)
- **Why:** v0.2 e2e leaks "Jean Martin" in markdown context — `StackedNER` fallback chain not French-tuned. **11/12** name scrubs.
- **Fix:** `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` at `crates/anno-rag/src/detect.rs:91` (TODO marker already there).
- **Acceptance:** 12/12 names scrubbed; e2e allow-list removed; warmup downloads multi-v1 ONNX directly (drop the candidate fallback chain).
- **Estimate:** S (1-2h dev)
- **Refs:** `docs/dev-notes/gliner2-fastino-v0.3-research.md` §3, recommendation #1

### #002 — `extract_structure` for Tabular Review (priority #2)
- **Why:** Tabular Review v1.1 plan assumes per-cell LLM prompting. anno's `extract_structure(text, &TaskSchema, threshold)` does it in one forward pass via the GLiNER2 schema interface.
- **Fix:** Wire `Pipeline::extract_structure(text, schema)` over `anno::backends::gliner2_fastino::GLiNER2Fastino::extract_structure`. Add MCP tool `tabular_review_create / add_column / refine_cell`.
- **Acceptance:** Demo NDA schema (parties, term, governing_law, jurisdiction) returns one `ExtractedStructure` per chunk with per-field offsets.
- **Estimate:** M (4-8h dev incl. MCP wiring)
- **Refs:** v1.1 plan `docs/superpowers/plans/2026-05-12-anno-rag-tabular-review-v1.1.md`; research §3 recommendation #2.

### #003 — Chunk-level clause classifier (priority #3)
- **Why:** Enables `WHERE clause_type = 'termination'` filtered search. Cheapest meaningful retrieval upgrade.
- **Fix:** Per chunk at ingest, call `model.classify(chunk, &["confidentiality","termination","ip_assignment","non_compete","governing_law","indemnity","payment_terms"], 0.0)`. Top label → `clause_type` column on `chunks` table.
- **Acceptance:** New nullable `clause_type: Utf8` column; populate during ingest; expose via MCP `search(query, filter: { clause_type })` param.
- **Estimate:** S (1-2h dev)
- **Refs:** research §3 recommendation #3.

### #004 — MCP error channel: return `Result<_, McpError>` (code-review I1)
- **Why:** v0.2 handlers format errors into JSON strings ("Error: ..."), indistinguishable from success at the JSON-RPC layer.
- **Fix:** `mcp.rs` handlers return `Result<String, rmcp::ErrorData>` (or `Result<CallToolResult, rmcp::McpError>` per rmcp 1.6).
- **Estimate:** S (~1h)
- **Refs:** final code review of v0.2 (in PR #1 conversation), Issue I1.

### #005 — Stop leaking `cloakpipe_core::DetectedEntity` from `Pipeline::detect` (code-review I2)
- **Why:** Vendored cloakpipe type appears in `anno-rag` public API, forcing external consumers to depend on it.
- **Fix:** Define `pub struct DetectedEntityOut` in `anno-rag-core` (or reuse `mcp::EntityInfo`); return that instead.
- **Estimate:** S (~30min)
- **Refs:** review Issue I2.

## v0.4 — HTTP API + GDPR core articles

### #006 — HTTP API (axum) mirroring MCP tools
- Routes: `POST /v1/search`, `POST /v1/ingest`, `GET /v1/documents`, `POST /v1/pseudonymize`, `POST /v1/rehydrate`, `POST /v1/detect`, etc.
- Bearer token auth (config-driven, OS keyring stored)
- Bind `127.0.0.1` by default; TLS required for non-loopback
- **Estimate:** M (1 week)
- **Spec:** `docs/superpowers/specs/2026-05-13-anno-rag-v0.4-http-gdpr.md` (this session)

### #007 — GDPR Art. 30 audit log (hash-chained)
- Append-only JSONL at `~/.anno-rag/audit/audit.jsonl`
- Each entry includes `sha256(prev_line)` for tamper-evidence
- Daily HMAC signature; CSV/PDF register export via `anno-rag register --since --until`
- **Estimate:** M (3-4 days)

### #008 — `forget(subject)` Art. 17 erasure
- Delete chunks + vault entries + drop affected LanceDB tags
- Sequence: `tbl.delete(...)` → `tbl.optimize(Prune older_than=0)` → `tbl.optimize(Compact materialize_deletions=true)`
- Audit-logged with proof-of-erasure receipt
- **Estimate:** M (2-3 days)

### #009 — `find_subject(ref)` Art. 15 access
- Returns all chunks referencing a subject ID/pseudonym
- Pre-step to forget; also satisfies right-of-access requests
- **Estimate:** S (1 day)

### #010 — TOML config loader + CLI flags
- `--data-dir <path>`, `--config <path>`, `--passphrase <env_var>` flags
- `~/.anno-rag/config.toml` overrides defaults (existing `AnnoRagConfig` already serde-roundtrips)
- **Estimate:** S (1-2h)

### #011 — MCP server graceful shutdown (code-review I5)
- `tokio::select!` on `service.waiting()` vs `tokio::signal::ctrl_c()`
- Flush audit log + close LanceDB connections cleanly
- **Estimate:** S (~1h)
- **Refs:** review Issue I5.

### #012 — `Error::Mcp` variant (code-review M2)
- Replaces today's `Error::Detect("MCP server failed: ...")` mapping with a typed variant.
- **Estimate:** XS (~15min)
- **Refs:** review Issue M2.

## v0.5+ — Quality + structure

### #013 — Hierarchical legal structure typing
- Pre-chunker detects `LIVRE`/`TITRE`/`Chapitre`/`Article N°`/`alinéa` boundaries
- Article = atomic chunk unit; metadata `{code, livre, titre, article, alinéa}` on each chunk
- **Refs:** v1 design §6.1

### #014 — Citation graph (ECLI, Cass./CE/CA, code articles, Légifrance IDs)
- Extract citations during ingest, store as edges, expose graph-hop in search
- **Refs:** v1 design §6.2, donor regexes from BO-ECLI Parser + pyJudilibre

### #015 — Privilege gating (`avocat_client` / `judicial` / `none`)
- Per-chunk privilege tag; MCP/HTTP never returns privileged chunks
- CLI `--include-privileged` audit-logged
- **Refs:** v1 design §6.5

### #016 — Cross-encoder rerank
- `antoinelouis/crossencoder-camembert-L6-mmarcoFR` (~140 MB FR-monolingual)
- Stage-2 reranker over hybrid BM25+vector top-50
- **Refs:** v1 design §6.3

### #017 — Tabular Review v1.1 (full)
- Full plan exists at `docs/superpowers/plans/2026-05-12-anno-rag-tabular-review-v1.1.md` (3146 lines, 51 tasks)
- Depends on #002 (extract_structure foundation)

## v1.0 — Production hardening

### #018 — Hybrid search FTS+vector
- Build FTS index after ingest, use native `RRFReranker`
- **Refs:** v1 design §4.3 query flow

### #019 — Index encryption at rest
- App-layer envelope encryption on `text` column until LanceDB ships native
- **Refs:** v1 design §11

### #020 — Process-safety lock file (`fd-lock`)
- Prevent CLI + MCP server opening same dir concurrently (LanceDB not multi-process safe)
- **Refs:** v1 design §11

### #021 — Watch mode
- Folder reindex on file change/delete via `notify` crate
- **Refs:** v1 design §16 v1.1

### #022 — 7-crate decomposition
- Split `anno-rag` → `anno-rag-core/-ingest/-detect/-embed/-store/-audit/-eval` + 3 binaries
- Trigger: `cargo build` exceeds 30s for incremental edits
- **Refs:** v1 design §5

## v0.5 — Memory & format coverage (from piighost-test-multi-format bench)

See `crates/anno-rag/docs/dev-notes/bench-v0.2-piighost-test.md` for the measurements behind these issues.

### #023 — Lazy-init `Embedder` and `Detector` in `Pipeline`
- **Why:** v0.2 MCP server loads ~2 GB of models at process start, before any tool call. For users who only ever call `vault_stats` or `rehydrate`, that's pure waste. On 8 GB laptops it's the difference between "anno-rag runs" and "OOM at startup."
- **Fix:** Wrap `Pipeline.embedder` and `Pipeline.detector` in `tokio::sync::OnceCell` (or `arc_swap::ArcSwapOption`). Initialize on first call to the relevant code path. Idle MCP server should be <100 MB RSS.
- **Acceptance:** `anno-rag mcp` cold-start RSS <100 MB before first tool call; first `search` call still completes in <10s (model load amortized).
- **Estimate:** S (2-3h)

### #024 — Drop bundled Tesseract, fork to system binary
- **Why:** `kreuzberg` `ocr` feature statically links libtesseract + leptonica + bundled tessdata (~500 MB resident). Most PDFs have a text layer and never need OCR.
- **Fix:** Remove `ocr` from the kreuzberg feature set. Detect empty text layer post-extraction and shell out to system `tesseract` (`Command::new`) if available; otherwise skip OCR with a warning. Install instructions in README.
- **Acceptance:** Peak RSS reduction ≥400 MB on bench corpus; PDFs with text layers ingest unchanged; PDFs without text layers either OCR via system tesseract or surface a clear "install tesseract" error.
- **Estimate:** M (4-6h)

### #025 — StackedNER → single-backend after v0.3 #001
- **Why:** `StackedNER` keeps every fallback model resident even after one backend succeeds. With GLiNER2Fastino multi-v1 landed (#001), the fallback chain is dead weight.
- **Fix:** Replace `StackedNER` construction in `Detector` with the single `GLiNER2Fastino` instance. Drop bert-base-NER + gliner_small + NuNER_Zero from warmup.
- **Acceptance:** Peak RSS reduction ≥800 MB; e2e name-scrub rate stays at 12/12 (the whole point of #001).
- **Estimate:** S (1h, but blocked on #001)
- **Depends on:** #001

### #026 — Embedder fp16 on CPU
- **Why:** `multilingual-e5-small` F32 weights are ~470 MB. candle 0.10 supports `DType::F16` on CPU via softfloat emulation. Embedding quality loss is empirically <0.5% on STS benchmarks.
- **Fix:** Load embedder with `DType::F16`, cast inputs to F16 before forward pass. Benchmark a small recall@10 regression on the v0.2 e2e corpus to confirm quality.
- **Acceptance:** Peak RSS reduction ~250 MB; recall@10 on e2e queries within 1% of fp32 baseline.
- **Estimate:** S (2-3h incl. recall verification)

### #027 — `dirs::home_dir()` ignores `HOME` env override on WSL
- **Why:** During bench, `HOME=/tmp/anno-rag-bench-home anno-rag ingest …` still wrote to the account home directory instead of the requested temporary home. The `dirs` crate reads `/etc/passwd` on Linux and ignores `$HOME` when the binary is invoked from a different shell context.
- **Fix:** In `AnnoRagConfig::data_dir()`, prefer `std::env::var("ANNO_RAG_DATA_DIR")` → `std::env::var("HOME")` → `dirs::home_dir()`. Tracked alongside #010 (TOML config loader).
- **Estimate:** XS (15min)

### #028 — Tabular format coverage (XLSX / CSV / TSV / JSONL)
- **Why:** v0.2 ingest skips **57%** of the piighost-test-multi-format corpus by file count. These formats hold the structured PII (employee SSNs in xlsx, expense IBANs in tsv) that's the whole point of the GDPR pitch.
- **Fix:** Either (a) enable kreuzberg's tabular extractor feature if present, or (b) add a thin pre-extractor in `anno-rag::ingest` that routes XLSX→calamine, CSV/TSV→`csv` crate, JSONL→line-split, each emitting a markdown table chunk that flows through the normal detect→pseudonymize→embed path.
- **Acceptance:** Bench rerun ingests 14/14 docs; structured PII (SSN, IBAN) in xlsx/csv is scrubbed in the `.anon.md` output.
- **Estimate:** M (1-2 days incl. testing on the bench corpus)

### #029 — Re-bench search latency via MCP server (warm path)
- **Why:** v0.2 search latencies measured via CLI are meaningless — every call pays full embedder + Detector + LanceDB cold-start. The MCP hot-path is what production users see.
- **Fix:** Add a `cargo bench` target that boots the MCP server, sends 100 `search` JSON-RPC calls, measures p50/p95/p99. Document in same bench doc.
- **Acceptance:** p95 search latency <100ms on the bench corpus with v0.3 #001 model loaded.
- **Estimate:** S (3-4h)

## Notes

- **Issue ID convention:** `#NNN` zero-padded; reference in commit messages and PR descriptions
- **Status tracking:** not currently — add `**Status:** in-progress / merged @ <SHA>` line to each entry as work moves
- **Priority:** rough ordering within each version; see linked spec sections for rationale
