# Config Management — Unified Configuration for Builds and Releases

**Date:** 2026-06-10
**Status:** Approved
**Scope:** `anno-rag`, `anno-rag-bin`, `anno-rag-mcp`, CI pipeline, release workflow

---

## Problem

`AnnoRagConfig` has ~35 fields. Today:

- Each consumer (`anno-rag-bin`, `anno-rag-mcp`) calls `AnnoRagConfig::default()` and overrides fields manually, independently.
- ~15 fields are never exposed to users (no CLI flag, no env var, no MCP param).
- `ANNO_GDPR_LAYERS` and `ANNO_ACCELERATOR` are mentioned in doc comments but never read in `config.rs`.
- There is no config file — desktop users have no persistent config between sessions.
- CI has no guard against new fields being added silently (no attribution, no docs).
- Release artifacts ship no configuration reference.

---

## Goals

1. Single loading pipeline shared by all consumers (CLI binary, MCP server).
2. All `AnnoRagConfig` fields reachable via config file, env var, and/or CLI flag.
3. Schema and reference docs generated automatically from source — zero drift possible.
4. CI fails if a field is added without metadata annotation.
5. Release artifacts include a filled `config.toml.example` and `CONFIGURATION.md`.

---

## Non-Goals

- Per-call MCP overrides (e.g., `ocr_mode` on `index`, `rerank` on `search`) are out of scope — those remain as call-level parameters handled in `anno-rag-mcp`.
- Remote config / dynamic reload at runtime.
- Multi-profile config (dev / staging / prod) — out of scope for this iteration.

---

## Architecture

### Loading Pipeline

All consumers call `AnnoRagConfig::load(cli_overrides: Option<ConfigOverrides>)`.

Precedence (lowest → highest):

```
1. Rust defaults (current Default impl)
2. ~/.anno-rag/config.toml          — desktop / first-time setup
3. ANNO_RAG_* env vars              — Docker / CI / one-shot override
4. CLI flags (ConfigOverrides)      — per-invocation override (binary only)
```

The config file is **optional**. If absent, the behavior is identical to today.
The MCP server calls `AnnoRagConfig::load(None)` — layers 1–3 only.

### New Crate: `anno-config-meta` (proc-macro)

A minimal proc-macro crate with no external dependencies beyond `proc-macro2` and `syn`.

Exposes two derives:

| Derive | What it does |
|--------|-------------|
| `ConfigMeta` | Collects `#[config_meta(...)]` attributes and generates `AnnoRagConfig::config_schema() -> &'static [FieldMeta]` |
| `ConfigCliArgs` | Generates a `ConfigOverrides` struct (all fields `Option<T>`) compatible with `clap::Args` |

`FieldMeta` struct:

```rust
pub struct FieldMeta {
    pub name: &'static str,
    pub env_var: &'static str,
    pub cli_flag: &'static str,
    pub doc: &'static str,
    pub default_value: &'static str,  // serde_json::to_string of the default
    pub since: &'static str,
    pub type_name: &'static str,
}
```

### Annotation on `AnnoRagConfig`

Every field gets `#[config_meta(...)]`. Fields without it fail to compile when `ConfigMeta` is derived — this is the CI guard.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ConfigMeta, ConfigCliArgs)]
pub struct AnnoRagConfig {
    #[config_meta(
        env = "ANNO_RAG_OCR_MODE",
        cli = "--ocr-mode",
        doc = "OCR strategy applied during ingest: `off` | `auto_embedded`. Default: `off`.",
        since = "0.11"
    )]
    pub ocr_mode: OcrMode,

    #[config_meta(
        env = "ANNO_GDPR_LAYERS",
        cli = "--gdpr-layers",
        doc = "PII detection layer set: `basic` | `defense` | `shadow` | `full`. Default: `defense`.",
        since = "0.10"
    )]
    pub gdpr_layers: GdprLayerSet,

    // ... all other fields
}
```

### `build.rs` in `anno-rag`

When built with `--features generate-schema`:

1. Instantiates `AnnoRagConfig::config_schema()`.
2. Serializes to `crates/anno-rag/config-schema.json` (committed to repo).
3. Renders `docs/reference/configuration.md` from the schema (table: field / env var / CLI flag / default / description / since).
4. Writes `crates/anno-rag/config.toml.example` (all fields commented out, default values shown).

These three files are committed. The CI checks they are up to date.

---

## New CLI Surface

### `ConfigOverrides` (generated)

`anno-rag-bin` gains all previously-invisible flags via the generated `ConfigOverrides` struct:

```
--ocr-mode              ANNO_RAG_OCR_MODE
--embedder-dtype        ANNO_RAG_EMBEDDER_DTYPE
--gdpr-layers           ANNO_GDPR_LAYERS
--accelerator           ANNO_ACCELERATOR
--rerank-model          ANNO_RAG_RERANK_MODEL
--rerank-batch-size     ANNO_RAG_RERANK_BATCH_SIZE
--memory-ner-mode       ANNO_RAG_MEMORY_NER_MODE
--pdf-structured-sidecar  ANNO_RAG_PDF_STRUCTURED_SIDECAR
--conflict-cosine-threshold  ANNO_RAG_CONFLICT_COSINE_THRESHOLD
--graph-max-hops        ANNO_RAG_GRAPH_MAX_HOPS
--graph-per-hop-limit   ANNO_RAG_GRAPH_PER_HOP_LIMIT
# ... and all other previously-unexposed fields
```

`ConfigOverrides` is `#[command(flatten)]`-ed into `IngestArgs`, `SearchArgs`, and `ServeArgs`.

