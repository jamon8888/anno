# Anno Config v2 — Typed Provider Registry

**Date**: 2026-06-24  
**Status**: Approved — ready for implementation planning  
**Branch target**: `feat/config-v2`  
**Last reviewed**: 2026-06-24 (full codebase read — config.rs complete, vlm routing.rs, MCP lib.rs)

---

## Problem Statement

`AnnoRagConfig` is a flat ~50-field struct in `crates/anno-rag/src/config.rs` (66 KB). Key pain points:

- `embed_model` and `embed_dim` are separate fields — must be kept in sync manually; mismatch silently corrupts the index
- VLM-OCR prompt language (`VLM_OCR_PROMPT_FR`) is hardcoded in `vlm.rs`; `prompt_template` is not configurable at all
- VLM config exists in `AnnoRagConfig` (v0.15: `vlm_backend`, `vlm_vllm_url`, `vlm_local_url`, `vlm_safetensors_model_id`, `vlm_gguf_model_id`) but `routing.rs:100-101` hardcodes `"lightonai/LightOnOCR-2-1B"` and ignores the model-id fields — the config fields are wired but never read
- No named profiles — switching from "lightweight dev" to "full legal stack" requires editing 10+ fields
- Cargo features (`embedded-ocr`, `vlm-ocr`) are undiscoverable; no documented build presets
- No hot-reload — `AnnoRagServer` uses `Arc<OnceCell<Arc<Pipeline>>>` which can only be set once; switching models requires a full MCP server restart

---

## Design

### 1. Config Schema (v2)

Schema version is declared at the top of `anno-rag.toml`. The `schema_version = 2` key triggers the new block parser; missing or `1` triggers the v1 compat shim.

```toml
schema_version = 2
profile = "legal-full"   # optional; see §3
data_dir = "~/.anno-rag"
accelerator = "auto"     # auto | cpu | cuda | metal
mcp_server_name = "anno-rag"
default_top_k = 10
gdpr_layers = "defense"  # basic | defense | shadow | full

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
warmup_model = "fastino/gliner2-multi-v1"

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
confidence_threshold = 0.6  # VLM quality score below this → embedded fallback

[ocr.embedded]
backend = "tesseract"     # tesseract | paddleocr
cache = true
batch_budget_secs = null  # null = unlimited

[ocr.vlm]                 # only active when mode = "vlm"
# Security: server_url MUST be a loopback address (localhost / 127.0.0.1 / [::1]).
# Non-loopback URLs are rejected at startup by the trust-boundary guard (Spec B §4.3).
# To reach a remote GPU server, set up a local port-forward first.
backend = "vllm"           # vllm (safetensors, vLLM server) | local (GGUF, llama-server)
vllm_url = "http://127.0.0.1:8000"
local_url = "http://127.0.0.1:8080"
safetensors_model_id = "lightonai/LightOnOCR-2-1B"
gguf_model_id = "Mungert/LightOnOCR-1B-1025-GGUF"
language = "fra"           # language hint injected into the OCR prompt
prompt_template = null     # null = built-in template; set to override entirely

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
embedding_dim = 1024       # separate from [models.embedder] dim; memory may use a different model
ner_mode = "async"         # disabled | async | sync
compaction_interval_secs = 86400
compaction_min_age_secs = 3600
conflict_cosine_threshold = 0.85
graph_max_hops = 2
graph_per_hop_limit = 50
entity_aliases = {}        # JSON object: { "me dupont": "dupont" }

[pdf]
native_mode = "off"        # off | structured
keep_headers = false
keep_footers = false
extract_annotations = false
hierarchy_clusters = 6
allow_single_column_tables = false
structured_sidecar = false
```

**`local-path` type**: `model_id` is interpreted as an absolute filesystem path to a directory containing `config.json` + safetensors weights. Used by the `offline` profile where no HF Hub access is available.

