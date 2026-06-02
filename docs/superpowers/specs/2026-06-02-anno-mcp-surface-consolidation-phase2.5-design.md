# Anno MCP Surface Consolidation (Phase 2.5) Design

**Date:** 2026-06-02
**Status:** Draft for review
**Scope:** Unify the indexing and search MCP surface behind 5 verb-only tools
(`index`, `search`, `sources`, `status`, `forget`) without touching the
underlying ingestion/search engines. Façade-only. 9 legacy tools stay
functional with deprecated descriptions; the existing implementations are
re-used through thin routing.

**Parent context:** the Anno knowledge service grew organically over several
phases — Phase 1/2 shipped a new multi-source path while the v0.1 legal
pipeline kept running. The result is that a user (or Claude reading the tool
list) faces ~9 indexing/search tools spread across 4 surfaces (`search`,
`legal_search`, `knowledge_search`, plus the future `knowledge_search(semantic)`
from Phase 3). The internal duplication is bounded and justified (see Phase 2
spec); the external surface, however, has become hard to navigate. This phase
fixes the external surface only.

**Related docs:**
- [`docs/product/roadmap.md`](../../product/roadmap.md) — full deferred backlog
- [`docs/superpowers/specs/2026-05-29-anno-local-knowledge-service-multisource-design.md`](2026-05-29-anno-local-knowledge-service-multisource-design.md) — parent multisource design
- [`docs/superpowers/specs/2026-06-01-anno-knowledge-local-folder-source-phase2-design.md`](2026-06-01-anno-knowledge-local-folder-source-phase2-design.md) — Phase 2 (shipped)
- [`docs/superpowers/specs/2026-06-02-anno-knowledge-vector-projection-phase3-design.md`](2026-06-02-anno-knowledge-vector-projection-phase3-design.md) — Phase 3 (next)

---

## 1. Goal

Today an MCP client (Claude Desktop, Codex) must choose between:

```text
Indexing:  legal_ingest        | knowledge_add_local_folder + knowledge_sync
Search:    search (reranked)   | legal_search                | knowledge_search(fast)
Inspect:   vault_stats         | knowledge_status | knowledge_sources
Cleanup:   (none for legal)    | knowledge_forget
```

After Phase 2.5, the same client uses:

```text
Indexing:  index(path, profile?)
Search:    search(query, mode?, scope?, filters?)
Inspect:   sources() | status()
Cleanup:   forget(target)
```

Legacy tools remain registered and functional so existing workflows and
scripts continue to run unchanged. Their descriptions point clients at the new
surface.

## 2. Decisions Locked In Brainstorming

These were chosen explicitly and are not open for re-litigation in the plan:

- **Façade-only (Option A).** Zero changes to `Pipeline`, `Store`,
  `KnowledgeIndexer`, `KnowledgeControlStore`, `legal_*`, `anno-rag-tabular`,
  or any storage plane. Routing only.
- **One indexing tool with `profile` (A1).** `index(path, profile?)` chooses
  pipeline. Profiles: `"general"` (knowledge only, idempotent), `"legal"`
  (legal corpus, not idempotent — same as today), `"all"` (both).
- **One search tool with `mode` + `scope` (S1).** `search(query, mode?, scope?, filters?)`.
  Defaults: `mode="fast"`, `scope="all"`, `top_k=10`.
- **Clean verb names, replacing the legacy `search` (N2).** The new `search`
  tool replaces the existing legacy reranked `search` — the previous handler
  is renamed `legacy_search` and marked deprecated. Other new names
  (`index`, `sources`, `status`, `forget`) don't collide.
- **Soft deprecation (D1).** Legacy tool handlers stay intact. Only their
  registered description changes to signal deprecation and point at the
  replacement. Effective removal is a future phase.

## 3. Non-Goals

- No refactor of `Pipeline`, `Store`, `KnowledgeIndexer`, or `KnowledgeControlStore`.
- No new storage plane, no schema migration, no data movement.
- No effective removal of any tool. Legacy tools stay registered and functional.
- No idempotence change for `legal_ingest` / `index(profile="legal")`. Current
  behavior is preserved (re-running re-extracts and re-enriches). Tracked in
  the roadmap.
- No model-loading changes. `mode="fast"` continues to load nothing; semantic
  paths load embedder/detector exactly as they do today.
- No new search ranking algorithm. The new `search` *routes* between
  existing rankers (FTS for knowledge, reranked LanceDB for legal); the
  RRF fusion from Phase 3 is not part of this phase.
