# MCP Surface Consolidation (Phase 2.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify the indexing and search MCP surface behind 5 verb-only tools (`index`, `search`, `sources`, `status`, `forget`). Legacy `search` is renamed to `legacy_search`; 9 legacy tools stay functional with deprecated descriptions.

**Architecture:** Façade-first. The new tools route to handler functions
extracted from the existing tools. Narrow additive helpers on `anno-rag::Store`,
`Pipeline`, and the knowledge control store are introduced only where the
unified `sources`/`status`/`forget` tools need existing state that is currently
private. They do not change existing signatures, schemas, model load paths, or
old tool behavior.

**Tech Stack:** Rust workspace, `rmcp` MCP framework, existing `anno-rag`, `anno-knowledge-store`, `anno-rag-mcp`. No new crates, no new storage planes, no new model load paths.

**Spec:** [`docs/superpowers/specs/2026-06-02-anno-mcp-surface-consolidation-phase2.5-design.md`](../specs/2026-06-02-anno-mcp-surface-consolidation-phase2.5-design.md)

**Roadmap:** [`docs/product/roadmap.md`](../../product/roadmap.md)

**Build/test commands (respect build-isolation rules in CLAUDE.md):**
```powershell
# ALWAYS first — refuse to build if cargo/rustc already running:
Get-Process cargo,rustc -ErrorAction SilentlyContinue

# Check only (fast, no link):
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check -Profile dev-fast

# Unit tests for one crate:
cargo nextest run --package <crate>

# NEVER cargo test/build --workspace locally.
```

**GitHub build fork:** Push implementation branches and `main` syncs to
`build-fork` (`https://github.com/candy-hacienda/anno.git`) for CI while
`jamon8888/anno` Actions is blocked by billing/spending-limit failures. Do not
use the old `origin/codex/knowledge-plans-phase25-phase3` branch as an
implementation base; it predates the #37 squash merge and will produce a noisy
diff. Start from current `main`.

---

## Spec Refinement (one paragraph)

The spec originally said "no file in `crates/anno-rag/src/` is modified." That
goal is not compatible with a useful `sources()`/`status()`/`forget()` surface:
legal corpus state lives behind `Pipeline`/`Store`, and exact knowledge
path-forget needs a source lookup by provider path. This plan allows small
additive helpers only: read/list/count helpers, one folder delete helper, and a
knowledge path-resolution helper. No existing signature or old handler behavior
changes.

---

## Prerequisite

