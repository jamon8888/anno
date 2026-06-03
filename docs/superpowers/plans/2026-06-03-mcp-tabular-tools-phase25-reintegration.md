# MCP Tabular Tools Phase 2.5 Reintegration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reintegrate the useful tabular MCP review work from `codex/mcp-tabular-tools` without regressing the newer Phase 2.5 unified MCP surface.

**Architecture:** Start from `codex/mcp-surface-phase25` or from `build-fork/main` after the Phase 2.5 PR is merged. Port tabular review improvements as additive changes only. Preserve the unified MCP tools (`index`, `search`, `sources`, `status`, `forget`), keep legacy knowledge tools and `knowledge.rs`, and append the `review_*` tools to the advertised MCP surface.

**Tech Stack:** Rust workspace, `rmcp`, `anno-rag-mcp`, `anno-rag-tabular`, existing GitHub Actions on the build fork `candy-hacienda/anno`. No new crates and no broad local workspace builds.

**Source plans/specs:**
- Phase 2.5 MCP surface: [`docs/superpowers/plans/2026-06-02-anno-mcp-surface-consolidation-phase2.5.md`](2026-06-02-anno-mcp-surface-consolidation-phase2.5.md)
- Tabular review v1.1: [`docs/superpowers/plans/2026-05-12-anno-rag-tabular-review-v1.1.md`](2026-05-12-anno-rag-tabular-review-v1.1.md)
- Local extraction quality note: [`docs/superpowers/plans/2026-05-27-anno-tabular-local-legal-extraction-quality.md`](2026-05-27-anno-tabular-local-legal-extraction-quality.md)

---

## Current State Snapshot

As of 2026-06-03:

| Branch | Commit | Role |
|--------|--------|------|
| `codex/mcp-surface-phase25` | `3e0d34d5` | Latest MCP Phase 2.5 surface; pushed to `build-fork/codex/mcp-surface-phase25`. |
| `codex/mcp-tabular-tools` | `ef9ea5ab` | Stale tabular review MCP branch checked out in `C:\Users\NMarchitecte\.config\superpowers\worktrees\anno\anno-rag-mcp-tabular-tools`. |
| `build-fork/main` | current tracking branch | Clean local `main` base. |

The branch `codex/mcp-tabular-tools` is not patch-equivalent merged into `main` or `build-fork/main`. It must not be merged directly because the diff from `codex/mcp-surface-phase25` deletes or overwrites current MCP files:

```text
D crates/anno-rag-mcp/src/knowledge.rs
D crates/anno-rag-mcp/src/indexer.rs
M crates/anno-rag-mcp/src/health.rs
M crates/anno-rag-mcp/src/lib.rs
```

Direct merge risk: HIGH. It can remove the Phase 2.5 knowledge service glue and regress the unified MCP tool contract.

---

## Reintegration Rules

These rules are non-negotiable during execution:

1. Do not run `git merge codex/mcp-tabular-tools`.
2. Do not delete `crates/anno-rag-mcp/src/knowledge.rs`.
3. Do not delete `crates/anno-rag-mcp/src/indexer.rs`.
4. Keep `all_tool_names()` ordered with Phase 2.5 unified tools first:

```text
index
search
sources
status
forget
```

5. Keep `legacy_search` after the new `search`.
6. Keep legacy knowledge tools functional:

```text
knowledge_sources
knowledge_status
knowledge_search
knowledge_add_local_folder
knowledge_sync
knowledge_forget
```

7. Add tabular review tools after the Phase 2.5 and legacy tool groups.
8. Preserve docs language that the installed MCP schema is the source of truth; `anno_health` is useful but not a complete schema dump.
9. Do not run broad local Rust builds. Use targeted checks only when explicitly allowed; otherwise validate through the fork CI.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/anno-rag-mcp/src/health.rs` | Modify | Preserve Phase 2.5 ordering and append `review_*` tool names. |
| `crates/anno-rag-mcp/tests/health.rs` | Modify | Assert health output includes the unified tools, legacy tools, and tabular review tools. |
| `crates/anno-rag-mcp/src/lib.rs` | Modify | Port `review_extract`, extraction status tracking, duplicate extraction guard, doc-id validation, and atomic review creation behavior. |
| `crates/anno-rag-tabular/src/storage/reviews.rs` | Modify only if needed | Port storage rollback helpers required by atomic review creation. |
| `crates/anno-rag-tabular/src/storage/columns.rs` | Modify only if needed | Port column failure rollback behavior required by atomic review creation. |
| `docs/developers/mcp-tools.md` | Modify | Merge the finalized `review_*` tool table into the Phase 2.5 MCP docs without losing unified surface guidance. |
| `docs/user-guide/tabular-review.md` | Modify | Add `review_extract` and `extraction_status` user workflow guidance. |
| `scripts/release/local-pipeline-gate.ps1` | Modify only if still applicable | Port strict-mode MCP smoke parser fix if it still applies on the new base. |

No other files should be modified.

---

## Task 0: Create the Clean Integration Branch

**Files:** none.

- [ ] **Step 1: Verify no local builds are running**

Run:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
```