- No surface changes to `legal_extract_*`, `legal_timeline`, `legal_risk_review`,
  `legal_mandatory_clause_audit`, `legal_prescription_check`, `legal_validate_field`,
  `legal_graph_query`, `legal_rehydrate_citation`, `memory_*`, `tabular_*`,
  `vault_stats`, `anno_health`, `download_models`, `detect`, `rehydrate`,
  `anno_init_vault`. Those remain as-is.

## 4. The new surface — 5 tools

### 4.1 `index`

```rust
/// Parameters for `index`.
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct IndexParams {
    /// Absolute path to the folder to index.
    pub path: String,
    /// Indexing profile: "general" (knowledge only), "legal" (legal corpus),
    /// or "all" (both). Default: "general".
    #[serde(default = "default_index_profile")]
    pub profile: String,
}
fn default_index_profile() -> String { "general".to_string() }

#[tool(
    description = "Index a local folder for search. profile='general' uses the \
        knowledge plane (idempotent re-runs skip unchanged files). \
        profile='legal' runs the legal-enrichment pipeline (re-runs re-extract). \
        profile='all' does both. Returns a unified summary."
)]
async fn index(&self, Parameters(p): Parameters<IndexParams>) -> String;
```

**Response shape:**
```json
{
  "ok": true,
  "profile": "general",
  "knowledge": { "source_id": "...", "summary": { "seen": 10, "fts_ready": 10, "failed": 0, "truncated": false } },
  "legal":     null   // populated when profile is "legal" or "all"
}
```

For `profile="legal"`, `knowledge` is `null` and `legal` carries the
`legal_ingest` summary unchanged. For `profile="all"`, both are populated;
errors in one half don't block the other (`ok` reflects overall success).

### 4.2 `search`

```rust
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SearchUnifiedParams {
    /// User query.
    pub query: String,
    /// Maximum results.
    #[serde(default = "default_search_top_k")]
    pub top_k: usize,
    /// "fast" (default) uses SQLite FTS only — no models. "semantic" loads
    /// the embedder and (for legal scope) the reranker.
    #[serde(default)]
    pub mode: Option<String>,
    /// "all" (default), "legal", or "knowledge".
    #[serde(default)]
    pub scope: Option<String>,
    /// Forwarded to legal_search when scope includes legal.
    /// Recognized keys: doc_type, mandatory_clause_status, etc.
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
}
fn default_search_top_k() -> usize { 10 }

#[tool(
    description = "Search Anno's local indexes. mode='fast' (default) uses \
        SQLite FTS5 — no models loaded. mode='semantic' loads the embedder. \
        scope='all' (default) searches knowledge + legal corpora; 'knowledge' \
        or 'legal' targets one. filters apply to legal results."
)]
async fn search(&self, Parameters(p): Parameters<SearchUnifiedParams>) -> String;
```

**Response shape:**
```json
{
  "ok": true,
  "mode_used": "fast",
  "scope_used": "all",
  "hits": [ /* unified shape: chunk_id, source_kind, snippet_pseudo, score */ ],
  "warnings": [
    "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results."
  ]
}
```

`hits` is a single array with both knowledge and legal entries when both run.
Each hit carries a `source` field (`"knowledge"` or `"legal"`) so callers
can distinguish. **No cross-scope fusion in this phase** — hits are
concatenated in the order knowledge-first, legal-second, each half ranked by
its own scorer. Phase 3 adds intra-scope RRF *within* the knowledge half
(FTS ⊕ vectors); cross-scope fusion between knowledge and legal results
is a separate concern parked on the roadmap.

### 4.3 `sources`

```rust
#[tool(
    description = "List all indexed sources: knowledge folders, and legal corpus \
        paths derived from the chunks table. Does not load models."
)]
async fn sources(&self) -> String;
```

**Response shape:**
```json
{
  "ok": true,
  "sources": [
    { "id": "<uuid>",           "kind": "knowledge_folder", "label": "local:contracts", "path": "C:/docs/contracts" },
    { "id": "legal:C:/corpus",  "kind": "legal_corpus",     "label": "C:/corpus",        "path": "C:/corpus",        "indexed_chunks": 1342 }
  ]
}
```

The `legal_corpus` entries are synthesized from `SELECT DISTINCT folder_path FROM chunks` in the existing `Store::chunks` table. Their `id` is the synthetic `"legal:{path}"` string so that `forget(id)` knows which side to remove from.

