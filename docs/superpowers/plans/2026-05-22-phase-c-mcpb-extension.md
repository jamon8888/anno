# Phase C — .mcpb Claude Desktop Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship anno-rag as a one-click Claude Desktop Extension (.mcpb) with lazy MCP startup so models are not needed before install.

**Architecture:** `AnnoRagServer` gains a `tokio::sync::OnceCell<Arc<Pipeline>>` that defers `Pipeline::new` to the first tool call. A new `download_models` MCP tool backgrounds the 970 MB download and returns immediately. Three platform-specific `.mcpb` files are packaged inline in the existing `release.yml` matrix jobs and attached to the GitHub Release.

**Tech Stack:** Rust, tokio 1.51 (sync::OnceCell), rmcp tool macros, GitHub Actions bash, Python 3 (manifest templating), zip (cross-platform), gh CLI.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `scripts/release/mcpb-manifest-template.json` | Create | Base manifest with placeholders |
| `crates/anno-rag-mcp/src/lib.rs` | Modify | Lazy pipeline, `serve_stdio_lazy`, `download_models` tool |
| `crates/anno-rag-bin/src/main.rs` | Modify | Mcp branch auto-detect + `serve_stdio_lazy` call |
| `.github/workflows/release.yml` | Modify | .mcpb packaging steps per matrix job |
| `docs/release/README-release.md` | Modify | Extension install section |

---

## Task C1: Manifest template

**Files:**
- Create: `scripts/release/mcpb-manifest-template.json`

- [ ] **Step 1: Create `scripts/release/` if it does not exist**

```bash
mkdir -p scripts/release
```

- [ ] **Step 2: Write the manifest template**

Create `scripts/release/mcpb-manifest-template.json` with this exact content (the Python script in Task C5 fills in `version`, `compatibility.platforms`, `server.entry_point`, and `mcp_config.command`):

```json
{
  "$schema": "https://raw.githubusercontent.com/modelcontextprotocol/mcpb/main/manifest-schema.json",
  "manifest_version": "0.3",
  "name": "hacienda-anno-rag",
  "display_name": "Hacienda / anno-rag",
  "version": "__VERSION__",
  "description": "Local RAG memory for Claude. Ingest documents, search offline. First use: ask me to 'Set up anno-rag' to download models (~970 MB).",
  "author": {
    "name": "Hacienda"
  },
  "license": "MIT OR Apache-2.0",
  "server": {
    "type": "binary",
    "entry_point": "__BINARY__"
  },
  "mcp_config": {
    "command": "__COMMAND__",
    "args": ["mcp"]
  },
  "compatibility": {
    "platforms": ["__PLATFORM__"]
  },
  "user_config": [
    {
      "id": "ANNO_MODELS_DIR",
      "name": "Models directory",
      "description": "Path to downloaded model files. Leave blank and ask me to 'Set up anno-rag' to download automatically.",
      "type": "directory",
      "required": false
    },
    {
      "id": "ANNO_RAG_VAULT_PASSPHRASE",
      "name": "Vault passphrase",
      "description": "Leave blank to use the OS keyring (recommended). Only set for headless or scripted use.",
      "type": "string",
      "sensitive": true,
      "required": false
    },
    {
      "id": "ANNO_NO_DOWNLOADS",
      "name": "Block model downloads",
      "description": "Prevent all network model downloads. Enable after models are downloaded.",
      "type": "boolean",
      "required": false,
      "default": false
    },
    {
      "id": "TESSERACT_PATH",
      "name": "Tesseract path (optional)",
      "description": "Path to the tesseract executable for OCR support. Leave blank to disable OCR.",
      "type": "file",
      "required": false
    }
  ]
}
```

- [ ] **Step 3: Verify the file is valid JSON**

```bash
python3 -c "import json; json.load(open('scripts/release/mcpb-manifest-template.json')); print('JSON OK')"
```

Expected output: `JSON OK`

- [ ] **Step 4: Commit**

```bash
git add scripts/release/mcpb-manifest-template.json
git commit -m "feat(release): add .mcpb manifest template"
```

---

## Task C2: Lazy `OnceCell<Arc<Pipeline>>` in `AnnoRagServer`

This task refactors `crates/anno-rag-mcp/src/lib.rs` to defer `Pipeline::new` to the first tool call that needs it. The existing `serve_stdio(pipeline, cfg)` function is preserved for tests.

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add at the bottom of `crates/anno-rag-mcp/src/lib.rs`, inside a `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod lazy_tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    /// Server built with new_lazy returns a "not downloaded" error on any
    /// tool call when ANNO_MODELS_DIR is not set and models not present.
    #[tokio::test]
    async fn lazy_server_returns_error_when_models_absent() {
        // Ensure ANNO_MODELS_DIR is not set for this test (use a fresh config
        // pointing at a temp dir with no model files).
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = AnnoRagConfig::default();
        cfg.data_dir = dir.path().to_path_buf();
        // key: all-zeros is fine for this test — pipeline init fails before key use
        let key = [0u8; 32];

        // Temporarily clear ANNO_MODELS_DIR so the lazy check sees no models.
        let saved = std::env::var("ANNO_MODELS_DIR").ok();
        unsafe { std::env::remove_var("ANNO_MODELS_DIR") };

        let server = AnnoRagServer::new_lazy(cfg, key);
        // Call search — this forces pipeline init, which should return Config error
        let result = server
            .search(rmcp::handler::server::wrapper::Parameters(SearchParams {
                query: "test".into(),
                top_k: 1,
                rerank: false,
            }))
            .await;
        // Restore env var
        if let Some(v) = saved {
            std::env::set_var("ANNO_MODELS_DIR", v);
        }
        assert!(
            result.contains("Models not downloaded"),
            "expected 'Models not downloaded' in: {result}"
        );
    }
}
```

