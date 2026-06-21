# Configuration Reference

> Auto-generated from `AnnoRagConfig`. Do not edit by hand — run `cargo run -p anno-rag --bin schema-gen --features generate-schema`.

Precedence (lowest → highest): defaults → `~/.anno-rag/config.toml` → env vars → CLI flags.

| Field | Env var | CLI flag | Default | Since | Description |
|-------|---------|----------|---------|-------|-------------|
| `data_dir` | `ANNO_RAG_DATA_DIR` | `--data-dir` | `"~/.anno-rag"` | 0.1 | Root directory for vault, index, and model weights. Default: platform data dir (Windows: %APPDATA%\anno-rag, macOS: ~/Library/Application Support/anno-rag) |
| `embed_model` | `ANNO_RAG_EMBED_MODEL` | `--embed-model` | `"AlpEge/bge-m3-onnx-int8"` | 0.1 | HuggingFace model ID for the embedder. Default: OrdalieTech/Solon-embeddings-large-0.1 |
| `embed_dim` | `ANNO_RAG_EMBED_DIM` | `--embed-dim` | `1024` | 0.1 | Vector dimension; must match embedder output. Default: 1024 |
| `default_top_k` | `ANNO_RAG_DEFAULT_TOP_K` | `--default-top-k` | `10` | 0.1 | Default number of results returned by search. Default: 10 |
| `chunk_max_chars` | `ANNO_RAG_CHUNK_MAX_CHARS` | `--chunk-max-chars` | `2048` | 0.1 | Max chunk size in characters. Default: 2048 |
| `chunk_overlap` | `ANNO_RAG_CHUNK_OVERLAP` | `--chunk-overlap` | `256` | 0.1 | Chunk overlap in characters. Default: 256 |
| `gdpr_layers` | `ANNO_GDPR_LAYERS` | `--gdpr-layers` | `"defense"` | 0.10 | PII detection layer set: basic\|defense\|shadow\|full. Default: defense |
| `vector_index_threshold` | `ANNO_RAG_VECTOR_INDEX_THRESHOLD` | `--vector-index-threshold` | `1000` | 0.5 | Chunk count above which IVF_HNSW_SQ index is built. Default: 1000 |
| `ner_warmup_model` | `ANNO_RAG_NER_WARMUP_MODEL` | `--ner-warmup-model` | `"fastino/gliner2-multi-v1"` | 0.6 | HF Hub model ID to pre-warm on startup. Default: fastino/gliner2-multi-v1 |
| `ner_model_id` | `ANNO_RAG_NER_MODEL` | `--ner-model` | `"SemplificaAI/gliner2-multi-v1-onnx"` | 0.12 | HuggingFace model ID for the ONNX NER detector. Default: SemplificaAI/gliner2-multi-v1-onnx |
| `ner_pii_model_id` | `ANNO_RAG_NER_PII_MODEL` | `--ner-pii-model` | `"SemplificaAI/gliner2-privacy-filter-PII-multi"` | 0.14 | HuggingFace model ID for the ONNX PII NER detector. Default: SemplificaAI/gliner2-privacy-filter-PII-multi |
| `ner_onnx_precision` | `ANNO_RAG_NER_ONNX_PRECISION` | `--ner-onnx-precision` | `"fp16"` | 0.13 | ONNX graph precision for NER: fp16 (default, ~250 MB) or fp32 (~500 MB). Default: fp16 |
| `index_distance` | `ANNO_RAG_INDEX_DISTANCE` | `--index-distance` | `"cosine"` | 0.13 | Vector index distance: cosine (default), l2, or dot. Default: cosine |
| `index_num_partitions` | `ANNO_RAG_INDEX_NUM_PARTITIONS` | `--index-num-partitions` | *(unset)* | 0.13 | IVF partitions for the vector index. Default: auto (unset) |
| `search_nprobes` | `ANNO_RAG_SEARCH_NPROBES` | `--search-nprobes` | `20` | 0.13 | IVF partitions probed per query (recall vs speed). Default: 20 |
| `search_refine_factor` | `ANNO_RAG_SEARCH_REFINE_FACTOR` | `--search-refine-factor` | `10` | 0.13 | SQ refine factor (1 = off, 10 = default). Default: 10 |
| `ner_candle_model_id` | `ANNO_RAG_NER_CANDLE_MODEL` | `--ner-candle-model` | `"fastino/gliner2-multi-v1"` | 0.12 | HuggingFace model ID for the Candle NER detector. Default: fastino/gliner2-multi-v1 |
| `mcp_server_name` | `ANNO_RAG_MCP_SERVER_NAME` | `--mcp-server-name` | `"anno-rag"` | 0.3 | MCP server name advertised on initialize. Default: anno-rag |
| `ocr_mode` | `ANNO_RAG_OCR_MODE` | `--ocr-mode` | `"auto_embedded"` | 0.11 | OCR mode: off\|auto_embedded. Default: auto_embedded |
| `enable_ocr` | `ANNO_RAG_ENABLE_OCR` | `--enable-ocr` | `false` | 0.4 | [DEPRECATED] Use --ocr-mode auto_embedded instead. Default: false |
| `tesseract_path` | `ANNO_RAG_TESSERACT_PATH` | `--tesseract-path` | *(unset)* | 0.4 | [DEPRECATED] Legacy path to tesseract binary; ignored by embedded OCR. Default: none |
| `ocr_batch_budget_secs` | `ANNO_RAG_OCR_BATCH_BUDGET_SECS` | `--ocr-batch-budget-secs` | *(unset)* | 0.11 | Per-folder OCR wall-clock budget in seconds. Default: none (unlimited) |
| `ocr_cache_enabled` | `ANNO_RAG_OCR_CACHE_ENABLED` | `--ocr-cache-enabled` | `true` | 0.11 | Enable kreuzberg extraction cache. Default: true |
| `ocr_backend` | `ANNO_RAG_OCR_BACKEND` | `--ocr-backend` | *(unset)* | 0.12 | Primary OCR backend passed to kreuzberg (e.g. paddleocr). Default: none (tesseract) |
| `advanced_pdf_native` | `ANNO_RAG_ADVANCED_PDF_NATIVE` | `--advanced-pdf-native` | `"off"` | 0.11 | Native PDF extraction profile: off\|structured. Default: off |
| `pdf_keep_headers` | `ANNO_RAG_PDF_KEEP_HEADERS` | `--pdf-keep-headers` | `false` | 0.11 | Preserve running headers in advanced native PDF. Default: false |
| `pdf_keep_footers` | `ANNO_RAG_PDF_KEEP_FOOTERS` | `--pdf-keep-footers` | `false` | 0.11 | Preserve running footers in advanced native PDF. Default: false |
| `pdf_extract_annotations` | `ANNO_RAG_PDF_EXTRACT_ANNOTATIONS` | `--pdf-extract-annotations` | `false` | 0.11 | Extract PDF annotations in advanced native PDF. Default: false |
| `pdf_hierarchy_clusters` | `ANNO_RAG_PDF_HIERARCHY_CLUSTERS` | `--pdf-hierarchy-clusters` | `6` | 0.11 | Font-size cluster count for PDF hierarchy (1-7). Default: 6 |
| `pdf_allow_single_column_tables` | `ANNO_RAG_PDF_ALLOW_SINGLE_COLUMN_TABLES` | `--pdf-allow-single-column-tables` | `false` | 0.11 | Allow single-column pseudo-tables in PDF extraction. Default: false |
| `pdf_structured_sidecar` | `ANNO_RAG_PDF_STRUCTURED_SIDECAR` | `--pdf-structured-sidecar` | `false` | 0.12 | Emit diagnostic structured sidecar for advanced PDFs. Default: false |
| `embedder_dtype` | `ANNO_RAG_EMBEDDER_DTYPE` | `--embedder-dtype` | *(unset)* | 0.12 | Embedder weight dtype: f32 (default) or f16 (experimental). Default: none (f32) |
| `accelerator` | `ANNO_ACCELERATOR` | `--accelerator` | `"auto"` | 0.10 | Runtime accelerator: auto\|cpu\|metal\|cuda. Default: auto |
| `rerank_model` | `ANNO_RAG_RERANK_MODEL` | `--rerank-model` | `"onnx-community/bge-reranker-v2-m3-ONNX"` | 0.12 | HF Hub model ID for the cross-encoder reranker. Default: onnx-community/bge-reranker-v2-m3-ONNX |
| `rerank_onnx_file` | `ANNO_RAG_RERANK_ONNX_FILE` | `--rerank-onnx-file` | `"onnx/model_int8.onnx"` | 0.12 | ONNX file within rerank_model. Default: onnx/model_int8.onnx |
| `rerank_pool_size` | `ANNO_RAG_RERANK_POOL_SIZE` | `--rerank-pool-size` | `30` | 0.12 | RRF candidates to over-fetch before reranking. Default: 30 |
| `rerank_batch_size` | `ANNO_RAG_RERANK_BATCH_SIZE` | `--rerank-batch-size` | `8` | 0.12 | Max (query,passage) pairs per ONNX reranker batch. Default: 8 |
| `memory_collection_name` | `ANNO_RAG_MEMORY_COLLECTION_NAME` | `--memory-collection-name` | `"memories"` | 0.8 | LanceDB collection name for memories. Default: memories |
| `memory_embedding_dim` | `ANNO_RAG_MEMORY_EMBEDDING_DIM` | `--memory-embedding-dim` | `1024` | 0.8 | Embedding dimension for memory vectors. Default: 1024 |
| `memory_ner_mode` | `ANNO_RAG_MEMORY_NER_MODE` | `--memory-ner-mode` | `"async"` | 0.9 | NER mode for memory_save: disabled\|async\|sync. Default: async |
| `compaction_interval_secs` | `ANNO_RAG_COMPACTION_INTERVAL_SECS` | `--compaction-interval-secs` | `86400` | 0.9 | Seconds between background compactions. Default: 86400 (24h) |
| `compaction_min_age_secs` | `ANNO_RAG_COMPACTION_MIN_AGE_SECS` | `--compaction-min-age-secs` | `3600` | 0.9 | Minimum tombstone age before compaction (seconds). Default: 3600 |
| `entity_aliases` | `ANNO_RAG_ENTITY_ALIASES` | `--entity-aliases` | `{}` | 0.10 | JSON object mapping canonical entity surface forms to substituted forms. Default: {} |
| `conflict_cosine_threshold` | `ANNO_RAG_CONFLICT_COSINE_THRESHOLD` | `--conflict-cosine-threshold` | `0.85` | 0.10 | Cosine threshold for memory conflict detection (0.0-1.0). Default: 0.85 |
| `graph_max_hops` | `ANNO_RAG_GRAPH_MAX_HOPS` | `--graph-max-hops` | `2` | 0.10 | Maximum BFS hop count for graph_recall. Default: 2 |
| `graph_per_hop_limit` | `ANNO_RAG_GRAPH_PER_HOP_LIMIT` | `--graph-per-hop-limit` | `50` | 0.10 | Max candidates per BFS hop in graph_recall. Default: 50 |

> **Runtime-only env vars** (not in `config.toml`): `ANNO_MODELS_DIR` (model weights override), `ANNO_RAG_VAULT_PASSPHRASE`, `ANNO_RAG_VAULT_KMS_PROVIDER`, `ANNO_RAG_VAULT_KMS_KEY_ID`.