### 4.4 `status`

```rust
#[tool(
    description = "Anno-wide index health: source counts, chunks, vault stats, \
        and model load state. Does not load models."
)]
async fn status(&self) -> String;
```

**Response shape:**
```json
{
  "ok": true,
  "knowledge": { "sources": 2, "objects": 1240, "chunks": 5678, "failures": 3, "vector_ready": 0, "vector_pending": 1240 },
  "legal":     { "chunks": 1342, "last_index_at": "2026-05-30T14:22:00Z" },
  "vault":     { "tokens": 3421, "per_category": { "PERSON": 1200, "ORG": 240, "..." : 0 } },
  "models":    { "embedder_loaded": false, "detector_loaded": false }
}
```

Aggregates the existing `knowledge_status`, `vault_stats`, and Pipeline
load-state telemetry without changing any of them.

### 4.5 `forget`

```rust
#[derive(Debug, Clone, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ForgetParams {
    /// Source id (UUID), folder path, or legal corpus id (`legal:{path}`).
    pub target: String,
}

#[tool(
    description = "Remove an indexed source. Accepts a source_id (UUID), a \
        folder path, or a legal corpus id. Cascades to knowledge SQLite/FTS \
        and (when applicable) to chunks LanceDB. Does not load models."
)]
async fn forget(&self, Parameters(p): Parameters<ForgetParams>) -> String;
```

**Response shape:**
```json
{ "ok": true, "removed": { "knowledge_objects": 12, "legal_chunks": 245 } }
```

`target` resolution rules:
- If it parses as a UUID → treat as `source_id`, route to `knowledge_forget`-equivalent
- If it starts with `legal:` → strip prefix, route to legal corpus removal (`Store::delete_doc_rows` for matching `folder_path`)
- Otherwise (a folder path) → cascade to both: lookup knowledge source by stable_key matching the path AND call legal corpus removal for that path

## 5. Deprecation — 9 tools, descriptions only

Implementation is **byte-identical** to today. Only the registered
`description` string in `#[tool(...)]` changes. The handler bodies, response
shapes, and side effects are untouched.

| Tool | New description |
|---|---|
| `search` (legacy reranked) | `"Deprecated — use 'search(scope=\"legal\", mode=\"semantic\")' for equivalent behavior. Continues to work."` |
| `legal_search` | `"Deprecated — use 'search(query, scope=\"legal\", filters={...})' instead. Continues to work."` |
| `legal_ingest` | `"Deprecated — use 'index(path, profile=\"legal\")' instead. Continues to work."` |
| `knowledge_search` | `"Deprecated — use 'search(query, scope=\"knowledge\")' instead. Continues to work."` |
| `knowledge_add_local_folder` | `"Deprecated — use 'index(path)' instead. Continues to work."` |
| `knowledge_sync` | `"Deprecated — use 'index(path)' (re-indexes idempotently). Continues to work."` |
| `knowledge_sources` | `"Deprecated — use 'sources()' instead. Continues to work."` |
| `knowledge_status` | `"Deprecated — use 'status()' instead. Continues to work."` |
| `knowledge_forget` | `"Deprecated — use 'forget(target)' instead. Continues to work."` |

**Important:** the legacy `search` tool is renamed to `legacy_search` *only in
its `available_tools` listing and registered method name*. Existing rmcp
routing relies on the method name matching the registered tool, so the
actual `#[tool] async fn search` declaration is renamed
`legacy_search`, and a NEW `#[tool] async fn search` is added that
dispatches to the new unified handler. Any caller that explicitly invokes
`search` gets the new behavior; any caller that uses the old name
`legacy_search` (after this phase) gets the old behavior.

For other deprecated tools (legal_search, knowledge_*, legal_ingest), the
method names are unchanged — only the `description` attribute moves.

## 6. Internal routing — no logic, only dispatch

The new tools route to handler functions extracted from the existing tools.
Today's pattern (illustrative for `knowledge_search`):

```rust
#[tool(description = "...")]
async fn knowledge_search(&self, Parameters(p): Parameters<KnowledgeSearchParams>) -> String {
    let service = match self.knowledge().await { Ok(s) => s, Err(e) => return format!("Error: {e}") };
    match service.search(p) { Ok(r) => serde_json::to_string_pretty(&r).unwrap(), Err(e) => format!("Error: {e}") }
}
```