This plan depends on Phase 1 + Phase 2 being merged (`33184a7b`, PR #37 squash)
plus the follow-up privacy/forget fix (`0978454e`, PR #40 / local main). The 9
legacy tools that will become deprecated must currently exist and pass tests:
- `legal_ingest`, `legal_search` in `crates/anno-rag-mcp/src/lib.rs`
- `knowledge_search`, `knowledge_add_local_folder`, `knowledge_sync`, `knowledge_sources`, `knowledge_status`, `knowledge_forget` in `crates/anno-rag-mcp/src/lib.rs`
- `search` (legacy reranked) in `crates/anno-rag-mcp/src/lib.rs`
- `vault_stats` (used by new `status()`) in `crates/anno-rag-mcp/src/lib.rs`

Task 0 verifies this before any other work.

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/anno-rag/src/store.rs` | Modify | Add `list_indexed_folder_paths() -> Result<Vec<String>>`, `count_chunks() -> Result<u64>`, `count_chunks_for_folder(folder_path) -> Result<u64>`, and `delete_folder_rows(folder_path) -> Result<u64>`. Raw folder paths stay internal. |
| `crates/anno-rag/src/pipeline.rs` | Modify | Add pass-through helpers: `store_list_indexed_folder_paths`, `store_count_chunks`, and `forget_legal_folder_path(folder_path) -> Result<u64>`. |
| `crates/anno-knowledge-store/src/control_store.rs` / `crates/anno-rag-mcp/src/knowledge.rs` | Modify | Add exact source lookup/forget by local provider path without exposing raw paths in MCP output. |
| `crates/anno-rag-mcp/src/lib.rs` | Modify | Extract 9 `*_impl` helpers from existing tools, add 5 new tools, rename legacy `search` to `legacy_search`, update 9 deprecated descriptions. |
| `crates/anno-rag-mcp/src/health.rs` | Modify | Update `all_tool_names()` ordering: 5 new tools first; legacy tools (incl. `legacy_search`) last. Add new test. |

No other files are modified.

---

## Task 0: Pre-Flight Checks

**Files:** none (verification only)

- [ ] **Step 1: Verify prerequisite tools exist**

```powershell
Select-String -Path crates\anno-rag-mcp\src\lib.rs -Pattern "fn legal_ingest|fn legal_search|fn knowledge_search|fn knowledge_add_local_folder|fn knowledge_sync|fn knowledge_sources|fn knowledge_status|fn knowledge_forget|fn search\b|fn vault_stats" | Select-Object -First 12
```
Expected: at least 10 hits (the 9 to-be-deprecated tools + `vault_stats`). If any are missing, STOP — Phase 2 must be merged first.

- [ ] **Step 2: Kill stale builds**

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
```

- [ ] **Step 3: Impact analysis on the targets**

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
npx gitnexus impact --repo anno Store --direction upstream
npx gitnexus impact --repo anno Pipeline --direction upstream
```
Expected: any impact is fine since we ADD methods only. STOP and warn the user if HIGH/CRITICAL is reported on a *new* basis (i.e., something changed since the last phase).

---

## Task 1: Additive Read-Only Helpers on anno-rag

**Files:**
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-knowledge-store/src/control_store.rs`
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`

- [ ] **Step 1: Write failing test for `Store::list_indexed_folder_paths`**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/anno-rag/src/store.rs`:
```rust
    #[tokio::test]
    async fn list_indexed_folder_paths_returns_distinct_folders() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let store = Store::open(&cfg).await.expect("open");
        // Empty index returns empty list.
        let paths = store.list_indexed_folder_paths().await.expect("list");
        assert!(paths.is_empty());
    }

    #[tokio::test]
    async fn count_chunks_returns_zero_on_empty_store() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let store = Store::open(&cfg).await.expect("open");
        assert_eq!(store.count_chunks().await.expect("count"), 0);
    }
```

- [ ] **Step 2: Run tests — verify failure**

```powershell
cargo nextest run --package anno-rag list_indexed_folder_paths_returns_distinct_folders
cargo nextest run --package anno-rag count_chunks_returns_zero_on_empty_store
```
Expected: FAIL — `list_indexed_folder_paths`, `count_chunks` not defined.

- [ ] **Step 3: Implement `list_indexed_folder_paths` and `count_chunks`**

In `crates/anno-rag/src/store.rs`, locate `impl Store` and add two methods:
```rust
    /// List distinct `folder_path` values currently present in the chunks table.
    /// Used by the unified MCP `sources()` tool to enumerate legal corpus paths.
    /// Pure read; no behavioral effect on existing search/ingest paths.
    ///
    /// # Errors
    /// Returns [`Error::Lance`] if the LanceDB query fails or [`Error::Arrow`]
    /// on column decode failure.
    pub async fn list_indexed_folder_paths(&self) -> Result<Vec<String>> {
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};
        let stream = self
            .tbl
            .query()
            .select(lancedb::query::Select::columns(&["folder_path"]))
            .execute()
            .await
            .map_err(|e| Error::Store(format!("list_indexed_folder_paths query: {e}")))?;
        let batches: Vec<arrow_array::RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| Error::Store(format!("list_indexed_folder_paths collect: {e}")))?;
        let mut seen = std::collections::BTreeSet::new();
        for batch in &batches {
            let col = batch
                .column_by_name("folder_path")
                .ok_or_else(|| Error::Store("folder_path column missing".into()))?
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .ok_or_else(|| Error::Store("folder_path wrong type".into()))?;
            for i in 0..col.len() {
                if !col.is_null(i) {
                    seen.insert(col.value(i).to_string());
                }
            }
        }
        Ok(seen.into_iter().collect())
    }

    /// Count rows in the chunks table.
    /// Used by the unified MCP `status()` tool. Pure read.
    ///
    /// # Errors
    /// Returns [`Error::Store`] if the LanceDB `count_rows` call fails.
    pub async fn count_chunks(&self) -> Result<u64> {
        let n = self
            .tbl
            .count_rows(None)
            .await
            .map_err(|e| Error::Store(format!("count_chunks: {e}")))?;
        Ok(n as u64)
    }
```

(The existing `memory_row_count` at line ~833 follows this exact pattern. Same `Error::Store` wrapping, same `count_rows(None)` call.)

- [ ] **Step 4: Run tests — verify pass**

```powershell
cargo nextest run --package anno-rag list_indexed_folder_paths_returns_distinct_folders
cargo nextest run --package anno-rag count_chunks_returns_zero_on_empty_store
```
Expected: PASS.

- [ ] **Step 5: Add folder-level legal delete helpers**

`Store::delete_doc_rows` currently filters by `source_path` and returns
`Result<()>`, so do not use it for corpus deletion. Add folder-level helpers in
`Store`:

```rust
pub async fn count_chunks_for_folder(&self, folder_path: &str) -> Result<u64> {
    let escaped = folder_path.replace('\'', "''");
    let n = self
        .tbl
        .count_rows(Some(&format!("folder_path = '{escaped}'")))
        .await
        .map_err(|e| Error::Store(format!("count_chunks_for_folder: {e}")))?;
    Ok(n as u64)
}

pub async fn delete_folder_rows(&self, folder_path: &str) -> Result<u64> {
    let before = self.count_chunks_for_folder(folder_path).await?;
    let escaped = folder_path.replace('\'', "''");
    self.tbl
        .delete(&format!("folder_path = '{escaped}'"))
        .await
        .map_err(|e| Error::Store(format!("delete_folder_rows: {e}")))?;
    Ok(before)
}
```

In `crates/anno-rag/src/pipeline.rs`, locate `impl Pipeline` and add (near the other public methods, after `embedder()`):
```rust
    /// Remove all chunk rows whose `folder_path` matches `path`.
    /// Thin wrapper around `Store::delete_folder_rows` so the MCP `forget`
    /// tool can cascade to the legal corpus without making the store field
    /// public. Returns the number of rows removed.
    ///
    /// # Errors
    /// Returns LanceDB / Arrow errors propagated from the store layer.
    pub async fn forget_legal_folder_path(&self, path: &str) -> Result<u64> {
        self.store.delete_folder_rows(path).await
    }
```

Add a test proving that two source files in the same folder are removed
together and a different folder remains.

- [ ] **Step 6: Add minimal test for the wrapper**

In `crates/anno-rag/src/pipeline.rs` test module (or create one):
```rust
#[cfg(test)]
mod knowledge_phase25_tests {
    use super::*;

    #[tokio::test]
    async fn forget_legal_folder_path_on_empty_store_is_noop() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        // Pipeline::new may fail without models — skip gracefully in that case.
        let pipeline = match Pipeline::new(cfg, [0u8; 32]).await {
            Ok(p) => p,
            Err(e) => { eprintln!("skipping: {e}"); return; }
        };
        // Should not panic, should not error on missing path.
        let _ = pipeline.forget_legal_folder_path("C:/does/not/exist").await;
    }
}
```

- [ ] **Step 7: Run tests + cargo check**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
cargo nextest run --package anno-rag list_indexed_folder_paths_returns_distinct_folders
cargo nextest run --package anno-rag count_chunks_returns_zero_on_empty_store
```
Expected: check PASS, both tests PASS.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag/src/store.rs crates/anno-rag/src/pipeline.rs
git commit -m "feat(rag): additive read-only helpers for MCP surface consolidation

Adds Store::list_indexed_folder_paths, Store::count_chunks,
Store::delete_folder_rows, and Pipeline pass-throughs needed by Phase 2.5's new
sources()/status()/forget() MCP tools.
No existing signature changes. No behavioral change to search, ingest,
legal_search, memory_recall, or any other path."
```

---

## Task 2: Extract `*_impl` Helpers From Existing Tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

The 9 deprecated tools and `vault_stats` currently have business logic inline in their `#[tool] async fn` bodies. Extracting the bodies into named `*_impl` methods makes the logic shareable between the legacy tool and the new unified tool without code duplication. **No behavior change.**

- [ ] **Step 1: Identify extraction targets**

Run to print every tool we'll extract:
```powershell
Select-String -Path crates\anno-rag-mcp\src\lib.rs -Pattern "async fn legal_ingest|async fn legal_search|async fn knowledge_search|async fn knowledge_add_local_folder|async fn knowledge_sync|async fn knowledge_sources|async fn knowledge_status|async fn knowledge_forget|async fn search\b|async fn vault_stats"
```
Expected: 10 hits. Note line numbers.

- [ ] **Step 2: Run existing tests as the safety net**

```powershell
cargo nextest run --package anno-rag-mcp -E 'test(knowledge|all_tool_names)'
```
Expected: all currently-passing tests pass. These are the regression net for the extraction refactor.

- [ ] **Step 3: Extract one impl at a time**

For each tool, the pattern is:

**Before:**
```rust
#[tool(description = "...")]
async fn knowledge_search(&self, Parameters(p): Parameters<KnowledgeSearchParams>) -> String {
    let service = match self.knowledge().await { Ok(s) => s, Err(e) => return format!("Error: {e}") };
    match service.search(p) {
        Ok(r) => serde_json::to_string_pretty(&r).unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    }
}
```

**After:**
```rust
// Extracted: returns Result-shaped output, no String formatting.
async fn knowledge_search_impl(
    &self,
    p: crate::knowledge::KnowledgeSearchParams,
) -> std::result::Result<crate::knowledge::KnowledgeSearchResponse, String> {
    let service = self.knowledge().await.map_err(|e| format!("{e}"))?;
    service.search(p).map_err(|e| format!("{e}"))
}

#[tool(description = "...")]
async fn knowledge_search(&self, Parameters(p): Parameters<KnowledgeSearchParams>) -> String {
    match self.knowledge_search_impl(p).await {
        Ok(r) => serde_json::to_string_pretty(&r).unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    }
}
```

Repeat for all 10 tools. The `*_impl` method goes inside the same `impl AnnoRagServer` block, **not** in the `#[tool_router] impl` block (because `*_impl` are not MCP tools — they're internal helpers).

Naming: `legal_ingest_impl`, `legal_search_impl`, `knowledge_search_impl`, `knowledge_add_local_folder_impl`, `knowledge_sync_impl`, `knowledge_sources_impl`, `knowledge_status_impl`, `knowledge_forget_impl`, `legacy_search_impl` (note: `search` becomes `legacy_search` in Task 8 — for now extract as `search_impl`), `vault_stats_impl`.

For each `*_impl`, choose a precise return type:
- `knowledge_search_impl` → `Result<KnowledgeSearchResponse, String>`
- `knowledge_sources_impl` → `Result<Vec<serde_json::Value>, String>` (matches existing service shape)
- `knowledge_status_impl` → `Result<KnowledgeStatus, String>`
- `knowledge_add_local_folder_impl` → `Result<String, String>` (returns source_id)
- `knowledge_sync_impl` → `Result<SyncSummary, String>`
- `knowledge_forget_impl` → `Result<u64, String>` (returns removed count)
- `legal_ingest_impl` → `Result<serde_json::Value, String>` (returns the legal_ingest summary; use Value because the type lives in legal extract module)
- `legal_search_impl` → `Result<serde_json::Value, String>` (same reason)
- `search_impl` (legacy reranked) → `Result<serde_json::Value, String>`
- `vault_stats_impl` → `Result<serde_json::Value, String>`

- [ ] **Step 4: Verify all existing tests still pass**

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
cargo nextest run --package anno-rag-mcp
```
Expected: all tests still pass. If any fail, the extraction altered behavior — fix.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "refactor(mcp): extract *_impl helpers from 10 existing tools

Mechanical refactor — each tool body is moved to an async *_impl method
returning Result<T, String>; the tool's #[tool] body becomes a thin
wrapper that formats the impl's result into the MCP String response.

Enables shared routing in Phase 2.5 new tools without duplicating
business logic. No behavioral change; existing tests pass unchanged."
```

---

## Task 3: New `index` Tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `crates/anno-rag-mcp/src/lib.rs` (or wherever the MCP tests live):
```rust
    #[tokio::test]
    async fn index_general_routes_to_knowledge_path() {
        // Smoke test: an empty folder with profile=general should produce a
        // response shape containing a "knowledge" key and "ok": true.
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let folder = dir.path().join("corpus");
        std::fs::create_dir_all(&folder).expect("mkdir");
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server
            .index_impl_routing(IndexParams {
                path: folder.display().to_string(),
                profile: "general".into(),
            })
            .await;
        // Result is JSON Value. Verify structural keys.
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert_eq!(v["profile"], "general");
        assert!(v.get("knowledge").is_some());
    }

    #[test]
    fn index_unknown_profile_returns_error() {
        // Profile "weird" must not panic; must return a JSON error.
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        // The validation should happen synchronously (or via simple dispatch),
        // so we can use a non-async test by calling the validation helper directly.
        let result = super::validate_profile("weird");
        assert!(result.is_err());
    }
```

The test calls `index_impl_routing` (not the MCP tool itself, to avoid needing the full router). The implementer adds this helper in the next step.

- [ ] **Step 2: Run tests — verify failure**

```powershell
cargo nextest run --package anno-rag-mcp index_general_routes_to_knowledge_path
cargo nextest run --package anno-rag-mcp index_unknown_profile_returns_error
```
Expected: FAIL — types and methods not defined.

- [ ] **Step 3: Add `IndexParams` and validation helper**

Near the existing `KnowledgeAddFolderParams` struct in `crates/anno-rag-mcp/src/lib.rs`, add:
```rust
/// Parameters for `index` — the unified indexing tool.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct IndexParams {
    /// Absolute path to the folder to index.
    pub path: String,
    /// "general" (knowledge only, idempotent), "legal", or "all". Default: "general".
    #[serde(default = "default_index_profile")]
    pub profile: String,
}

fn default_index_profile() -> String {
    "general".to_string()
}

/// Module-private validator. Returns Err for unknown profiles.
fn validate_profile(profile: &str) -> std::result::Result<(), String> {
    match profile {
        "general" | "legal" | "all" => Ok(()),
        other => Err(format!("unknown profile '{other}' (use 'general', 'legal', or 'all')")),
    }
}
```

- [ ] **Step 4: Implement `index_impl_routing`**

Add inside `impl AnnoRagServer`:
```rust
    /// Internal routing helper for the `index` MCP tool — exposed at this
    /// visibility so unit tests can call it without going through the rmcp
    /// router.
    pub(crate) async fn index_impl_routing(&self, p: IndexParams) -> String {
        if let Err(e) = validate_profile(&p.profile) {
            return serde_json::json!({"ok": false, "error": e}).to_string();
        }

        let mut knowledge_result: Option<serde_json::Value> = None;
        let mut legal_result: Option<serde_json::Value> = None;
        let mut errors: Vec<String> = Vec::new();

        if p.profile == "general" || p.profile == "all" {
            // Knowledge path: add_local_folder + sync.
            match self.knowledge_add_local_folder_impl(&p.path).await {
                Ok(source_id) => {
                    // Sync the just-added source.
                    match self
                        .knowledge_sync_impl(crate::KnowledgeSyncParams {
                            source_id: Some(source_id.clone()),
                        })
                        .await
                    {
                        Ok(summary) => {
                            knowledge_result = Some(serde_json::json!({
                                "source_id": source_id,
                                "summary": summary,
                            }));
                        }
                        Err(e) => errors.push(format!("knowledge sync: {e}")),
                    }
                }
                Err(e) => errors.push(format!("knowledge add: {e}")),
            }
        }

        if p.profile == "legal" || p.profile == "all" {
            match self.legal_ingest_impl(crate::LegalIngestParams { folder: p.path.clone(), recursive: true }).await {
                Ok(summary) => legal_result = Some(summary),
                Err(e) => errors.push(format!("legal ingest: {e}")),
            }
        }

        let ok = errors.is_empty();
        serde_json::json!({
            "ok": ok,
            "profile": p.profile,
            "knowledge": knowledge_result,
            "legal": legal_result,
            "errors": if errors.is_empty() { serde_json::Value::Null } else { serde_json::json!(errors) },
        })
        .to_string()
    }
```

The current `LegalIngestParams` shape is `{ folder: String, recursive: bool }`.
Use that exact type.

- [ ] **Step 5: Add the `#[tool] index` method**

Inside the existing `#[tool_router] impl AnnoRagServer` block, add:
```rust
    /// Unified index tool. Routes by profile.
    #[tool(
        description = "Index a local folder for search. profile='general' uses \
            the knowledge plane (idempotent — unchanged files are skipped on \
            re-run). profile='legal' runs the legal-enrichment pipeline \
            (currently NOT idempotent — re-runs re-extract and re-enrich). \
            profile='all' does both. Returns a unified summary with both halves."
    )]
    async fn index(&self, Parameters(p): Parameters<IndexParams>) -> String {
        self.index_impl_routing(p).await
    }
```

- [ ] **Step 6: Run tests — verify pass**

```powershell
cargo nextest run --package anno-rag-mcp index_general_routes_to_knowledge_path
cargo nextest run --package anno-rag-mcp index_unknown_profile_returns_error
```
Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add unified 'index' tool with profile routing"
```

---

## Task 4: New `search` Tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

The legacy `search` keeps its current name during this task; renaming to `legacy_search` happens in Task 8 (to keep diffs separable and avoid breaking `rmcp` macro expansion mid-task).

For now, the **new** `search` tool is added with a different name temporarily — `search_unified` — and Task 8 swaps the names. Why: rmcp registers tools by their method name, and we cannot have two `async fn search` methods in the same impl. The two-step rename in Task 8 avoids touching the registration layer mid-implementation.

- [ ] **Step 1: Write failing tests**

```rust
    #[tokio::test]
    async fn search_fast_all_returns_legal_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("all".into()),
                filters: None,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert_eq!(v["mode_used"], "fast");
        assert_eq!(v["scope_used"], "all");
        // Warning about legal scope skipped in fast mode.
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(warnings.iter().any(|w| w.as_str().unwrap_or("").contains("legal scope skipped")));
    }

    #[tokio::test]
    async fn search_fast_knowledge_returns_no_warning() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server
            .search_impl_routing(SearchUnifiedParams {
                query: "contrat".into(),
                top_k: 5,
                mode: Some("fast".into()),
                scope: Some("knowledge".into()),
                filters: None,
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["scope_used"], "knowledge");
        let warnings = v["warnings"].as_array().expect("warnings array");
        assert!(warnings.is_empty());
    }
```

- [ ] **Step 2: Run — verify failure**

```powershell
cargo nextest run --package anno-rag-mcp search_fast_all_returns_legal_warning
cargo nextest run --package anno-rag-mcp search_fast_knowledge_returns_no_warning
```
Expected: FAIL — types not defined.

- [ ] **Step 3: Add `SearchUnifiedParams`**

Near the other Search params:
```rust
/// Parameters for the unified `search` tool.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SearchUnifiedParams {
    /// User query.
    pub query: String,
    /// Maximum results. Default: 10.
    #[serde(default = "default_search_unified_top_k")]
    pub top_k: usize,
    /// "fast" (default) uses SQLite FTS only. "semantic" loads embedder.
    #[serde(default)]
    pub mode: Option<String>,
    /// "all" (default), "legal", or "knowledge".
    #[serde(default)]
    pub scope: Option<String>,
    /// Optional filters forwarded to legal_search when scope includes legal.
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
}

fn default_search_unified_top_k() -> usize {
    10
}
```

- [ ] **Step 4: Implement `search_impl_routing`**

```rust
    pub(crate) async fn search_impl_routing(&self, p: SearchUnifiedParams) -> String {
        let mode = p.mode.as_deref().unwrap_or("fast").to_string();
        let scope = p.scope.as_deref().unwrap_or("all").to_string();
        let mut hits: Vec<serde_json::Value> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // Knowledge half.
        if scope == "all" || scope == "knowledge" {
            let kparams = crate::knowledge::KnowledgeSearchParams {
                query: p.query.clone(),
                top_k: p.top_k,
                mode: Some(mode.clone()),
            };
            match self.knowledge_search_impl(kparams).await {
                Ok(r) => {
                    for h in &r.hits {
                        let mut v = serde_json::to_value(h)
                            .unwrap_or(serde_json::Value::Null);
                        if let serde_json::Value::Object(ref mut m) = v {
                            m.insert("source".into(), serde_json::Value::String("knowledge".into()));
                        }
                        hits.push(v);
                    }
                }
                Err(e) => warnings.push(format!("knowledge scope failed: {e}")),
            }
        }

        // Legal half.
        if scope == "all" || scope == "legal" {
            if mode == "fast" {
                warnings.push(
                    "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results.".into(),
                );
            } else {
                // Build LegalSearchParams from the unified params + filters.
                let lparams = build_legal_search_params(&p);
                match self.legal_search_impl(lparams).await {
                    Ok(r) => {
                        // Legal returns its own JSON; we extract its hits if present.
                        if let Some(legal_hits) = r.get("hits").and_then(|h| h.as_array()) {
                            for h in legal_hits {
                                let mut v = h.clone();
                                if let serde_json::Value::Object(ref mut m) = v {
                                    m.insert("source".into(), serde_json::Value::String("legal".into()));
                                }
                                hits.push(v);
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("legal scope failed: {e}")),
                }
            }
        }

        serde_json::json!({
            "ok": true,
            "mode_used": mode,
            "scope_used": scope,
            "hits": hits,
            "warnings": warnings,
        })
        .to_string()
    }
```

And a helper:
```rust
fn build_legal_search_params(p: &SearchUnifiedParams) -> crate::LegalSearchParams {
    // The exact field names depend on the existing LegalSearchParams struct.
    // The implementer should look up its definition and map p.query, p.top_k,
    // and p.filters into it. If p.filters is a JSON object with known keys
    // (doc_type, mandatory_clause_status, etc.), forward them; otherwise
    // ignore them.
    crate::LegalSearchParams {
        query: p.query.clone(),
        top_k: p.top_k,
        // ... map filters here as the existing struct dictates
        ..Default::default()
    }
}
```

(The implementer should `grep -n "pub struct LegalSearchParams" crates/anno-rag-mcp/src/lib.rs` to find the exact shape. If `LegalSearchParams` doesn't have `Default`, build it field-by-field with `None`/`""` defaults.)

- [ ] **Step 5: Add the `#[tool] search_unified` method**

Inside `#[tool_router]`:
```rust
    /// Unified search tool — temporary name during Phase 2.5.
    /// Task 8 swaps this to `search` and renames the legacy to `legacy_search`.
    #[tool(
        description = "Search Anno's local indexes. mode='fast' (default) uses \
            SQLite FTS5 — no models loaded. mode='semantic' loads the embedder. \
            scope='all' (default), 'knowledge', or 'legal'. filters forwarded \
            to legal scope."
    )]
    async fn search_unified(&self, Parameters(p): Parameters<SearchUnifiedParams>) -> String {
        self.search_impl_routing(p).await
    }
```

- [ ] **Step 6: Run tests — verify pass**

```powershell
cargo nextest run --package anno-rag-mcp search_fast_all_returns_legal_warning
cargo nextest run --package anno-rag-mcp search_fast_knowledge_returns_no_warning
```
Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add unified search (temp name 'search_unified', renamed in Task 8)"
```

---

## Task 5: New `sources` Tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
    #[tokio::test]
    async fn sources_aggregates_knowledge_and_legal_corpora() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server.sources_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        let sources = v["sources"].as_array().expect("sources array");
        // Empty store: zero sources of either kind.
        assert!(sources.is_empty() || sources.iter().all(|s|
            s["kind"] == "knowledge_folder" || s["kind"] == "legal_corpus"
        ));
    }
```

- [ ] **Step 2: Implement `sources_impl_routing`**

```rust
    pub(crate) async fn sources_impl_routing(&self) -> String {
        let mut sources: Vec<serde_json::Value> = Vec::new();

        // Knowledge folders.
        if let Ok(knowledge_sources) = self.knowledge_sources_impl().await {
            for s in &knowledge_sources {
                let mut entry = s.clone();
                if let serde_json::Value::Object(ref mut m) = entry {
                    m.insert("kind".into(), serde_json::Value::String("knowledge_folder".into()));
                }
                sources.push(entry);
            }
        }

        // Legal corpus paths — only available when pipeline is initialized.
        if let Ok(pipeline) = self.pipeline().await {
            if let Ok(paths) = pipeline.store_list_indexed_folder_paths().await {
                for path in paths {
                    sources.push(serde_json::json!({
                        "id": legal_folder_id(&path),
                        "kind": "legal_corpus",
                        "label": legal_folder_id(&path),
                    }));
                }
            }
        }

        serde_json::json!({ "ok": true, "sources": sources }).to_string()
    }
```

Add the legal id helper near the new routing helpers:

```rust
fn legal_folder_id(path: &str) -> String {
    let stable = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, path.as_bytes())
        .simple()
        .to_string();
    format!("legal_folder_{}", &stable[..12])
}
```

Note: `pipeline.store_list_indexed_folder_paths()` doesn't exist yet — `Pipeline::store` is private. The implementer adds a tiny accessor in `anno-rag` to expose the helper from Task 1:

In `crates/anno-rag/src/pipeline.rs`, near `forget_legal_folder_path`:
```rust
    /// Pass-through to `Store::list_indexed_folder_paths` for the MCP `sources()` tool.
    pub async fn store_list_indexed_folder_paths(&self) -> Result<Vec<String>> {
        self.store.list_indexed_folder_paths().await
    }

    /// Pass-through to `Store::count_chunks` for the MCP `status()` tool.
    pub async fn store_count_chunks(&self) -> Result<u64> {
        self.store.count_chunks().await
    }
```

(These are pure pass-throughs. Adding them is the only realistic way to expose the Task 1 helpers without making `Pipeline::store` public.)

- [ ] **Step 3: Add the `#[tool] sources` method**

```rust
    #[tool(description = "List all indexed sources. Labels and ids are pseudonymous; raw local paths are not returned. Does not load models.")]
    async fn sources(&self) -> String {
        self.sources_impl_routing().await
    }
```

- [ ] **Step 4: Run + commit**

```powershell
cargo nextest run --package anno-rag-mcp sources_aggregates_knowledge_and_legal_corpora
git add crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add unified sources() tool aggregating knowledge + legal"
```

---

## Task 6: New `status` Tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
    #[tokio::test]
    async fn status_returns_unified_health() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server.status_impl_routing().await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert!(v.get("knowledge").is_some());
        assert!(v.get("vault").is_some());
        assert!(v.get("models").is_some());
        // Empty: knowledge.objects == 0 and vault stats present
        assert_eq!(v["knowledge"]["objects"], 0);
        assert_eq!(v["models"]["embedder_loaded"], false);
    }
```

- [ ] **Step 2: Implement `status_impl_routing`**

```rust
    pub(crate) async fn status_impl_routing(&self) -> String {
        // Knowledge half — always available (SQLite-only).
        let knowledge = self
            .knowledge_status_impl()
            .await
            .ok()
            .and_then(|s| serde_json::to_value(s).ok())
            .unwrap_or(serde_json::Value::Null);

        // Vault stats — pull through existing impl.
        let vault = self.vault_stats_impl().await.unwrap_or(serde_json::Value::Null);

        // Legal corpus chunk count — best-effort; needs pipeline init.
        let legal = match self.pipeline().await {
            Ok(p) => match p.store_count_chunks().await {
                Ok(n) => serde_json::json!({ "chunks": n }),
                Err(_) => serde_json::Value::Null,
            },
            Err(_) => serde_json::Value::Null,
        };

        // Model load state — Pipeline carries the flags.
        let models = match self.pipeline().await {
            Ok(p) => serde_json::json!({
                "embedder_loaded": p.embedder_loaded(),
                "detector_loaded": p.detector_loaded_or_false(),
            }),
            Err(_) => serde_json::json!({ "embedder_loaded": false, "detector_loaded": false }),
        };

        serde_json::json!({
            "ok": true,
            "knowledge": knowledge,
            "legal": legal,
            "vault": vault,
            "models": models,
        })
        .to_string()
    }
```

The `Pipeline::embedder_loaded` already exists (line ~170). `detector_loaded_or_false` does not. The implementer should `grep -n "fn detector_loaded" crates/anno-rag/src/pipeline.rs` and either use the existing accessor or add a one-liner:
```rust
    pub fn detector_loaded(&self) -> bool {
        self.detector.get().is_some()  // exact expression depends on the field type
    }
```
If absent, add it (pure read accessor, additive). If present, use it.

- [ ] **Step 3: Add the `#[tool] status` method**

```rust
    #[tool(description = "Anno-wide index health: source counts, chunks, vault stats, model load state. Does not load models.")]
    async fn status(&self) -> String {
        self.status_impl_routing().await
    }
```

- [ ] **Step 4: Run + commit**

```powershell
cargo nextest run --package anno-rag-mcp status_returns_unified_health
git add crates/anno-rag/src/pipeline.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add unified status() tool aggregating knowledge + legal + vault + models"
```

---

## Task 7: New `forget` Tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
    #[tokio::test]
    async fn forget_uuid_routes_to_knowledge_forget() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        // Non-existent UUID: forget should return ok with zero removed.
        let out = server
            .forget_impl_routing(ForgetParams {
                target: uuid::Uuid::nil().to_string(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
        assert_eq!(v["removed"]["knowledge_objects"], 0);
    }

    #[tokio::test]
    async fn forget_legal_id_is_noop_when_unknown() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };
        let server = AnnoRagServer::new(cfg, [0u8; 32]);
        let out = server
            .forget_impl_routing(ForgetParams {
                target: "legal_folder_000000000000".into(),
            })
            .await;
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["ok"], true);
    }
```

- [ ] **Step 2: Add `ForgetParams` and routing**

```rust
/// Parameters for the unified `forget` tool.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ForgetParams {
    /// Source id (UUID), legal corpus id returned by sources(), or explicit
    /// local folder path supplied by the user.
    pub target: String,
}

