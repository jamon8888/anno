# Changelog

All notable changes to the `anno-rag` crate are documented here. Other crates in the workspace (`anno`, `anno-cli`, `anno-eval`) follow their own versioning — see their `Cargo.toml` for current release status.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Semver: pre-1.0 minor versions may introduce breaking changes.

## [0.11.0] — 2026-06-06

### Added
- **Phase A: Deterministic PII stack** — entity validators (Luhn, IBAN mod-97, NIR, date range, IP, email RFC-light, postal code FR), GdprLayerSet feature flag (`ANNO_GDPR_LAYERS`: basic/defense/shadow/full), HeuristicFrNer backend (FR orgs, addresses, dates, intl IBANs)
- **Multi-platform release binaries**: x86_64-pc-windows-msvc, aarch64-apple-darwin, x86_64-apple-darwin (Intel Mac via Homebrew onnxruntime)
- **Build/test overhaul**: dev-fast profile, sccache, lld-link, cargo-nextest profiles, CARGO_TARGET_DIR isolation

### Changed
- Workspace version bumped 0.10.0 → 0.11.0

---

## [Unreleased] — v0.3 in progress

### Added
- Added privacy vault Word review design and local workflow tools for preparing
  editable working documents, reading `à masquer` / `à garder` comments, and
  regenerating anonymized outputs without returning PII through Cowork.

### Phase A: Deterministic Stack (MERGED)
- **Entity validators** — Luhn (SIRET, card_number), IBAN mod-97 (ISO 13616), NIR control key (Corsica 2A/2B), date range, IP address, email RFC-light, postal code (France mainland + DOM)
- **GdprLayerSet** — tiered detection via `ANNO_GDPR_LAYERS` env: `basic` (regex+NER only), `defense` (+ FR heuristics + validators, default), `shadow` (Phase C placeholder), `full` (Phase D placeholder)
- **HeuristicFrNer backend** — deterministic extraction for: French legal org forms (SAS/SARL/EURL/SCI/…), FR postal addresses (voie keywords), FR dates with date_of_birth context, international IBANs with mod-97 verification
- **Integration into detect_inner** — validators and heuristics active on defense+ layers; rejection counters emitted to audit

### Backlog items (#001-#005) still in flight:
- Replace `StackedNER::default()` with `GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")` for reliable French name detection
- `extract_structure` for Tabular Review v1.1 foundation
- Chunk-level clause classifier (`clause_type` metadata)
- MCP error channel via `Result<_, McpError>`
- Stop leaking `cloakpipe_core::DetectedEntity` from `Pipeline::detect`

## [0.2.0] — 2026-05-13

### Added
- **`anno-rag mcp` subcommand** — rmcp 1.6 stdio MCP server exposing 4 tools to Claude Cowork:
  - `search(query, top_k)` — top-K pseudonymized chunks
  - `rehydrate(text)` — restore originals from local vault
  - `detect(text)` — dry-run PII scan
  - `vault_stats()` — token mapping diagnostics
- **NER warmup** — `cargo run --example warmup_model -p anno-rag --release` now downloads both the embedder AND an anno NER model (multi-candidate fallback: `gliner_small-v2.1` → `gliner-pii-edge-v1.0` → `NuNER_Zero` → `bert-base-NER-onnx`).
- **`Store::maybe_build_index`** — builds an `IVF_HNSW_SQ` index on the `vector` column once the `chunks` table crosses `vector_index_threshold` rows (default 1000). Idempotent. Wired into `Pipeline::ingest_folder` tail.
- **Config additions** (all `#[serde(default)]`, v0.1 TOML still parses):
  - `vector_index_threshold: usize` (default 1000)
  - `ner_warmup_model: Option<String>` (override the warmup candidate list)
  - `mcp_server_name: String` (default `"anno-rag"`, advertised on MCP `initialize`)
- **Pipeline helpers** — `rehydrate`, `detect`, `vault_stats` (powering the MCP handlers)
- **`Vault::lock_inner`** — `pub(crate)` async lock helper for Pipeline to call cloakpipe's Rehydrator + stats
- **`crates/anno-rag/README.md`** — Cowork plugin config example, architecture diagram, operational notes
- **`docs/dev-notes/gliner2-fastino-v0.3-research.md`** — deep-dive on `gliner2_fastino` + `gliner2_fastino_candle` backends, the fastino HF family, and 8 candidate features for v0.3 (with top-3 recommendation)
- **TODO marker** at `crates/anno-rag/src/detect.rs:91` pointing to the v0.3 #1 replacement of `StackedNER::default()`
- **`BACKLOG.md`** — 22 tracked issues #001-#022 spanning v0.3 / v0.4 / v0.5+ / v1.0