Phase 2.5 splits the body into `*_impl`:

```rust
// Extracted: pure handler, no #[tool] attribute
async fn knowledge_search_impl(&self, p: KnowledgeSearchParams)
    -> std::result::Result<KnowledgeSearchResponse, String>
{
    let service = self.knowledge().await.map_err(|e| format!("{e}"))?;
    service.search(p).map_err(|e| format!("{e}"))
}

// Legacy tool — body is now a thin wrapper around the impl
#[tool(description = "Deprecated — use 'search(query, scope=\"knowledge\")' instead. Continues to work.")]
async fn knowledge_search(&self, Parameters(p): Parameters<KnowledgeSearchParams>) -> String {
    match self.knowledge_search_impl(p).await {
        Ok(r) => serde_json::to_string_pretty(&r).unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    }
}

// New unified tool — routes by scope
#[tool(description = "Search Anno's local indexes...")]
async fn search(&self, Parameters(p): Parameters<SearchUnifiedParams>) -> String {
    let mode = p.mode.as_deref().unwrap_or("fast");
    let scope = p.scope.as_deref().unwrap_or("all");
    let mut hits = Vec::new();
    let mut warnings = Vec::new();

    if scope == "all" || scope == "knowledge" {
        match self.knowledge_search_impl(KnowledgeSearchParams {
            query: p.query.clone(), top_k: p.top_k, mode: p.mode.clone(),
        }).await {
            Ok(r) => hits.extend(r.hits.into_iter().map(|h| with_source(h, "knowledge"))),
            Err(e) => warnings.push(format!("knowledge scope failed: {e}")),
        }
    }
    if scope == "all" || scope == "legal" {
        if mode == "fast" {
            warnings.push("legal scope skipped in fast mode (requires models). Use mode='semantic'.".into());
        } else {
            match self.legal_search_impl(p.query.clone(), p.top_k, p.filters.clone()).await {
                Ok(r) => hits.extend(r.hits.into_iter().map(|h| with_source(h, "legal"))),
                Err(e) => warnings.push(format!("legal scope failed: {e}")),
            }
        }
    }
    serde_json::json!({
        "ok": true, "mode_used": mode, "scope_used": scope,
        "hits": hits, "warnings": warnings,
    }).to_string()
}
```

The same impl-extraction pattern applies to `legal_ingest`,
`knowledge_add_local_folder`, `knowledge_sync`, `knowledge_sources`,
`knowledge_status`, `knowledge_forget`, and the legacy reranked `search`.
**No business logic moves**; only `match`/`return` plumbing is shared
between the legacy tool and the new tool.

## 7. Graceful degradation matrix

| scope \ mode | `fast` | `semantic` |
|---|---|---|
| `knowledge` | ✅ SQLite FTS5 (no models) | ✅ (Phase 3 adds vectors; pre-Phase-3 falls back to FTS) |
| `legal` | ⚠️ Skipped + warning (legal_search needs embedder) | ✅ legal_search reranked |
| `all` | knowledge runs, legal skipped + warning | both run, results concatenated |

The warning is always machine-readable: `warnings` is an array of strings in
the response. Claude can read it and offer the user a follow-up like
"want me to retry with semantic mode for the full results?".

## 8. `health.rs` updates

`all_tool_names()` now lists 5 new tools first (the recommended surface), then
all existing tools (including the deprecated ones, including `legacy_search`):

```rust
pub fn all_tool_names() -> Vec<String> {
    [
        // New unified surface (recommended)
        "index", "search", "sources", "status", "forget",
        // Memory (unchanged)
        "memory_save", "memory_recall", /* ... */
        // Legal analysis tools (unchanged)
        "legal_extract_contract", "legal_timeline", /* ... */
        // Tabular (unchanged)
        /* ... */
        // Utilities (unchanged)
        "vault_stats", "anno_health", "download_models", /* ... */
        // Deprecated — kept functional for back-compat
        "legacy_search", "legal_search", "legal_ingest",
        "knowledge_search", "knowledge_add_local_folder", "knowledge_sync",
        "knowledge_sources", "knowledge_status", "knowledge_forget",
    ]
    .into_iter().map(String::from).collect()
}
```

The order signals priority. Claude reads top-to-bottom when scanning the tool
list; the new tools appear first.

## 9. Test strategy

