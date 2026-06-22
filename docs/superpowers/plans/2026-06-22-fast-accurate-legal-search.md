# Fast & Accurate Legal Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make legal ingestion non-blocking (detached async jobs) and legal search accurate-by-default (cross-encoder reranked hybrid), while fixing the FTS-index-never-built, legal-ingest-timeout, and corpus_list-count-only defects.

**Architecture:** Reuse the dormant `index_jobs` SQLite table (in `anno-knowledge-store`) for job tracking. `legal_ingest` spawns a detached `tokio` task holding `Arc<Pipeline>` and returns a `job_id` immediately; a new `job_status` MCP tool polls progress. Legal semantic search routes through a new `Pipeline::legal_search_reranked` that over-fetches a hybrid pool (vector + BM25 FTS + RRF) and reorders it with the existing `bge-reranker-v2-m3` cross-encoder. Knowledge sync now builds the LanceDB FTS index, and `Store::search` lazily builds it on a missing-index error.

**Tech Stack:** Rust, rusqlite (SQLite + FTS5), LanceDB (`lancedb`), tokio, rmcp (`#[tool]` macros), ONNX cross-encoder reranker (feature `rerank`).

**Spec:** `docs/superpowers/specs/2026-06-22-fast-accurate-legal-search-design.md`

**Build/test commands (this repo, Windows):**
- Check one crate: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check`
- Test one crate: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate>`
- Lint before commit: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\lint-check.ps1`
- **Never** run `cargo test --workspace` locally (links all test binaries — hours).
- Before any build: `Get-Process cargo,rustc -ErrorAction SilentlyContinue` (concurrency guard).

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `crates/anno-knowledge-store/src/migrations.rs` | Add `index_jobs` progress columns (schema v2) | Modify |
| `crates/anno-knowledge-store/src/jobs.rs` | `JobRow` type + job CRUD on `KnowledgeControlStore` | Create |
| `crates/anno-knowledge-store/src/control_store.rs` | `impl` block for job methods (or `include` jobs mod) | Modify |
| `crates/anno-knowledge-store/src/lib.rs` | Re-export `JobRow`, `JobStatus` | Modify |
| `crates/anno-rag-mcp/src/knowledge.rs` | Delegating job methods on `KnowledgeService` | Modify |
| `crates/anno-rag-mcp/src/indexer.rs` | Build FTS index after non-truncated knowledge sync | Modify |
| `crates/anno-rag/src/store.rs` | Lazy FTS-build fallback in `Store::search` | Modify |
| `crates/anno-rag/src/pipeline.rs` | `legal_search_reranked` (behind `rerank` feature) | Modify |
| `crates/anno-rag-mcp/src/lib.rs` | Job spawn, `job_status` tool, startup sweep, `corpus_list` fix, rerank wiring, `active_ingest_jobs` field | Modify |
| `crates/anno-rag-mcp/src/wire.rs` | `JobStatusParams`, `JobStatusResult`, `rerank` on `LegalSearchParams` | Modify |
| `crates/anno-rag-mcp/src/search.rs` | `index_building` status surfacing | Modify |

---

## Task 1: `index_jobs` schema v2 — progress columns

**Files:**
- Modify: `crates/anno-knowledge-store/src/migrations.rs`
- Test: same file (`#[cfg(test)] mod tests`)

The existing v1 `index_jobs` table has `job_id, object_id, job_type, status, attempts, not_before, last_error`. We add `corpus_id, files_done, files_total, created_at, updated_at` via a v2 migration.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `migrations.rs`:

```rust
#[test]
fn migrate_v2_adds_job_progress_columns() {
    let conn = Connection::open_in_memory().expect("open memory db");
    run_migrations(&conn).expect("migrate");

    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(index_jobs)")
        .expect("prepare")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect");

    for expected in ["corpus_id", "files_done", "files_total", "created_at", "updated_at"] {
        assert!(cols.contains(&expected.to_string()), "missing column {expected}");
    }

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");
    assert_eq!(version, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store`
Expected: FAIL — `missing column corpus_id` (and `user_version` is 1, not 2).

- [ ] **Step 3: Implement the v2 migration**

In `migrations.rs`, change the version constant and add the migration:

```rust
const SCHEMA_VERSION: i64 = 2;
```

In `run_migrations`, after the `version < 1` block, add:

```rust
    if version < 2 {
        migrate_v2(conn)?;
        conn.pragma_update(None, "user_version", 2_i64)?;
    }
```

Add the function below `migrate_v1`:

```rust
/// v2: add progress + corpus tracking columns to `index_jobs`.
///
/// SQLite `ALTER TABLE ADD COLUMN` is append-only and cannot be re-run, so
/// each add is guarded by a probe of `PRAGMA table_info`.
fn migrate_v2(conn: &Connection) -> Result<()> {
    let existing: Vec<String> = conn
        .prepare("PRAGMA table_info(index_jobs)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let adds = [
        ("corpus_id", "ALTER TABLE index_jobs ADD COLUMN corpus_id TEXT"),
        ("files_done", "ALTER TABLE index_jobs ADD COLUMN files_done INTEGER NOT NULL DEFAULT 0"),
        ("files_total", "ALTER TABLE index_jobs ADD COLUMN files_total INTEGER NOT NULL DEFAULT 0"),
        ("created_at", "ALTER TABLE index_jobs ADD COLUMN created_at TEXT"),
        ("updated_at", "ALTER TABLE index_jobs ADD COLUMN updated_at TEXT"),
    ];
    for (col, sql) in adds {
        if !existing.iter().any(|c| c == col) {
            conn.execute(sql, [])?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store`
