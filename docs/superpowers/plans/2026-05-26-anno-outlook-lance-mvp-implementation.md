# Outlook Lance MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first testable Outlook-only indexing spine: SQLite ledger, fake Graph sync boundary, LanceDB `outlook_chunks_v1`, model-free MCP status, and FTS Outlook search.

**Architecture:** Keep `anno-rag` as the owner of privacy, sync, and storage. Add an Outlook vertical under `anno-rag::outlook` with SQLite for transactional Graph state and LanceDB table `outlook_chunks_v1` for search. Keep `anno-rag-mcp` thin: MCP tools call an `OutlookService` lazy subsystem and only initialize `Pipeline` when indexing or semantic search needs models.

**Tech Stack:** Rust 1.88+, `rusqlite` bundled SQLite, existing `lancedb = 0.29.0`, existing `tokio`, existing Anno `Vault`/`Detector`/`Embedder`, Microsoft Graph DTOs behind an injectable client trait.

---

## File Structure

- Create `crates/anno-rag/src/outlook/mod.rs`: public Outlook module boundary and re-exports.
- Create `crates/anno-rag/src/outlook/ids.rs`: deterministic UUID v5 ids for accounts, folders, messages, revisions, chunks.
- Create `crates/anno-rag/src/outlook/types.rs`: small data structs shared by ledger, store, service, and MCP.
- Create `crates/anno-rag/src/outlook/ledger.rs`: SQLite ledger, schema migrations, sync-run and cursor transactions.
- Create `crates/anno-rag/src/outlook/store.rs`: LanceDB `outlook_chunks_v1` schema, upsert, indexes, FTS search, index status.
- Create `crates/anno-rag/src/outlook/normalize.rs`: Graph JSON/mail DTO to normalized Outlook message.
- Create `crates/anno-rag/src/outlook/graph.rs`: `OutlookGraphClient` trait and fake client for tests.
- Create `crates/anno-rag/src/outlook/service.rs`: orchestration over ledger, graph, pipeline, and store.
- Modify `crates/anno-rag/src/lib.rs`: export `outlook`.
- Modify `crates/anno-rag/src/config.rs`: add `outlook_ledger_path()`.
- Modify `crates/anno-rag/src/error.rs`: add `Outlook(String)`.
- Modify `crates/anno-rag/Cargo.toml`: add direct dependency already present in workspace: `rusqlite`.
- Modify `crates/anno-rag-mcp/src/lib.rs`: add lazy `OutlookService` and Outlook MCP tools.
- Modify `crates/anno-rag-mcp/src/health.rs`: include new Outlook tools in health output.
- Create focused tests under `crates/anno-rag/tests/` only for slow Lance/Graph integration; keep fast unit tests in module `#[cfg(test)]`.

Execution constraints:

- Do not modify `Pipeline::ingest_folder` or the existing `chunks` table.
- Before editing an existing public symbol such as `Pipeline`, `Store`, `AnnoRagConfig`, or `AnnoRagServer`, run GitNexus impact analysis if available and record the direct callers in the implementation notes.
- Do not stage unrelated dirty files. Use exact `git add` paths from each task.
- Use the repo fast loop: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check`.

---

### Task 1: Add Outlook Module Skeleton and Config Path

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag/src/lib.rs`
- Modify: `crates/anno-rag/src/config.rs`
- Modify: `crates/anno-rag/src/error.rs`
- Create: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write the failing config test**

Add this test inside `crates/anno-rag/src/config.rs` test module:

```rust
#[test]
fn outlook_ledger_path_derives_from_data_dir() {
    let c = AnnoRagConfig {
        data_dir: PathBuf::from("/tmp/anno-rag"),
        ..Default::default()
    };

    assert_eq!(
        c.outlook_ledger_path(),
        PathBuf::from("/tmp/anno-rag/outlook.sqlite3")
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test -p anno-rag outlook_ledger_path_derives_from_data_dir --lib
```

Expected: compile failure mentioning `no method named outlook_ledger_path`.

- [ ] **Step 3: Add the minimal module and config implementation**

In `crates/anno-rag/src/config.rs`, add this method inside `impl AnnoRagConfig`:

```rust
/// Path to the Outlook sync ledger SQLite database.
#[must_use]
pub fn outlook_ledger_path(&self) -> PathBuf {
    self.data_dir.join("outlook.sqlite3")
}
```

In `crates/anno-rag/src/lib.rs`, add:

```rust
pub mod outlook;
```

Create `crates/anno-rag/src/outlook/mod.rs`:

```rust
//! Outlook-only source integration.
//!
//! The Outlook vertical uses SQLite for Graph sync state and LanceDB for
//! pseudonymized searchable chunks. It intentionally does not generalize to the
//! full Microsoft 365 suite in the MVP.

pub mod ids;
pub mod ledger;
pub mod store;
pub mod types;
```

In `crates/anno-rag/src/error.rs`, add this enum variant before `Io`:

```rust
/// Outlook source integration error.
#[error("outlook: {0}")]
Outlook(String),
```

In `crates/anno-rag/Cargo.toml`, add the direct dependency:

```toml
rusqlite             = { workspace = true }
```

- [ ] **Step 4: Run the test and fast check**

Run:

```powershell
cargo test -p anno-rag outlook_ledger_path_derives_from_data_dir --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: test passes, check passes for `anno-rag`.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/src/lib.rs crates/anno-rag/src/config.rs crates/anno-rag/src/error.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add outlook module boundary"
```

---

### Task 2: Deterministic Outlook IDs

**Files:**
- Create: `crates/anno-rag/src/outlook/ids.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write the failing ID tests**

Create `crates/anno-rag/src/outlook/ids.rs` with tests first:

```rust
use uuid::Uuid;