**Auto-dim detection**: `Embedder::load` already downloads and parses `config.json` from HuggingFace (or reads it from local cache). The `candle_transformers::models::bert::Config` struct has a `hidden_size` field that is the actual output dimension. In v2, `Embedder::load` reads `config.hidden_size` and returns it alongside the loaded model; `Store::open` uses that value instead of `cfg.embed_dim`. No lookup table, no separate probe, no manual sync required. The `embed_dim` config field is removed; `ANNO_RAG_EMBED_DIM` is deprecated (accepted for one version as a validation override, then removed).

**`memory.embedding_dim`** stays independent from `models.embedder` dim by design — the memory store may use a different embedder than the document index (currently defaults to 1024 while the document embedder uses 768).

**`[models.*]` type discriminant** makes it possible to add provider backends without new top-level fields.

**VLM loopback security constraint**: `RoutingVlmClient::from_config` enforces that `vllm_url` and `local_url` must resolve to a loopback host (`localhost`, `127.0.0.1`, `[::1]`). Remote GPU servers must be reached via a local port-forward. This is not a config v2 change — it exists today and is preserved.

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

Example user profile (note: VLM URLs must remain loopback — use a port-forward for remote GPU):
```toml
# ~/.anno-rag/profiles/my-fr-legal.toml
[models.embedder]
model_id = "dangvantuan/sentence-camembert-large"

[ocr]
mode = "vlm"

[ocr.vlm]
vllm_url = "http://127.0.0.1:8000"   # port-forward from remote GPU server
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

**V1 → V2 field mapping** (complete):

| V1 flat field | V2 block path |
|---------------|--------------|
| `embed_model` | `models.embedder.model_id` |
| `embed_dim` | *(deprecated — auto-detected; still accepted as override)* |
| `embedder_dtype` | `models.embedder.dtype` |
| `ner_model_id` | `models.ner.model_id` |
| `ner_onnx_precision` | `models.ner.precision` |
| `ner_candle_model_id` | `models.ner.candle_fallback_id` |
| `ner_warmup_model` | `models.ner.warmup_model` |
| `ner_pii_model_id` | `models.ner_pii.model_id` |
| `rerank_model` | `models.reranker.model_id` |
| `rerank_onnx_file` | `models.reranker.onnx_file` |
| `rerank_pool_size` | `models.reranker.pool_size` |
| `rerank_batch_size` | `models.reranker.batch_size` |
| `ocr_mode` | `ocr.mode` |
| `ocr_backend` | `ocr.embedded.backend` |
| `ocr_cache_enabled` | `ocr.embedded.cache` |
| `ocr_batch_budget_secs` | `ocr.embedded.batch_budget_secs` |
| `vlm_backend` | `ocr.vlm.backend` |
| `vlm_vllm_url` | `ocr.vlm.vllm_url` |
| `vlm_local_url` | `ocr.vlm.local_url` |
| `vlm_confidence_threshold` | `ocr.confidence_threshold` |
| `vlm_safetensors_model_id` | `ocr.vlm.safetensors_model_id` |
| `vlm_gguf_model_id` | `ocr.vlm.gguf_model_id` |
| `index_distance` | `index.distance` |
| `vector_index_threshold` | `index.vector_index_threshold` |
| `index_num_partitions` | `index.num_partitions` |
| `search_nprobes` | `index.nprobes` |
| `search_refine_factor` | `index.refine_factor` |
| `chunk_max_chars` | `chunking.max_chars` |
| `chunk_overlap` | `chunking.overlap` |
| `memory_collection_name` | `memory.collection_name` |
| `memory_embedding_dim` | `memory.embedding_dim` |
| `memory_ner_mode` | `memory.ner_mode` |
| `compaction_interval_secs` | `memory.compaction_interval_secs` |
| `compaction_min_age_secs` | `memory.compaction_min_age_secs` |
| `conflict_cosine_threshold` | `memory.conflict_cosine_threshold` |
| `graph_max_hops` | `memory.graph_max_hops` |
| `graph_per_hop_limit` | `memory.graph_per_hop_limit` |
| `entity_aliases` | `memory.entity_aliases` |
| `advanced_pdf_native` | `pdf.native_mode` |
| `pdf_keep_headers` | `pdf.keep_headers` |
| `pdf_keep_footers` | `pdf.keep_footers` |
| `pdf_extract_annotations` | `pdf.extract_annotations` |
| `pdf_hierarchy_clusters` | `pdf.hierarchy_clusters` |
| `pdf_allow_single_column_tables` | `pdf.allow_single_column_tables` |
| `pdf_structured_sidecar` | `pdf.structured_sidecar` |
| `enable_ocr` | *(deprecated — use `ocr.mode = "auto_embedded"`)* |
| `tesseract_path` | *(deprecated — embedded OCR does not use it)* |

**Env var mapping** (all existing vars preserved — new v2 vars added alongside):

| V1 env var | V2 config path |
|------------|---------------|
| `ANNO_RAG_EMBED_MODEL` | `models.embedder.model_id` |
| `ANNO_RAG_VLM_BACKEND` | `ocr.vlm.backend` |
| `ANNO_RAG_VLM_VLLM_URL` | `ocr.vlm.vllm_url` |
| `ANNO_RAG_VLM_LOCAL_URL` | `ocr.vlm.local_url` |
| `ANNO_RAG_VLM_SAFETENSORS_MODEL_ID` | `ocr.vlm.safetensors_model_id` |
| `ANNO_RAG_VLM_GGUF_MODEL_ID` | `ocr.vlm.gguf_model_id` |
| `ANNO_RAG_VLM_CONFIDENCE_THRESHOLD` | `ocr.confidence_threshold` |
| … | … |

New v2-only vars: `ANNO_RAG_OCR_VLM_LANGUAGE`, `ANNO_RAG_OCR_VLM_PROMPT_TEMPLATE`.

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
description = "Adds VLM-OCR client (LightOnOCR). Requires loopback VLM server at runtime."
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

**Structural prerequisite**: `AnnoRagServer` in `anno-rag-mcp/src/lib.rs` currently uses `Arc<OnceCell<Arc<Pipeline>>>` — this can only be set once and cannot be replaced. Hot-reload requires changing this to `Arc<RwLock<Option<Arc<Pipeline>>>>`. This is a real structural change to the MCP server, not just config plumbing. It must be implemented before the reload signal handler can work.

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
| `anno-rag` | `config.rs` split into `config/v2/` module tree; `v1_compat.rs` shim; `Embedder::load` returns actual dim from `config.hidden_size`; `Store::open` uses returned dim instead of `cfg.embed_dim`; new fields: `ocr.vlm.language`, `ocr.vlm.prompt_template`. Callees of `Pipeline::new` (`vault::open`, `model_cache::migrate_legacy_cache`, `legal/store::setup_indexes`) all read from `AnnoRagConfig` and need updating for the v2 field paths. |
| `anno-rag-bin` | `config_cmd.rs` gains `migrate` subcommand; `main.rs` gains reload signal handler |
| `anno-rag-mcp` | `Arc<OnceCell<Arc<Pipeline>>>` → `Arc<RwLock<Option<Arc<Pipeline>>>>` for hot-swap; reload signal listener |
| `anno-rag-tabular` | `routing.rs`: wire `vlm_safetensors_model_id` / `vlm_gguf_model_id` from config instead of hardcoded strings |
| `anno-config-meta` | Proc-macro extended for nested block types (currently only flat struct) |

---

## Out of Scope

- Remote config sources (HTTP, Vault, etcd) — out of scope; `local-path` covers air-gapped
- Multi-tenant per-corpus config — future work
- Config encryption — handled by the vault layer, not the config layer
- GUI config editor — Hacienda Workbench concern
- Allowing non-loopback VLM URLs — intentionally excluded (trust-boundary policy)