Expected: PASS (both `migrate_v2_adds_job_progress_columns` and the existing `migrations_are_idempotent`).

- [ ] **Step 5: Commit**

```bash
git add crates/anno-knowledge-store/src/migrations.rs
git commit -m "feat(knowledge-store): index_jobs schema v2 with progress columns"
```

---

## Task 2: `JobRow` type + job CRUD on `KnowledgeControlStore`

**Files:**
- Create: `crates/anno-knowledge-store/src/jobs.rs`
- Modify: `crates/anno-knowledge-store/src/control_store.rs` (add `mod` include path) and `crates/anno-knowledge-store/src/lib.rs` (re-export)
- Test: `crates/anno-knowledge-store/src/jobs.rs` (`#[cfg(test)] mod tests`)

Job IDs are generated by the caller (the MCP layer has `uuid`); the store takes `job_id: &str`.

- [ ] **Step 1: Write the failing test**

Create `crates/anno-knowledge-store/src/jobs.rs` with:

```rust
//! Async ingestion job tracking on the `index_jobs` table.

use crate::control_store::KnowledgeControlStore;
use crate::Result;
use chrono::Utc;
use rusqlite::params;

/// Lifecycle state of an ingestion job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// In-flight.
    Running,
    /// Completed successfully (may have per-file failures recorded).
    Done,
    /// Process died mid-job; needs a re-run.
    Interrupted,
    /// Zero files succeeded.
    Failed,
}

impl JobStatus {
    /// Serialized form stored in SQLite.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }
}

/// A row from `index_jobs`.
#[derive(Debug, Clone)]
pub struct JobRow {
    /// Job id (caller-generated UUID string).
    pub job_id: String,
    /// Job family, e.g. `legal_ingest`.
    pub job_type: String,
    /// Owning corpus id (string form), if any.
    pub corpus_id: Option<String>,
    /// Status string (`running`/`done`/`interrupted`/`failed`).
    pub status: String,
    /// Files processed so far.
    pub files_done: i64,
    /// Total files discovered for the job.
    pub files_total: i64,
    /// Last non-fatal error, if any.
    pub last_error: Option<String>,
}

impl KnowledgeControlStore {
    /// Insert a new `running` job row.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn insert_job(
        &self,
        job_id: &str,
        job_type: &str,
        corpus_id: Option<&str>,
        files_total: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "INSERT INTO index_jobs \
             (job_id, job_type, corpus_id, status, attempts, files_done, files_total, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'running', 0, 0, ?4, ?5, ?5)",
            params![job_id, job_type, corpus_id, files_total, now],
        )?;
        Ok(())
    }

    /// Return the `job_id` of a `running` job for `corpus_id`, if one exists.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn running_job_for_corpus(&self, corpus_id: &str) -> Result<Option<String>> {
        let conn = self.conn_lock();
        let mut stmt = conn.prepare(
            "SELECT job_id FROM index_jobs \
             WHERE corpus_id = ?1 AND status = 'running' LIMIT 1",
        )?;
        let mut rows = stmt.query(params![corpus_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get::<_, String>(0)?)),
            None => Ok(None),
        }
    }

    /// Update `files_done` for an in-flight job.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn update_job_progress(&self, job_id: &str, files_done: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "UPDATE index_jobs SET files_done = ?2, updated_at = ?3 WHERE job_id = ?1",
            params![job_id, files_done, now],
        )?;
        Ok(())
    }

    /// Set terminal (or any) status, optionally recording `last_error`.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn set_job_status(
        &self,
        job_id: &str,
        status: JobStatus,
        last_error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        conn.execute(
            "UPDATE index_jobs SET status = ?2, last_error = ?3, updated_at = ?4 WHERE job_id = ?1",
            params![job_id, status.as_str(), last_error, now],
        )?;
        Ok(())
    }

    /// Fetch a single job row.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn get_job(&self, job_id: &str) -> Result<Option<JobRow>> {
        let conn = self.conn_lock();
        let mut stmt = conn.prepare(
            "SELECT job_id, job_type, corpus_id, status, files_done, files_total, last_error \
             FROM index_jobs WHERE job_id = ?1",
        )?;
        let mut rows = stmt.query(params![job_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(JobRow {
                job_id: row.get(0)?,
                job_type: row.get(1)?,
                corpus_id: row.get(2)?,
                status: row.get(3)?,
                files_done: row.get(4)?,
                files_total: row.get(5)?,
                last_error: row.get(6)?,
            })),
            None => Ok(None),
        }
    }

    /// Mark every `running` job `interrupted`. Run once at startup to recover
    /// from a process that died mid-ingest. Returns the number of rows changed.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn mark_running_jobs_interrupted(&self) -> Result<usize> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn_lock();
        let changed = conn.execute(
            "UPDATE index_jobs SET status = 'interrupted', updated_at = ?1 WHERE status = 'running'",
            params![now],
        )?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> KnowledgeControlStore {
        let dir = tempdir().expect("temp dir");
        KnowledgeControlStore::open(dir.path().join("k.sqlite3")).expect("open")
    }

    #[test]
    fn job_lifecycle_running_to_done() {
        let s = store();
        s.insert_job("job-1", "legal_ingest", Some("corp-1"), 3).expect("insert");
        assert_eq!(s.running_job_for_corpus("corp-1").expect("q"), Some("job-1".into()));

        s.update_job_progress("job-1", 2).expect("progress");
        s.set_job_status("job-1", JobStatus::Done, None).expect("done");

        let row = s.get_job("job-1").expect("get").expect("some");
        assert_eq!(row.status, "done");
        assert_eq!(row.files_done, 2);
        assert_eq!(row.files_total, 3);
        assert_eq!(s.running_job_for_corpus("corp-1").expect("q"), None);
    }

    #[test]
    fn interrupted_sweep_marks_running() {
        let s = store();
        s.insert_job("job-2", "legal_ingest", Some("corp-2"), 1).expect("insert");
        let changed = s.mark_running_jobs_interrupted().expect("sweep");
        assert_eq!(changed, 1);
        assert_eq!(s.get_job("job-2").expect("get").expect("some").status, "interrupted");
    }
}
```