### `anno-rag config` subcommand (new)

| Subcommand | Behavior |
|------------|----------|
| `config init` | Writes `~/.anno-rag/config.toml` from `config.toml.example` if not present, prints path |
| `config show` | Prints effective config after all layers merged, annotated by source (file / env / default) |
| `config validate` | Deserializes and validates `~/.anno-rag/config.toml` without starting the pipeline |

`config show` output format:

```
data_dir          = /home/user/.anno-rag        [default]
ocr_mode          = auto_embedded               [env: ANNO_RAG_OCR_MODE]
gdpr_layers       = full                        [file: ~/.anno-rag/config.toml]
accelerator       = cpu                         [cli: --accelerator]
```

---

## MCP Server

The MCP server calls `AnnoRagConfig::load(None)` at startup.
No changes to per-call parameters — those remain call-level overrides in `anno-rag-mcp`.

---

## CI Changes

### New job: `Config schema check`

Added to `ci.yml`, runs on every PR:

```yaml
config-schema:
  name: Config schema check
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
    - run: cargo build -p anno-rag --features generate-schema
    - run: |
        git diff --exit-code \
          crates/anno-rag/config-schema.json \
          docs/reference/configuration.md \
          crates/anno-rag/config.toml.example
```

Two failure modes:
- **Compile error** — a field is missing `#[config_meta(...)]`.
- **git diff non-empty** — schema or docs are out of date with the committed files.

---

## Release Artifacts

`release.yml` packaging steps (`package-windows.ps1`, `package-unix.sh`) gain a step that copies into the release archive:

| File | Source |
|------|--------|
| `config-schema.json` | `crates/anno-rag/config-schema.json` |
| `config.toml.example` | `crates/anno-rag/config.toml.example` |
| `CONFIGURATION.md` | `docs/reference/configuration.md` |

These are also attached as GitHub Release assets alongside the binaries.

---

## File Layout

```
anno/
├── crates/
│   ├── anno-config-meta/            ← NEW proc-macro crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               ← ConfigMeta + ConfigCliArgs derives
│   │       ├── config_meta.rs       ← proc-macro impl for metadata collection
│   │       └── config_cli_args.rs   ← proc-macro impl for clap struct generation
│   ├── anno-rag/
│   │   ├── build.rs                 ← NEW — schema + docs + example generation
│   │   ├── config-schema.json       ← NEW — committed, regenerated by build.rs
│   │   ├── config.toml.example      ← NEW — committed, regenerated by build.rs
│   │   └── src/
│   │       └── config.rs            ← add #[config_meta] annotations + load()
│   └── anno-rag-bin/
│       └── src/
│           └── main.rs              ← replace manual overrides with ConfigOverrides
├── docs/
│   └── reference/
│       └── configuration.md         ← NEW — generated by build.rs, committed
└── .github/
    └── workflows/
        └── ci.yml                   ← add config-schema job
```

---

## Implementation Phases

| Phase | Scope | Effort |
|-------|-------|--------|
| 1 | `anno-config-meta` proc-macro (`ConfigMeta` only, no CLI derive yet) | 2–3 days |
| 2 | Annotate all `AnnoRagConfig` fields, add `build.rs`, commit generated files | 1 day |
| 3 | `AnnoRagConfig::load()` — config file + env var loading | 1 day |
| 4 | `ConfigCliArgs` derive + wire into `anno-rag-bin` | 1 day |
| 5 | `anno-rag config` subcommand (init / show / validate) | 1 day |
| 6 | CI job + release artifact packaging | 0.5 day |

Total: ~7–8 jours de développement.

---

## Risks

| Risk | Mitigation |
|------|-----------|
| Proc-macro syn parsing fragile sur types complexes | Phase 1 only needs field names + string attributes — no deep type inspection |
| `build.rs` slow down incremental builds | Gate behind `--features generate-schema`, not run by default |
| TOML deserialization breaks on unknown fields | Use `#[serde(deny_unknown_fields)]` with a clear error message |
| Env var naming collisions with existing undocumented vars | Audit existing `ANNO_*` usages in config.rs before Phase 2 |
| `data_dir` already reads `ANNO_RAG_DATA_DIR` in `Default::default()` | Phase 3 migrates this into `from_env()` — no behavioral change, but the special-case logic is removed from `default_data_dir()` |
| `ConfigCliArgs` generates `Option<T>` for each field; fields already `Option<T>` become `Option<Option<T>>` | Proc-macro detects `Option<_>` type and generates `Option<T>` (unwrapped) for those fields |
