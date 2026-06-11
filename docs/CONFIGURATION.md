# anno-rag Configuration Guide

anno-rag resolves configuration from three layers, merged left-to-right (right wins):

```
~/.anno-rag/config.toml  →  environment variables  →  CLI flags
```

## Quick start

```bash
# Scaffold a config file at the default location
anno-rag config init

# Show the effective resolved config (all layers merged)
anno-rag config show

# Validate the config file without starting the server
anno-rag config validate

# Show the JSON schema for editor auto-complete
anno-rag config show --schema
```

The config file is optional. If it does not exist, all defaults apply.

---

## Common recipes

### Enable OCR for scanned PDFs

OCR is on by default (`auto_embedded`). The `embedded-ocr` Cargo feature must be
compiled in — the distributed binary ships with it and sets `ocr_mode = "auto_embedded"`
out of the box.

**Config file:**
```toml
ocr_mode = "auto_embedded"
```

**Environment:**
```bash
export ANNO_RAG_OCR_MODE=auto_embedded
```

**CLI (one-shot ingest):**
```bash
anno-rag ingest ~/docs --ocr-mode auto_embedded
```

To cap how long OCR runs per folder (useful for large batches):
```toml
ocr_mode = "auto_embedded"
ocr_batch_budget_secs = 120   # give up after 2 min, defer the rest
```

To use PaddleOCR as the primary backend instead of Tesseract:
```toml
ocr_mode = "auto_embedded"
ocr_backend = "paddleocr"
```

---

### Switch the embedding model

The default is `OrdalieTech/Solon-embeddings-large-0.1` (1024-dim), a French legal
embedding model from OrdalieTech.

```toml
embed_model = "OrdalieTech/Solon-embeddings-large-0.1"
embed_dim   = 1024
```

**Important:** `embed_dim` must exactly match the output dimension of `embed_model`.
If you change this on an existing index you must re-ingest all documents — the stored
vectors will have the wrong dimension.

Other validated models and their dims:

| Model | Dim | Notes |
|-------|-----|-------|
| `OrdalieTech/Solon-embeddings-large-0.1` | 1024 | **Default** — French legal |
| `intfloat/multilingual-e5-small` | 384 | Lightweight multilingual |
| `intfloat/multilingual-e5-base`  | 768 | Multilingual, balanced |
| `intfloat/multilingual-e5-large` | 1024 | Multilingual, high recall |
| `BAAI/bge-m3`                    | 1024 | Multilingual, top-tier |

---

### Set the GLiNER / NER warmup model

anno-rag pre-warms an NER model at startup. The default is `fastino/gliner2-multi-v1`,
the GLiNER2 Candle/LoRA model optimised for French legal NER.

```toml
ner_warmup_model = "fastino/gliner2-multi-v1"
```

Other validated GLiNER variants:

| Model | Notes |
|-------|-------|
| `fastino/gliner2-multi-v1` | **Default** — GLiNER2 Candle LoRA, French legal |
| `urchade/gliner_multi-v2.1` | Multilingual, balanced |
| `urchade/gliner_large-v2.1` | Higher recall, more RAM |
| `numind/NuNER_Zero`          | Zero-shot NER, lighter |

To disable NER warmup (saves ~30 s cold start, NER runs on first request):
```toml
ner_warmup_model = ""   # empty string disables warmup
```

---

### Choose a GPU accelerator

```toml
accelerator = "auto"    # default — pick best available at runtime
accelerator = "cpu"     # always use CPU
accelerator = "metal"   # Apple Silicon (requires gpu-metal feature)
accelerator = "cuda"    # NVIDIA (requires gpu-cuda feature)
```

`auto` probes CUDA with a 2-second timeout and falls back to Metal, then CPU.
Force a specific value when you know what's available to skip the probe.

**Build requirements:**

| Accelerator | Cargo feature | Platform |
|-------------|---------------|----------|
| `metal` | `gpu-metal` | macOS Apple Silicon |
| `cuda`  | `gpu-cuda`  | Linux / Windows + NVIDIA driver |
| `cpu`   | (none)      | any |

---

### Enable the cross-encoder reranker

The reranker requires the `rerank` Cargo feature (compiled into the distributed
binary). It re-scores the top-N candidates from the hybrid retrieval step using
a cross-encoder, significantly improving precision.