- [ ] **Step 2: Add the `conn_lock` helper + module wiring**

In `control_store.rs`, the struct holds `conn: Mutex<Connection>`. Add a small private accessor used by `jobs.rs` (place inside the existing `impl KnowledgeControlStore`):

```rust
    /// Lock the connection mutex. Shared by the jobs module.
    pub(crate) fn conn_lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("knowledge sqlite mutex poisoned")
    }
```

In `lib.rs`, register the module and re-export:

```rust
pub mod jobs;
```

```rust
pub use jobs::{JobRow, JobStatus};
```

- [ ] **Step 3: Run test to verify it fails then passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store`
Expected: PASS for `job_lifecycle_running_to_done` and `interrupted_sweep_marks_running`.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-knowledge-store/src/jobs.rs crates/anno-knowledge-store/src/control_store.rs crates/anno-knowledge-store/src/lib.rs
git commit -m "feat(knowledge-store): JobRow + index_jobs CRUD"
```

---

## Task 3: Delegating job methods on `KnowledgeService`

**Files:**
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`
- Test: `crates/anno-rag-mcp/src/knowledge.rs` (`#[cfg(test)] mod tests`, add if absent)

`KnowledgeService` wraps the store with thin delegators (see `status`, `sources`). Add the same for jobs so the MCP service never touches the raw store.

- [ ] **Step 1: Write the failing test**

Add to `knowledge.rs` (create `#[cfg(test)] mod tests` if it does not exist):

```rust
#[cfg(test)]
mod job_delegation_tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    #[test]
    fn service_tracks_job_lifecycle() {
        let dir = tempfile::tempdir().expect("dir");
        let mut cfg = AnnoRagConfig::default();
        cfg.data_dir = Some(dir.path().to_path_buf());
        let svc = KnowledgeService::open(&cfg).expect("open");

        svc.insert_job("j1", "legal_ingest", Some("c1"), 5).expect("insert");
        svc.update_job_progress("j1", 3).expect("progress");
        svc.set_job_status("j1", anno_knowledge_store::JobStatus::Done, None).expect("done");

        let row = svc.get_job("j1").expect("get").expect("some");
        assert_eq!(row.files_done, 3);
        assert_eq!(row.status, "done");
    }
}
```

> Note: confirm `AnnoRagConfig` exposes `data_dir: Option<PathBuf>` and that `knowledge_db_path(cfg)` derives from it. If the field name differs, set the data dir via the same mechanism `KnowledgeService::open` reads (check `knowledge_db_path` at the top of this file).

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check`
Expected: FAIL — `no method named insert_job found for struct KnowledgeService`.

- [ ] **Step 3: Add the delegating methods**

In `impl KnowledgeService`, add:

```rust
    /// Insert a new running ingestion job.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn insert_job(
        &self,
        job_id: &str,
        job_type: &str,
        corpus_id: Option<&str>,
        files_total: i64,
    ) -> anno_knowledge_store::Result<()> {
        self.store.insert_job(job_id, job_type, corpus_id, files_total)
    }

    /// Return the running job id for a corpus, if any.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn running_job_for_corpus(&self, corpus_id: &str) -> anno_knowledge_store::Result<Option<String>> {
        self.store.running_job_for_corpus(corpus_id)
    }

    /// Update job progress.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn update_job_progress(&self, job_id: &str, files_done: i64) -> anno_knowledge_store::Result<()> {
        self.store.update_job_progress(job_id, files_done)
    }

    /// Set job status.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn set_job_status(
        &self,
        job_id: &str,
        status: anno_knowledge_store::JobStatus,
        last_error: Option<&str>,
    ) -> anno_knowledge_store::Result<()> {
        self.store.set_job_status(job_id, status, last_error)
    }

    /// Fetch a job row.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn get_job(&self, job_id: &str) -> anno_knowledge_store::Result<Option<anno_knowledge_store::JobRow>> {
        self.store.get_job(job_id)
    }

    /// Mark stale running jobs interrupted (startup recovery).
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn mark_running_jobs_interrupted(&self) -> anno_knowledge_store::Result<usize> {
        self.store.mark_running_jobs_interrupted()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS for `service_tracks_job_lifecycle`.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/knowledge.rs
git commit -m "feat(mcp): KnowledgeService job delegation methods"
```

---

## Task 4: Build knowledge FTS index after non-truncated sync

**Files:**
- Modify: `crates/anno-rag-mcp/src/indexer.rs` (end of `sync_local_scope`, around line 190)
- Test: covered by the integration step in Task 11; add a focused unit assertion here.

Root cause: `maybe_build_fts_index()` is only called from the legal pipeline's `optimize_after_ingest()`. Knowledge sync writes LanceDB rows but never builds the inverted index.

> **Important:** confirm which store backs knowledge fast search. The SQLite FTS5 table (`knowledge_objects_fts`) is auto-populated and needs no build. If knowledge fast search reads **only** SQLite (see `KnowledgeControlStore::search_fast`), then the LanceDB FTS build is **not** required for knowledge and this task is a no-op — in that case, verify with Task 11's integration test FIRST and skip this task if SQLite already returns hits. The build is definitely required for the **legal** LanceDB store (Task 5 path). Treat this task as conditional on the Task 11 result.

- [ ] **Step 1: Add a diagnostic test for the sync→search path**

This is validated end-to-end in Task 11. Here, add a `tracing` breadcrumb so truncated vs complete is observable. In `sync_local_scope`, just before `Ok(summary)`:

```rust
    if !summary.truncated {
        tracing::info!(
            fts_ready = summary.fts_ready,
            "knowledge sync complete (SQLite FTS auto-populated)"
        );
    }
```

- [ ] **Step 2: Verify the SQLite FTS path returns hits**

Run the Task 11 integration test first. If knowledge `search(mode=fast)` returns hits after sync, this task is complete (no LanceDB FTS build needed for knowledge). If it still returns empty, proceed to Step 3.

- [ ] **Step 3 (only if SQLite path insufficient): build LanceDB FTS after sync**

If knowledge fast search routes through the LanceDB `Store`, obtain the store handle in `sync_local_scope` (it already has `pipeline`/`store` access via the caller) and call:

```rust
    if !summary.truncated {
        match store.maybe_build_fts_index().await {
            Ok(true) => tracing::info!("built knowledge FTS index"),
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "knowledge FTS build skipped"),
        }
    }