impl AnnoRagServer {
    pub(crate) async fn forget_impl_routing(&self, p: ForgetParams) -> String {
        let mut knowledge_removed: u64 = 0;
        let mut legal_removed: u64 = 0;
        let mut errors: Vec<String> = Vec::new();

        // Resolution: UUID, legal_folder_* id, or explicit user path.
        if p.target.starts_with("legal_folder_") {
            // Legal corpus only. Resolve the pseudonymous id by scanning known
            // internal folder paths and recomputing legal_folder_id(path).
            match self.pipeline().await {
                Ok(pipeline) => match self.resolve_legal_folder_id(&pipeline, &p.target).await {
                    Ok(Some(path)) => match pipeline.forget_legal_folder_path(&path).await {
                        Ok(n) => legal_removed += n,
                        Err(e) => errors.push(format!("legal forget: {e}")),
                    },
                    Ok(None) => {}
                    Err(e) => errors.push(format!("legal resolve: {e}")),
                },
                Err(e) => errors.push(format!("pipeline: {e}")),
            }
        } else if uuid::Uuid::parse_str(&p.target).is_ok() {
            // Knowledge source id.
            match self.knowledge_forget_impl(crate::KnowledgeForgetParams { source_id: p.target.clone() }).await {
                Ok(n) => knowledge_removed += n,
                Err(e) => errors.push(format!("knowledge forget: {e}")),
            }
        } else {
            // Explicit user folder path — cascade both planes.
            match self.knowledge_forget_by_path(&p.target).await {
                Ok(n) => knowledge_removed += n,
                Err(e) => errors.push(format!("knowledge forget: {e}")),
            }
            if let Ok(pipeline) = self.pipeline().await {
                match pipeline.forget_legal_folder_path(&p.target).await {
                    Ok(n) => legal_removed += n,
                    Err(e) => errors.push(format!("legal forget: {e}")),
                }
            }
        }

        let ok = errors.is_empty();
        serde_json::json!({
            "ok": ok,
            "removed": { "knowledge_objects": knowledge_removed, "legal_chunks": legal_removed },
            "errors": if errors.is_empty() { serde_json::Value::Null } else { serde_json::json!(errors) },
        })
        .to_string()
    }