```toml
# Uses onnx-community/bge-reranker-v2-m3-ONNX (~571 MB download on first use)
# No field needed — just enable the feature and the default model is used.
```

To change the model or tune the pool:
```toml
rerank_model      = "onnx-community/bge-reranker-v2-m3-ONNX"   # default
rerank_onnx_file  = "onnx/model_int8.onnx"                      # default (INT8)
rerank_pool_size  = 30    # candidates fetched before reranking (default)
rerank_batch_size = 8     # pairs per ONNX forward pass (default)
```

For higher quality at the cost of ~25 % more ONNX memory, switch to the Q4F16 file:
```toml
rerank_onnx_file = "onnx/model_q4f16.onnx"
```

---

### Tune GDPR / PII detection layers

```toml
gdpr_layers = "defense"   # default — balanced precision/recall
```

| Value | What it activates |
|-------|-------------------|
| `basic`   | Regex-only (fast, low recall) |
| `defense` | Regex + heuristics (default) |
| `shadow`  | defense + NER model |
| `full`    | All layers including graph-entity cross-referencing |

`full` is the most thorough but adds latency proportional to document length.

---

### Change the memory NER mode

Controls how entity extraction runs when `memory_save` is called.

```toml
memory_ner_mode = "async"   # default — returns immediately, NER runs in background
memory_ner_mode = "sync"    # blocks until NER completes (higher latency, older behavior)
memory_ner_mode = "disabled"  # skips NER entirely (fastest)
```

---

### Enable structured native PDF extraction

For PDFs with a real text layer (not scanned), structured extraction produces
richer chunking with header/footer/table awareness:

```toml
advanced_pdf_native = "structured"

# Optional fine-tuning:
pdf_keep_headers               = false  # strip running page headers (default)
pdf_keep_footers               = false  # strip running page footers (default)
pdf_extract_annotations        = false  # include comment/highlight annotations
pdf_hierarchy_clusters         = 6      # font-size clusters for heading detection (1–7)
pdf_allow_single_column_tables = false  # include single-column pseudo-tables
pdf_structured_sidecar         = false  # write a diagnostic JSON sidecar alongside each PDF
```

---

### Use half-precision embedder (experimental)

Halves RSS for the embedder (~236 MB → ~118 MB) at the risk of NaN vectors on CPU:

```toml
embedder_dtype = "f16"   # experimental — test before deploying
```

Leave unset (or set to `"f32"`) for stable behaviour.

---

### Tune chunking

```toml
chunk_max_chars = 2048   # default — maximum characters per chunk
chunk_overlap   = 256    # default — overlap between adjacent chunks
```

Smaller `chunk_max_chars` improves retrieval precision for short queries;
larger values preserve more context per chunk.

---

### Tune the vector index

The IVF_HNSW_SQ index is only built when the chunk count exceeds a threshold
(flat scan is used below it). Lower the threshold for early indexing on small
corpora; raise it to delay index build until the corpus is larger:

```toml
vector_index_threshold = 1000   # default
```

---

### Configure memory graph recall

```toml
graph_max_hops    = 2    # default — BFS depth over entity relationships
graph_per_hop_limit = 50  # default — max candidates per hop
```

Increasing `graph_max_hops` improves recall across loosely-linked entities but
can fan out exponentially on popular entities. Raise `graph_per_hop_limit` when
you have many memories per entity.

---

### Memory conflict detection threshold

When a new memory shares an entity with an existing one, anno-rag auto-invalidates
the old one if cosine similarity exceeds this threshold:

```toml
conflict_cosine_threshold = 0.85   # default (0.0–1.0)
```

Raise toward 1.0 to be more permissive (keep both); lower to catch more
re-statements.

---

### Entity aliases

Map surface forms that appear in documents to a canonical form used in the vault:

```toml
entity_aliases = { "me dupont" = "dupont", "maitre dupont" = "dupont" }
```

Or via environment variable (JSON object):
```bash
export ANNO_RAG_ENTITY_ALIASES='{"me dupont":"dupont"}'
```

---

### MCP server name

```toml
mcp_server_name = "anno-rag"   # default — name advertised on MCP initialize
```

---

## Per-command flag overrides