```

- [ ] **Step 4: Commit**

```bash
git add crates/anno-rag-mcp/src/indexer.rs
git commit -m "fix(mcp): ensure FTS index after knowledge sync"
```

---

## Task 5: Lazy FTS-build fallback in `Store::search`

**Files:**
- Modify: `crates/anno-rag/src/store.rs` (`Store::search`, lines 1191-1228)
- Test: `crates/anno-rag/src/store.rs` (`#[cfg(test)] mod tests`)

Make the legal hybrid search self-healing: if the inverted index is missing (ingest was interrupted before `optimize_after_ingest`), build it inline and retry once instead of crashing.

- [ ] **Step 1: Write the failing test**

Add to the store tests:

```rust
#[tokio::test]
async fn search_builds_fts_index_when_missing() {
    // Build a store, insert a few rows WITHOUT calling maybe_build_fts_index,
    // then call search() and assert it returns Ok (not the Lance
    // "INVERTED index" error) and that an index now exists.
    let store = test_store_with_rows(&[
        ("le contrat de résiliation", vec![0.1; 1024]),
        ("clause de confidentialité", vec![0.2; 1024]),
    ])
    .await;

    let qv = vec![0.1f32; 1024];
    let hits = store.search("résiliation", &qv, 5).await.expect("search ok");
    assert!(!hits.is_empty());

    let indices = store.tbl.list_indices().await.expect("indices");
    assert!(indices.iter().any(|i| i.columns.iter().any(|c| c == "text_pseudo")));
}
```

> Use the existing test helper for building a store with rows if present; otherwise add `test_store_with_rows` mirroring existing store-test setup in this file.

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: FAIL — search returns `Err(Lance(... "Cannot perform full text search unless an INVERTED index ..."))`.

- [ ] **Step 3: Implement the fallback**

Refactor `Store::search` to extract the query body into a private `try_search` and wrap it:

```rust
    pub async fn search(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        match self.try_search(query_text, query_vec, k).await {
            Ok(hits) => Ok(hits),
            Err(e) if is_missing_fts_index_error(&e) => {
                tracing::warn!("FTS index missing at search time; building inline and retrying");
                self.maybe_build_fts_index().await?;
                self.try_search(query_text, query_vec, k).await
            }
            Err(e) => Err(e),
        }
    }

    async fn try_search(
        &self,
        query_text: &str,
        query_vec: &[f32],
        k: usize,
    ) -> Result<Vec<SearchHit>> {
        // ... existing body of search() verbatim ...
    }
```

Add the error classifier near the bottom of the file:

```rust
/// True when a LanceDB error is the "no inverted index" failure, which we can
/// recover from by building the FTS index and retrying once.
fn is_missing_fts_index_error(e: &Error) -> bool {
    e.to_string().contains("INVERTED index")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag`
Expected: PASS for `search_builds_fts_index_when_missing`.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/store.rs
git commit -m "fix(store): lazily build FTS index on missing-index search error"
```

---

## Task 6: `Pipeline::legal_search_reranked`

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs` (near `legal_search`, line 2068, and `search_reranked`, line 779)
- Test: `crates/anno-rag/src/pipeline.rs` (`#[cfg(all(test, feature = "rerank"))]`)

Combine the legal hybrid pool with the existing cross-encoder. Mirror `search_reranked`'s privacy-preserving rehydration (FTS/embed see pseudonyms; only the rerank stage sees plaintext).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(feature = "rerank")]
#[tokio::test]
async fn legal_search_reranked_reorders_pool() {
    // Ingest two legal chunks where the lexical-but-irrelevant one ranks
    // higher under RRF and the semantically-relevant one ranks higher under
    // the cross-encoder. Assert the cross-encoder winner is first.
    let pipeline = test_pipeline_with_legal_chunks(&[
        ("La clause de non-concurrence engage le salarié pour douze mois.", "doc-a"),
        ("Le présent contrat mentionne le mot clause à plusieurs reprises clause clause.", "doc-b"),
    ])
    .await;

    let filters = anno_rag::legal::types::LegalSearchFilters::default();
    let hits = pipeline
        .legal_search_reranked("durée de la clause de non-concurrence", 2, filters, 30)
        .await
        .expect("rerank ok");

    assert_eq!(hits.first().expect("hit").doc_id.to_string(), "doc-a-uuid");
}
```

> Use the existing legal-pipeline test harness in this file; adapt doc-id assertions to the harness's id scheme.

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -Features rerank`
Expected: FAIL — `no method named legal_search_reranked`.