    /// Forget knowledge sources whose stable_key/provider_key matches `path`.
    /// Helper for the explicit user path-resolution branch of `forget`.
    async fn knowledge_forget_by_path(&self, path: &str) -> std::result::Result<u64, String> {
        let service = self.knowledge().await.map_err(|e| format!("{e}"))?;
        service.forget_source_by_path(path).map_err(|e| format!("{e}"))
    }

    async fn resolve_legal_folder_id(
        &self,
        pipeline: &anno_rag::pipeline::Pipeline,
        id: &str,
    ) -> std::result::Result<Option<String>, String> {
        let paths = pipeline
            .store_list_indexed_folder_paths()
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(paths.into_iter().find(|path| legal_folder_id(path) == id))
    }
}
```

Add `KnowledgeControlStore::source_by_provider_key(path)` and
`KnowledgeService::forget_source_by_path(path)` instead of matching labels.
This keeps `sources()` private-by-default while making explicit user path
forget exact.

- [ ] **Step 3: Add `#[tool] forget`**

```rust
    #[tool(description = "Remove an indexed source. Accepts a source_id (UUID), a legal corpus id from sources(), or an explicit folder path. Does not load models.")]
    async fn forget(&self, Parameters(p): Parameters<ForgetParams>) -> String {
        self.forget_impl_routing(p).await
    }
```