All config fields are exposed as CLI flags on `ingest` and `search`.
Flags override the config file and env vars for that single invocation.

```bash
# One-shot ingest with OCR, larger model, metal GPU
anno-rag ingest ~/docs \
  --ocr-mode auto_embedded \
  --embed-model intfloat/multilingual-e5-large \
  --embed-dim 1024 \
  --accelerator metal

# One-shot search with full GDPR and reranker pool of 50
anno-rag search "clause résiliation" \
  --gdpr-layers full \
  --rerank-pool-size 50
```

---

## Complete field reference

| Field | Env var | CLI flag | Type | Default | Since |
|-------|---------|----------|------|---------|-------|
| `data_dir` | `ANNO_RAG_DATA_DIR` | `--data-dir` | path | `~/.anno-rag` | 0.1 |
| `embed_model` | `ANNO_RAG_EMBED_MODEL` | `--embed-model` | string | `OrdalieTech/Solon-embeddings-large-0.1` | 0.1 |
| `embed_dim` | `ANNO_RAG_EMBED_DIM` | `--embed-dim` | usize | `1024` | 0.1 |
| `default_top_k` | `ANNO_RAG_DEFAULT_TOP_K` | `--default-top-k` | usize | `10` | 0.1 |
| `chunk_max_chars` | `ANNO_RAG_CHUNK_MAX_CHARS` | `--chunk-max-chars` | usize | `2048` | 0.1 |
| `chunk_overlap` | `ANNO_RAG_CHUNK_OVERLAP` | `--chunk-overlap` | usize | `256` | 0.1 |
| `gdpr_layers` | `ANNO_GDPR_LAYERS` | `--gdpr-layers` | `basic\|defense\|shadow\|full` | `defense` | 0.10 |
| `vector_index_threshold` | `ANNO_RAG_VECTOR_INDEX_THRESHOLD` | `--vector-index-threshold` | usize | `1000` | 0.5 |
| `ner_warmup_model` | `ANNO_RAG_NER_WARMUP_MODEL` | `--ner-warmup-model` | string? | `fastino/gliner2-multi-v1` | 0.6 |
| `mcp_server_name` | `ANNO_RAG_MCP_SERVER_NAME` | `--mcp-server-name` | string | `anno-rag` | 0.3 |
| `ocr_mode` | `ANNO_RAG_OCR_MODE` | `--ocr-mode` | `off\|auto_embedded` | `auto_embedded` | 0.11 |
| `enable_ocr` | `ANNO_RAG_ENABLE_OCR` | `--enable-ocr` | bool | `false` | 0.4 |
| `ocr_batch_budget_secs` | `ANNO_RAG_OCR_BATCH_BUDGET_SECS` | `--ocr-batch-budget-secs` | u64? | _(unlimited)_ | 0.11 |
| `ocr_cache_enabled` | `ANNO_RAG_OCR_CACHE_ENABLED` | `--ocr-cache-enabled` | bool | `true` | 0.11 |
| `ocr_backend` | `ANNO_RAG_OCR_BACKEND` | `--ocr-backend` | string? | _(tesseract)_ | 0.12 |
| `advanced_pdf_native` | `ANNO_RAG_ADVANCED_PDF_NATIVE` | `--advanced-pdf-native` | `off\|structured` | `off` | 0.11 |
| `pdf_keep_headers` | `ANNO_RAG_PDF_KEEP_HEADERS` | `--pdf-keep-headers` | bool | `false` | 0.11 |
| `pdf_keep_footers` | `ANNO_RAG_PDF_KEEP_FOOTERS` | `--pdf-keep-footers` | bool | `false` | 0.11 |
| `pdf_extract_annotations` | `ANNO_RAG_PDF_EXTRACT_ANNOTATIONS` | `--pdf-extract-annotations` | bool | `false` | 0.11 |
| `pdf_hierarchy_clusters` | `ANNO_RAG_PDF_HIERARCHY_CLUSTERS` | `--pdf-hierarchy-clusters` | usize | `6` | 0.11 |
| `pdf_allow_single_column_tables` | `ANNO_RAG_PDF_ALLOW_SINGLE_COLUMN_TABLES` | `--pdf-allow-single-column-tables` | bool | `false` | 0.11 |
| `pdf_structured_sidecar` | `ANNO_RAG_PDF_STRUCTURED_SIDECAR` | `--pdf-structured-sidecar` | bool | `false` | 0.12 |
| `embedder_dtype` | `ANNO_RAG_EMBEDDER_DTYPE` | `--embedder-dtype` | string? | _(f32)_ | 0.12 |
| `accelerator` | `ANNO_ACCELERATOR` | `--accelerator` | `auto\|cpu\|metal\|cuda` | `auto` | 0.10 |
| `rerank_model` | `ANNO_RAG_RERANK_MODEL` | `--rerank-model` | string | `onnx-community/bge-reranker-v2-m3-ONNX` | 0.12 |
| `rerank_onnx_file` | `ANNO_RAG_RERANK_ONNX_FILE` | `--rerank-onnx-file` | string | `onnx/model_int8.onnx` | 0.12 |
| `rerank_pool_size` | `ANNO_RAG_RERANK_POOL_SIZE` | `--rerank-pool-size` | usize | `30` | 0.12 |
| `rerank_batch_size` | `ANNO_RAG_RERANK_BATCH_SIZE` | `--rerank-batch-size` | usize | `8` | 0.12 |
| `memory_collection_name` | `ANNO_RAG_MEMORY_COLLECTION_NAME` | `--memory-collection-name` | string | `memories` | 0.8 |
| `memory_embedding_dim` | `ANNO_RAG_MEMORY_EMBEDDING_DIM` | `--memory-embedding-dim` | usize | `1024` | 0.8 |
| `memory_ner_mode` | `ANNO_RAG_MEMORY_NER_MODE` | `--memory-ner-mode` | `disabled\|async\|sync` | `async` | 0.9 |
| `compaction_interval_secs` | `ANNO_RAG_COMPACTION_INTERVAL_SECS` | `--compaction-interval-secs` | u64 | `86400` | 0.9 |
| `compaction_min_age_secs` | `ANNO_RAG_COMPACTION_MIN_AGE_SECS` | `--compaction-min-age-secs` | u64 | `3600` | 0.9 |
| `entity_aliases` | `ANNO_RAG_ENTITY_ALIASES` | `--entity-aliases` | JSON obj | `{}` | 0.10 |
| `conflict_cosine_threshold` | `ANNO_RAG_CONFLICT_COSINE_THRESHOLD` | `--conflict-cosine-threshold` | f32 | `0.85` | 0.10 |
| `graph_max_hops` | `ANNO_RAG_GRAPH_MAX_HOPS` | `--graph-max-hops` | u8 | `2` | 0.10 |
| `graph_per_hop_limit` | `ANNO_RAG_GRAPH_PER_HOP_LIMIT` | `--graph-per-hop-limit` | usize | `50` | 0.10 |