- [ ] **Step 3: Implement `legal_search_reranked`**

Add after `legal_search_scoped` (around line 2150):

```rust
    /// Legal hybrid search followed by cross-encoder reranking.
    ///
    /// 1. Hybrid-retrieve an over-fetch pool of `pool_size` (vector + BM25 FTS
    ///    + RRF) via [`Self::legal_search`].
    /// 2. Rehydrate each candidate's `text_pseudo` to plaintext — the
    ///    cross-encoder must score real entities, not `<PERSON_42>`.
    /// 3. Score `(plaintext_query, rehydrated_text)` pairs, reorder desc,
    ///    truncate to `top_k`.
    ///
    /// Privacy invariant: embed + FTS run on the pseudonymized query; plaintext
    /// is used only for the rerank stage, on already-retrieved candidates.
    ///
    /// # Errors
    /// [`Error::Detect`] / [`Error::Vault`] / [`Error::Embed`] /
    /// [`Error::Store`] / [`Error::Legal`] / [`Error::Rerank`] per failing layer.
    #[cfg(feature = "rerank")]
    pub async fn legal_search_reranked(
        &self,
        query: &str,
        top_k: usize,
        filters: crate::legal::types::LegalSearchFilters,
        pool_size: usize,
    ) -> Result<Vec<crate::legal::types::LegalSearchHit>> {
        let pool = pool_size.max(top_k).max(1);
        let mut hits = self.legal_search(query, pool, filters).await?;
        if hits.is_empty() {
            return Ok(hits);
        }

        let mut passages: Vec<String> = Vec::with_capacity(hits.len());
        for h in &hits {
            let r = self.rehydrate(&h.text_pseudo).await?;
            passages.push(r.text);
        }
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();

        let reranker = self.reranker().await?;
        let scores = reranker.score_pairs_batched(query, &refs, self.cfg.rerank_batch_size)?;

        let mut order: Vec<usize> = (0..hits.len()).collect();
        order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));

        let mut reranked: Vec<crate::legal::types::LegalSearchHit> = order
            .into_iter()
            .map(|i| {
                let mut h = hits[i].clone();
                h.score = scores[i];
                h
            })
            .collect();
        reranked.truncate(top_k);
        hits.clear();
        Ok(reranked)
    }

    /// Scoped variant: rerank within an explicit document set.
    ///
    /// # Errors
    /// As [`Self::legal_search_reranked`].
    #[cfg(feature = "rerank")]
    pub async fn legal_search_scoped_reranked(
        &self,
        query: &str,
        top_k: usize,
        filters: crate::legal::types::LegalSearchFilters,
        allowed_doc_ids: &[uuid::Uuid],
        pool_size: usize,
    ) -> Result<Vec<crate::legal::types::LegalSearchHit>> {
        let pool = pool_size.max(top_k).max(1);
        let hits = self
            .legal_search_scoped(query, pool, filters, allowed_doc_ids)
            .await?;
        if hits.is_empty() {
            return Ok(hits);
        }
        let mut passages: Vec<String> = Vec::with_capacity(hits.len());
        for h in &hits {
            passages.push(self.rehydrate(&h.text_pseudo).await?.text);
        }
        let refs: Vec<&str> = passages.iter().map(String::as_str).collect();
        let reranker = self.reranker().await?;
        let scores = reranker.score_pairs_batched(query, &refs, self.cfg.rerank_batch_size)?;
        let mut order: Vec<usize> = (0..hits.len()).collect();
        order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));
        let mut reranked: Vec<crate::legal::types::LegalSearchHit> = order
            .into_iter()
            .map(|i| {
                let mut h = hits[i].clone();
                h.score = scores[i];
                h
            })
            .collect();
        reranked.truncate(top_k);
        Ok(reranked)
    }
```

> Verify `LegalSearchHit` derives `Clone` (it is constructed by value in `legal_search`; add `#[derive(Clone)]` to it in `legal/types.rs` if missing). Verify `self.reranker()` and `score_pairs_batched` signatures match `search_reranked` (lines 798-799).

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag -Features rerank`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/legal/types.rs
git commit -m "feat(pipeline): legal_search_reranked cross-encoder path"
```

---

## Task 7: Wire rerank into MCP legal search (default ON, `rerank=false` escape hatch)

**Files:**
- Modify: `crates/anno-rag-mcp/src/wire.rs` (`LegalSearchParams`)
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`legal_search_impl_with_effective`, lines 772-779)
- Test: `crates/anno-rag-mcp/src/lib.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Add the `rerank` field to `LegalSearchParams`**

In `wire.rs`, add to the `LegalSearchParams` struct:

```rust
    /// Rerank legal results with the cross-encoder. Default `true` (accuracy-first).
    /// Set `false` for RRF-only hybrid (faster).
    #[serde(default = "default_legal_rerank")]
    pub rerank: bool,
