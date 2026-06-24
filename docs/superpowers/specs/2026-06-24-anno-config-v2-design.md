# Anno Config v2 — Typed Provider Registry

**Date**: 2026-06-24  
**Status**: Approved — ready for implementation planning  
**Branch target**: `feat/config-v2`

---

## Problem Statement

`AnnoRagConfig` is a flat ~40-field struct in `crates/anno-rag/src/config.rs` (66 KB). Key pain points:

- `embed_model` and `embed_dim` are separate fields — must be kept in sync manually; mismatch silently corrupts the index
- VLM-OCR prompt is hardcoded (`VLM_OCR_PROMPT_FR` constant in `vlm.rs`); VLM server URL/model not in `AnnoRagConfig`
- `OcrMode` enum has no `Vlm` variant — VLM config lives in `anno-rag-tabular`, detached from the main config
- No named profiles — switching from "lightweight dev" to "full legal stack" requires editing 10+ fields
- Cargo features (`embedded-ocr`, `vlm-ocr`) are undiscoverable; no documented build presets
- No hot-reload — switching models requires a full MCP server restart

---

## Design

### 1. Config Schema (v2)

Schema version is declared at the top of `anno-rag.toml`. The `schema_version = 2` key triggers the new block parser; missing or `1` triggers the v1 compat shim.

```toml
schema_version = 2
profile = "legal-full"   # optional; see §3
data_dir = "~/.anno-rag"
accelerator = "auto"     # auto | cpu | cuda | metal

[models.embedder]
type = "hf-candle"        # hf-candle | hf-onnx | openai-compat | local-path
model_id = "intfloat/multilingual-e5-base"
dtype = "f32"             # f32 | f16
# dim: AUTO-DETECTED from model_id via ModelDimRegistry; no manual field

[models.ner]
type = "hf-onnx"
model_id = "SemplificaAI/gliner2-multi-v1-onnx"
precision = "fp16"        # fp16 | fp32
candle_fallback_id = "fastino/gliner2-multi-v1"

[models.ner_pii]
type = "hf-onnx"
model_id = "SemplificaAI/gliner2-privacy-filter-PII-multi"

[models.reranker]
type = "hf-onnx"
model_id = "onnx-community/bge-reranker-v2-m3-ONNX"
onnx_file = "onnx/model_int8.onnx"
pool_size = 30
batch_size = 8

[ocr]
mode = "auto_embedded"    # off | auto_embedded | vlm
confidence_threshold = 0.7  # VLM quality score below this → embedded fallback

[ocr.embedded]
backend = "tesseract"     # tesseract | paddleocr
cache = true
batch_budget_secs = null  # null = unlimited

[ocr.vlm]                 # only active when mode = "vlm"
server_url = "http://localhost:8000"
model_id = "mistralai/Mistral-7B-Instruct-v0.3"
language = "fra"          # replaces hardcoded VLM_OCR_PROMPT_FR constant
prompt_template = null    # null = built-in template; set to override entirely

[index]
distance = "cosine"
vector_index_threshold = 1000
num_partitions = null
nprobes = 20
refine_factor = 10

[chunking]
max_chars = 2048
overlap = 256

[memory]
collection_name = "memories"
ner_mode = "async"        # disabled | async | sync
compaction_interval_secs = 86400

[pdf]
native_mode = "off"       # off | structured
keep_headers = false
keep_footers = false
extract_annotations = false
hierarchy_clusters = 6
allow_single_column_tables = false
structured_sidecar = false
```

**`local-path` type**: `model_id` is interpreted as an absolute filesystem path to a directory containing `config.json` + safetensors weights. Used by the `offline` profile where no HF Hub access is available.

**`ModelDimRegistry`**: a compile-time lookup table (HF model ID → known output dim) in `anno-rag`. Covers all shipped defaults. Unknown model IDs trigger a one-time runtime probe on first load (embed a short test string, measure output length), result cached to `data_dir/.model_dims.json`.

**`[models.*]` type discriminant** makes it possible to add provider backends (e.g. `openai-compat`, `local-path`) without adding new top-level fields.

---

### 2. Profile System

Profiles are named config overlays. Resolution order (last wins):

```
built-in defaults
    ↓
built-in profile (if profile = "...")
    ↓
user profile file (~/.anno-rag/profiles/<name>.toml)
    ↓
anno-rag.toml explicit fields
    ↓
ANNO_RAG_* env vars  ← highest priority, always
```

**Built-in profiles** (compiled into the binary):

| Name | Embedder | NER | OCR | Reranker | RAM target |
|------|----------|-----|-----|----------|------------|
| `lightweight` | e5-small (384d) | disabled | off | disabled | ~300 MB |
| `standard` | e5-base (768d) | gliner2 ONNX fp16 | auto_embedded | disabled | ~700 MB |
| `legal` | e5-base (768d) | gliner2 ONNX fp16 | auto_embedded | bge-reranker int8 | ~1 GB |
| `legal-full` | e5-large (1024d) | gliner2 fp16 + PII | vlm | bge-reranker int8 | ~1.5 GB + VLM server |
| `offline` | local-path | local-path | off | disabled | depends |

**User-defined profiles**: drop a file at `~/.anno-rag/profiles/<name>.toml` using the same block syntax (no `schema_version` needed). Reference it with `profile = "<name>"` in the main config.

Example user profile:
```toml
# ~/.anno-rag/profiles/my-fr-legal.toml
[models.embedder]
model_id = "dangvantuan/sentence-camembert-large"

[ocr]
mode = "vlm"

[ocr.vlm]
server_url = "http://gpu-server:8000"
language = "fra"
```