- [ ] **Step 4: Run + commit**

```powershell
cargo nextest run --package anno-rag-mcp forget_uuid_routes_to_knowledge_forget
cargo nextest run --package anno-rag-mcp forget_legal_id_is_noop_when_unknown
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add unified forget() tool with target resolution"
```

---

## Task 8: Rename Legacy `search` to `legacy_search` and Promote `search_unified` to `search`

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

This task is purely a rename. Done in one commit so the tool surface is consistent.

- [ ] **Step 1: Rename the existing legacy reranked tool**

Find the existing `async fn search(...)` method (the legacy reranked LanceDB one). Rename it to `legacy_search`:
```rust
    #[tool(
        description = "Deprecated — use 'search(scope=\"legal\", mode=\"semantic\")' for equivalent behavior. Continues to work."
    )]
    async fn legacy_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        // body unchanged
        ...
    }
```

Rename its associated `search_impl` (from Task 2) to `legacy_search_impl`.

- [ ] **Step 2: Rename `search_unified` to `search`**

The temporary `search_unified` tool added in Task 4 becomes the canonical `search`:
```rust
    #[tool(
        description = "Search Anno's local indexes. mode='fast' (default) uses \
            SQLite FTS5 — no models loaded. mode='semantic' loads the embedder. \
            scope='all' (default), 'knowledge', or 'legal'. filters forwarded \
            to legal scope."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchUnifiedParams>) -> String {
        self.search_impl_routing(p).await
    }
```