```

Add the default function in `wire.rs`:

```rust
fn default_legal_rerank() -> bool {
    true
}
```

> Update any struct-literal construction of `LegalSearchParams` in tests (e.g. lib.rs:4446 sets `rerank: false`) to keep compiling — the field already named `rerank` there is for the deprecated `search` path; confirm there is no name clash. If `LegalSearchParams` is also built positionally elsewhere, add `rerank: true`.

- [ ] **Step 2: Write the failing test**

```rust
#[test]
fn legal_search_params_default_rerank_is_true() {
    let p: LegalSearchParams =
        serde_json::from_value(serde_json::json!({ "query": "clause" })).expect("parse");
    assert!(p.rerank, "legal rerank must default ON");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check`
Expected: FAIL — missing field `rerank` / function `default_legal_rerank`.

- [ ] **Step 4: Route to the reranked path**

In `legal_search_impl_with_effective`, replace the dispatch block (lines 772-779). Destructure `rerank` from `p` (add `rerank,` to the `LegalSearchParams { ... }` pattern at line 740), then:

```rust
        let pool = self.cfg.rerank_pool_size;
        let result = match self.legal_document_ids_for_effective(effective).await? {
            Some(doc_ids) => {
                if rerank {
                    #[cfg(feature = "rerank")]
                    {
                        pipeline
                            .legal_search_scoped_reranked(&query, top_k, filters, &doc_ids, pool)
                            .await
                    }
                    #[cfg(not(feature = "rerank"))]
                    {
                        pipeline.legal_search_scoped(&query, top_k, filters, &doc_ids).await
                    }
                } else {
                    pipeline.legal_search_scoped(&query, top_k, filters, &doc_ids).await
                }
            }
            None => {
                if rerank {
                    #[cfg(feature = "rerank")]
                    {
                        pipeline.legal_search_reranked(&query, top_k, filters, pool).await
                    }
                    #[cfg(not(feature = "rerank"))]
                    {
                        pipeline.legal_search(&query, top_k, filters).await
                    }
                } else {
                    pipeline.legal_search(&query, top_k, filters).await
                }
            }
        };
```

> When the binary lacks the `rerank` feature, also push a warning into the response. If `LegalSearchResult` has no warnings field, log via the existing `tracing::info!` audit line (`reranked = cfg!(feature = "rerank") && rerank`).

- [ ] **Step 5: Run test to verify it passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/anno-rag-mcp/src/wire.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): rerank legal search by default with rerank=false escape hatch"
```

---

## Task 8: Async job spawn in `legal_ingest_impl` + in-memory dedup guard

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`AnnoRagServer` struct + both constructors + `legal_ingest_impl`)
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add the dedup field to `AnnoRagServer`**

Add to the struct (after `extraction_status`):

```rust
    /// corpus_id (string) → running job_id, guards against duplicate ingest.
    active_ingest_jobs: Arc<RwLock<HashMap<String, String>>>,
```

Initialize in **both** `new` and `new_lazy`:

```rust
            active_ingest_jobs: Arc::new(RwLock::new(HashMap::new())),
```

- [ ] **Step 2: Convert `legal_ingest_impl` to spawn-and-return**

Replace the synchronous body with: validate path, ensure models ready, dedup-check, insert job, spawn, return immediately.

```rust
    async fn legal_ingest_impl(
        &self,
        p: LegalIngestParams,
        corpus_id: Option<anno_corpus_core::CorpusId>,
    ) -> Result<serde_json::Value, String> {
        let folder = self.validate_existing_mcp_path("folder", &p.folder)?;
        // Ensure models are ready (downloads happen here, synchronously, once).
        self.require_models().await?;
        let pipeline = self
            .pipeline_arc()
            .ok_or_else(|| "pipeline unavailable after warmup".to_string())?;
        let knowledge = self
            .knowledge()
            .await
            .map_err(|e| e.to_string())?
            .clone_handle(); // see note below

        let corpus_key = corpus_id.map(|c| c.to_string()).unwrap_or_else(|| folder.display().to_string());

        // Dedup: return the existing running job if one is in flight.
        if let Some(existing) = self.active_ingest_jobs.read().await.get(&corpus_key).cloned() {
            return Ok(serde_json::json!({
                "ok": true, "job_id": existing, "status": "running", "deduped": true
            }));
        }

        let job_id = uuid::Uuid::new_v4().to_string();
        knowledge
            .insert_job(&job_id, "legal_ingest", Some(&corpus_key), 0)
            .map_err(|e| e.to_string())?;
        self.active_ingest_jobs
            .write()
            .await
            .insert(corpus_key.clone(), job_id.clone());

        let active = Arc::clone(&self.active_ingest_jobs);
        let cfg = Arc::clone(&self.cfg);
        let folder_for_task = folder.clone();
        let recursive = p.recursive;
        let job_id_task = job_id.clone();
        let corpus_key_task = corpus_key.clone();

        tokio::spawn(async move {
            let out = corpus_id
                .map(|cid| corpus_legal_output_dir(cfg.as_ref(), cid))
                .unwrap_or_else(|| folder_for_task.join("anon"));

            let result = if let Some(cid) = corpus_id {
                pipeline
                    .ingest_folder_scoped_summary(
                        &folder_for_task,
                        recursive,
                        &out,
                        anno_rag::pipeline::LegalIngestScope { corpus_id: cid, root: folder_for_task.clone() },
                    )
                    .await
                    .map(|s| s.ingested)
            } else {
                pipeline.ingest_folder(&folder_for_task, recursive, &out).await
            };

            match result {
                Ok(ingested) => {
                    let _ = knowledge.update_job_progress(&job_id_task, ingested as i64);
                    let status = if ingested == 0 {
                        anno_knowledge_store::JobStatus::Failed
                    } else {
                        anno_knowledge_store::JobStatus::Done
                    };
                    let _ = knowledge.set_job_status(&job_id_task, status, None);
                }
                Err(e) => {
                    let _ = knowledge.set_job_status(
                        &job_id_task,
                        anno_knowledge_store::JobStatus::Failed,
                        Some(&e.to_string()),
                    );
                }
            }
            active.write().await.remove(&corpus_key_task);
        });

        Ok(serde_json::json!({
            "ok": true, "job_id": job_id, "status": "running", "folder": p.folder
        }))
    }
```