> **Deprecated fields** (`enable_ocr`, `tesseract_path`) are still parsed for backwards compatibility but should not be used in new configs — use `ocr_mode = "auto_embedded"` instead.

---

## Example: full production config

```toml
# ~/.anno-rag/config.toml

# Storage
data_dir = "/opt/anno-rag/data"

# Embedder — Solon-large (default, 1024-dim, French legal)
embed_model = "OrdalieTech/Solon-embeddings-large-0.1"
embed_dim   = 1024

# OCR — enabled by default; set a 3-minute budget per folder
ocr_mode              = "auto_embedded"
ocr_batch_budget_secs = 180

# Structured PDF extraction
advanced_pdf_native   = "structured"
pdf_keep_headers      = false
pdf_keep_footers      = false
pdf_extract_annotations = true

# NER — fastino GLiNER2, pre-warm on startup (default)
ner_warmup_model = "fastino/gliner2-multi-v1"

# GPU — auto-detect best available
accelerator = "auto"

# GDPR — shadow layers for high-sensitivity corpora
gdpr_layers = "shadow"

# Reranker — default model, bigger pool
rerank_pool_size  = 50
rerank_batch_size = 16

# Memory
memory_ner_mode            = "async"
compaction_interval_secs   = 43200   # compact every 12h
conflict_cosine_threshold  = 0.90    # tighter conflict detection

# Retrieval
default_top_k             = 15
vector_index_threshold    = 500
```