- [ ] **Step 3: Run tests — verify both still work**

```powershell
cargo nextest run --package anno-rag-mcp search_fast_all_returns_legal_warning
cargo nextest run --package anno-rag-mcp search_fast_knowledge_returns_no_warning
```
Expected: PASS (tests reference `search_impl_routing`, name unchanged).

If any existing test references the legacy `search` by name (via the `legacy_search` rename), update it to use `legacy_search` or `legacy_search_impl`.

- [ ] **Step 4: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): rename legacy search to legacy_search; promote unified search to canonical name"
```

---

## Task 9: Update Descriptions of 8 Other Deprecated Tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

`legacy_search` got its new description in Task 8. The remaining 8 tools need theirs updated.

- [ ] **Step 1: Update each `#[tool(description = "...")]` attribute**

For each of these 8 tools, change the `description` string to the deprecation banner:

| Tool method | New description |
|---|---|
| `legal_search` | `"Deprecated — use 'search(query, scope=\"legal\", filters={...})' instead. Continues to work."` |
| `legal_ingest` | `"Deprecated — use 'index(path, profile=\"legal\")' instead. Continues to work."` |
| `knowledge_search` | `"Deprecated — use 'search(query, scope=\"knowledge\")' instead. Continues to work."` |
| `knowledge_add_local_folder` | `"Deprecated — use 'index(path)' instead. Continues to work."` |
| `knowledge_sync` | `"Deprecated — use 'index(path)' (re-indexes idempotently). Continues to work."` |
| `knowledge_sources` | `"Deprecated — use 'sources()' instead. Continues to work."` |
| `knowledge_status` | `"Deprecated — use 'status()' instead. Continues to work."` |
| `knowledge_forget` | `"Deprecated — use 'forget(target)' instead. Continues to work."` |