| Test | Crate | Model-gated | Covers |
|------|-------|:-----------:|--------|
| `index_general_routes_to_knowledge` | `anno-rag-mcp` | no | dispatch path |
| `index_legal_routes_to_legal_ingest` | `anno-rag-mcp` | yes | dispatch path (legal_ingest needs models) |
| `index_all_runs_both_pipelines` | `anno-rag-mcp` | yes | combo path |
| `index_unknown_profile_returns_error` | `anno-rag-mcp` | no | input validation |
| `search_fast_knowledge_returns_hits` | `anno-rag-mcp` | no | knowledge FTS path |
| `search_fast_all_returns_legal_warning` | `anno-rag-mcp` | no | graceful degradation |
| `search_semantic_legal_routes_to_legal_search` | `anno-rag-mcp` | yes | legal scope path |
| `search_concatenates_knowledge_and_legal_hits` | `anno-rag-mcp` | yes | scope="all" semantic |
| `search_filters_passed_to_legal_search` | `anno-rag-mcp` | yes | filter forwarding |
| `sources_aggregates_knowledge_and_legal_corpora` | `anno-rag-mcp` | no | union path |
| `status_returns_unified_health` | `anno-rag-mcp` | no | aggregation structure |
| `forget_uuid_routes_to_knowledge_forget` | `anno-rag-mcp` | no | target resolution: UUID |
| `forget_legal_prefix_routes_to_chunks_delete` | `anno-rag-mcp` | no | target resolution: legal: prefix |
| `forget_path_cascades_both_planes` | `anno-rag-mcp` | no | target resolution: path |
| `available_tools_lists_new_before_deprecated` | `anno-rag-mcp` | no | ordering signal |
| `available_tools_includes_legacy_search` | `anno-rag-mcp` | no | renamed legacy is listed |
| `deprecated_tool_descriptions_mention_replacement` | `anno-rag-mcp` | no | description quality |
| `deprecated_tools_still_respond_with_same_shape` | `anno-rag-mcp` | no | back-compat |

Plus full non-regression: every existing test in `anno-rag`, `anno-rag-mcp`,
`anno-knowledge-store`, `anno-knowledge-core`, `anno-source-local` must remain
green. Façade-only means *no observable behavior change* for any pre-existing
caller.

## 10. Acceptance criteria

- 5 new MCP tools (`index`, `search`, `sources`, `status`, `forget`) are registered, respond correctly, and route through impl helpers — no inline business logic in their handlers.
- 9 deprecated tools (`legacy_search`, `legal_search`, `legal_ingest`, `knowledge_search`, `knowledge_add_local_folder`, `knowledge_sync`, `knowledge_sources`, `knowledge_status`, `knowledge_forget`) respond exactly as today; only their registered descriptions change.
- The legacy `search` is renamed to `legacy_search`; the new `search` carries the unified routing.
- `available_tools` lists the 5 new tools first and includes all 9 deprecated entries.
- `search(scope="all", mode="fast")` returns knowledge FTS hits plus a warning describing the skipped legal scope.
- `search(scope="legal", mode="semantic", filters={"doc_type": "contract"})` returns the same results as a direct `legal_search` call with the same parameters.
- `forget("C:/docs")` (a folder path) removes both the matching knowledge source and the matching legal chunks in one call.
- `status()` aggregates knowledge counts, legal chunk count, vault stats, and model load state in a single JSON response without loading models.
- **No file in `crates/anno-rag/src/`, `crates/anno-knowledge-*/src/`, `crates/anno-source-local/src/`, or `crates/anno-rag-tabular/src/` is modified** by this phase. Only `crates/anno-rag-mcp/src/lib.rs` and `crates/anno-rag-mcp/src/health.rs` change.
- All pre-existing tests across the workspace remain green.
- Targeted Phase 2.5 tests pass.

## 11. Deferred

Everything not in this spec is in [`docs/product/roadmap.md`](../../product/roadmap.md). Specifically:

- Effective removal of deprecated tools (medium-term, after migration confirmed)
- Idempotence of `index(profile="legal")` (medium-term, contained refactor)
- Cross-scope RRF fusion in unified `search` (knowledge⊕legal). Phase 3 ships intra-knowledge RRF (FTS⊕vectors); fusing knowledge-side and legal-side scores requires score normalization that isn't worth the effort until both sides have settled.
- Internal pipeline unification — `Pipeline::ingest_folder` becomes a shim (long-term, 2-3 months)
- New profiles (`tabular`, source-specific) (case-by-case)
- Auto mode in `search` (long-term, after production telemetry)