- [ ] **Step 2: Run the test to confirm it does not compile yet**

```bash
cargo test -p anno-rag-mcp --lib lazy_tests 2>&1 | head -20
```

Expected: compile error — `new_lazy` is not defined.

- [ ] **Step 3: Update the `use` block at the top of `lib.rs`**

Add `OnceCell` to the existing `use std::sync::Arc;` section:

```rust
use std::sync::Arc;
use tokio::sync::OnceCell;
```

- [ ] **Step 4: Replace the `AnnoRagServer` struct definition**

Find:
```rust
/// State held by the MCP server: a fully-initialized Pipeline plus config.
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<Pipeline>,
    cfg: AnnoRagConfig,
    #[allow(dead_code)] // populated + consumed by the rmcp #[tool_router] macro
    tool_router: ToolRouter<Self>,
}
```

Replace with:
```rust
/// State held by the MCP server.
///
/// `pipeline` is lazily initialized on the first tool call that needs it.
/// `key` is kept so the lazy init closure can build the pipeline without
/// `serve_stdio_lazy` having to hold an `Arc<Pipeline>` upfront.
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    #[allow(dead_code)] // populated + consumed by the rmcp #[tool_router] macro
    tool_router: ToolRouter<Self>,
}
```

- [ ] **Step 5: Add the `pipeline()` and `pipeline_arc()` helpers**

Add after the struct definition, before `// ---- Tool parameter types ----`:

```rust
impl AnnoRagServer {
    /// Returns a reference to the initialized pipeline, initializing it on
    /// first call.  Returns `Error::Config` if `ANNO_MODELS_DIR` is not set
    /// (i.e. models have not been downloaded yet).
    async fn pipeline(&self) -> anno_rag::error::Result<&Pipeline> {
        self.pipeline
            .get_or_try_init(|| {
                let cfg = Arc::clone(&self.cfg);
                let key = self.key;
                async move {
                    if std::env::var("ANNO_MODELS_DIR").is_err() {
                        return Err(anno_rag::error::Error::Config(
                            "Models not downloaded. Ask me to 'Set up anno-rag' \
                             or run `anno-rag download-models` in a terminal, \
                             then restart the extension."
                                .into(),
                        ));
                    }
                    Pipeline::new((*cfg).clone(), key).await.map(Arc::new)
                }
            })
            .await
            .map(|arc| arc.as_ref())
    }

    /// Returns a cloned `Arc<Pipeline>` if the pipeline is already initialized.
    /// Used by the async NER background task in `memory_save`.
    fn pipeline_arc(&self) -> Option<Arc<Pipeline>> {
        self.pipeline.get().cloned()
    }
}
```

- [ ] **Step 6: Update `AnnoRagServer::new` and add `new_lazy`**

Find the existing `impl AnnoRagServer` block near line 765:
```rust
impl AnnoRagServer {
    /// Construct a new MCP server. Owns the pipeline through `Arc`.
    #[must_use]
    pub fn new(pipeline: Pipeline, cfg: AnnoRagConfig) -> Self {
        Self {
            pipeline: Arc::new(pipeline),
            cfg,
            tool_router: Self::tool_router(),
        }
    }
}
```

Replace with:
```rust
impl AnnoRagServer {
    /// Construct a server with a pre-built pipeline (eager path).
    /// Used by `serve_stdio` and tests that provide a ready `Pipeline`.
    #[must_use]
    pub fn new(pipeline: Pipeline, cfg: AnnoRagConfig) -> Self {
        let cell = OnceCell::new();
        // The set always succeeds on a freshly created cell.
        let _ = cell.set(Arc::new(pipeline));
        Self {
            pipeline: Arc::new(cell),
            cfg: Arc::new(cfg),
            key: [0u8; 32], // unused: cell is already initialized
            tool_router: Self::tool_router(),
        }
    }

    /// Construct a server that defers `Pipeline::new` to the first tool call.
    /// Used by `serve_stdio_lazy` (the `.mcpb` / Claude Desktop path).
    #[must_use]
    pub fn new_lazy(cfg: AnnoRagConfig, key: [u8; 32]) -> Self {
        Self {
            pipeline: Arc::new(OnceCell::new()),
            cfg: Arc::new(cfg),
            key,
            tool_router: Self::tool_router(),
        }
    }
}
```

- [ ] **Step 7: Add `serve_stdio_lazy`**

Add after the existing `serve_stdio` function:

```rust
/// Start the MCP server on stdio with lazy pipeline initialization.
/// The pipeline is built on the first tool call that needs it.
/// Returns an error only if the MCP transport itself fails; model-load
/// errors are surfaced per-tool as `"Error: config error: ..."` strings.
///
/// # Errors
/// Returns [`anno_rag::error::Error::Detect`] if the rmcp transport fails.
pub async fn serve_stdio_lazy(
    cfg: AnnoRagConfig,
    key: [u8; 32],
) -> anno_rag::error::Result<()> {
    let server = AnnoRagServer::new_lazy(cfg, key);
    tracing::info!("anno-rag MCP server starting (lazy) on stdio");

    let transport = rmcp::transport::stdio();
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server failed to start: {e}")))?;

    let cancel = service.cancellation_token();
    let signal_task = tokio::spawn(async move {
        shutdown_signal_mcp().await;
        cancel.cancel();
    });

    let quit = service
        .waiting()
        .await
        .map_err(|e| anno_rag::error::Error::Detect(format!("MCP server error: {e}")))?;
    signal_task.abort();
    tracing::info!(
        target: "anno_rag::mcp::shutdown",
        event = "stopped",
        reason = ?quit,
        "anno-rag MCP server stopped cleanly"
    );
    Ok(())
}
```