The handler bodies (which now call `*_impl`) remain unchanged.

- [ ] **Step 2: Add a test that verifies descriptions are correctly marked**

```rust
    #[test]
    fn deprecated_tools_have_deprecation_banner_in_description() {
        // Read the source file at compile time and verify each deprecated tool
        // has the word "Deprecated" in its description. This is a static check
        // that catches forgotten updates.
        let src = include_str!("lib.rs");
        let deprecated = [
            "legal_search", "legal_ingest", "knowledge_search",
            "knowledge_add_local_folder", "knowledge_sync",
            "knowledge_sources", "knowledge_status", "knowledge_forget",
            "legacy_search",
        ];
        for name in deprecated {
            // Look for the tool method declaration and verify "Deprecated" appears
            // in the preceding #[tool(description = "...")] block.
            let needle = format!("async fn {name}");
            let pos = src.find(&needle).unwrap_or_else(|| panic!("tool {name} not found"));
            let before = &src[..pos];
            let tool_block_start = before.rfind("#[tool(").unwrap_or_else(|| panic!("no #[tool( before {name}"));
            let tool_block = &src[tool_block_start..pos];
            assert!(
                tool_block.contains("Deprecated"),
                "tool {name} description missing 'Deprecated' marker: {tool_block}",
            );
        }
    }
```

- [ ] **Step 3: Run + commit**

```powershell
cargo nextest run --package anno-rag-mcp deprecated_tools_have_deprecation_banner_in_description
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "docs(mcp): mark 8 legacy tools as deprecated in their descriptions"
```

---

## Task 10: Update `all_tool_names()` Ordering

**Files:**
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Write failing test**

In `crates/anno-rag-mcp/src/health.rs` test module:
```rust
    #[test]
    fn all_tool_names_lists_new_unified_tools_first() {
        let names = all_tool_names();
        // The 5 new tools should be at the front, in this order.
        assert_eq!(names[0], "index");
        assert_eq!(names[1], "search");
        assert_eq!(names[2], "sources");
        assert_eq!(names[3], "status");
        assert_eq!(names[4], "forget");
    }

    #[test]
    fn all_tool_names_includes_legacy_search() {
        let names = all_tool_names();
        assert!(names.iter().any(|n| n == "legacy_search"));
        // legacy_search should appear AFTER the new unified tools.
        let new_search_idx = names.iter().position(|n| n == "search").expect("search present");
        let legacy_idx = names.iter().position(|n| n == "legacy_search").expect("legacy_search present");
        assert!(legacy_idx > new_search_idx, "legacy_search should appear after new 'search'");
    }

    #[test]
    fn all_tool_names_still_includes_legacy_phase2_tools() {
        let names = all_tool_names();
        // Phase 2 tools must still be advertised for back-compat.
        for legacy in [
            "knowledge_search", "knowledge_add_local_folder", "knowledge_sync",
            "knowledge_sources", "knowledge_status", "knowledge_forget",
            "legal_ingest", "legal_search",
        ] {
            assert!(names.iter().any(|n| n == legacy), "missing legacy tool {legacy}");
        }
    }
```