const OUTLOOK_NAMESPACE: Uuid = Uuid::from_u128(0x6e2d_3f25_8d0e_5d3e_9a65_8f5d_8b0a_0001);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_is_stable_and_tenant_sensitive() {
        let a = account_id(Some("tenant-a"), "user-1");
        let b = account_id(Some("tenant-a"), "user-1");
        let c = account_id(Some("tenant-b"), "user-1");

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn message_revision_and_chunk_ids_are_stable() {
        let account = account_id(Some("tenant-a"), "user-1");
        let message = message_id(account, "immutable-message-id");
        let revision = revision_id(message, "change-key-1", &[1, 2, 3, 4]);
        let chunk0 = chunk_id(revision, 0);
        let chunk1 = chunk_id(revision, 1);

        assert_eq!(message, message_id(account, "immutable-message-id"));
        assert_eq!(revision, revision_id(message, "change-key-1", &[1, 2, 3, 4]));
        assert_ne!(chunk0, chunk1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test -p anno-rag outlook::ids --lib
```

Expected: compile failure for missing functions.

- [ ] **Step 3: Implement the ID functions**

Add above the test module in `ids.rs`:

```rust
fn uuid_v5(parts: &[&[u8]]) -> Uuid {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(part);
        bytes.push(0);
    }
    Uuid::new_v5(&OUTLOOK_NAMESPACE, &bytes)
}

/// Stable account id derived from Microsoft tenant id and subject/user id.
#[must_use]
pub fn account_id(tenant_id: Option<&str>, provider_subject: &str) -> Uuid {
    uuid_v5(&[
        b"account",
        tenant_id.unwrap_or("consumers").as_bytes(),
        provider_subject.as_bytes(),
    ])
}

/// Stable folder id scoped to an Outlook account.
#[must_use]
pub fn folder_id(account_id: Uuid, provider_folder_id: &str) -> Uuid {
    uuid_v5(&[
        b"folder",
        account_id.as_bytes(),
        provider_folder_id.as_bytes(),
    ])
}

/// Stable message id scoped to an Outlook account.
#[must_use]
pub fn message_id(account_id: Uuid, immutable_message_id: &str) -> Uuid {
    uuid_v5(&[
        b"message",
        account_id.as_bytes(),
        immutable_message_id.as_bytes(),
    ])
}

/// Stable revision id for a message version.
#[must_use]
pub fn revision_id(message_id: Uuid, change_key: &str, content_hash: &[u8]) -> Uuid {
    uuid_v5(&[
        b"revision",
        message_id.as_bytes(),
        change_key.as_bytes(),
        content_hash,
    ])
}

/// Stable chunk id for a revision chunk.
#[must_use]
pub fn chunk_id(revision_id: Uuid, chunk_idx: u32) -> Uuid {
    uuid_v5(&[
        b"chunk",
        revision_id.as_bytes(),
        &chunk_idx.to_be_bytes(),
    ])
}
```

Ensure `crates/anno-rag/src/outlook/mod.rs` exports the module:

```rust
pub mod ids;
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag outlook::ids --lib
```

Expected: both ID tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/ids.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add deterministic outlook ids"
```

---

### Task 3: Outlook Shared Types

**Files:**
- Create: `crates/anno-rag/src/outlook/types.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write the type normalization tests**

Create `crates/anno-rag/src/outlook/types.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_status_round_trips_as_storage_string() {
        assert_eq!(SyncRunStatus::Running.as_str(), "running");
        assert_eq!(SyncRunStatus::Completed.as_str(), "completed");
        assert_eq!(SyncRunStatus::Failed.as_str(), "failed");
        assert_eq!(SyncRunStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn job_state_round_trips_as_storage_string() {
        assert_eq!(IndexJobState::Pending.as_str(), "pending");
        assert_eq!(IndexJobState::Running.as_str(), "running");
        assert_eq!(IndexJobState::Done.as_str(), "done");
        assert_eq!(IndexJobState::Failed.as_str(), "failed");
        assert_eq!(IndexJobState::Skipped.as_str(), "skipped");
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test -p anno-rag outlook::types --lib
```

Expected: compile failure for missing enums.

- [ ] **Step 3: Add the shared types**

Add above the test module in `types.rs`:

```rust
/// Outlook sync run status stored in SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncRunStatus {
    /// A sync run is active.
    Running,
    /// A sync run committed successfully.
    Completed,
    /// A sync run failed.
    Failed,
    /// A sync run was cancelled by the user or superseded.
    Cancelled,
}

impl SyncRunStatus {
    /// Stable SQLite representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Outlook indexing job state stored in SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexJobState {
    /// Job is available to claim.
    Pending,
    /// Job is claimed by a worker.
    Running,
    /// Job completed and its Lance row exists.
    Done,
    /// Job failed and can be retried.
    Failed,
    /// Job was intentionally skipped.
    Skipped,
}

impl IndexJobState {
    /// Stable SQLite representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Metadata recorded before model-backed indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlookMessageMeta {
    /// Stable Anno message id.
    pub message_id: Uuid,
    /// Current revision id.
    pub revision_id: Uuid,
    /// Account id.
    pub account_id: Uuid,
    /// Provider folder id.
    pub provider_folder_id: String,
    /// Provider immutable message id.
    pub provider_message_id: String,
    /// Provider change key.
    pub change_key: String,
    /// Pseudonymized or hashed subject for status/search summaries.
    pub subject_pseudo: String,
    /// Received timestamp.
    pub received_at: DateTime<Utc>,
    /// Sent timestamp.
    pub sent_at: Option<DateTime<Utc>>,
    /// Whether the message has attachments.
    pub has_attachments: bool,
    /// Whether the message is read.
    pub is_read: bool,
    /// Importance string from Graph.
    pub importance: Option<String>,
    /// True when Graph reports deletion.
    pub deleted: bool,
}

/// Pseudonymized chunk ready for LanceDB.
#[derive(Debug, Clone)]
pub struct OutlookChunkRecord {
    /// Stable chunk id.
    pub chunk_id: Uuid,
    /// Stable message id.
    pub message_id: Uuid,
    /// Stable revision id.
    pub revision_id: Uuid,
    /// Account id.
    pub account_id: Uuid,
    /// Provider folder id.
    pub folder_id: String,
    /// Chunk index.
    pub chunk_idx: u32,
    /// Pseudonymized subject.
    pub subject_pseudo: String,
    /// Pseudonymized body chunk.
    pub text_pseudo: String,
    /// Pseudonymized sender/recipient summary.
    pub participants_pseudo: String,
    /// Received timestamp.
    pub received_at: DateTime<Utc>,
    /// Sent timestamp.
    pub sent_at: Option<DateTime<Utc>>,
    /// Attachment flag.
    pub has_attachments: bool,
    /// Read flag.
    pub is_read: bool,
    /// Importance.
    pub importance: Option<String>,
    /// Pseudonymized metadata JSON.
    pub metadata_json_pseudo: String,
    /// Embedding vector.
    pub vector: Vec<f32>,
}

/// Outlook status returned by service and MCP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlookStatus {
    /// Number of connected accounts.
    pub account_count: u64,
    /// Pending indexing jobs.
    pub pending_jobs: u64,
    /// Last successful sync timestamp if any.
    pub last_success_at: Option<DateTime<Utc>>,
    /// Last sync error if any.
    pub last_error: Option<String>,
}
```

Ensure `mod.rs` exports:

```rust
pub mod types;
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag outlook::types --lib
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/types.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add outlook shared types"
```

---

### Task 4: SQLite Ledger Schema and Status

**Files:**
- Create: `crates/anno-rag/src/outlook/ledger.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write failing ledger tests**

Create `crates/anno-rag/src/outlook/ledger.rs`:

```rust
use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use crate::outlook::types::OutlookStatus;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn cfg_for(dir: &Path) -> AnnoRagConfig {
        AnnoRagConfig {
            data_dir: dir.to_path_buf(),
            ..Default::default()
        }
    }

    #[test]
    fn open_creates_required_tables_and_returns_empty_status() {
        let tmp = tempdir().expect("tempdir");
        let ledger = OutlookLedger::open(&cfg_for(tmp.path())).expect("open ledger");

        for table in [
            "outlook_accounts",
            "outlook_folders",
            "outlook_delta_cursors",
            "outlook_messages",
            "outlook_sync_runs",
            "outlook_index_jobs",
            "outlook_source_audit",
        ] {
            assert!(ledger.table_exists(table).expect("table lookup"), "{table}");
        }

        let status = ledger.status().expect("status");
        assert_eq!(status.account_count, 0);
        assert_eq!(status.pending_jobs, 0);
        assert_eq!(status.last_success_at, None);
        assert_eq!(status.last_error, None);
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag outlook::ledger --lib
```

Expected: compile failure for missing `OutlookLedger`.

- [ ] **Step 3: Implement `OutlookLedger::open`, schema, and status**

Add above the test module:

```rust
/// SQLite-backed Outlook sync ledger.
pub struct OutlookLedger {
    conn: Connection,
}

impl OutlookLedger {
    /// Open and initialize the Outlook ledger.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] when the SQLite database cannot be opened or migrated.
    pub fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(Error::from)?;
        let conn = Connection::open(cfg.outlook_ledger_path())
            .map_err(|err| Error::Outlook(format!("open ledger: {err}")))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| Error::Outlook(format!("set WAL: {err}")))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|err| Error::Outlook(format!("set synchronous: {err}")))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|err| Error::Outlook(format!("busy_timeout: {err}")))?;
        init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Return high-level Outlook status without touching model-backed pipeline state.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] on SQLite query errors.
    pub fn status(&self) -> Result<OutlookStatus> {
        let account_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM outlook_accounts", [], |row| row.get::<_, u64>(0))
            .map_err(|err| Error::Outlook(format!("count accounts: {err}")))?;
        let pending_jobs = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM outlook_index_jobs WHERE state = 'pending'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|err| Error::Outlook(format!("count jobs: {err}")))?;
        let last_success_at = None;
        let last_error: Option<String> = self
            .conn
            .query_row(
                "SELECT error_message FROM outlook_sync_runs WHERE error_message IS NOT NULL ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| Error::Outlook(format!("last error: {err}")))?;
        Ok(OutlookStatus {
            account_count,
            pending_jobs,
            last_success_at,
            last_error,
        })
    }

    #[cfg(test)]
    fn table_exists(&self, table: &str) -> Result<bool> {
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |_| Ok(()),
            )
            .optional()
            .map_err(|err| Error::Outlook(format!("table exists: {err}")))?
            .is_some();
        Ok(exists)
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS outlook_accounts (
            account_id BLOB PRIMARY KEY,
            provider_subject TEXT NOT NULL,
            tenant_id TEXT,
            display_name_pseudo TEXT NOT NULL,
            token_ref TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS outlook_folders (
            account_id BLOB NOT NULL,
            provider_folder_id TEXT NOT NULL,
            display_name_pseudo TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (account_id, provider_folder_id)
        );

        CREATE TABLE IF NOT EXISTS outlook_delta_cursors (
            account_id BLOB NOT NULL,
            provider_folder_id TEXT NOT NULL,
            delta_link_encrypted BLOB NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (account_id, provider_folder_id)
        );

        CREATE TABLE IF NOT EXISTS outlook_messages (
            message_id BLOB PRIMARY KEY,
            revision_id BLOB NOT NULL,
            account_id BLOB NOT NULL,
            provider_folder_id TEXT NOT NULL,
            provider_message_id TEXT NOT NULL,
            change_key TEXT NOT NULL,
            subject_pseudo TEXT NOT NULL,
            received_at TEXT NOT NULL,
            sent_at TEXT,
            has_attachments INTEGER NOT NULL,
            is_read INTEGER NOT NULL,
            importance TEXT,
            deleted INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (account_id, provider_message_id)
        );

        CREATE TABLE IF NOT EXISTS outlook_sync_runs (
            run_id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id BLOB NOT NULL,
            provider_folder_id TEXT NOT NULL,
            sync_type TEXT NOT NULL,
            started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            completed_at TEXT,
            status TEXT NOT NULL,
            objects_seen INTEGER NOT NULL DEFAULT 0,
            objects_added INTEGER NOT NULL DEFAULT 0,
            objects_updated INTEGER NOT NULL DEFAULT 0,
            objects_deleted INTEGER NOT NULL DEFAULT 0,
            errors_count INTEGER NOT NULL DEFAULT 0,
            cursor_before TEXT,
            cursor_after TEXT,
            error_message TEXT
        );

        CREATE TABLE IF NOT EXISTS outlook_index_jobs (
            job_id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id BLOB NOT NULL,
            revision_id BLOB NOT NULL,
            state TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            claimed_at TEXT,
            claim_token TEXT,
            last_error TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (message_id, revision_id)
        );

        CREATE TABLE IF NOT EXISTS outlook_source_audit (
            audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_kind TEXT NOT NULL,
            account_id BLOB,
            message_id BLOB,
            detail_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE INDEX IF NOT EXISTS idx_outlook_jobs_state ON outlook_index_jobs(state, created_at);
        CREATE INDEX IF NOT EXISTS idx_outlook_messages_account_folder ON outlook_messages(account_id, provider_folder_id, received_at);
        CREATE INDEX IF NOT EXISTS idx_outlook_sync_runs_status ON outlook_sync_runs(status, started_at);
        "#,
    )
    .map_err(|err| Error::Outlook(format!("init schema: {err}")))?;
    Ok(())
}
```

- [ ] **Step 4: Run tests and fast check**

Run:

```powershell
cargo test -p anno-rag outlook::ledger --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: tests pass, check passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/ledger.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add outlook sqlite ledger"
```

---

### Task 5: Transactional Sync Batch Commit

**Files:**
- Modify: `crates/anno-rag/src/outlook/ledger.rs`
- Modify: `crates/anno-rag/src/outlook/types.rs`

- [ ] **Step 1: Write the failing transaction test**

Add to `types.rs`:

```rust
/// A committed Outlook sync batch.
#[derive(Debug, Clone)]
pub struct OutlookSyncBatch {
    /// Account id.
    pub account_id: Uuid,
    /// Provider folder id.
    pub provider_folder_id: String,
    /// Cursor before the batch.
    pub cursor_before: Option<String>,
    /// Cursor after the batch.
    pub cursor_after: String,
    /// Changed messages.
    pub messages: Vec<OutlookMessageMeta>,
    /// Provider ids for deleted messages.
    pub deleted_provider_message_ids: Vec<String>,
}
```

Add this test to `ledger.rs` tests:

```rust
#[test]
fn commit_sync_batch_upserts_messages_jobs_and_cursor_atomically() {
    use crate::outlook::types::{OutlookMessageMeta, OutlookSyncBatch};
    use chrono::TimeZone;
    use uuid::Uuid;

    let tmp = tempdir().expect("tempdir");
    let ledger = OutlookLedger::open(&cfg_for(tmp.path())).expect("open ledger");
    let account = Uuid::from_u128(1);
    let message = Uuid::from_u128(2);
    let revision = Uuid::from_u128(3);

    ledger.insert_account_for_test(account).expect("account");
    ledger
        .commit_sync_batch(&OutlookSyncBatch {
            account_id: account,
            provider_folder_id: "inbox".to_string(),
            cursor_before: None,
            cursor_after: "delta-1".to_string(),
            messages: vec![OutlookMessageMeta {
                message_id: message,
                revision_id: revision,
                account_id: account,
                provider_folder_id: "inbox".to_string(),
                provider_message_id: "provider-message".to_string(),
                change_key: "ck1".to_string(),
                subject_pseudo: "subject".to_string(),
                received_at: Utc.with_ymd_and_hms(2026, 5, 26, 12, 0, 0).unwrap(),
                sent_at: None,
                has_attachments: false,
                is_read: true,
                importance: Some("normal".to_string()),
                deleted: false,
            }],
            deleted_provider_message_ids: vec![],
        })
        .expect("commit");

    assert_eq!(ledger.cursor_for_test(account, "inbox").expect("cursor"), Some("delta-1".to_string()));
    assert_eq!(ledger.pending_job_count_for_test().expect("jobs"), 1);
    assert!(ledger.message_exists_for_test(message).expect("message"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test -p anno-rag commit_sync_batch_upserts_messages_jobs_and_cursor_atomically --lib
```

Expected: compile failure for missing ledger methods.

- [ ] **Step 3: Implement batch commit and test helpers**

Add methods to `impl OutlookLedger`:

```rust
/// Commit a Graph sync batch and advance the folder cursor in one SQLite transaction.
///
/// # Errors
/// Returns [`Error::Outlook`] if any write fails. The transaction rolls back.
pub fn commit_sync_batch(&mut self, batch: &crate::outlook::types::OutlookSyncBatch) -> Result<()> {
    let tx = self
        .conn
        .transaction()
        .map_err(|err| Error::Outlook(format!("begin sync batch: {err}")))?;

    tx.execute(
        "INSERT INTO outlook_sync_runs (account_id, provider_folder_id, sync_type, status, cursor_before)
         VALUES (?1, ?2, 'incremental', 'running', ?3)",
        rusqlite::params![
            batch.account_id.as_bytes().as_slice(),
            batch.provider_folder_id,
            batch.cursor_before,
        ],
    )
    .map_err(|err| Error::Outlook(format!("insert sync run: {err}")))?;
    let run_id = tx.last_insert_rowid();

    for msg in &batch.messages {
        tx.execute(
            "INSERT INTO outlook_messages (
                message_id, revision_id, account_id, provider_folder_id, provider_message_id,
                change_key, subject_pseudo, received_at, sent_at, has_attachments,
                is_read, importance, deleted
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(message_id) DO UPDATE SET
                revision_id = excluded.revision_id,
                provider_folder_id = excluded.provider_folder_id,
                change_key = excluded.change_key,
                subject_pseudo = excluded.subject_pseudo,
                received_at = excluded.received_at,
                sent_at = excluded.sent_at,
                has_attachments = excluded.has_attachments,
                is_read = excluded.is_read,
                importance = excluded.importance,
                deleted = excluded.deleted,
                updated_at = CURRENT_TIMESTAMP",
            rusqlite::params![
                msg.message_id.as_bytes().as_slice(),
                msg.revision_id.as_bytes().as_slice(),
                msg.account_id.as_bytes().as_slice(),
                msg.provider_folder_id,
                msg.provider_message_id,
                msg.change_key,
                msg.subject_pseudo,
                msg.received_at.to_rfc3339(),
                msg.sent_at.map(|t| t.to_rfc3339()),
                i64::from(msg.has_attachments),
                i64::from(msg.is_read),
                msg.importance,
                i64::from(msg.deleted),
            ],
        )
        .map_err(|err| Error::Outlook(format!("upsert message: {err}")))?;

        tx.execute(
            "INSERT INTO outlook_index_jobs (message_id, revision_id, state)
             VALUES (?1, ?2, 'pending')
             ON CONFLICT(message_id, revision_id) DO UPDATE SET
                state = 'pending',
                updated_at = CURRENT_TIMESTAMP",
            rusqlite::params![
                msg.message_id.as_bytes().as_slice(),
                msg.revision_id.as_bytes().as_slice(),
            ],
        )
        .map_err(|err| Error::Outlook(format!("enqueue job: {err}")))?;
    }

    for provider_id in &batch.deleted_provider_message_ids {
        tx.execute(
            "UPDATE outlook_messages
             SET deleted = 1, updated_at = CURRENT_TIMESTAMP
             WHERE account_id = ?1 AND provider_message_id = ?2",
            rusqlite::params![batch.account_id.as_bytes().as_slice(), provider_id],
        )
        .map_err(|err| Error::Outlook(format!("mark deleted: {err}")))?;
    }

    tx.execute(
        "INSERT INTO outlook_delta_cursors (account_id, provider_folder_id, delta_link_encrypted)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(account_id, provider_folder_id) DO UPDATE SET
            delta_link_encrypted = excluded.delta_link_encrypted,
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![
            batch.account_id.as_bytes().as_slice(),
            batch.provider_folder_id,
            batch.cursor_after.as_bytes(),
        ],
    )
    .map_err(|err| Error::Outlook(format!("update cursor: {err}")))?;

    tx.execute(
        "UPDATE outlook_sync_runs
         SET status = 'completed',
             completed_at = CURRENT_TIMESTAMP,
             cursor_after = ?1,
             objects_seen = ?2,
             objects_added = ?2
         WHERE run_id = ?3",
        rusqlite::params![batch.cursor_after, batch.messages.len() as i64, run_id],
    )
    .map_err(|err| Error::Outlook(format!("complete sync run: {err}")))?;

    tx.commit()
        .map_err(|err| Error::Outlook(format!("commit sync batch: {err}")))?;
    Ok(())
}

#[cfg(test)]
fn insert_account_for_test(&self, account_id: uuid::Uuid) -> Result<()> {
    self.conn
        .execute(
            "INSERT INTO outlook_accounts (account_id, provider_subject, display_name_pseudo, token_ref)
             VALUES (?1, 'subject', 'Account', 'token-ref')",
            [account_id.as_bytes().as_slice()],
        )
        .map_err(|err| Error::Outlook(format!("insert test account: {err}")))?;
    Ok(())
}

#[cfg(test)]
fn cursor_for_test(&self, account_id: uuid::Uuid, folder_id: &str) -> Result<Option<String>> {
    let raw: Option<Vec<u8>> = self
        .conn
        .query_row(
            "SELECT delta_link_encrypted FROM outlook_delta_cursors WHERE account_id = ?1 AND provider_folder_id = ?2",
            rusqlite::params![account_id.as_bytes().as_slice(), folder_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|err| Error::Outlook(format!("cursor for test: {err}")))?;
    raw.map(|bytes| String::from_utf8(bytes).map_err(|err| Error::Outlook(format!("cursor utf8: {err}"))))
        .transpose()
}

#[cfg(test)]
fn pending_job_count_for_test(&self) -> Result<u64> {
    self.conn
        .query_row(
            "SELECT COUNT(*) FROM outlook_index_jobs WHERE state = 'pending'",
            [],
            |row| row.get(0),
        )
        .map_err(|err| Error::Outlook(format!("pending job count: {err}")))
}

#[cfg(test)]
fn message_exists_for_test(&self, message_id: uuid::Uuid) -> Result<bool> {
    let exists = self
        .conn
        .query_row(
            "SELECT 1 FROM outlook_messages WHERE message_id = ?1",
            [message_id.as_bytes().as_slice()],
            |_| Ok(()),
        )
        .optional()
        .map_err(|err| Error::Outlook(format!("message exists: {err}")))?
        .is_some();
    Ok(exists)
}
```

Change the ledger binding in the test from `let ledger` to `let mut ledger`.

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag outlook::ledger --lib
```

Expected: ledger tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/ledger.rs crates/anno-rag/src/outlook/types.rs
git commit -m "feat: commit outlook sync batches transactionally"
```

---

### Task 6: LanceDB Outlook Table Schema and Index Status

**Files:**
- Create: `crates/anno-rag/src/outlook/store.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write schema tests**

Create `crates/anno-rag/src/outlook/store.rs`:

```rust
use crate::config::AnnoRagConfig;
use crate::error::{Error, Result};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// LanceDB table name for Outlook chunks.
pub const OUTLOOK_CHUNKS_TABLE: &str = "outlook_chunks_v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlook_schema_contains_search_and_filter_columns() {
        let schema = outlook_chunks_schema(384);
        for name in [
            "chunk_id",
            "message_id",
            "revision_id",
            "account_id",
            "folder_id",
            "subject_pseudo",
            "text_pseudo",
            "participants_pseudo",
            "received_at",
            "has_attachments",
            "is_read",
            "importance",
            "vector",
        ] {
            assert!(schema.field_with_name(name).is_ok(), "{name}");
        }
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag outlook_schema_contains_search_and_filter_columns --lib
```

Expected: compile failure for missing `outlook_chunks_schema`.

- [ ] **Step 3: Implement the schema and store shell**

Add above the test module:

```rust
/// Build the Arrow schema for Outlook chunks.
#[must_use]
pub fn outlook_chunks_schema(dim: usize) -> Arc<Schema> {
    let vector_field = Arc::new(Field::new("item", DataType::Float32, true));
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::FixedSizeBinary(16), false),
        Field::new("message_id", DataType::FixedSizeBinary(16), false),
        Field::new("revision_id", DataType::FixedSizeBinary(16), false),
        Field::new("account_id", DataType::FixedSizeBinary(16), false),
        Field::new("folder_id", DataType::Utf8, false),
        Field::new("conversation_id_hash", DataType::FixedSizeBinary(32), true),
        Field::new("internet_id_hash", DataType::FixedSizeBinary(32), true),
        Field::new("chunk_idx", DataType::UInt32, false),
        Field::new("subject_pseudo", DataType::Utf8, false),
        Field::new("text_pseudo", DataType::Utf8, false),
        Field::new("participants_pseudo", DataType::Utf8, false),
        Field::new(
            "received_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new(
            "sent_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("has_attachments", DataType::Boolean, false),
        Field::new("is_read", DataType::Boolean, false),
        Field::new("importance", DataType::Utf8, true),
        Field::new("metadata_json_pseudo", DataType::Utf8, false),
        Field::new(
            "vector",
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            DataType::FixedSizeList(vector_field, dim as i32),
            false,
        ),
        Field::new(
            "indexed_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
    ]))
}

/// LanceDB Outlook chunk store.
#[derive(Clone)]
pub struct OutlookStore {
    table: lancedb::Table,
    dim: usize,
}

impl OutlookStore {
    /// Open or create the Outlook chunks table.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] on invalid paths or LanceDB failures.
    pub async fn open(cfg: &AnnoRagConfig) -> Result<Self> {
        let path = cfg.index_path();
        let uri = path
            .to_str()
            .ok_or_else(|| Error::Outlook(format!("non-utf8 index path: {}", path.display())))?;
        let conn = lancedb::connect(uri)
            .execute()
            .await
            .map_err(|err| Error::Outlook(format!("outlook store connect: {err}")))?;
        let names = conn
            .table_names()
            .execute()
            .await
            .map_err(|err| Error::Outlook(format!("outlook store list: {err}")))?;
        let schema = outlook_chunks_schema(cfg.embed_dim);
        let table = if names.iter().any(|name| name == OUTLOOK_CHUNKS_TABLE) {
            conn.open_table(OUTLOOK_CHUNKS_TABLE).execute().await
        } else {
            let empty = arrow_array::RecordBatchIterator::new(std::iter::empty(), schema);
            let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(empty);
            conn.create_table(OUTLOOK_CHUNKS_TABLE, reader)
                .execute()
                .await
        }
        .map_err(|err| Error::Outlook(format!("outlook store open: {err}")))?;

        Ok(Self {
            table,
            dim: cfg.embed_dim,
        })
    }
}
```

Ensure `mod.rs` exports:

```rust
pub mod store;
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test -p anno-rag outlook_schema_contains_search_and_filter_columns --lib
```

Expected: schema test passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/store.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add outlook lancedb schema"
```

---

### Task 7: LanceDB Outlook Upsert and FTS Search

**Files:**
- Modify: `crates/anno-rag/src/outlook/store.rs`
- Create: `crates/anno-rag/tests/outlook_store.rs`

- [ ] **Step 1: Write ignored Lance integration test**

Create `crates/anno-rag/tests/outlook_store.rs`:

```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag::outlook::store::OutlookStore;
use anno_rag::outlook::types::OutlookChunkRecord;
use chrono::TimeZone;
use tempfile::tempdir;
use uuid::Uuid;

fn cfg_for(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        embed_dim: 8,
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "LanceDB table creation is slow; run during Outlook store work"]
async fn outlook_store_upsert_then_fts_search_returns_summary_columns() {
    let tmp = tempdir().expect("tempdir");
    let store = OutlookStore::open(&cfg_for(tmp.path())).await.expect("open");

    store
        .upsert_chunks(vec![OutlookChunkRecord {
            chunk_id: Uuid::from_u128(1),
            message_id: Uuid::from_u128(2),
            revision_id: Uuid::from_u128(3),
            account_id: Uuid::from_u128(4),
            folder_id: "inbox".to_string(),
            chunk_idx: 0,
            subject_pseudo: "Contrat Alpha".to_string(),
            text_pseudo: "Le projet Alpha demande une validation rapide.".to_string(),
            participants_pseudo: "Alice <alice@example.test>".to_string(),
            received_at: chrono::Utc.with_ymd_and_hms(2026, 5, 26, 9, 0, 0).unwrap(),
            sent_at: None,
            has_attachments: false,
            is_read: true,
            importance: Some("normal".to_string()),
            metadata_json_pseudo: "{}".to_string(),
            vector: vec![0.0; 8],
        }])
        .await
        .expect("upsert");
    store.setup_fts_index().await.expect("fts");

    let hits = store.search_fts("Alpha", 5).await.expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message_id, Uuid::from_u128(2));
    assert!(hits[0].subject_pseudo.contains("Alpha"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag --test outlook_store outlook_store_upsert_then_fts_search_returns_summary_columns -- --ignored
```

Expected: compile failure for missing `upsert_chunks`, `setup_fts_index`, `search_fts`.

- [ ] **Step 3: Implement upsert, FTS index, and FTS search**

Add to `store.rs` imports:

```rust
use crate::outlook::types::OutlookChunkRecord;
use arrow_array::{
    builder::FixedSizeBinaryBuilder, BooleanArray, FixedSizeBinaryArray,
    FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
    TimestampMicrosecondArray, UInt32Array,
};
use futures::TryStreamExt;
use uuid::Uuid;
```

Add these types and methods:

```rust
/// Outlook search hit returned from LanceDB.
#[derive(Debug, Clone)]
pub struct OutlookSearchHit {
    /// Message id.
    pub message_id: Uuid,
    /// Revision id.
    pub revision_id: Uuid,
    /// Subject.
    pub subject_pseudo: String,
    /// Snippet text.
    pub snippet_pseudo: String,
    /// Participants.
    pub participants_pseudo: String,
    /// Folder id.
    pub folder_id: String,
    /// Relevance score.
    pub score: f32,
}

impl OutlookStore {
    /// Upsert Outlook chunks by stable chunk id.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] for invalid vector dimensions, Arrow errors, or LanceDB failures.
    pub async fn upsert_chunks(&self, records: Vec<OutlookChunkRecord>) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let batch = records_to_batch(&records, self.dim)?;
        let schema = outlook_chunks_schema(self.dim);
        let reader = RecordBatchIterator::new(std::iter::once(Ok(batch)), schema);
        let mut merge = self.table.merge_insert(&["chunk_id"]);
        merge.when_matched_update_all(None);
        merge.when_not_matched_insert_all();
        merge
            .execute(Box::new(reader))
            .await
            .map_err(|err| Error::Outlook(format!("outlook upsert: {err}")))?;
        Ok(())
    }

    /// Build FTS index for Outlook text columns.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] on LanceDB index failures.
    pub async fn setup_fts_index(&self) -> Result<()> {
        use lancedb::index::scalar::FtsIndexBuilder;
        use lancedb::index::Index;

        let existing = self
            .table
            .list_indices()
            .await
            .map_err(|err| Error::Outlook(format!("outlook list indices: {err}")))?;
        let already = existing.iter().any(|idx| {
            idx.columns.iter().any(|column| column == "text_pseudo")
        });
        if already {
            return Ok(());
        }
        let fts = FtsIndexBuilder::default()
            .base_tokenizer("simple".to_string())
            .lower_case(true);
        self.table
            .create_index(
                &["subject_pseudo", "text_pseudo", "participants_pseudo"],
                Index::FTS(fts),
            )
            .execute()
            .await
            .map_err(|err| Error::Outlook(format!("outlook fts index: {err}")))?;
        Ok(())
    }

    /// Keyword search over Outlook chunks.
    ///
    /// # Errors
    /// Returns [`Error::Outlook`] when LanceDB query or row decoding fails.
    pub async fn search_fts(&self, query_text: &str, limit: usize) -> Result<Vec<OutlookSearchHit>> {
        use lance_index::scalar::FullTextSearchQuery;
        use lancedb::query::{ExecutableQuery, QueryBase, Select};

        let stream = self
            .table
            .query()
            .full_text_search(FullTextSearchQuery::new(query_text.to_string()))
            .select(Select::columns(&[
                "message_id",
                "revision_id",
                "subject_pseudo",
                "text_pseudo",
                "participants_pseudo",
                "folder_id",
            ]))
            .limit(limit)
            .execute()
            .await
            .map_err(|err| Error::Outlook(format!("outlook fts search: {err}")))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|err| Error::Outlook(format!("outlook fts stream: {err}")))?;

        let mut hits = Vec::new();
        for batch in &batches {
            let message_ids = required_col::<FixedSizeBinaryArray>(batch, "message_id")?;
            let revision_ids = required_col::<FixedSizeBinaryArray>(batch, "revision_id")?;
            let subjects = required_col::<StringArray>(batch, "subject_pseudo")?;
            let texts = required_col::<StringArray>(batch, "text_pseudo")?;
            let participants = required_col::<StringArray>(batch, "participants_pseudo")?;
            let folders = required_col::<StringArray>(batch, "folder_id")?;
            for row in 0..batch.num_rows() {
                hits.push(OutlookSearchHit {
                    message_id: Uuid::from_slice(message_ids.value(row))
                        .map_err(|err| Error::Outlook(format!("message uuid: {err}")))?,
                    revision_id: Uuid::from_slice(revision_ids.value(row))
                        .map_err(|err| Error::Outlook(format!("revision uuid: {err}")))?,
                    subject_pseudo: subjects.value(row).to_string(),
                    snippet_pseudo: texts.value(row).to_string(),
                    participants_pseudo: participants.value(row).to_string(),
                    folder_id: folders.value(row).to_string(),
                    score: 0.0,
                });
            }
        }
        Ok(hits)
    }
}

fn records_to_batch(records: &[OutlookChunkRecord], dim: usize) -> Result<RecordBatch> {
    let mut chunk_id_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 16);
    let mut message_id_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 16);
    let mut revision_id_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 16);
    let mut account_id_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 16);
    let mut conversation_hash_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 32);
    let mut internet_hash_b = FixedSizeBinaryBuilder::with_capacity(records.len(), 32);
    let mut folder_ids = Vec::with_capacity(records.len());
    let mut chunk_idx = Vec::with_capacity(records.len());
    let mut subjects = Vec::with_capacity(records.len());
    let mut texts = Vec::with_capacity(records.len());
    let mut participants = Vec::with_capacity(records.len());
    let mut received = Vec::with_capacity(records.len());
    let mut sent = Vec::with_capacity(records.len());
    let mut has_attachments = Vec::with_capacity(records.len());
    let mut is_read = Vec::with_capacity(records.len());
    let mut importance = Vec::with_capacity(records.len());
    let mut metadata = Vec::with_capacity(records.len());
    let mut vector_values = Vec::with_capacity(records.len() * dim);
    let mut indexed = Vec::with_capacity(records.len());

    for record in records {
        if record.vector.len() != dim {
            return Err(Error::Outlook(format!(
                "outlook vector len {} != dim {}",
                record.vector.len(),
                dim
            )));
        }
        chunk_id_b
            .append_value(record.chunk_id.as_bytes())
            .map_err(Error::Arrow)?;
        message_id_b
            .append_value(record.message_id.as_bytes())
            .map_err(Error::Arrow)?;
        revision_id_b
            .append_value(record.revision_id.as_bytes())
            .map_err(Error::Arrow)?;
        account_id_b
            .append_value(record.account_id.as_bytes())
            .map_err(Error::Arrow)?;
        conversation_hash_b.append_null();
        internet_hash_b.append_null();
        folder_ids.push(record.folder_id.as_str());
        chunk_idx.push(record.chunk_idx);
        subjects.push(record.subject_pseudo.as_str());
        texts.push(record.text_pseudo.as_str());
        participants.push(record.participants_pseudo.as_str());
        received.push(record.received_at.timestamp_micros());
        sent.push(record.sent_at.map(|t| t.timestamp_micros()));
        has_attachments.push(record.has_attachments);
        is_read.push(record.is_read);
        importance.push(record.importance.as_deref());
        metadata.push(record.metadata_json_pseudo.as_str());
        vector_values.extend_from_slice(&record.vector);
        indexed.push(chrono::Utc::now().timestamp_micros());
    }

    let item = std::sync::Arc::new(arrow_schema::Field::new("item", DataType::Float32, true));
    let vector_array = FixedSizeListArray::try_new(
        item,
        dim as i32,
        std::sync::Arc::new(Float32Array::from(vector_values)),
        None,
    )
    .map_err(Error::from)?;

    RecordBatch::try_new(
        outlook_chunks_schema(dim),
        vec![
            std::sync::Arc::new(chunk_id_b.finish()),
            std::sync::Arc::new(message_id_b.finish()),
            std::sync::Arc::new(revision_id_b.finish()),
            std::sync::Arc::new(account_id_b.finish()),
            std::sync::Arc::new(StringArray::from(folder_ids)),
            std::sync::Arc::new(conversation_hash_b.finish()),
            std::sync::Arc::new(internet_hash_b.finish()),
            std::sync::Arc::new(UInt32Array::from(chunk_idx)),
            std::sync::Arc::new(StringArray::from(subjects)),
            std::sync::Arc::new(StringArray::from(texts)),
            std::sync::Arc::new(StringArray::from(participants)),
            std::sync::Arc::new(TimestampMicrosecondArray::from(received)),
            std::sync::Arc::new(TimestampMicrosecondArray::from(sent)),
            std::sync::Arc::new(BooleanArray::from(has_attachments)),
            std::sync::Arc::new(BooleanArray::from(is_read)),
            std::sync::Arc::new(StringArray::from(importance)),
            std::sync::Arc::new(StringArray::from(metadata)),
            std::sync::Arc::new(vector_array),
            std::sync::Arc::new(TimestampMicrosecondArray::from(indexed)),
        ],
    )
    .map_err(Error::from)
}

fn required_col<'a, T: 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    batch
        .column_by_name(name)
        .and_then(|col| col.as_any().downcast_ref::<T>())
        .ok_or_else(|| Error::Outlook(format!("missing or wrong column {name}")))
}
```

- [ ] **Step 4: Run ignored integration test**

Run:

```powershell
cargo test -p anno-rag --test outlook_store outlook_store_upsert_then_fts_search_returns_summary_columns -- --ignored
```

Expected: test passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/store.rs crates/anno-rag/tests/outlook_store.rs
git commit -m "feat: add outlook lancedb upsert and fts search"
```

---

### Task 8: Graph DTO Normalization Without Network

**Files:**
- Create: `crates/anno-rag/src/outlook/normalize.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write normalization test**

Create `crates/anno-rag/src/outlook/normalize.rs`:

```rust
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_message_normalizes_html_body_to_plain_text() {
        let json = r#"{
            "id": "msg-1",
            "changeKey": "ck-1",
            "subject": "Hello",
            "receivedDateTime": "2026-05-26T09:00:00Z",
            "sentDateTime": "2026-05-26T08:59:00Z",
            "hasAttachments": true,
            "isRead": false,
            "importance": "normal",
            "body": { "contentType": "html", "content": "<p>Hello <b>world</b></p>" },
            "from": { "emailAddress": { "name": "Alice", "address": "alice@example.test" } },
            "toRecipients": [
                { "emailAddress": { "name": "Bob", "address": "bob@example.test" } }
            ]
        }"#;
        let raw: GraphMessage = serde_json::from_str(json).expect("json");
        let normalized = normalize_message(&raw, "inbox").expect("normalize");

        assert_eq!(normalized.provider_message_id, "msg-1");
        assert_eq!(normalized.change_key, "ck-1");
        assert_eq!(normalized.subject, "Hello");
        assert!(normalized.body_text.contains("Hello world"));
        assert!(normalized.participants.contains("Alice"));
        assert!(normalized.participants.contains("bob@example.test"));
        assert_eq!(normalized.provider_folder_id, "inbox");
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag graph_message_normalizes_html_body_to_plain_text --lib
```

Expected: compile failure for missing types/functions.

- [ ] **Step 3: Implement Graph DTO and normalizer**

Add above the test module:

```rust
/// Minimal Graph message shape used by the Outlook MVP.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMessage {
    /// Graph message id. Request immutable ids from Graph when fetching.
    pub id: String,
    /// Graph change key.
    pub change_key: String,
    /// Subject.
    #[serde(default)]
    pub subject: String,
    /// Received timestamp.
    pub received_date_time: DateTime<Utc>,
    /// Sent timestamp.
    #[serde(default)]
    pub sent_date_time: Option<DateTime<Utc>>,
    /// Attachment flag.
    #[serde(default)]
    pub has_attachments: bool,
    /// Read flag.
    #[serde(default)]
    pub is_read: bool,
    /// Importance.
    #[serde(default)]
    pub importance: Option<String>,
    /// Body.
    pub body: GraphBody,
    /// Sender.
    #[serde(default)]
    pub from: Option<GraphRecipient>,
    /// To recipients.
    #[serde(default)]
    pub to_recipients: Vec<GraphRecipient>,
}

/// Graph message body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphBody {
    /// Body content type.
    pub content_type: String,
    /// Body content.
    pub content: String,
}

/// Graph recipient wrapper.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRecipient {
    /// Email address.
    pub email_address: GraphEmailAddress,
}

/// Graph email address.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphEmailAddress {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Address.
    #[serde(default)]
    pub address: String,
}

/// Normalized message before privacy processing.
#[derive(Debug, Clone)]
pub struct NormalizedOutlookMessage {
    /// Provider folder id.
    pub provider_folder_id: String,
    /// Provider message id.
    pub provider_message_id: String,
    /// Change key.
    pub change_key: String,
    /// Subject.
    pub subject: String,
    /// Body text.
    pub body_text: String,
    /// Participants text.
    pub participants: String,
    /// Received timestamp.
    pub received_at: DateTime<Utc>,
    /// Sent timestamp.
    pub sent_at: Option<DateTime<Utc>>,
    /// Attachment flag.
    pub has_attachments: bool,
    /// Read flag.
    pub is_read: bool,
    /// Importance.
    pub importance: Option<String>,
}

/// Normalize a Graph message into Anno's Outlook object shape.
///
/// # Errors
/// Returns [`Error::Outlook`] when the body content type is unsupported.
pub fn normalize_message(raw: &GraphMessage, provider_folder_id: &str) -> Result<NormalizedOutlookMessage> {
    let body_text = match raw.body.content_type.as_str() {
        "html" | "HTML" => html_to_text(&raw.body.content),
        "text" | "Text" => raw.body.content.clone(),
        other => {
            return Err(Error::Outlook(format!(
                "unsupported outlook body content type {other}"
            )))
        }
    };
    let mut participants = Vec::new();
    if let Some(from) = &raw.from {
        participants.push(format_address(&from.email_address));
    }
    participants.extend(raw.to_recipients.iter().map(|r| format_address(&r.email_address)));
    Ok(NormalizedOutlookMessage {
        provider_folder_id: provider_folder_id.to_string(),
        provider_message_id: raw.id.clone(),
        change_key: raw.change_key.clone(),
        subject: raw.subject.clone(),
        body_text,
        participants: participants.join("; "),
        received_at: raw.received_date_time,
        sent_at: raw.sent_date_time,
        has_attachments: raw.has_attachments,
        is_read: raw.is_read,
        importance: raw.importance.clone(),
    })
}

fn format_address(addr: &GraphEmailAddress) -> String {
    if addr.name.is_empty() {
        addr.address.clone()
    } else {
        format!("{} <{}>", addr.name, addr.address)
    }
}

fn html_to_text(html: &str) -> String {
    let without_tags = regex::Regex::new(r"<[^>]+>")
        .expect("static html tag regex")
        .replace_all(html, " ");
    without_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
```

In `mod.rs`, add:

```rust
pub mod normalize;
```

- [ ] **Step 4: Run test**

Run:

```powershell
cargo test -p anno-rag graph_message_normalizes_html_body_to_plain_text --lib
```

Expected: test passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/normalize.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: normalize outlook graph messages"
```

---

### Task 9: Outlook Service Model-Free Status

**Files:**
- Create: `crates/anno-rag/src/outlook/service.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write service status test**

Create `crates/anno-rag/src/outlook/service.rs`:

```rust
use crate::config::AnnoRagConfig;
use crate::error::Result;
use crate::outlook::ledger::OutlookLedger;
use crate::outlook::types::OutlookStatus;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn service_status_does_not_require_models_or_pipeline() {
        let tmp = tempdir().expect("tempdir");
        let cfg = AnnoRagConfig {
            data_dir: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let service = OutlookService::open(cfg).expect("service");
        let status = service.status().expect("status");

        assert_eq!(status.account_count, 0);
        assert_eq!(status.pending_jobs, 0);
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag service_status_does_not_require_models_or_pipeline --lib
```

Expected: compile failure for missing `OutlookService`.

- [ ] **Step 3: Implement service shell**

Add above the test module:

```rust
/// Outlook orchestration service.
pub struct OutlookService {
    ledger: OutlookLedger,
}

impl OutlookService {
    /// Open Outlook service without initializing detector, vault, embedder, or LanceDB.
    ///
    /// # Errors
    /// Returns ledger initialization errors.
    pub fn open(cfg: AnnoRagConfig) -> Result<Self> {
        let ledger = OutlookLedger::open(&cfg)?;
        Ok(Self { ledger })
    }

    /// Model-free Outlook status.
    ///
    /// # Errors
    /// Returns ledger query errors.
    pub fn status(&self) -> Result<OutlookStatus> {
        self.ledger.status()
    }
}
```

In `mod.rs`, add:

```rust
pub mod service;
```

- [ ] **Step 4: Run test and check**

Run:

```powershell
cargo test -p anno-rag service_status_does_not_require_models_or_pipeline --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: test passes, check passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/service.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: add model-free outlook service status"
```

---

### Task 10: MCP Lazy Outlook Subsystem and `outlook_status`

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Write MCP lazy test**

Add this test to `crates/anno-rag-mcp/src/lib.rs` test module:

```rust
#[tokio::test]
async fn outlook_status_works_without_pipeline_models() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = anno_rag::config::AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [7u8; 32]);

    let body = server.outlook_status().await;

    assert!(body.contains("\"account_count\":0"), "{body}");
    assert!(server.pipeline_arc().is_none(), "outlook_status must not initialize Pipeline");
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-mcp outlook_status_works_without_pipeline_models --lib
```

Expected: compile failure for missing `outlook_status` and server field.

- [ ] **Step 3: Add `OutlookService` OnceCell to MCP server**

Modify `AnnoRagServer`:

```rust
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    outlook: Arc<OnceCell<Arc<anno_rag::outlook::service::OutlookService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}
```

In both constructors, set:

```rust
outlook: Arc::new(OnceCell::new()),
```

Add helper:

```rust
async fn outlook(&self) -> anno_rag::error::Result<&anno_rag::outlook::service::OutlookService> {
    self.outlook
        .get_or_try_init(|| {
            let cfg = Arc::clone(&self.cfg);
            async move {
                anno_rag::outlook::service::OutlookService::open((*cfg).clone())
                    .map(Arc::new)
            }
        })
        .await
        .map(|arc| arc.as_ref())
}
```

Add tool method inside the `#[tool_router] impl AnnoRagServer` block:

```rust
#[tool(description = "Return Outlook sync and indexing status without loading model-backed pipeline state.")]
async fn outlook_status(&self) -> String {
    match self.outlook().await.and_then(|svc| svc.status()) {
        Ok(status) => serde_json::to_string(&status)
            .unwrap_or_else(|e| format!(r#"{{"error":"json: {e}"}}"#)),
        Err(e) => format!(r#"{{"error":"{e}"}}"#),
    }
}
```

In `health.rs`, add `"outlook_status"` to the hardcoded tool list.

- [ ] **Step 4: Run MCP test and check**

Run:

```powershell
cargo test -p anno-rag-mcp outlook_status_works_without_pipeline_models --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: test passes, check passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs
git commit -m "feat: expose model-free outlook status tool"
```

---

### Task 11: Fake Graph Client and Page-Budgeted Sync

**Files:**
- Create: `crates/anno-rag/src/outlook/graph.rs`
- Modify: `crates/anno-rag/src/outlook/service.rs`
- Modify: `crates/anno-rag/src/outlook/mod.rs`

- [ ] **Step 1: Write fake sync test**

Add to `service.rs` tests:

```rust
#[tokio::test]
async fn sync_with_fake_graph_commits_messages_and_pending_jobs() {
    use crate::outlook::graph::FakeOutlookGraphClient;

    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let mut service = OutlookService::open(cfg).expect("service");
    let graph = FakeOutlookGraphClient::with_single_message("msg-1", "ck-1", "Alpha");

    let report = service
        .sync_folder_with_client(&graph, "tenant", "subject", "inbox", 10)
        .await
        .expect("sync");

    assert_eq!(report.seen, 1);
    assert_eq!(service.status().expect("status").pending_jobs, 1);
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag sync_with_fake_graph_commits_messages_and_pending_jobs --lib
```

Expected: compile failure for missing graph client and sync method.

- [ ] **Step 3: Implement graph trait, fake client, and service sync**

Create `graph.rs`:

```rust
use crate::error::Result;
use crate::outlook::normalize::GraphMessage;
use async_trait::async_trait;

/// One page of Outlook Graph messages.
#[derive(Debug, Clone)]
pub struct OutlookGraphPage {
    /// Messages.
    pub messages: Vec<GraphMessage>,
    /// Next page cursor.
    pub next_link: Option<String>,
    /// Delta cursor at page completion.
    pub delta_link: Option<String>,
}

/// Outlook Graph client boundary.
#[async_trait]
pub trait OutlookGraphClient: Send + Sync {
    /// Fetch one folder delta page.
    async fn delta_page(
        &self,
        folder_id: &str,
        cursor: Option<&str>,
        page_budget: usize,
    ) -> Result<OutlookGraphPage>;
}

/// Test fake Graph client.
pub struct FakeOutlookGraphClient {
    page: OutlookGraphPage,
}

impl FakeOutlookGraphClient {
    /// Build a fake with one text message.
    #[must_use]
    pub fn with_single_message(id: &str, change_key: &str, subject: &str) -> Self {
        let json = serde_json::json!({
            "id": id,
            "changeKey": change_key,
            "subject": subject,
            "receivedDateTime": "2026-05-26T09:00:00Z",
            "sentDateTime": "2026-05-26T08:59:00Z",
            "hasAttachments": false,
            "isRead": true,
            "importance": "normal",
            "body": { "contentType": "text", "content": format!("{subject} body") },
            "from": { "emailAddress": { "name": "Alice", "address": "alice@example.test" } },
            "toRecipients": []
        });
        let msg: GraphMessage = serde_json::from_value(json).expect("fake message json");
        Self {
            page: OutlookGraphPage {
                messages: vec![msg],
                next_link: None,
                delta_link: Some("delta-fake".to_string()),
            },
        }
    }
}

#[async_trait]
impl OutlookGraphClient for FakeOutlookGraphClient {
    async fn delta_page(
        &self,
        _folder_id: &str,
        _cursor: Option<&str>,
        _page_budget: usize,
    ) -> Result<OutlookGraphPage> {
        Ok(self.page.clone())
    }
}
```

In `mod.rs`, add:

```rust
pub mod graph;
```

In `service.rs`, add:

```rust
use crate::outlook::graph::OutlookGraphClient;
use crate::outlook::ids;
use crate::outlook::normalize::normalize_message;
use crate::outlook::types::{OutlookMessageMeta, OutlookSyncBatch};
use sha2::{Digest, Sha256};

/// Sync report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlookSyncReport {
    /// Messages seen.
    pub seen: usize,
}
```

Add method:

```rust
/// Sync one Outlook folder using an injected Graph client.
///
/// # Errors
/// Returns normalization or ledger errors.
pub async fn sync_folder_with_client<C: OutlookGraphClient>(
    &mut self,
    client: &C,
    tenant_id: &str,
    provider_subject: &str,
    provider_folder_id: &str,
    page_budget: usize,
) -> Result<OutlookSyncReport> {
    let account_id = ids::account_id(Some(tenant_id), provider_subject);
    let page = client
        .delta_page(provider_folder_id, None, page_budget)
        .await?;
    let mut messages = Vec::with_capacity(page.messages.len());
    for raw in &page.messages {
        let normalized = normalize_message(raw, provider_folder_id)?;
        let message_id = ids::message_id(account_id, &normalized.provider_message_id);
        let content_hash = Sha256::digest(format!(
            "{}\n{}\n{}",
            normalized.subject, normalized.body_text, normalized.participants
        ));
        let revision_id = ids::revision_id(message_id, &normalized.change_key, &content_hash);
        messages.push(OutlookMessageMeta {
            message_id,
            revision_id,
            account_id,
            provider_folder_id: provider_folder_id.to_string(),
            provider_message_id: normalized.provider_message_id,
            change_key: normalized.change_key,
            subject_pseudo: normalized.subject,
            received_at: normalized.received_at,
            sent_at: normalized.sent_at,
            has_attachments: normalized.has_attachments,
            is_read: normalized.is_read,
            importance: normalized.importance,
            deleted: false,
        });
    }
    self.ledger.commit_sync_batch(&OutlookSyncBatch {
        account_id,
        provider_folder_id: provider_folder_id.to_string(),
        cursor_before: None,
        cursor_after: page.delta_link.unwrap_or_else(|| "delta-unknown".to_string()),
        messages,
        deleted_provider_message_ids: vec![],
    })?;
    Ok(OutlookSyncReport {
        seen: page.messages.len(),
    })
}
```

- [ ] **Step 4: Run sync test**

Run:

```powershell
cargo test -p anno-rag sync_with_fake_graph_commits_messages_and_pending_jobs --lib
```

Expected: test passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag/src/outlook/graph.rs crates/anno-rag/src/outlook/service.rs crates/anno-rag/src/outlook/mod.rs
git commit -m "feat: sync outlook pages through fake graph client"
```

---

### Task 12: Pipeline Outlook Indexing Entry Point

**Files:**
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/outlook/types.rs`
- Modify: `crates/anno-rag/src/outlook/store.rs`

- [ ] **Step 1: Run impact analysis before editing `Pipeline`**

Use GitNexus MCP if available:

```text
gitnexus_impact({target: "Pipeline", direction: "upstream"})
```

Record the direct callers in the task notes. If the index is stale and MCP is unavailable, run:

```powershell
npx gitnexus analyze
```

Then retry the impact query.

- [ ] **Step 2: Write a unit test for chunk planning without models**

Add to `types.rs`:

```rust
/// Raw normalized Outlook message for privacy/indexing.
#[derive(Debug, Clone)]
pub struct OutlookIndexInput {
    /// Message metadata.
    pub meta: OutlookMessageMeta,
    /// Raw subject.
    pub subject: String,
    /// Raw body text.
    pub body_text: String,
    /// Raw participants.
    pub participants: String,
}
```

Add to `pipeline.rs` tests:

```rust
#[test]
fn outlook_chunk_planner_splits_body_with_configured_window() {
    let chunks = plan_outlook_chunks("a ".repeat(3000).as_str(), 1024, 128);
    assert!(chunks.len() >= 3);
    assert_eq!(chunks[0].idx, 0);
    assert!(chunks[0].text.len() <= 1024);
}
```

- [ ] **Step 3: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag outlook_chunk_planner_splits_body_with_configured_window --lib
```

Expected: compile failure for missing planner.

- [ ] **Step 4: Implement chunk planning and pipeline method**

Add near `IngestOutcome` in `pipeline.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedOutlookChunk {
    idx: u32,
    text: String,
    char_start: u32,
    char_end: u32,
}

fn plan_outlook_chunks(text: &str, max_chars: usize, overlap: usize) -> Vec<PlannedOutlookChunk> {
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let step = max_chars.saturating_sub(overlap).max(1);
    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        out.push(PlannedOutlookChunk {
            idx: out.len() as u32,
            text: chars[start..end].iter().collect(),
            char_start: start as u32,
            char_end: end as u32,
        });
        if end == chars.len() {
            break;
        }
        start += step;
    }
    out
}
```

Add a public method to `impl Pipeline`:

```rust
/// Privacy-process and embed an Outlook message into pseudonymized chunk records.
///
/// # Errors
/// Returns detector, vault, or embedder errors.
pub async fn prepare_outlook_chunks(
    &self,
    input: &crate::outlook::types::OutlookIndexInput,
) -> Result<Vec<crate::outlook::types::OutlookChunkRecord>> {
    let chunks = plan_outlook_chunks(
        &input.body_text,
        self.cfg.chunk_max_chars,
        self.cfg.chunk_overlap,
    );
    let detector = self.detector_get_or_init()?;
    let mut pseudo_texts = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        let ents = detector.detect(&chunk.text)?;
        pseudo_texts.push(self.vault.pseudonymize(&chunk.text, &ents).await?);
    }
    let vectors = self.embedder().await?.embed_batch(&pseudo_texts)?;
    let subject_ents = detector.detect(&input.subject)?;
    let subject_pseudo = self.vault.pseudonymize(&input.subject, &subject_ents).await?;
    let participants_ents = detector.detect(&input.participants)?;
    let participants_pseudo = self
        .vault
        .pseudonymize(&input.participants, &participants_ents)
        .await?;

    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| crate::outlook::types::OutlookChunkRecord {
            chunk_id: crate::outlook::ids::chunk_id(input.meta.revision_id, idx as u32),
            message_id: input.meta.message_id,
            revision_id: input.meta.revision_id,
            account_id: input.meta.account_id,
            folder_id: input.meta.provider_folder_id.clone(),
            chunk_idx: chunk.idx,
            subject_pseudo: subject_pseudo.clone(),
            text_pseudo: pseudo_texts[idx].clone(),
            participants_pseudo: participants_pseudo.clone(),
            received_at: input.meta.received_at,
            sent_at: input.meta.sent_at,
            has_attachments: input.meta.has_attachments,
            is_read: input.meta.is_read,
            importance: input.meta.importance.clone(),
            metadata_json_pseudo: "{}".to_string(),
            vector: vectors[idx].clone(),
        })
        .collect())
}
```

- [ ] **Step 5: Run tests and check**

Run:

```powershell
cargo test -p anno-rag outlook_chunk_planner_splits_body_with_configured_window --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: test passes, check passes.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag/src/pipeline.rs crates/anno-rag/src/outlook/types.rs crates/anno-rag/src/outlook/store.rs
git commit -m "feat: prepare outlook chunks through privacy pipeline"
```

---

### Task 13: MCP `outlook_search` FTS-Only Surface

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Write MCP parameter and no-query test**

Add test to `lib.rs` tests:

```rust
#[tokio::test]
async fn outlook_search_rejects_empty_query_without_pipeline_init() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = anno_rag::config::AnnoRagConfig {
        data_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [7u8; 32]);

    let body = server
        .outlook_search(Parameters(OutlookSearchParams {
            query: String::new(),
            top_k: 5,
        }))
        .await;

    assert!(body.contains("query must not be empty"), "{body}");
    assert!(server.pipeline_arc().is_none());
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
cargo test -p anno-rag-mcp outlook_search_rejects_empty_query_without_pipeline_init --lib
```

Expected: compile failure for missing params/tool.

- [ ] **Step 3: Add FTS-only search tool shape**

Add params:

```rust
/// Parameters for `outlook_search`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct OutlookSearchParams {
    /// Query text.
    pub query: String,
    /// Number of results.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}