Expected: no output. If cargo or rustc appears, stop and ask before killing anything.

- [ ] **Step 2: Verify the main worktree is clean**

Run:

```powershell
git status --short --branch
```

Expected:

```text
## main...build-fork/main
```

- [ ] **Step 3: Fetch the fork and local branch refs**

Run:

```powershell
git fetch build-fork main codex/mcp-surface-phase25
```

Expected: fetch completes without errors.

- [ ] **Step 4: Create an isolated worktree from the Phase 2.5 base**

Run this only if `codex/mcp-tabular-tools-on-phase25` does not already exist:

```powershell
git worktree add C:\Users\NMarchitecte\.config\superpowers\worktrees\anno\mcp-tabular-tools-on-phase25 -b codex/mcp-tabular-tools-on-phase25 codex/mcp-surface-phase25
```

Expected: new worktree created on `codex/mcp-tabular-tools-on-phase25`.

- [ ] **Step 5: Confirm the new branch has the Phase 2.5 MCP contract**

Run from the new worktree:

```powershell
Select-String -Path crates\anno-rag-mcp\src\health.rs -Pattern '"index"|"legacy_search"|"knowledge_sources"'
```

Expected: all three names are present.

- [ ] **Step 6: Run GitNexus impact checks before symbol edits**

Run from the new worktree:

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
npx gitnexus impact --repo anno all_tool_names --direction upstream
```

Expected: impact is understood and reported. If GitNexus reports HIGH or CRITICAL risk not already covered by this plan, stop and warn the user before editing.

---

## Task 1: Preserve Phase 2.5 Health Surface and Add Review Tool Advertisement

**Files:**
- Modify: `crates/anno-rag-mcp/src/health.rs`
- Modify: `crates/anno-rag-mcp/tests/health.rs`

- [ ] **Step 1: Add a failing unit test for review tool advertisement**

Add this test to `crates/anno-rag-mcp/src/health.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn all_tool_names_includes_tabular_review_tools_after_knowledge_tools() {
    let names = all_tool_names();

    for tool in [
        "review_create",
        "review_add_rows",
        "review_extract",
        "review_refine_cell",
        "review_set_cell",
        "review_lock_cell",
        "review_unlock_cell",
        "review_export",
        "review_get",
    ] {
        assert!(
            names.iter().any(|n| n == tool),
            "missing tabular review tool {tool}"
        );
    }

    let knowledge_forget_idx = names
        .iter()
        .position(|n| n == "knowledge_forget")
        .expect("knowledge_forget present");
    let first_review_idx = names
        .iter()
        .position(|n| n == "review_create")
        .expect("review_create present");

    assert!(
        first_review_idx > knowledge_forget_idx,
        "review tools should be appended after the Phase 2.5 knowledge tools"
    );
}
```

- [ ] **Step 2: Add a failing integration assertion for health output**

In `crates/anno-rag-mcp/tests/health.rs`, add this assertion to `anno_health_reports_engine_version_and_tool_set` after the existing tool assertions:

```rust
assert!(health
    .available_tools
    .contains(&"review_extract".to_string()));
assert!(health
    .available_tools
    .contains(&"knowledge_forget".to_string()));
assert!(health
    .available_tools
    .contains(&"legacy_search".to_string()));