`anno-rag config show` output gains a **Source** column showing which layer (`[default]`, `[profile:legal-full]`, `[file]`, `[env]`) each value came from.

---

### 3. Migration & Backward Compatibility

**V1 shim**: `AnnoRagConfig::load()` checks `schema_version`. Missing or `1` → run `v1_compat::parse()` which maps flat fields to the v2 block structure in memory. No file is modified. A deprecation warning is printed once to stderr:

```
Warning: config.toml is schema v1 — run 'anno-rag config migrate' to upgrade.
```

**`anno-rag config migrate` command**:
- Reads v1 file, runs shim, serializes resolved v2 struct to TOML
- Writes `anno-rag.toml` alongside the old file (old file untouched)
- Prints a human-readable diff showing field renames and dropped deprecated fields (`tesseract_path`, `enable_ocr`)
- User reviews and deletes the old file manually

**Env var mapping** (all existing vars preserved):

| V1 env var | V2 config path |
|------------|---------------|
| `ANNO_RAG_EMBED_MODEL` | `models.embedder.model_id` |
| `ANNO_RAG_EMBED_DIM` | *(deprecated — auto-detected; still accepted for override)* |
| `ANNO_RAG_OCR_MODE` | `ocr.mode` |
| `ANNO_RAG_OCR_BACKEND` | `ocr.embedded.backend` |
| `ANNO_RAG_NER_MODEL` | `models.ner.model_id` |
| `ANNO_RAG_RERANK_MODEL` | `models.reranker.model_id` |
| … | … |

New v2-only vars follow block-path convention: `ANNO_RAG_OCR_VLM_SERVER_URL`, `ANNO_RAG_OCR_VLM_LANGUAGE`, `ANNO_RAG_MODELS_EMBEDDER_DTYPE`.

**Deprecation timeline**: v1 compat shim remains until v1.0, removed with a semver major bump.

---

### 4. Build Profiles

New file `build-profiles.toml` (checked in, source of truth for CI and justfile):

```toml
[profiles.lite]
features = []
description = "Embedder + NER only. No OCR. Smallest binary (~80 MB)."
default_runtime_profile = "lightweight"

[profiles.standard]
features = ["embedded-ocr"]
description = "Adds Kreuzberg embedded OCR (tesseract/paddleocr). ~120 MB."
default_runtime_profile = "standard"

[profiles.legal]
features = ["embedded-ocr", "vlm-ocr"]
description = "Adds VLM-OCR client. Requires external VLM server at runtime."
default_runtime_profile = "legal-full"

[profiles.full]
features = ["embedded-ocr", "vlm-ocr", "cuda"]
description = "GPU-accelerated. Requires CUDA toolkit at build time."
default_runtime_profile = "legal-full"
```

**Justfile targets**:
```
just build              # default = standard
just build profile=lite
just build profile=legal
just build profile=full
just release profile=legal
```

**Runtime feature guard**: on startup, `anno-rag` checks that the runtime config's `ocr.mode = "vlm"` is only set when the `vlm-ocr` Cargo feature was compiled in. If not:

```
Error: config sets ocr.mode = "vlm" but this binary was built without the vlm-ocr feature.
       Rebuild with: just build profile=legal
       Or set:       ocr.mode = "auto_embedded"
```

---

### 5. Hot-Reload

**Trigger**:
- `SIGHUP` (Unix)
- Named Windows event `Global\AnnoRagReload`, polled every 2 s
- `anno-rag config reload` CLI command (sends signal to running server PID from lockfile)

**Provider swap safety**: providers are held behind `Arc<RwLock<ProviderSet>>`. On reload, a new `ProviderSet` is built in the background. Once ready, a write lock atomically swaps the pointer. In-flight requests holding a read lock complete against the old set; new requests get the new one. No request is dropped.

**What gets reloaded vs. rejected**:

| Config change | Action |
|---------------|--------|
| `models.embedder` | Re-init embedder; **warn + abort reload if new dim ≠ stored index dim** |
| `models.ner`, `models.reranker` | Re-init that provider only |
| `ocr.*` | Re-init OCR pipeline; in-flight ingests complete against old config |
| `profile` | Full provider reinit |
| `data_dir` | **Rejected** — requires restart; logged as error, reload aborted |
| `index.*`, `chunking.*`, `memory.*` | Accepted; takes effect on next operation |

**`anno-rag config show --watch`**: live-rerenders the config table on each reload signal (useful for debugging active servers).

---

## Rust Crate Impact

| Crate | Change |
|-------|--------|
| `anno-rag` | `config.rs` split into `config/v2/` module tree; `v1_compat.rs` shim; `ModelDimRegistry` |
| `anno-rag-bin` | `config_cmd.rs` gains `migrate` subcommand; `main.rs` gains reload signal handler |
| `anno-rag-mcp` | `ProviderSet` behind `Arc<RwLock<_>>` for hot-swap |
| `anno-rag-tabular` | VLM config moved into `AnnoRagConfig`; `llm::vlm` reads from shared config |
| `anno-config-meta` | Proc-macro extended for nested block types (currently only flat struct) |

---

## Out of Scope

- Remote config sources (HTTP, Vault, etcd) — out of scope; `local-path` covers air-gapped
- Multi-tenant per-corpus config — future work
- Config encryption — handled by the vault layer, not the config layer
- GUI config editor — Hacienda Workbench concern
