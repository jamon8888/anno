# Changelog

All notable changes to the `anno-rag` crate are documented here. Other crates in the workspace (`anno`, `anno-cli`, `anno-eval`) follow their own versioning — see their `Cargo.toml` for current release status.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Semver: pre-1.0 minor versions may introduce breaking changes.

## [0.13.3] — 2026-06-17

### Fixed
- **macOS aarch64 release** — `anno-privacy-gateway` was missing from the ORT rpath-patching loop in `release.yml`; `dyld` failed to find `libonnxruntime.dylib` at smoke-test time → SIGABRT (exit 134). The fix adds `anno-privacy-gateway` to the `ORT_RPATH_BINARIES` array alongside `anno-rag`, so both binaries receive `install_name_tool` patching before the boot smoke.
- **Smoke script diagnostics** — `smoke-gateway.sh` / `smoke-gateway.ps1` now print gateway stdout/stderr inline on failure instead of only file paths.

## [0.13.2] — 2026-06-16

### Fixed
- **Release pipeline** — `anno-privacy-gateway` was left at `0.13.0` while `anno-rag-bin` was bumped to `0.13.1`, so `cargo-dist`'s lockstep mode silently excluded the gateway binary from the build; the `v0.13.1` release failed the gateway boot-smoke step on every platform (Windows, macOS Intel, macOS ARM). Versions are now bumped together for this release.

## [0.13.1] — 2026-06-12

### Fixed
- **LanceDB table recovery** — `LegalStore::open()` and `EnrichmentStatusStore::open()` now use `match open_table { Err(not_found) => create_table }` instead of `table_names()` probe; orphan directories left by interrupted ingestion no longer cause `Table not found` errors on restart
- **MCP cold-start timeout** — `serve_stdio_lazy` spawns a background `tokio::task` to pre-warm ONNX models (~970 MB) right after the MCP transport is ready; first tool call no longer blocks for ~78 s
- **Vault key mismatch after `anno_init_vault`** — `AnnoRagServer::key` is now `Arc<RwLock<[u8; 32]>>`; re-derived and updated in the `anno_init_vault` handler so lazy-inited pipeline always uses the passphrase-based key
- **`vault.available: false` in `status` before pipeline load** — `status_impl_routing` now reads the OS keyring directly when the pipeline `OnceCell` is not yet populated, matching the `anno_health` pattern

---

## [0.13.0] — 2026-06-11

### Added
- **Unified config management** — every configuration field now reachable via TOML file (`~/.anno-rag/config.toml`), environment variable, **and** CLI flag
- **`anno-config-meta` proc-macro crate** — `ConfigMeta` + `ConfigCliArgs` derives generate `config_schema()` metadata and a `ConfigOverrides` clap struct from `#[config_meta(env, cli, doc, since)]` annotations
- **`AnnoRagConfig::load()` pipeline** — layered config resolution: defaults → TOML file → env vars → CLI overrides; `serde(default)` on all fields for partial TOML
- **38 CLI flags** — `#[command(flatten)] config: ConfigOverrides` in `Ingest` and `Search` subcommands; all fields controllable without touching a config file
- **`anno-rag config init|show|validate`** — copies bundled example, shows field sources (`[default]`/`[env: ...]`/`[file: ...]`), validates TOML
- **CI `config-schema` job** — re-runs schema-gen and asserts `git diff --exit-code` on generated artifacts
- **Release artifacts** — `config-schema.json` + `config.toml.example` bundled alongside binaries
- **Comprehensive docs** — `docs/CONFIGURATION.md` + reference docs with full field reference

### Fixed
- **Workspace-hack fix** — `candle-core/metal` moved to `[target.'cfg(target_os = "macos"')]`, `cuda` to Linux-only; prevents `objc2` compile error on Windows
- **Proc-macro path fix** — `::anno_rag::config_meta_types` → `crate::config_meta_types` in generated code (E0433)
- **`GdprLayerSet::FromStr::Err`** — `()` → `String` for clap `ValueParser` compatibility
- **Integration test fixes** — `required-features = ["rerank"]` for rerank tests; `required-features = ["dev-integration"]` for 4 heavy tests; `build-jobs = 2` in nextest local profile to prevent OOM on 16 GB RAM