- [ ] **Step 8: Update all tool handlers to use `self.pipeline().await?`**

Each of the 10 tool handlers currently accesses `self.pipeline.X(...)`. Replace each `self.pipeline` access with `let p = match self.pipeline().await { Ok(p) => p, Err(e) => return format!("Error: {e}") };`, then use `p.X(...)`.

Below is the complete replacement for each handler. Work through them in order.

**`search`** — find the entire `async fn search` body inside `#[tool_router] impl AnnoRagServer` and replace:

```rust
async fn search(&self, Parameters(params): Parameters<SearchParams>) -> String {
    let p = match self.pipeline().await {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    let result = if params.rerank {
        #[cfg(feature = "rerank")]
        {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "search",
                score_source = "cross_encoder",
                ""
            );
            p.search_reranked(&params.query, params.top_k, self.cfg.rerank_pool_size)
                .await
        }
        #[cfg(not(feature = "rerank"))]
        {
            return "Error: rerank requested but server built without \
                    the `rerank` feature"
                .to_string();
        }
    } else {
        tracing::info!(
            target: "anno_rag::audit",
            tool = "search",
            score_source = "rrf",
            ""
        );
        p.search(&params.query, params.top_k).await
    };
    match result {
        Ok(hits) => {
            let wire = SearchResult {
                hits: hits
                    .into_iter()
                    .map(|h| SearchHitWire {
                        doc_id: h.doc_id.to_string(),
                        chunk_id: h.chunk_id.to_string(),
                        source_path: h.source_path,
                        folder_path: h.folder_path,
                        chunk_idx: h.chunk_idx,
                        text_pseudo: h.text_pseudo,
                        page: h.page,
                        char_start: h.char_start,
                        char_end: h.char_end,
                        score: h.score,
                    })
                    .collect(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => format!("Error: {e}"),
    }
}
```

**`rehydrate`** — replace body:

```rust
async fn rehydrate(&self, Parameters(params): Parameters<RehydrateParams>) -> String {
    let p = match self.pipeline().await {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    match p.rehydrate(&params.text).await {
        Ok(r) => {
            let wire = RehydrateResult {
                text: r.text,
                tokens_rehydrated: r.tokens_rehydrated,
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => format!("Error: {e}"),
    }
}
```

**`detect`** — replace body (`detect` is a sync call on `&Pipeline`):

```rust
async fn detect(&self, Parameters(params): Parameters<DetectParams>) -> String {
    let p = match self.pipeline().await {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    match p.detect(&params.text) {
        Ok(entities) => {
            let info: Vec<EntityInfo> = entities
                .into_iter()
                .map(|e| EntityInfo {
                    original: e.original,
                    category: format!("{:?}", e.category),
                    confidence: e.confidence,
                    source: format!("{:?}", e.source),
                    start: e.start,
                    end: e.end,
                })
                .collect();
            serde_json::to_string_pretty(&DetectResult { entities: info })
                .unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => format!("Error: {e}"),
    }
}
```

**`vault_stats`** — replace body:

```rust
async fn vault_stats(&self) -> String {
    let p = match self.pipeline().await {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    let s = p.vault_stats().await;
    let wire = VaultStatsResult {
        total_mappings: s.total_mappings,
        categories: s.categories,
    };
    serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
}
```

**`memory_save`** — special: also replaces `Arc::clone(&self.pipeline)` with `self.pipeline_arc()`:

```rust
async fn memory_save(&self, Parameters(p): Parameters<MemorySaveParams>) -> String {
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let start = std::time::Instant::now();
    let kind = p.kind.as_deref().and_then(parse_kind);
    let session_id = p.session_id.clone();
    let r = pipeline
        .save_memory(&p.text, kind, session_id.clone())
        .await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(s) => {
            if self.cfg.memory_ner_mode == MemoryNerMode::Async {
                // pipeline() already succeeded so get() is Some(_).
                if let Some(arc_pipeline) = self.pipeline_arc() {
                    let id = s.id.clone();
                    let text = p.text.clone();
                    tokio::spawn(async move {
                        if let Err(e) = arc_pipeline
                            .save_memory_ner_task(id.clone(), text, kind, session_id)
                            .await
                        {
                            tracing::warn!(
                                target: "anno_rag::memory::audit",
                                tool = "memory_save_ner_task",
                                memory_id = %id.as_string(),
                                result = "error",
                                "{e}"
                            );
                        }
                    });
                }
            }
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_save",
                result = "ok",
                duration_ms = elapsed,
                ""
            );
            let wire = MemorySaveResultWire {
                id: s.id.as_string(),
                stored_text: s.stored_text,
                redacted_text: s.redacted_text,
                token_count: s.token_refs.len(),
                ner_mode: s.ner_mode,
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_save",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

**`memory_recall`** — replace body (add `let p = ...` at top, replace all `self.pipeline.` with `p.`):

```rust
async fn memory_recall(&self, Parameters(p): Parameters<MemoryRecallParams>) -> String {
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let start = std::time::Instant::now();
    let kinds = p
        .kinds
        .as_ref()
        .map(|v| v.iter().filter_map(|k| parse_kind(k)).collect::<Vec<_>>());
    let r = if p.rerank {
        #[cfg(feature = "rerank")]
        {
            tracing::info!(
                target: "anno_rag::audit",
                tool = "memory_recall",
                score_source = "cross_encoder",
                ""
            );
            pipeline
                .recall_memory_reranked(
                    &p.query,
                    p.top_k,
                    p.session_id,
                    kinds,
                    p.as_of,
                    p.graph_expand,
                    self.cfg.rerank_pool_size,
                )
                .await
        }
        #[cfg(not(feature = "rerank"))]
        {
            return "Error: rerank requested but server built without \
                    the `rerank` feature"
                .to_string();
        }
    } else {
        tracing::info!(
            target: "anno_rag::audit",
            tool = "memory_recall",
            score_source = "rrf",
            ""
        );
        pipeline
            .recall_memory(
                &p.query,
                p.top_k,
                p.session_id,
                kinds,
                p.as_of,
                p.graph_expand,
            )
            .await
    };
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(hits) => {
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_recall",
                result = "ok",
                duration_ms = elapsed,
                n = hits.len(),
                ""
            );
            let wire = MemoryRecallResultWire {
                hits: hits
                    .into_iter()
                    .map(|h| MemoryHitWire {
                        id: h.id,
                        text: h.text,
                        kind: format!("{:?}", h.kind).to_lowercase(),
                        created_at: h.created_at,
                        entity_refs: h.entity_refs,
                        score: h.score,
                    })
                    .collect(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_recall",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

**`memory_forget`** — replace body:

```rust
async fn memory_forget(&self, Parameters(p): Parameters<MemoryForgetParams>) -> String {
    let id = match &p.id {
        Some(s) => match uuid::Uuid::parse_str(s) {
            Ok(u) => Some(anno_rag::memory::MemoryId(u)),
            Err(e) => return format!("Error: bad id: {e}"),
        },
        None => None,
    };
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let start = std::time::Instant::now();
    let r = pipeline
        .forget_memory(id, p.query, p.limit, p.dry_run)
        .await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(res) => {
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_forget",
                result = "ok",
                duration_ms = elapsed,
                n = res.forgotten_ids.len(),
                ""
            );
            let wire = MemoryForgetResultWire {
                forgotten_ids: res.forgotten_ids,
                vault_tokens_purged: res.vault_tokens_purged,
                note: "logically forgotten; physical erasure within 24h".into(),
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_forget",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

**`memory_list`** — replace body:

```rust
async fn memory_list(&self, Parameters(p): Parameters<MemoryListParams>) -> String {
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let start = std::time::Instant::now();
    let kind = p.kind.as_deref().and_then(parse_kind);
    let r = pipeline
        .list_memories(p.session_id, kind, p.limit, p.cursor)
        .await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(page) => {
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_list",
                result = "ok",
                duration_ms = elapsed,
                n = page.items.len(),
                ""
            );
            let wire = MemoryListResultWire {
                items: page
                    .items
                    .into_iter()
                    .map(|h| MemoryHitWire {
                        id: h.id,
                        text: h.text,
                        kind: format!("{:?}", h.kind).to_lowercase(),
                        created_at: h.created_at,
                        entity_refs: h.entity_refs,
                        score: h.score,
                    })
                    .collect(),
                next_cursor: page.next_cursor,
            };
            serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_list",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

**`memory_graph_recall`** — replace body:

```rust
async fn memory_graph_recall(
    &self,
    Parameters(p): Parameters<MemoryGraphRecallParams>,
) -> String {
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let start = std::time::Instant::now();
    let r = pipeline
        .graph_recall(&p.entity, p.max_hops, p.per_hop_limit, p.as_of)
        .await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(res) => {
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_graph_recall",
                result = "ok",
                duration_ms = elapsed,
                nodes = res.nodes.len(),
                edges = res.edges.len(),
                memories = res.memories.len(),
                ""
            );
            serde_json::to_string_pretty(&res).unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_graph_recall",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

**`memory_invalidate`** — replace body:

```rust
async fn memory_invalidate(
    &self,
    Parameters(p): Parameters<MemoryInvalidateParams>,
) -> String {
    let id = match uuid::Uuid::parse_str(&p.id) {
        Ok(u) => anno_rag::memory::MemoryId(u),
        Err(e) => return format!("Error: bad id: {e}"),
    };
    let pipeline = match self.pipeline().await {
        Ok(pl) => pl,
        Err(e) => return format!("Error: {e}"),
    };
    let when = p.at.unwrap_or_else(chrono::Utc::now);
    let start = std::time::Instant::now();
    let r = pipeline.invalidate_memory(&id, Some(when)).await;
    let elapsed = start.elapsed().as_millis() as u64;
    match r {
        Ok(invalidated) => {
            tracing::info!(
                target: "anno_rag::memory::audit",
                tool = "memory_invalidate",
                result = if invalidated { "ok" } else { "noop" },
                duration_ms = elapsed,
                ""
            );
            serde_json::to_string_pretty(&MemoryInvalidateResultWire {
                id: p.id,
                invalidated,
                valid_to: when.to_rfc3339(),
            })
            .unwrap_or_else(|e| format!("Error: {e}"))
        }
        Err(e) => {
            tracing::warn!(
                target: "anno_rag::memory::audit",
                tool = "memory_invalidate",
                result = "error",
                duration_ms = elapsed,
                "{e}"
            );
            format!("Error: {e}")
        }
    }
}
```

- [ ] **Step 9: Run the library tests**

```bash
cargo test -p anno-rag-mcp --lib 2>&1 | tail -10
```

Expected: all tests pass including `lazy_tests::lazy_server_returns_error_when_models_absent`.

If you get compile errors about `tempfile`, add it as a dev-dependency:

```bash
grep -q "tempfile" crates/anno-rag-mcp/Cargo.toml || \
  cargo add --dev tempfile --manifest-path crates/anno-rag-mcp/Cargo.toml
```

- [ ] **Step 10: Check that the full workspace still compiles**

```bash
cargo check --workspace 2>&1 | tail -10
```

Expected: `Finished` with no errors. If `main.rs` complains about `serve_stdio` signature — it's fine for now; Task C3 fixes main.rs.

- [ ] **Step 11: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/Cargo.toml
git commit -m "feat(mcp): lazy OnceCell<Pipeline> + serve_stdio_lazy"
```

---

## Task C3: `main.rs` Mcp branch auto-detect

**Files:**
- Modify: `crates/anno-rag-bin/src/main.rs`

The `Mcp` command currently falls into the `match` after `Pipeline::new`. We short-circuit before `Pipeline::new` just like `Bench` and `DownloadModels` already do, and detect the default models path before spawning async workers.

- [ ] **Step 1: Write the failing test in `main.rs`**

Add a unit test that checks the auto-detect env-var logic in isolation. Because `main` is hard to test directly, test the path logic:

```rust
#[cfg(test)]
mod mcp_autodetect_tests {
    #[test]
    fn models_cache_path_matches_default_autodetect_layout() {
        let cfg = anno_rag::config::AnnoRagConfig::default();
        let models = cfg.models_cache();
        // The auto-detect in the Mcp branch checks for e5-small and gliner2 subdirs
        // under models_cache().
        assert!(models.ends_with("models"));
        // We check the subdir names used in the auto-detect step.
        assert_eq!(
            models.join("multilingual-e5-small").file_name().unwrap(),
            "multilingual-e5-small"
        );
        assert_eq!(
            models.join("gliner2-multi-v1-onnx").file_name().unwrap(),
            "gliner2-multi-v1-onnx"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it passes (it's a path check, should pass)**

```bash
cargo test -p anno-rag-bin --lib mcp_autodetect_tests 2>&1 | tail -5
```

Expected: `test mcp_autodetect_tests::models_cache_path_matches_default_autodetect_layout ... ok`

- [ ] **Step 3: Update the `Mcp` branch in `main()`**

In `crates/anno-rag-bin/src/main.rs`, find the existing short-circuit block that starts:

```rust
// DownloadModels needs no Pipeline — short-circuit before keyring lookup.
if let Cmd::DownloadModels { dir } = &cli.cmd {
```

**Above** that block (after the `Bench` short-circuit), add:

```rust
// Mcp uses lazy pipeline init — short-circuit before Pipeline::new.
// Auto-detect the default models path here, before any async threads
// start, so set_var is safe (called in single-threaded context).
if let Cmd::Mcp = &cli.cmd {
    // Derive the key now (needed by the lazy init closure inside the server).
    let key = derive_key()?;
    // If ANNO_MODELS_DIR is not set, check the default download location.
    if std::env::var("ANNO_MODELS_DIR").is_err() {
        let default_models = cfg.models_cache();
        if default_models.join("multilingual-e5-small").exists()
            && default_models.join("gliner2-multi-v1-onnx").exists()
        {
            // set_var is safe: called before any tokio worker threads start.
            std::env::set_var("ANNO_MODELS_DIR", &default_models);
            tracing::info!(
                "auto-detected models at {}",
                default_models.display()
            );
        }
    }
    anno_rag_mcp::serve_stdio_lazy(cfg, key).await?;
    return Ok(());
}
```

Also remove `Cmd::Mcp` from the `match cli.cmd` arms at the bottom of `main` (it now short-circuits above). Find:
```rust
        Cmd::Mcp => {
            anno_rag_mcp::serve_stdio(pipeline, cfg).await?;
        }
```
Delete that arm. The `match` must remain exhaustive — add `Cmd::Mcp => unreachable!("handled above")` in its place:
```rust
        Cmd::Mcp => unreachable!("handled above before Pipeline::new"),
```

- [ ] **Step 4: Verify the workspace compiles**

```bash
cargo check --workspace 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Run existing tests**

```bash
cargo test -p anno-rag-bin --lib 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 6: Smoke-test `anno-rag mcp --help`** (just `--help` exits immediately, tests arg parsing)

```bash
cargo run -p anno-rag-bin -- mcp --help 2>&1 || true
```

Expected: shows the help text for `mcp` subcommand without errors.

- [ ] **Step 7: Commit**

```bash
git add crates/anno-rag-bin/src/main.rs
git commit -m "feat(cli): lazy Mcp branch with auto-detect default models dir"
```

---

## Task C4: `download_models` MCP tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Add to `lazy_tests` module at the bottom of `lib.rs`:

```rust
    /// When the models directory already has the expected subdirs, the tool
    /// reports them as already present without starting a download.
    #[tokio::test]
    async fn download_models_tool_reports_already_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let models_dir = dir.path().join("models");
        // Create the two sentinel subdirectories that the tool checks.
        std::fs::create_dir_all(models_dir.join("multilingual-e5-small")).unwrap();
        std::fs::create_dir_all(models_dir.join("gliner2-multi-v1-onnx")).unwrap();

        let mut cfg = AnnoRagConfig::default();
        cfg.data_dir = dir.path().to_path_buf(); // models_cache() = dir/models
        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);

        let result = server.download_models().await;
        assert!(
            result.contains("already present"),
            "expected 'already present' in: {result}"
        );
        assert!(
            result.contains(models_dir.to_str().unwrap()),
            "expected path in: {result}"
        );
    }

    /// When models are absent and no download is in progress, the tool
    /// starts a background download and returns immediately.
    #[tokio::test]
    async fn download_models_tool_starts_download_when_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = AnnoRagConfig::default();
        cfg.data_dir = dir.path().to_path_buf();

        // Ensure models are absent
        assert!(!dir.path().join("models").join("multilingual-e5-small").exists());

        let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
        let result = server.download_models().await;
        // Should return "Downloading" immediately — network not needed
        assert!(
            result.contains("Downloading") || result.contains("in progress"),
            "expected 'Downloading' or 'in progress' in: {result}"
        );
    }
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p anno-rag-mcp --lib lazy_tests::download_models 2>&1 | head -20
```

Expected: compile error — `download_models` method not found.

- [ ] **Step 3: Add `DownloadModelsResult` wire type**

After the existing wire type definitions (around line 110), add:

```rust
#[derive(Serialize)]
struct DownloadModelsResult {
    status: String,
    path: String,
    message: String,
}
```

- [ ] **Step 4: Add the `download_models` tool to `#[tool_router] impl AnnoRagServer`**

Add after the last tool (`memory_invalidate`), still inside the `#[tool_router] impl AnnoRagServer` block:

```rust
    /// Download anno-rag model weights (~970 MB) in the background.
    ///
    /// Returns immediately. If already downloaded, reports the path.
    /// If a download is in progress, reports status. Otherwise starts
    /// a background download to `~/.anno-rag/models` and returns.
    #[tool(
        description = "Download anno-rag model weights (~970 MB) to the local machine. \
                       Returns immediately — download runs in the background. \
                       Call again in a few minutes to check if ready. \
                       No parameters needed."
    )]
    async fn download_models(&self) -> String {
        let models_dir = self.cfg.models_cache();
        let e5_dir = models_dir.join("multilingual-e5-small");
        let gliner_dir = models_dir.join("gliner2-multi-v1-onnx");

        // Already present — nothing to do.
        if e5_dir.exists() && gliner_dir.exists() {
            let wire = DownloadModelsResult {
                status: "already_present".into(),
                path: models_dir.display().to_string(),
                message: format!(
                    "Models already present at {}. \
                     Set ANNO_MODELS_DIR={} in extension settings \
                     (or leave blank — the default path is detected automatically).",
                    models_dir.display(),
                    models_dir.display()
                ),
            };
            return serde_json::to_string_pretty(&wire)
                .unwrap_or_else(|e| format!("Error: {e}"));
        }

        // Download already in progress.
        let lock_file = models_dir.join(".download-lock");
        if lock_file.exists() {
            let wire = DownloadModelsResult {
                status: "in_progress".into(),
                path: models_dir.display().to_string(),
                message: format!(
                    "Download already in progress to {}. \
                     Ask again in a few minutes.",
                    models_dir.display()
                ),
            };
            return serde_json::to_string_pretty(&wire)
                .unwrap_or_else(|e| format!("Error: {e}"));
        }

        // Start a new background download.
        if let Err(e) = std::fs::create_dir_all(&models_dir) {
            return format!("Error: could not create models dir: {e}");
        }
        if let Err(e) = std::fs::write(&lock_file, b"downloading") {
            return format!("Error: could not write lock file: {e}");
        }

        let cfg_clone = Arc::clone(&self.cfg);
        tokio::task::spawn(async move {
            let result = anno_rag::download_models::download(&cfg_clone).await;
            // Remove lock whether success or failure.
            let _ = std::fs::remove_file(cfg_clone.models_cache().join(".download-lock"));
            if let Err(e) = result {
                tracing::warn!(
                    target: "anno_rag::mcp::download_models",
                    "background download failed: {e}"
                );
            } else {
                tracing::info!(
                    target: "anno_rag::mcp::download_models",
                    "background download complete"
                );
            }
        });

        let wire = DownloadModelsResult {
            status: "downloading".into(),
            path: models_dir.display().to_string(),
            message: format!(
                "Downloading anno-rag models to {} (~970 MB total). \
                 This runs in the background and takes 2–15 minutes. \
                 Ask me again in a few minutes — I will confirm when ready.",
                models_dir.display()
            ),
        };
        serde_json::to_string_pretty(&wire).unwrap_or_else(|e| format!("Error: {e}"))
    }
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p anno-rag-mcp --lib lazy_tests 2>&1 | tail -10
```

Expected: all 3 tests pass (`lazy_server_returns_error_when_models_absent`, `download_models_tool_reports_already_present`, `download_models_tool_starts_download_when_absent`).

- [ ] **Step 6: Update the server instructions string in `get_info`**

Find in `impl ServerHandler for AnnoRagServer`:
```rust
            "anno-rag MCP server. Tools: search (pseudonymized retrieval), \
             rehydrate (restore originals), detect (dry-run scan), vault_stats, \
             memory_save / memory_recall / memory_forget / memory_list \
             (PII-safe session memory; GDPR Art. 17 cascades to vault tokens), \
             memory_graph_recall / memory_invalidate (v0.2 entity-graph + \
             bi-temporal). memory_recall accepts as_of + graph_expand.",
```

Replace with:
```rust
            "anno-rag MCP server. Tools: search (pseudonymized retrieval), \
             rehydrate (restore originals), detect (dry-run scan), vault_stats, \
             memory_save / memory_recall / memory_forget / memory_list \
             (PII-safe session memory; GDPR Art. 17 cascades to vault tokens), \
             memory_graph_recall / memory_invalidate (v0.2 entity-graph + \
             bi-temporal), download_models (background model download — \
             call this on first use if models are not yet downloaded). \
             memory_recall accepts as_of + graph_expand.",
```

- [ ] **Step 7: Check workspace**

```bash
cargo check --workspace 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 8: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add download_models background tool"
```

---

## Task C5: `.mcpb` packaging in `release.yml`

**Files:**
- Modify: `.github/workflows/release.yml`

Add two steps to the `build-local-artifacts` job: one to package the `.mcpb` and one to upload it as a separate artifact that the `host` job will include in the GitHub Release.

- [ ] **Step 1: Verify `python3` and `zip` availability on all three matrix runners**

This is a documentation check — no file changes.

- `ubuntu-22.04` (not used here, but global): python3 ✓, zip ✓
- `windows-latest` (Git Bash shell): python3 ✓, zip ✓ (via Git for Windows)
- `macos-13`, `macos-14`: python3 ✓, zip ✓

Confirm by reading the comment block at the top of the `build-local-artifacts` job to understand the matrix structure. The matrix entries are generated by `dist plan` at runtime, but the runners are always from the set above.

- [ ] **Step 2: Add the "Package .mcpb extension" step**

In `.github/workflows/release.yml`, find the "Gateway boot smoke (Unix)" step:

```yaml
      - name: Gateway boot smoke (Unix)
        if: runner.os != 'Windows'
        shell: bash
        run: ./scripts/release/smoke-gateway.sh "./target/${{ join(matrix.targets, '-') }}/release/anno-privacy-gateway"
```

After it (before the `- id: cargo-dist` "Post-build" step), insert:

```yaml
      # ANNO-PATCH: .mcpb extension packaging — re-apply after every `dist generate`.
      - name: Package .mcpb extension
        shell: bash
        run: |
          TARGET="${{ join(matrix.targets, '-') }}"
          VERSION="${RELEASE_TAG#v}"
          MCPB_NAME="hacienda-${RELEASE_TAG}-${TARGET}.mcpb"
          if [ "${{ runner.os }}" = "Windows" ]; then
            BIN="anno-rag.exe"
            PLATFORM="win32"
          else
            BIN="anno-rag"
            PLATFORM="darwin"
          fi
          mkdir -p mcpb-staging/server
          cp "target/${TARGET}/release/${BIN}" mcpb-staging/server/
          python3 - <<'PYEOF'
          import json, sys, os
          template_path = "scripts/release/mcpb-manifest-template.json"
          with open(template_path) as f:
              m = json.load(f)
          version   = os.environ["MCPB_VERSION"]
          platform  = os.environ["MCPB_PLATFORM"]
          binary    = os.environ["MCPB_BIN"]
          m["version"] = version
          m["compatibility"]["platforms"] = [platform]
          m["server"]["entry_point"] = "server/" + binary
          m["mcp_config"]["command"] = "${__dirname}/server/" + binary
          with open("mcpb-staging/manifest.json", "w") as f:
              json.dump(m, f, indent=2)
          print(f"manifest written: version={version} platform={platform} binary={binary}")
          PYEOF
          cd mcpb-staging && zip -r "../${MCPB_NAME}" . && cd ..
          echo "MCPB_NAME=${MCPB_NAME}" >> "$GITHUB_ENV"
        env:
          MCPB_VERSION: ${{ env.RELEASE_TAG != '' && env.RELEASE_TAG || '0.0.0-dev' }}
          MCPB_PLATFORM: ${{ runner.os == 'Windows' && 'win32' || 'darwin' }}
          MCPB_BIN: ${{ runner.os == 'Windows' && 'anno-rag.exe' || 'anno-rag' }}
```

Note: The `MCPB_VERSION` env var strips the `v` prefix inside the Python script via `os.environ["MCPB_VERSION"]`. The `RELEASE_TAG` variable (set in the workflow `env:` block at the top) has the `v` prefix; the Python script trims it.

Actually, correct the version trimming — do it in bash before passing to Python:

```yaml
      - name: Package .mcpb extension
        shell: bash
        run: |
          TARGET="${{ join(matrix.targets, '-') }}"
          MCPB_NAME="hacienda-${RELEASE_TAG}-${TARGET}.mcpb"
          VERSION="${RELEASE_TAG#v}"
          if [ "${{ runner.os }}" = "Windows" ]; then
            BIN="anno-rag.exe"
            PLATFORM="win32"
          else
            BIN="anno-rag"
            PLATFORM="darwin"
          fi
          mkdir -p mcpb-staging/server
          cp "target/${TARGET}/release/${BIN}" mcpb-staging/server/
          python3 -c "
          import json, sys
          m = json.load(open('scripts/release/mcpb-manifest-template.json'))
          m['version'] = sys.argv[1]
          m['compatibility']['platforms'] = [sys.argv[2]]
          m['server']['entry_point'] = 'server/' + sys.argv[3]
          m['mcp_config']['command'] = '\${__dirname}/server/' + sys.argv[3]
          json.dump(m, open('mcpb-staging/manifest.json', 'w'), indent=2)
          print('manifest OK')
          " "$VERSION" "$PLATFORM" "$BIN"
          cd mcpb-staging && zip -r "../${MCPB_NAME}" . && cd ..
          echo "MCPB_NAME=${MCPB_NAME}" >> "$GITHUB_ENV"
```

- [ ] **Step 3: Add the "Upload .mcpb artifact" step**

Immediately after the "Package .mcpb extension" step and still before `- id: cargo-dist`:

```yaml
      - name: Upload .mcpb artifact
        uses: actions/upload-artifact@v7
        with:
          name: artifacts-mcpb-${{ join(matrix.targets, '_') }}
          path: ${{ env.MCPB_NAME }}
```

The `artifacts-mcpb-*` name means the `pattern: artifacts-*` download in the `host` job will pick it up and place the `.mcpb` file in `artifacts/`. The existing `gh release create ... artifacts/*` command will then include it in the GitHub Release.

- [ ] **Step 4: Validate the modified workflow YAML parses correctly**

```bash
python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml'))" 2>/dev/null \
  && echo "YAML OK" \
  || python3 -c "
import yaml, sys
try:
    yaml.safe_load(open('.github/workflows/release.yml'))
    print('YAML OK')
except yaml.YAMLError as e:
    print(f'YAML ERROR: {e}')
    sys.exit(1)
"
```

Note: `pyyaml` is standard on all three platforms. If unavailable, install with `pip3 install pyyaml`.

Expected: `YAML OK`

- [ ] **Step 5: Locally simulate the manifest generation**

Run the Python one-liner by hand to confirm it works with the template:

```bash
VERSION="0.2.0"
PLATFORM="darwin"
BIN="anno-rag"
python3 -c "
import json, sys
m = json.load(open('scripts/release/mcpb-manifest-template.json'))
m['version'] = sys.argv[1]
m['compatibility']['platforms'] = [sys.argv[2]]
m['server']['entry_point'] = 'server/' + sys.argv[3]
m['mcp_config']['command'] = '\${__dirname}/server/' + sys.argv[3]
json.dump(m, open('/tmp/test-manifest.json', 'w'), indent=2)
print('OK')
" "$VERSION" "$PLATFORM" "$BIN"
cat /tmp/test-manifest.json
```

Expected: Valid JSON with `version: "0.2.0"`, `platforms: ["darwin"]`, `entry_point: "server/anno-rag"`, `command: "${__dirname}/server/anno-rag"`.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat(release): package .mcpb per-platform in build-local-artifacts"
```

---

## Task C6: Update `README-release.md`

**Files:**
- Modify: `docs/release/README-release.md`

- [ ] **Step 1: Add a "Claude Desktop Extension (one-click)" section**

In `docs/release/README-release.md`, find the existing `## Claude Desktop` section heading. Insert a **new** section **above** it:

```markdown
## Claude Desktop Extension (one-click install)

Download the `.mcpb` file for your platform from the [GitHub Release](https://github.com/jamon8888/anno/releases):

| Platform | File |
|---|---|
| Windows 11 x64 | `hacienda-<tag>-x86_64-pc-windows-msvc.mcpb` |
| macOS Apple Silicon | `hacienda-<tag>-aarch64-apple-darwin.mcpb` |
| macOS Intel | `hacienda-<tag>-x86_64-apple-darwin.mcpb` |

**Install:**

1. Open Claude Desktop → Settings → Extensions.
2. Drag the `.mcpb` file into the window (or click "Install Extension…").
3. Fill in config fields if prompted (all optional — leave blank for defaults).
4. Click **Install**. The server registers instantly.

**First use — download models:**

Model weights (~970 MB) are not bundled. On first use, ask Claude:

> "Set up anno-rag"

Claude calls the `download_models` tool, which downloads models in the background (~2–15 min depending on your connection). Ask again after a few minutes — anno-rag will confirm when ready.

**Power-user shortcut (models already on disk):**

If you have already run `anno-rag download-models`, open the extension settings and set **Models directory** to the path it printed. anno-rag will use models from disk immediately without any network call.

```

- [ ] **Step 2: Update the existing `## Claude Desktop` manual config section**

The existing section shows the raw JSON config. Add a note at the top to clarify it's for manual config (non-extension installs):

Find the line:
```markdown
## Claude Desktop
```

Replace with:
```markdown
## Claude Desktop (manual config)

> **Note:** If you installed via the `.mcpb` extension (above), skip this section — Claude Desktop manages the config automatically.
```

- [ ] **Step 3: Verify the Markdown renders correctly**

```bash
python3 -c "
with open('docs/release/README-release.md') as f:
    content = f.read()
# Check the new section appears before the manual config section
ext_pos = content.index('Claude Desktop Extension')
manual_pos = content.index('Claude Desktop (manual config)')
assert ext_pos < manual_pos, 'Extension section must come before manual config section'
print('Section order OK')
print(f'Total length: {len(content)} chars')
"
```

Expected: `Section order OK`

- [ ] **Step 4: Commit**

```bash
git add docs/release/README-release.md
git commit -m "docs(release): add .mcpb one-click install section"
```

---

## Final verification

- [ ] **Run the full library test suite**

```bash
cargo test --workspace --lib 2>&1 | tail -15
```

Expected: all tests pass, no failures.

- [ ] **Verify `anno-rag-mcp` exports `serve_stdio_lazy`**

```bash
cargo doc -p anno-rag-mcp --no-deps 2>&1 | grep -E "warning|error" | head -10 || echo "doc OK"
```

Expected: no errors.

- [ ] **Local .mcpb dry-run**

```bash
VERSION="0.2.0-test"
BIN="anno-rag"
PLATFORM="darwin"
mkdir -p /tmp/mcpb-test/server
# Use the debug binary if available
if [ -f "target/debug/anno-rag" ]; then
  cp target/debug/anno-rag /tmp/mcpb-test/server/anno-rag
else
  echo "(binary not built — copy manually for full test)"
fi
python3 -c "
import json, sys
m = json.load(open('scripts/release/mcpb-manifest-template.json'))
m['version'] = sys.argv[1]
m['compatibility']['platforms'] = [sys.argv[2]]
m['server']['entry_point'] = 'server/' + sys.argv[3]
m['mcp_config']['command'] = '\${__dirname}/server/' + sys.argv[3]
json.dump(m, open('/tmp/mcpb-test/manifest.json', 'w'), indent=2)
" "$VERSION" "$PLATFORM" "$BIN"
cd /tmp && zip -r hacienda-test.mcpb mcpb-test/
echo "Produced /tmp/hacienda-test.mcpb — drag to Claude Desktop to verify"
ls -lh /tmp/hacienda-test.mcpb
```