- [ ] **Step 2: Run — verify failure**

```powershell
cargo nextest run --package anno-rag-mcp all_tool_names_lists_new_unified_tools_first
```
Expected: FAIL — new tools not in the list yet.

- [ ] **Step 3: Update `all_tool_names()`**

In `crates/anno-rag-mcp/src/health.rs`, replace the `all_tool_names()` body so the new tools come first:
```rust
pub fn all_tool_names() -> Vec<String> {
    [
        // New unified surface (Phase 2.5) — recommended
        "index", "search", "sources", "status", "forget",
        // ... keep the rest of the existing entries unchanged ...
        // ... append legacy_search at the end of the deprecated section ...
        "legacy_search",
    ]
    .into_iter().map(String::from).collect()
}
```

The implementer should preserve every existing entry (just reorder so the 5 new ones come first and `legacy_search` is appended near the deprecated section).

- [ ] **Step 4: Run all 3 health tests**

```powershell
cargo nextest run --package anno-rag-mcp all_tool_names
```
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/health.rs
git commit -m "feat(mcp): list 5 new unified tools first in all_tool_names()"
```

---

## Task 11: Final Verification

**Files:** none

- [ ] **Step 1: Format**

```powershell
cargo fmt --check
```
Expected: PASS. If not, run `cargo fmt` then re-check.

- [ ] **Step 2: Check all touched crates**

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```
Expected: zero warnings, zero errors.

- [ ] **Step 3: Targeted tests for new tools**

```powershell
cargo nextest run --package anno-rag-mcp -E 'test(index|search|sources|status|forget|deprecated|all_tool_names)'
```
Expected: PASS.

- [ ] **Step 4: Non-regression — pre-existing Phase 1/2 tests still pass**

```powershell
cargo nextest run --package anno-rag-mcp -E 'test(knowledge)'
cargo nextest run --package anno-knowledge-core
cargo nextest run --package anno-knowledge-store
cargo nextest run --package anno-source-local
```
Expected: PASS — façade-only means no observable behavior change for any prior caller.

- [ ] **Step 5: Detect scope of changes**

```powershell
npx gitnexus detect-changes
```
Expected: changes limited to:
- `crates/anno-rag/src/store.rs` (Task 1 additive readers)
- `crates/anno-rag/src/pipeline.rs` (Task 1 additive wrapper + Task 5/6 pass-throughs)
- `crates/anno-rag-mcp/src/lib.rs` (Tasks 2-9: extraction + new tools + rename + descriptions)
- `crates/anno-rag-mcp/src/health.rs` (Task 10: ordering)

No changes to `crates/anno-knowledge-*`, `crates/anno-source-local`, `crates/anno-rag-tabular`, or any test that wasn't directly added by this plan.

- [ ] **Step 6: Re-index GitNexus**

```powershell
npx gitnexus analyze
```

---

## Acceptance Criteria

- 5 new MCP tools (`index`, `search`, `sources`, `status`, `forget`) respond and route correctly through `*_impl_routing` helpers.
- The legacy reranked `search` is renamed to `legacy_search`; the new `search` is the unified entry point.
- 9 tools have their descriptions updated to mark deprecation; all 9 continue to respond exactly as today (handlers unchanged except for delegation to `*_impl`).
- `available_tools` (`all_tool_names()`) lists `index, search, sources, status, forget` first, followed by all pre-existing entries, with `legacy_search` listed near the deprecated section.
- `search(scope="all", mode="fast")` returns knowledge hits plus a warning about the skipped legal scope.
- `search(scope="legal", filters={"doc_type": "contract"})` returns the same hits as `legal_search` with equivalent params.
- `forget("legal_folder_<id>")` cascades to the legal corpus chunks via
  `Pipeline::forget_legal_folder_path`.
- `forget(uuid)` cascades to knowledge sources via the existing knowledge_forget path.
- `forget("C:/path")` cascades to both planes only when the path is explicitly
  supplied by the caller; `sources()` never exposes raw paths.
- `sources()` lists knowledge folders AND legal corpus paths (via `Store::list_indexed_folder_paths`).
- `status()` aggregates knowledge counts, legal chunk count, vault stats, and model load state without loading models.
- Small additive helpers on `anno-rag` and `anno-knowledge-store` are added.
  **No existing signature is changed.**
- All Phase 1 / Phase 2 tests pass.
- `npx gitnexus detect-changes` reports only the expected files.

## Self-Review Against Spec

Covered:
- §2 (Decisions A/A1/S1/N2/D1): Tasks 2-10 implement all five
- §4.1 `index`: Task 3
- §4.2 `search`: Task 4 + Task 8 (rename)
- §4.3 `sources`: Task 5
- §4.4 `status`: Task 6
- §4.5 `forget`: Task 7
- §5 deprecation banners: Tasks 8, 9
- §6 routing pattern (extracted `*_impl`): Task 2
- §7 graceful degradation matrix: Tasks 4 (fast+all warning), 6 (status reports models_loaded=false)
- §8 `all_tool_names()` ordering: Task 10
- §9 test strategy: each task has its TDD tests

Deviation from §10 acceptance ("no file modified outside anno-rag-mcp/src/"): documented in the "Spec Refinement" header above. Three additive helpers on `anno-rag` are necessary for `sources`/`status`/`forget` to work; each is purely additive (new public method) and changes no existing signature.

Known limitations:
- `index(profile="legal")` is not idempotent (matches current `legal_ingest`).
  Description explicitly says so.
- The `search` name is intentionally repurposed. Legacy requests that include
  `rerank` route to `legacy_search`; clients that called `search` without
  `rerank` should migrate to `legacy_search` if they need the old legal/vector
  response shape.

Both items are tracked in [`docs/product/roadmap.md`](../../product/roadmap.md).