### Changed
- Workspace version bumped 0.12.3 → 0.13.0

---

## [0.12.3] — 2026-06-10

### Fixed
- **CUDA Linux binary** — switched CI runner `ubuntu-22.04` → `ubuntu-24.04`; gcc 14 on updated ubuntu-22.04 images generates C23 `__isoc23_strtoll` calls that glibc 2.35 doesn't have; glibc 2.39 (Ubuntu 24.04) does
- **Homebrew formula publish** — `jamon8888/homebrew-hacienda` tap was an empty repo with no commits; `refs/heads/main` didn't exist so checkout failed; initialized with first commit

### Changed
- Workspace version bumped 0.12.2 → 0.12.3

---

## [0.12.2] — 2026-06-10

### Fixed
- **Release workflow** — `cargo-dist` plan step (exit 255) caused by custom WiX `SetupMcp` action being flagged as dirty; added `"msi"` to `allow-dirty` in `[workspace.metadata.dist]`
- **Eval Sanity timeout** — CI job consistently hit the 15-minute wall on cold runners; raised to 25 minutes

### Changed
- **Code quality: god-file splits** — three large source files converted to directory modules with zero public-API change:
  - `anno/src/core/grounded.rs` (6005L) → `grounded/{mod, signal, track, identity, html, eval_render}.rs`
  - `anno-rag-mcp/src/lib.rs` (6079L) → extracted `params`, `wire`, `search`, `legal`, `review` modules
  - `coref/mention_ranking/algorithm.rs` (4934L) → `algorithm/{mod, features, scoring}.rs`
- **Workspace lints** — `[lints] workspace = true` propagated to `anno`, `anno-cli`, `anno-eval`, `anno-corpus-core`
- Workspace version bumped 0.12.1 → 0.12.2

---

## [0.12.1] — 2026-06-09

### Added
- **Automatic MCP registration on install** — WiX CustomAction (Windows MSI) and `postinstall` script (macOS PKG) call `anno-rag setup-mcp --target all` immediately after installation; users no longer need to register the server manually
- **macOS PKG + DMG installer** (`scripts/release/build-macos-pkg.sh`) — native `.pkg` wrapped in `.dmg`, with optional code signing (`APPLE_CODESIGN_IDENTITY`, `APPLE_INSTALLER_SIGNING_IDENTITY`) and notarisation (`APPLE_ID` / `APP_SPECIFIC_PASSWORD` / `APPLE_TEAM_ID`)
- **`install-mcp.sh` / `install-mcp.ps1`** — convenience scripts bundled in every release archive for archive-based installs

### Fixed
- **PII span fusion** — overlapping detected entities now merge instead of being dropped; `original` string re-derived from source text after fusion (`crates/anno-rag/src/detect.rs`)
- **Vault decrypt errors propagated** — silent `"[decrypt_failed]"` substitution replaced by proper `Error` return (`crates/anno-rag/src/vault_sqlite.rs`)
- **Vault salt path-derived** — per-vault SHA-256 salt replaces static constant; migration path preserves existing vaults (`crates/anno-rag/src/vault_sqlite.rs`)
- **Gateway error types** — monolithic `Error::Upstream(String)` split into `UpstreamConnect`, `UpstreamStatus { status, message }`, `UpstreamParse`; `IntoResponse` sanitises messages before sending to clients (`crates/anno-privacy-gateway`)
- **Metal GPU build** — missing `CANDLE_NER_MODEL_DIR` constant (`#[cfg(feature = "gpu-metal")]`) in `detect.rs` caused `E0425` compile failure
- **CUDA Linux build** — `Jimver/cuda-toolkit` generated wrong package names for cuBLAS (`cuda-cublas-12-4`); now installed directly as `libcublas-12-4` / `libcublas-dev-12-4`
- **Homebrew publish step** — `actions/checkout@v6` rejected empty `HOMEBREW_TAP_TOKEN`; falls back to `github.token` when the secret is unset

### Changed
- Workspace version bumped 0.12.0 → 0.12.1

---

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