```

Add tool method:

```rust
#[tool(description = "Search indexed Outlook mail. Uses FTS-only mode until semantic Outlook indexing is enabled.")]
async fn outlook_search(&self, Parameters(params): Parameters<OutlookSearchParams>) -> String {
    if params.query.trim().is_empty() {
        return r#"{"error":"query must not be empty"}"#.to_string();
    }
    let limit = params.top_k.clamp(1, 50);
    match anno_rag::outlook::store::OutlookStore::open(&self.cfg).await {
        Ok(store) => match store.search_fts(&params.query, limit).await {
            Ok(hits) => serde_json::to_string(&hits.len())
                .map(|count| format!(r#"{{"count":{count}}}"#))
                .unwrap_or_else(|e| format!(r#"{{"error":"json: {e}"}}"#)),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        },
        Err(e) => format!(r#"{{"error":"{e}"}}"#),
    }
}
```

In `health.rs`, add `"outlook_search"`.

- [ ] **Step 4: Run test and check**

Run:

```powershell
cargo test -p anno-rag-mcp outlook_search_rejects_empty_query_without_pipeline_init --lib
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: test passes, check passes.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs
git commit -m "feat: add outlook search mcp surface"
```

---

### Task 14: Verification and Documentation Pass

**Files:**
- Modify: `docs/superpowers/specs/2026-05-26-anno-outlook-lance-mvp-design.md`
- Modify: `docs/superpowers/plans/2026-05-26-anno-outlook-lance-mvp-implementation.md`

- [ ] **Step 1: Run focused tests**

Run:

```powershell
cargo test -p anno-rag outlook::ids --lib
cargo test -p anno-rag outlook::types --lib
cargo test -p anno-rag outlook::ledger --lib
cargo test -p anno-rag graph_message_normalizes_html_body_to_plain_text --lib
cargo test -p anno-rag service_status_does_not_require_models_or_pipeline --lib
cargo test -p anno-rag sync_with_fake_graph_commits_messages_and_pending_jobs --lib
cargo test -p anno-rag-mcp outlook_status_works_without_pipeline_models --lib
cargo test -p anno-rag-mcp outlook_search_rejects_empty_query_without_pipeline_init --lib
```

Expected: all focused tests pass.

- [ ] **Step 2: Run ignored Lance integration**

Run:

```powershell
cargo test -p anno-rag --test outlook_store outlook_store_upsert_then_fts_search_returns_summary_columns -- --ignored
```

Expected: integration test passes. If LanceDB table creation is slow, record the elapsed time in the implementation notes.

- [ ] **Step 3: Run targeted package checks**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: both checks pass.

- [ ] **Step 4: Run GitNexus change detection before any final commit**

Use GitNexus MCP if available:

```text
gitnexus_detect_changes({scope: "all"})
```

Confirm the changed symbols are limited to Outlook modules, `AnnoRagConfig`, `Error`, `Pipeline` Outlook entrypoint, and `AnnoRagServer` Outlook tools.

- [ ] **Step 5: Commit docs and final verification notes**

```powershell
git add docs/superpowers/specs/2026-05-26-anno-outlook-lance-mvp-design.md docs/superpowers/plans/2026-05-26-anno-outlook-lance-mvp-implementation.md
git commit -m "docs: plan outlook lance mvp implementation"
```

---

## Self-Review

Spec coverage:

- Outlook-only MVP: covered by file structure and Tasks 1-13.
- SQLite sync correctness: Tasks 4-5.
- LanceDB search projection: Tasks 6-7 and Task 13.
- Model-free MCP status: Tasks 9-10.
- No existing local/legal ingest rewrite: execution constraints and Task 12 only add a parallel Outlook preparation path.
- LanceDB performance rules: Task 7 creates table/search; Task 14 preserves benchmarks as verification work. Full vector index benchmark is intentionally after FTS vertical slice because it needs populated corpora.

Known deferred work after this plan:

- Real Microsoft OAuth/PKCE browser connect.
- Real `reqwest` Graph client with retry and `Retry-After`.
- Vector/hybrid Outlook search after corpus benchmarks choose the index type.
- Disconnect/forget with aggressive Lance cleanup.
- Attachments beyond metadata.

These are not required for the first testable Outlook Lance vertical slice.