> **Notes for the implementer:**
> - `self.knowledge().await` returns `&KnowledgeService`. The spawn needs an owned handle. Either (a) make `KnowledgeService` hold an `Arc`-shareable store and add a `clone_handle()` returning a `KnowledgeService` over a cloned `Arc<KnowledgeControlStore>` (preferred), or (b) re-open the service inside the task with `KnowledgeService::open(&cfg)` (cheap — SQLite open). Option (b) avoids touching `KnowledgeService`'s internals: replace `.clone_handle()` with opening inside the task using the captured `cfg`.
> - The corpus binding/document registration that the old code did after a successful sync (lib.rs:665-693) must move **into the spawned task** after `Ok(summary)`. Preserve it verbatim, using a corpus service handle re-opened in the task (same Arc/clone consideration). Keep the `summary.documents` registration loop intact.
> - Pick option (b) for both `knowledge` and `corpus` to minimize struct changes: open them inside the task from `cfg`.

- [ ] **Step 3: Write the test (dedup returns same job)**

```rust
#[tokio::test]
async fn legal_ingest_returns_job_id_and_dedups() {
    let dir = tempfile::tempdir().expect("dir");
    // ... construct AnnoRagServer with a temp data dir and a folder with 1 txt file ...
    // First call returns a job_id with status "running".
    // Immediate second call for the same folder returns deduped: true with the same job_id.
}
```

> If full pipeline construction is too heavy for a unit test, assert the dedup map logic directly via a thin helper `fn dedup_check(&self, key) -> Option<String>` extracted from the impl, and unit-test that helper instead.

- [ ] **Step 4: Run check + test**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS / compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): detached async legal_ingest with job tracking + dedup"
```

---

## Task 9: `job_status` MCP tool

**Files:**
- Modify: `crates/anno-rag-mcp/src/wire.rs` (`JobStatusParams`, `JobStatusResult`)
- Modify: `crates/anno-rag-mcp/src/lib.rs` (new `#[tool]`)
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add wire types**

In `wire.rs`:

```rust
/// Parameters for `job_status`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct JobStatusParams {
    /// Job id returned by `legal_ingest` / `index`.
    pub job_id: String,
}

/// Result for `job_status`.
#[derive(Debug, Clone, Serialize)]
pub struct JobStatusResult {
    /// `running` / `done` / `interrupted` / `failed` / `unknown`.
    pub status: String,
    /// Files processed so far.
    pub files_done: i64,
    /// Total files discovered (0 until first progress update).
    pub files_total: i64,
    /// Last error, if any.
    pub last_error: Option<String>,
}
```

- [ ] **Step 2: Add the tool**

In the `#[tool_router]` impl in `lib.rs` (near `corpus_get`):

```rust
    /// Poll the status of an async ingestion job.
    #[tool(description = "Poll an async ingestion job started by legal_ingest/index. Returns status, files_done, files_total, last_error.")]
    async fn job_status(&self, Parameters(p): Parameters<JobStatusParams>) -> String {
        let knowledge = match self.knowledge().await {
            Ok(k) => k,
            Err(e) => return serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        };
        match knowledge.get_job(&p.job_id) {
            Ok(Some(row)) => serde_json::json!({
                "ok": true,
                "job_id": row.job_id,
                "status": row.status,
                "files_done": row.files_done,
                "files_total": row.files_total,
                "last_error": row.last_error,
            })
            .to_string(),
            Ok(None) => serde_json::json!({
                "ok": true, "job_id": p.job_id, "status": "unknown"
            })
            .to_string(),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        }
    }
```

- [ ] **Step 3: Test**

```rust
#[tokio::test]
async fn job_status_unknown_id_returns_unknown() {
    // construct server with temp data dir; call job_status with a random id;
    // assert status == "unknown".
}
```

- [ ] **Step 4: Run + Commit**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS.

```bash
git add crates/anno-rag-mcp/src/wire.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): job_status tool"
```

---

## Task 10: Startup interrupted-job sweep

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (server startup / first-init path)
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Find the startup hook**

The sweep must run once when the knowledge service first initializes. The cleanest place is right after `self.knowledge()` first succeeds, or in `serve_stdio`. Add an idempotent call guarded so it only sweeps once per process (e.g. a `OnceCell<()>` or a bool in an existing `RwLock`).

- [ ] **Step 2: Implement**

Add a helper and call it from the server bootstrap (where `new`/`new_lazy` is turned into a running server, e.g. `serve_stdio`):

```rust
    /// Mark jobs left `running` by a previous (crashed) process as interrupted.
    /// Idempotent and cheap; safe to call at startup.
    pub async fn sweep_interrupted_jobs(&self) {
        if let Ok(k) = self.knowledge().await {
            match k.mark_running_jobs_interrupted() {
                Ok(n) if n > 0 => tracing::warn!(count = n, "marked stale ingest jobs interrupted"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "interrupted-job sweep failed"),
            }
        }
    }
```

Call `server.sweep_interrupted_jobs().await;` in `serve_stdio` before entering the request loop.

- [ ] **Step 3: Test**

```rust
#[tokio::test]
async fn sweep_marks_running_jobs_interrupted() {
    // construct server with temp data dir; insert a running job via the
    // knowledge service; call sweep_interrupted_jobs; assert get_job -> interrupted.
}
```

- [ ] **Step 4: Run + Commit**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS.

```bash
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): sweep interrupted ingest jobs at startup"
```

---