```

- [ ] **Step 3: Implement `review_tool_names()` without changing the Phase 2.5 prefix**

In `crates/anno-rag-mcp/src/health.rs`, keep the current Phase 2.5 vector exactly as-is through `"knowledge_forget"`, then append review tools:

```rust
/// Hardcoded list of tools exposed by the MCP server.
pub fn all_tool_names() -> Vec<String> {
    let mut tools = vec![
        // Unified MCP surface (Phase 2.5)
        "index",
        "search",
        "sources",
        "status",
        "forget",
        // Legacy retrieval
        "legacy_search",
        "rehydrate",
        "detect",
        "vault_stats",
        // Memory (GDPR Art.17)
        "memory_save",
        "memory_recall",
        "memory_forget",
        "memory_list",
        "memory_graph_recall",
        "memory_invalidate",
        // Engine management
        "anno_health",
        "anno_init_vault",
        "download_models",
        // Legal D1 - ingest + search
        "legal_ingest",
        "legal_search",
        "legal_graph_query",
        "legal_rehydrate_citation",
        // Legal D2 - extraction
        "legal_extract_contract",
        "legal_extract_case_file",
        "legal_timeline",
        "legal_risk_review",
        // Legal D3-D5 - audit + validation
        "legal_mandatory_clause_audit",
        "legal_prescription_check",
        "legal_validate_field",
        // Knowledge (Phase 1 - SQLite FTS, no ML models)
        "knowledge_sources",
        "knowledge_status",
        "knowledge_search",
        // Knowledge (Phase 2 - local folder source)
        "knowledge_add_local_folder",
        "knowledge_sync",
        "knowledge_forget",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();

    tools.extend(review_tool_names());
    tools
}

/// MCP tabular-review tools exposed by `AnnoRagServer`.
pub fn review_tool_names() -> Vec<String> {
    vec![
        "review_create",
        "review_add_rows",
        "review_extract",
        "review_refine_cell",
        "review_set_cell",
        "review_lock_cell",
        "review_unlock_cell",
        "review_export",
        "review_get",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
```

- [ ] **Step 4: Verify no forbidden deletion is present**

Run:

```powershell
git diff --name-status codex/mcp-surface-phase25..HEAD | Select-String -Pattern "D\s+crates/anno-rag-mcp/src/(knowledge|indexer)\.rs"
```

Expected: no output.

- [ ] **Step 5: Commit Task 1**

Run:

```powershell
git add crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/tests/health.rs
git commit -m "feat(mcp): advertise tabular review tools on phase25 surface"
```

---

## Task 2: Port Review Extraction Status and `review_extract`

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Copy only tabular extraction concepts from the stale branch**

Port these symbols from `codex/mcp-tabular-tools:crates/anno-rag-mcp/src/lib.rs` into the Phase 2.5 version of `crates/anno-rag-mcp/src/lib.rs`:

```text
ReviewExtractParams
ReviewExtractionStatus
ReviewExtractionStatusWire
ReviewExtractResult
try_mark_review_extraction_running
AnnoRagServer::start_review_extraction
AnnoRagServer::review_extract
ReviewGetResult.extraction_status
AnnoRagServer.extraction_status
```

Do not copy the stale legacy search handler. Do not remove `knowledge(&self) -> KnowledgeService`. Do not remove `index`, `search`, `sources`, `status`, or `forget`.

- [ ] **Step 2: Add failing unit tests for status serialization and duplicate starts**

Port these tests from the stale branch into the Phase 2.5 `#[cfg(test)] mod tests` in `crates/anno-rag-mcp/src/lib.rs`:

```text
completed_extraction_status_converts_to_wire
blocked_extraction_status_carries_human_error
running_extraction_status_blocks_duplicate_start
review_get_result_serializes_extraction_status
```

Expected behavior:
- `completed` serializes as `completed`.
- blocked errors serialize with a human-readable message.
- duplicate extraction starts are rejected while a review is already `running`.
- `review_get` responses include `extraction_status` when a status exists.

- [ ] **Step 3: Wire `review_add_rows` to start extraction through the shared helper**

Modify `review_add_rows` so it returns these fields when rows are added:

```text
rows_added
failed_doc_ids
extraction_started
extraction_error
```

Use `start_review_extraction` for background extraction. Keep the Phase 2.5 server fields and unified MCP handlers unchanged.

- [ ] **Step 4: Add the `review_extract` MCP tool**

Add `review_extract` near the other `review_*` tool methods. Its parameters are:

```rust
pub struct ReviewExtractParams {
    /// Review UUID returned by review_create.
    pub review_id: String,
    /// Rerun unlocked cells even if extraction has already produced values.
    pub force_reextract: bool,
}
```

Expected tool behavior:
- Invalid `review_id` returns `Error: bad review_id: ...`.
- Missing tabular storage returns `Error: tabular store not configured`.
- Existing review starts extraction through `start_review_extraction`.
- Duplicate running extraction returns `extraction_started: false` with an error message.

- [ ] **Step 5: Verify Phase 2.5 tools still exist**

Run:

```powershell
Select-String -Path crates\anno-rag-mcp\src\lib.rs -Pattern "fn index\(|fn search\(|fn sources\(|fn status\(|fn forget\(|fn legacy_search\(|fn knowledge_sources"
```

Expected: all seven patterns are present.

- [ ] **Step 6: Commit Task 2**

Run:

```powershell
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): add tabular review extraction status"
```

---

## Task 3: Port Tabular Safety Fixes Without MCP Regression

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify only if required: `crates/anno-rag-tabular/src/storage/reviews.rs`
- Modify only if required: `crates/anno-rag-tabular/src/storage/columns.rs`

- [ ] **Step 1: Port document-id validation**

Port the behavior from commit `2baffab2` so `review_add_rows` separates invalid UUID strings into `failed_doc_ids` instead of failing the whole request after partial work.

Add or port this test:

```text
parse_review_doc_ids_separates_invalid_uuid_strings
```

Expected: valid UUIDs are accepted and invalid strings appear in `failed_doc_ids`.

- [ ] **Step 2: Port atomic review creation**

Port the behavior from commits `56ef1a7f` and `306f965d` so `review_create` does not leave orphan reviews when template or column creation fails.

Add or port this test:

```text
review_create_rejects_bad_template_without_orphan_review
```

Expected: a bad template returns an error and no review remains in storage.

- [ ] **Step 3: Keep Phase 2.5 MCP handlers intact**

Run:

```powershell
git diff -- crates/anno-rag-mcp/src/lib.rs | Select-String -Pattern "knowledge\\(|knowledge_sources_impl|forget_impl_routing|legacy_search_impl"
```

Expected: output may show nearby context, but none of these symbols should be deleted.

- [ ] **Step 4: Commit Task 3**

Run:

```powershell
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-tabular/src/storage/reviews.rs crates/anno-rag-tabular/src/storage/columns.rs
git commit -m "fix(mcp): harden tabular review storage operations"
```

---

## Task 4: Reconcile MCP and User Documentation

**Files:**
- Modify: `docs/developers/mcp-tools.md`
- Modify: `docs/user-guide/tabular-review.md`
- Modify only if still applicable: `scripts/release/local-pipeline-gate.ps1`

- [ ] **Step 1: Merge the finalized `review_*` table into developer docs**

In `docs/developers/mcp-tools.md`, preserve the Phase 2.5 explanation of the unified tools and add this review table:

```markdown
## Review Tools

| Tool | Purpose | Response highlights |
|------|---------|---------------------|
| `review_create` | Create a tabular review and optionally materialize columns from a built-in template. | Returns `review_id`, review name, and `columns_loaded`. |
| `review_add_rows` | Add ingested document UUIDs as review rows. | Returns `rows_added`, `failed_doc_ids`, `extraction_started`, and `extraction_error`; starts extraction when rows were added. |
| `review_extract` | Start extraction for an existing review. | Returns row/column counts plus `extraction_started`; use `force_reextract=true` to rerun unlocked cells. |
| `review_get` | Read review state. | Returns columns, rows, latest cells, and `extraction_status` for polling background extraction. |
| `review_refine_cell` | Re-extract one cell with an extra instruction. | Writes a new cell version; locked cells are rejected until unlocked. |
| `review_set_cell` | Write a human override value to one cell. | Records a human-authored version and can lock it with `lock=true`. |
| `review_lock_cell` | Lock the latest cell value. | Prevents automatic extraction from overwriting the cell. |
| `review_unlock_cell` | Unlock a cell. | Allows future extraction or refinement to overwrite the cell. |
| `review_export` | Export the review as `csv`, `markdown`, or `xlsx`. | CSV/Markdown are returned in the tool response; XLSX requires an absolute `output_path`. |
```

- [ ] **Step 2: Add the polling workflow to the user guide**

In `docs/user-guide/tabular-review.md`, ensure the MCP workflow includes:

```markdown
1. Create a review with `review_create`.
2. Add ingested document UUIDs with `review_add_rows`.
3. Call `review_extract` when `review_add_rows.extraction_started` is `false`, or when a rerun is needed.
4. Poll `review_get` and inspect `extraction_status.state` until it is `completed`, `completed_with_errors`, or `blocked`.
5. Correct cells with `review_refine_cell` for targeted re-extraction, or `review_set_cell` for a human override.
6. Lock verified cells with `review_lock_cell`; unlock them with `review_unlock_cell` before changing them again.
7. Export with `review_export`.
```

- [ ] **Step 3: Port the strict-mode smoke parser fix only if still needed**

Compare the current file against the stale branch:

```powershell
git diff codex/mcp-surface-phase25..codex/mcp-tabular-tools -- scripts/release/local-pipeline-gate.ps1
```

If the strict-mode parser bug is still present in the new base, port the fix from commit `ef9ea5ab`. If the current parser is already strict-mode safe, leave the script untouched.

- [ ] **Step 4: Commit Task 4**

Run:

```powershell
git add docs/developers/mcp-tools.md docs/user-guide/tabular-review.md scripts/release/local-pipeline-gate.ps1
git commit -m "docs(mcp): document tabular review extraction tools"
```

---

## Task 5: Final Verification and Fork CI

**Files:** verification only.

- [ ] **Step 1: Detect unexpected deletions**

Run:

```powershell
git diff --name-status codex/mcp-surface-phase25..HEAD
```

Expected:
- no deletion of `crates/anno-rag-mcp/src/knowledge.rs`
- no deletion of `crates/anno-rag-mcp/src/indexer.rs`
- changed files limited to the file map above

- [ ] **Step 2: Verify Phase 2.5 and review tools are both advertised**

Run:

```powershell
Select-String -Path crates\anno-rag-mcp\src\health.rs -Pattern '"index"|"search"|"legacy_search"|"knowledge_forget"|"review_extract"'
```

Expected: all five names are present.

- [ ] **Step 3: Run GitNexus change detection before publishing**

Run:

```powershell
npx gitnexus detect_changes --repo anno --scope compare --base_ref codex/mcp-surface-phase25
```

Expected: changed symbols match this plan: health tool names, tabular review MCP handlers, tabular storage safety helpers, and docs.

- [ ] **Step 4: Optional targeted local check only if local builds are allowed**

Run only after confirming no cargo/rustc process is running and the user allows local Rust checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```

Expected: check completes without compiler errors.

- [ ] **Step 5: Push to the build fork**

Run:

```powershell
git push -u build-fork codex/mcp-tabular-tools-on-phase25
```

Expected: branch is pushed to `https://github.com/candy-hacienda/anno`.

- [ ] **Step 6: Open a fork PR**

If `candy-hacienda/anno#1` (`codex/mcp-surface-phase25` -> `main`) is still
open, create a stacked PR against `codex/mcp-surface-phase25` so the diff only
shows tabular reintegration. If Phase 2.5 has already merged into `main`, use
`--base main` instead.

Run:

```powershell
gh pr create --repo candy-hacienda/anno --base codex/mcp-surface-phase25 --head codex/mcp-tabular-tools-on-phase25 --title "feat(mcp): add tabular review extraction tools on phase 2.5 surface" --body-file C:\Users\NMarchitecte\.config\superpowers\worktrees\anno\mcp-tabular-tools-on-phase25\docs\superpowers\plans\2026-06-03-mcp-tabular-tools-phase25-reintegration.md --draft
```

Expected: draft PR is created on the fork.

- [ ] **Step 7: Watch CI without starting local builds**

Run:

```powershell
gh run list --repo candy-hacienda/anno --branch codex/mcp-tabular-tools-on-phase25 --limit 5
```

Expected: new workflow run appears. Investigate only CI failures caused by this branch; ignore runner shutdown failures unless they repeat.

---

## Acceptance Criteria

- `codex/mcp-tabular-tools` is not merged directly.
- The integration branch is based on `codex/mcp-surface-phase25` or on `build-fork/main` after Phase 2.5 lands.
- `all_tool_names()` keeps Phase 2.5 unified tools first.
- `legacy_search` and all legacy knowledge tools remain present.
- `review_extract` is advertised and implemented.
- `review_get` exposes `extraction_status`.
- Duplicate review extraction starts are blocked.
- Invalid tabular document IDs are returned in `failed_doc_ids`.
- Bad review template/column failures do not leave orphan reviews.
- `knowledge.rs` and `indexer.rs` are not deleted.
- Docs describe the final `review_*` flow without contradicting the Phase 2.5 MCP surface.
- Full validation happens through the fork CI unless the user explicitly allows targeted local Rust checks.

---

## Deferred Work

The Tabular Review v1.1 plan also mentions MCP resources and a grid UI. This reintegration plan does not implement them:

```text
review://{id}
review://{id}/cell/{row}/{col}
review://{id}/source/{doc}#span=...
anno-rag-tabular-ui
```

Those should remain deferred until the MCP tool surface is stable on top of Phase 2.5.