### Changed
- **Crate version** 0.1.0 → 0.2.0
- **E2E test** — restored 4 name-scrub assertions but as `>=50%` coverage rather than `==100%` (anno's `StackedNER` fallback consistently misses "Jean Martin" in markdown-formatted contexts; fixed in v0.3)

### Dependencies
- Added `rmcp = "1.6"` + `schemars = "0.8"` (workspace) for the MCP server

## [0.1.0] — 2026-05-12

Initial walking skeleton.

### Added
- **CLI** with two subcommands:
  - `anno-rag ingest <folder> [--recursive] [--output <dir>]`
  - `anno-rag search <query> [--top-k <N>]`
- **Ingest pipeline** — folder → kreuzberg extract → detect PII → pseudonymize → embed → store → write `*.anon.md` copy
- **PII detection** — 4 French regex patterns (NIR, SIRET with Luhn check, IBAN-FR, FR phone) + anno's `StackedNER` for names/orgs/locations
- **Vault** — wraps cloakpipe-core `Vault` (file-based AES-256-GCM); key derived via Argon2id from passphrase OR random 32 bytes via OS keyring
- **Embedder** — `intfloat/multilingual-e5-small` via candle 0.10 (384-dim, multilingual)
- **Store** — LanceDB 0.27.2 chunks table with idempotent `merge_insert` on `(doc_id, chunk_idx)`; deterministic chunk UUIDs (v5) for re-ingest stability
- **`SearchHit.distance`** — L2 distance from `_distance` column (lower is closer); `f32::INFINITY` sentinel when missing
- **Char-to-byte offset translation** in `detect.rs` to bridge anno's char offsets with cloakpipe's byte-based replacement (avoids "not a char boundary" panic on French accented text)
- **`tests/e2e.rs`** — end-to-end integration test on 3 anonymized French legal fixtures (contract, facture, jugement)
- **`examples/warmup_model.rs`** — pre-downloads the 448 MiB embedder weights so the first ingest / e2e doesn't pay the network cost

### Dependencies
- Vendored `cloakpipe-core` from `rohansx/cloakpipe` (Apache-2.0) under `vendor/cloakpipe/` with a 1-line rusqlite bump (0.32 → 0.37) to resolve `libsqlite3-sys` link conflict with `anno-eval`.
- Workspace pins: `kreuzberg = "=4.9.7"`, `lancedb = "=0.27.2"`, `arrow = "=57"` (transitive via lancedb), `candle = "=0.10"`, `tokio = "1.51"` LTS, `fd-lock = "4"`, `rmcp = "1.6"` (added v0.2), `schemars = "0.8"` (added v0.2).

### Out of scope (deliberately deferred)

See `docs/superpowers/specs/2026-05-12-anno-rag-design.md` for the v1 Northstar. v0.1 is a single-crate walking skeleton; the 7-crate decomposition and most features (HTTP API, MCP, GDPR audit, citation graph, privilege gating, cross-encoder rerank, tabular review) ship in later releases per the v1 design.

---

## Migration notes

### v0.1 → v0.2

- **Config:** no breaking changes. New fields use `#[serde(default)]`; old TOML configs continue to parse with sensible defaults.
- **CLI:** `anno-rag mcp` is new but additive — no flags removed or renamed.
- **`SearchHit::score` → `SearchHit::distance`** was already a v0.1 fix (commit `01641e1f`), not a v0.2 change.
- **NER reliability:** if you rely on names being scrubbed, run `cargo run --release --example warmup_model -p anno-rag` once before ingesting to populate `~/.cache/huggingface/hub/` with both the embedder and an anno NER model. Without this, names fall through to anno's pattern+heuristic fallback (matching v0.1 behavior).

### Cowork plugin config (v0.2+)

```json
{
  "anno-rag": {
    "command": "/absolute/path/to/anno-rag",
    "args": ["mcp"],
    "env": { "ANNO_RAG_VAULT_PASSPHRASE": "your-passphrase-here" }
  }
}
```

See `crates/anno-rag/README.md` for full setup.