## Task 11: `corpus_list` returns metadata + knowledge FTS integration test

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs` (`corpus_list`, lines 2055-2070)
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn corpus_list_returns_ids_not_just_count() {
    // construct server with temp data dir; register a corpus via the corpus
    // service; call corpus_list(); parse JSON; assert corpora[0].corpus_id present.
    let out = server.corpus_list().await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert!(v["corpora"].as_array().map_or(false, |a| !a.is_empty()));
    assert!(v["corpora"][0]["corpus_id"].is_string());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: FAIL — response has `count` but no `corpora` array.

- [ ] **Step 3: Implement**

Replace `corpus_list`:

```rust
    #[tool(description = "List indexed client corpora (id, label, health) without exposing raw filesystem paths. Does not load models.")]
    async fn corpus_list(&self) -> String {
        match self.corpus().await {
            Ok(service) => match service.list() {
                Ok(corpora) => serde_json::json!({
                    "ok": true,
                    "count": corpora.len(),
                    "corpora": corpora,
                })
                .to_string(),
                Err(e) => format!("Error: {e}"),
            },
            Err(e) => format!("Error: {e}"),
        }
    }
```

> `CorpusService::list()` returns `Vec<CorpusWire>` which is `Serialize` (see `corpus.rs`). Keep `count` for backward compatibility.

- [ ] **Step 4: Knowledge FTS end-to-end check (validates Task 4)**

Add an integration-style test (or a manual MCP step in Task 12) that: registers a synthetic French fixture folder, runs knowledge sync, then `search(mode=fast, scope=knowledge)` and asserts non-empty hits. If empty, return to Task 4 Step 3 (build LanceDB FTS). Use only synthetic fixtures (privacy rule).

- [ ] **Step 5: Run + Commit**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp`
Expected: PASS.

```bash
git add crates/anno-rag-mcp/src/lib.rs
git commit -m "fix(mcp): corpus_list returns corpus metadata, not just count"
```

---

## Task 12: `index_building` status in `search` + final verification

**Files:**
- Modify: `crates/anno-rag-mcp/src/search.rs` / the search MCP handler
- Test: manual MCP feature tour

- [ ] **Step 1: Surface missing-index as a status, not silent empty**

In the knowledge search path, when the underlying store reports a missing index (or the lazy build is in progress), include `"index_status": "building"` in the response warnings rather than returning bare empty hits. With Task 5's lazy build this should be rare, but the signal prevents the "silent 0 hits" confusion. Add a warning string:

```rust
warnings.push("index was missing and is being built; retry shortly".to_string());
```

guarded on the specific error classification (reuse the `INVERTED index` substring check, or a typed error if available).

- [ ] **Step 2: Lint everything**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\lint-check.ps1`
Expected: clean `cargo fmt --check` and `cargo clippy -D warnings` on all changed crates. Fix anything it reports (`cargo fmt --all`, `cargo clippy --fix --allow-dirty`).

- [ ] **Step 3: Rebuild the MCP binary**

Guard first: `Get-Process cargo,rustc -ErrorAction SilentlyContinue` (must be empty). Kill any running `anno-rag` that locks the exe: `Get-Process anno-rag -ErrorAction SilentlyContinue | Stop-Process -Force`. Then build with the `rerank` feature:

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode build
```

> Confirm the binary build enables the `rerank` feature (check `anno-rag-bin`'s default features / build script). If not default, add `--features rerank` to the build invocation.

- [ ] **Step 4: Manual MCP feature tour (the acceptance test)**

Restart the MCP server, then verify:
1. `corpus_list` → returns `corpora` array with ids.
2. `index(profile=legal, path=<synthetic fixtures>)` → returns `{job_id, status: running}` immediately (no timeout).
3. `job_status(job_id)` → progresses `running` → `done`.
4. `search(scope=legal, mode=semantic, query=...)` → non-empty, reranked hits; second call with `rerank=false` still returns hits (faster).
5. `search(scope=knowledge, mode=fast, query=...)` → non-empty hits.
6. `legacy_search(query=...)` → no raw Lance "INVERTED index" error.

- [ ] **Step 5: Final commit + push**

```bash
git add -A
git commit -m "feat: fast+accurate legal search (async jobs + reranked hybrid) — verified"
```

Push only when the user asks (per repo policy). If pushing: create a branch first if on `main`, run `lint-check.ps1` once more, then open a PR on `jamon8888/anno`.

---

## Self-Review Notes

- **Spec coverage:** Pillar 1 (async jobs) → Tasks 1,2,3,8,9,10; Pillar 2 (reranked hybrid) → Tasks 6,7; Pillar 3 fixes → FTS-never-built (Tasks 4,5,11-Step4), legal-ingest-timeout (Task 8), corpus_list (Task 11), silent-empty (Task 12). All four spec bug fixes and both pillars are covered.
- **Conditional task:** Task 4 is explicitly conditional on whether knowledge fast search reads SQLite (auto-populated FTS5) or LanceDB. The implementer verifies via Task 11-Step 4 before adding the LanceDB build. This avoids a speculative change.
- **Type consistency:** `JobStatus`/`JobRow` defined in Task 2, re-exported and used identically in Tasks 3, 8, 9, 10. `legal_search_reranked(query, top_k, filters, pool_size)` signature defined in Task 6 and called with the same arity in Task 7. `rerank` field defined in Task 7 wire change, consumed in the same task's dispatch.
- **Open verification points flagged inline** (not placeholders — they are "confirm X then proceed" guards): `AnnoRagConfig.data_dir` field name (Task 3), `LegalSearchHit: Clone` (Task 6), owned service handle vs re-open in spawn (Task 8), `rerank` feature default in `anno-rag-bin` (Task 12).
