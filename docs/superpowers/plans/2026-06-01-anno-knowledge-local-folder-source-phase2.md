# Knowledge Local Folder Source (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a discovery-only local-folder `SourceConnector` plus a service-layer indexer so `knowledge_sync` walks a folder, extracts with Kreuzberg, pseudonymizes through Anno's local PII pipeline, and writes chunk-level FTS rows — making `knowledge_search(mode=fast)` return real pseudonymized hits.

**Architecture:** New `anno-source-local` crate (core-only, no models, no Kreuzberg) does folder walk + content-hash change detection. A narrow `Pipeline::pseudonymize_knowledge_object` API (anno-rag) reuses the existing detector + vault without embedding or legal enrichment. A `KnowledgeIndexer` in `anno-rag-mcp` orchestrates discovery → `ingest::extract` → pseudonymize → SQLite/FTS write in one synchronous bounded `knowledge_sync` call with a resumable per-object state machine.

**Tech Stack:** Rust workspace, `rusqlite` (bundled SQLite + FTS5), `sha2`, `walkdir`, `uuid` v5, `chrono`, `serde`/`serde_json`, `rmcp`, existing `anno-rag` `Pipeline`/`detector`/`vault`/`ingest::extract`.

**Spec:** [`docs/superpowers/specs/2026-06-01-anno-knowledge-local-folder-source-phase2-design.md`](../specs/2026-06-01-anno-knowledge-local-folder-source-phase2-design.md)

**Build/test commands (respect build-isolation rules in CLAUDE.md):**
```powershell
# ALWAYS first — refuse to build if cargo/rustc already running:
Get-Process cargo,rustc -ErrorAction SilentlyContinue

# Check only (fast, no link):
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package <crate> -Mode check -Profile dev-fast

# Unit tests for one crate:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package <crate>

# NEVER cargo test/build --workspace locally.
```

---

## Prerequisite

This plan depends on Phase 1 being merged (the `anno-knowledge-core` and `anno-knowledge-store` crates, SQLite/FTS migrations, and the lazy `KnowledgeService` cell on `AnnoRagServer`). Phase 1 plan: [`2026-05-29-anno-local-knowledge-service-phase1.md`](2026-05-29-anno-local-knowledge-service-phase1.md). Task 0 verifies this before any other work.

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/anno-source-local/Cargo.toml` | Create | Crate metadata; depends on `anno-knowledge-core`, `sha2`, `walkdir`, `chrono`, `serde_json`. |
| `crates/anno-source-local/src/lib.rs` | Create | Public exports. |
| `crates/anno-source-local/src/error.rs` | Create | Connector errors. |
| `crates/anno-source-local/src/folder.rs` | Create | `LocalFolderSource`, `DiscoveredObject`, walk + filters + content-hash change detection + budget. |
| `Cargo.toml` (root) | Modify | Add `crates/anno-source-local` to `workspace.members`. |
| `crates/anno-knowledge-store/src/control_store.rs` | Modify | Real source/account/scope CRUD; object/revision/part/chunk commit; state transitions; forget cascade; real status. |
| `crates/anno-knowledge-store/src/lib.rs` | Modify | Export new public types. |
| `crates/anno-rag/Cargo.toml` | Modify | Add `anno-knowledge-core` dependency. |
| `crates/anno-rag/src/knowledge_privacy.rs` | Create | `PrivacyIndexInput`, `PseudonymizedChunk`, `Pipeline::pseudonymize_knowledge_object`. |
| `crates/anno-rag/src/pipeline.rs` | Modify | Make `vault` field and `detector_get_or_init` `pub(crate)` so the sibling `knowledge_privacy` module can reach them. |
| `crates/anno-rag/src/lib.rs` | Modify | `pub mod knowledge_privacy;`. |
| `crates/anno-rag-mcp/Cargo.toml` | Modify | Add `anno-source-local`. |
| `crates/anno-rag-mcp/src/indexer.rs` | Create | `KnowledgeIndexer` — sync orchestration, budget loop, state machine, deletion reconciliation. |
| `crates/anno-rag-mcp/src/knowledge.rs` | Modify | `KnowledgeService`: real `sources`/`status`/`search`, add `add_local_folder`/`sync`/`forget`. |
| `crates/anno-rag-mcp/src/lib.rs` | Modify | New MCP tools + `mod indexer;`. |
| `crates/anno-rag-mcp/src/health.rs` | Modify | Add three tool names. |

---

## Task 0: Pre-Flight And Impact Checks

**Files:** none (verification only)

- [ ] **Step 1: Confirm Phase 1 is merged**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-knowledge-store -Mode check -Profile dev-fast
```
Expected: PASS. If the crate does not exist, STOP — Phase 1 must land first. Report to the user.

- [ ] **Step 2: Verify no build already running, then GitNexus freshness**

Run:
```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
npx gitnexus status
```
Expected: no stray cargo/rustc; index present (run `npx gitnexus analyze` if stale).

- [ ] **Step 3: Impact analysis on the symbols this plan edits**

Run:
```powershell
npx gitnexus impact --repo anno Pipeline --direction upstream
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
```
Expected: record blast radius. We only ADD a method to `Pipeline` and ADD a field/tools to `AnnoRagServer` — no existing signatures change. If impact reports that adding an inherent method is HIGH/CRITICAL (it should not be), warn the user before proceeding.

---

## Task 1: `anno-source-local` Crate — Discovery + Change Detection

**Files:**
- Modify: `Cargo.toml` (root)
- Create: `crates/anno-source-local/Cargo.toml`
- Create: `crates/anno-source-local/src/lib.rs`
- Create: `crates/anno-source-local/src/error.rs`
- Create: `crates/anno-source-local/src/folder.rs`

- [ ] **Step 1: Add the crate to the workspace**

In root `Cargo.toml`, add to `workspace.members` (keep existing members; insert before `"workspace-hack"`):
```toml
    "crates/anno-source-local",
```

Create `crates/anno-source-local/Cargo.toml`:
```toml
[package]
name = "anno-source-local"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Local folder source connector (discovery + change detection) for Anno knowledge indexing."

[dependencies]
anno-knowledge-core = { path = "../anno-knowledge-core" }
chrono = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
thiserror = { workspace = true }
walkdir = { workspace = true }

[dev-dependencies]
tempfile = "3.27.0"

[lints]
workspace = true
```

Create `crates/anno-source-local/src/error.rs`:
```rust
//! Errors for the local folder source connector.

/// Result type for the local source connector.
pub type Result<T> = std::result::Result<T, LocalSourceError>;

/// Errors raised while discovering local folder objects.
#[derive(Debug, thiserror::Error)]
pub enum LocalSourceError {
    /// Filesystem IO failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The configured folder path does not exist or is not a directory.
    #[error("not a directory: {0}")]
    NotADirectory(String),
}
```

Create `crates/anno-source-local/src/lib.rs`:
```rust
//! Local folder source connector for Anno knowledge indexing.
//!
//! This crate is pure discovery + change detection. It does NOT load models,
//! call Kreuzberg, or touch SQLite. It depends only on `anno-knowledge-core`.

pub mod error;
pub mod folder;

pub use error::{LocalSourceError, Result};
pub use folder::{DiscoverBudget, DiscoveredObject, LocalFolderSource};
```

- [ ] **Step 2: Write failing tests for discovery + change detection**

Create `crates/anno-source-local/src/folder.rs` with the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).expect("write fixture");
        p
    }

    #[test]
    fn discovers_supported_files_and_skips_unsupported() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "a.txt", b"hello world");
        write(dir.path(), "b.md", b"# title\nbody");
        write(dir.path(), "ignore.bin", b"\x00\x01\x02");

        let src = LocalFolderSource::new(dir.path());
        let objects = src.discover(&DiscoverBudget::default()).expect("discover");

        let names: Vec<String> = objects
            .iter()
            .map(|o| o.external_id.rsplit(['/', '\\']).next().unwrap().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "a.txt"));
        assert!(names.iter().any(|n| n == "b.md"));
        assert!(!names.iter().any(|n| n == "ignore.bin"));
    }

    #[test]
    fn content_hash_is_stable_for_same_bytes_and_changes_on_edit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(dir.path(), "a.txt", b"version one");
        let src = LocalFolderSource::new(dir.path());

        let first = src.discover(&DiscoverBudget::default()).expect("discover");
        let h1 = first[0].content_hash;

        // Touch only: rewrite identical bytes -> same hash.
        fs::write(&p, b"version one").expect("rewrite");
        let second = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_eq!(second[0].content_hash, h1);

        // Real edit -> different hash.
        fs::write(&p, b"version two!!").expect("edit");
        let third = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_ne!(third[0].content_hash, h1);
    }

    #[test]
    fn budget_caps_file_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..10 {
            write(dir.path(), &format!("f{i}.txt"), b"x");
        }
        let src = LocalFolderSource::new(dir.path());
        let budget = DiscoverBudget { max_files: 3, max_total_bytes: u64::MAX };
        let objects = src.discover(&budget).expect("discover");
        assert_eq!(objects.len(), 3);
    }

    #[test]
    fn external_id_is_canonical_and_stable() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "a.txt", b"x");
        let src = LocalFolderSource::new(dir.path());
        let a = src.discover(&DiscoverBudget::default()).expect("discover");
        let b = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_eq!(a[0].external_id, b[0].external_id);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-source-local
```
Expected: FAIL — `LocalFolderSource` / `DiscoveredObject` / `DiscoverBudget` not defined.

- [ ] **Step 4: Implement the connector**

Prepend to `crates/anno-source-local/src/folder.rs` (above the test module):
```rust
//! Local folder discovery and content-hash change detection.

use crate::error::{LocalSourceError, Result};
use anno_knowledge_core::ObjectType;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Supported file extensions for the local folder source (lowercase, no dot).
const SUPPORTED_EXTS: &[&str] = &[
    "txt", "md", "markdown", "pdf", "doc", "docx", "rtf", "odt", "html", "htm",
    "csv", "tsv", "xlsx", "xls", "pptx", "ppt", "eml", "msg", "json", "xml",
];

/// Per-run discovery budget. Bounds work so a single `knowledge_sync` call
/// returns promptly on large folders; remaining files are picked up next run.
#[derive(Debug, Clone)]
pub struct DiscoverBudget {
    /// Maximum number of files returned in one run.
    pub max_files: usize,
    /// Maximum total bytes hashed in one run.
    pub max_total_bytes: u64,
}

impl Default for DiscoverBudget {
    fn default() -> Self {
        Self { max_files: 200, max_total_bytes: 512 * 1024 * 1024 }
    }
}

/// One discovered file. Pure data; the indexer assigns typed IDs and pseudonymizes.
#[derive(Debug, Clone)]
pub struct DiscoveredObject {
    /// Canonical absolute path used as the stable external id.
    pub external_id: String,
    /// Path on disk (same value, typed).
    pub path: PathBuf,
    /// Object family. Always `LocalFile` for this source.
    pub object_type: ObjectType,
    /// SHA-256 of the file bytes. Content-based revision identity.
    pub content_hash: [u8; 32],
    /// Last-modified time.
    pub mtime: DateTime<Utc>,
    /// File size in bytes.
    pub byte_size: u64,
    /// File name (raw; pseudonymized downstream by the indexer).
    pub title_raw: Option<String>,
    /// Raw metadata (raw; pseudonymized downstream): path, ext, size, mtime.
    pub metadata_raw: serde_json::Value,
}

/// A local folder configured as a knowledge source.
#[derive(Debug, Clone)]
pub struct LocalFolderSource {
    root: PathBuf,
}

impl LocalFolderSource {
    /// Create a source rooted at `root`.
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }

    /// Walk the folder and return discovered objects, bounded by `budget`.
    ///
    /// Files are visited in a stable sorted order so budget truncation is
    /// deterministic and `knowledge_sync` resumes predictably.
    ///
    /// # Errors
    /// Returns [`LocalSourceError`] if the root is not a directory or IO fails.
    pub fn discover(&self, budget: &DiscoverBudget) -> Result<Vec<DiscoveredObject>> {
        if !self.root.is_dir() {
            return Err(LocalSourceError::NotADirectory(self.root.display().to_string()));
        }

        let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(walkdir::DirEntry::into_path)
            .filter(|p| is_supported(p))
            .collect();
        paths.sort();

        let mut out = Vec::new();
        let mut bytes_used: u64 = 0;
        for path in paths {
            if out.len() >= budget.max_files {
                break;
            }
            let meta = std::fs::metadata(&path)?;
            let size = meta.len();
            if bytes_used.saturating_add(size) > budget.max_total_bytes && !out.is_empty() {
                break;
            }
            let bytes = std::fs::read(&path)?;
            let content_hash = sha256(&bytes);
            let mtime: DateTime<Utc> = meta.modified().map(DateTime::<Utc>::from).unwrap_or_else(|_| Utc::now());
            let external_id = canonical_id(&path);
            let title_raw = path.file_name().and_then(|s| s.to_str()).map(str::to_string);
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            let metadata_raw = serde_json::json!({
                "path": external_id,
                "ext": ext,
                "size": size,
                "mtime": mtime.to_rfc3339(),
            });

            bytes_used = bytes_used.saturating_add(size);
            out.push(DiscoveredObject {
                external_id: canonical_id(&path),
                path,
                object_type: ObjectType::File,
                content_hash,
                mtime,
                byte_size: size,
                title_raw,
                metadata_raw,
            });
        }
        Ok(out)
    }
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|e| SUPPORTED_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn canonical_id(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}
```

Note: `ObjectType::File` must match the Phase 1 `anno-knowledge-core` `ObjectType` variant name. The Phase 1 plan defines `ObjectType::File`. If Phase 1 shipped a different name (e.g. `LocalFile`), use that variant instead.

- [ ] **Step 5: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-source-local
```
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml crates/anno-source-local
git commit -m "feat: add anno-source-local discovery connector"
```

---

## Task 2: Store — Source / Account / Scope CRUD

**Files:**
- Modify: `crates/anno-knowledge-store/src/control_store.rs`
- Modify: `crates/anno-knowledge-store/src/lib.rs`

- [ ] **Step 1: Write failing CRUD tests**

Add to the `#[cfg(test)] mod tests` block in `control_store.rs`:
```rust
    #[test]
    fn add_local_folder_source_then_list_and_get_scopes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");

        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register");

        let sources = store.list_sources().expect("list");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_id, reg.source_id);

        let scopes = store.enabled_scopes_for_source(&reg.source_id).expect("scopes");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].scope_id, reg.scope_id);
        assert_eq!(scopes[0].provider_key, "C:/docs");
        assert_eq!(scopes[0].account_id, reg.account_id);
    }

    #[test]
    fn register_local_folder_is_idempotent_on_same_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let a = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register a");
        let b = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".to_string(),
                source_label_pseudo: "FOLDER_1".to_string(),
                scope_label_pseudo: "FOLDER_1".to_string(),
                provider_key: "C:/docs".to_string(),
            })
            .expect("register b");
        assert_eq!(a.source_id, b.source_id);
        assert_eq!(a.scope_id, b.scope_id);
        assert_eq!(store.list_sources().expect("list").len(), 1);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: FAIL — `register_local_folder` / `list_sources` / `enabled_scopes_for_source` / types not defined.

- [ ] **Step 3: Add input/output types and CRUD methods**

In `crates/anno-knowledge-store/src/control_store.rs`, add the imports at the top (merge with existing `use` lines):
```rust
use anno_knowledge_core::{
    AccountId, ObjectType, ScopeId, SourceId, SourceKind, SourceKindForId,
};
use chrono::Utc;
```

Add these public types near `TestChunkInput`:
```rust
/// Inputs to register a local folder as a source + synthetic account + scope.
#[derive(Debug, Clone)]
pub struct LocalFolderRegistration {
    /// Stable key for the source id (the folder path).
    pub stable_key: String,
    /// Pseudonymized source display label.
    pub source_label_pseudo: String,
    /// Pseudonymized scope display label.
    pub scope_label_pseudo: String,
    /// Provider key for the scope (the folder path).
    pub provider_key: String,
}

/// Result of registering a local folder.
#[derive(Debug, Clone)]
pub struct LocalFolderRegistered {
    /// New or existing source id.
    pub source_id: SourceId,
    /// New or existing synthetic account id.
    pub account_id: AccountId,
    /// New or existing scope id.
    pub scope_id: ScopeId,
}

/// A configured source row.
#[derive(Debug, Clone)]
pub struct SourceRow {
    /// Source id.
    pub source_id: SourceId,
    /// Source kind (serialized form).
    pub kind: String,
    /// Pseudonymized label.
    pub display_label_pseudo: String,
    /// Whether enabled.
    pub enabled: bool,
}

/// A configured scope row.
#[derive(Debug, Clone)]
pub struct ScopeRow {
    /// Scope id.
    pub scope_id: ScopeId,
    /// Owning account id.
    pub account_id: AccountId,
    /// Provider key (folder path).
    pub provider_key: String,
    /// Pseudonymized label.
    pub display_label_pseudo: String,
    /// Whether enabled.
    pub enabled: bool,
}
```

Add these methods inside `impl KnowledgeControlStore`:
```rust
    /// Register (idempotently) a local folder as source + synthetic account + scope.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn register_local_folder(
        &self,
        reg: LocalFolderRegistration,
    ) -> Result<LocalFolderRegistered> {
        let source_id = SourceId::from_parts(SourceKindForId::LocalFolder, &reg.stable_key);
        let account_id = AccountId::from_parts(source_id, "local");
        let scope_id = ScopeId::from_parts(account_id, &reg.provider_key);
        let now = Utc::now().to_rfc3339();
        let kind = serde_json::to_value(SourceKind::LocalFolder)?
            .as_str()
            .expect("source kind serializes to string")
            .to_string();

        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_sources \
             (source_id, kind, display_label_pseudo, created_at, enabled) \
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![source_id.as_string(), kind, reg.source_label_pseudo, now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_accounts \
             (account_id, source_id, provider_subject, tenant_id, display_label_pseudo, \
              scopes_granted_json, auth_ref, created_at, last_seen_at) \
             VALUES (?1, ?2, 'local', NULL, ?3, '[]', NULL, ?4, NULL)",
            params![account_id.as_string(), source_id.as_string(), reg.source_label_pseudo, now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO source_scopes \
             (scope_id, account_id, kind, provider_key, display_label_pseudo, \
              sync_policy_json, enabled) \
             VALUES (?1, ?2, 'local_folder', ?3, ?4, ?5, 1)",
            params![
                scope_id.as_string(),
                account_id.as_string(),
                reg.provider_key,
                reg.scope_label_pseudo,
                "{\"enabled\":true,\"max_pages_per_run\":5,\"include_attachments\":false}",
            ],
        )?;
        Ok(LocalFolderRegistered { source_id, account_id, scope_id })
    }

    /// List configured sources.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn list_sources(&self) -> Result<Vec<SourceRow>> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT source_id, kind, display_label_pseudo, enabled \
             FROM knowledge_sources ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SourceRow {
                source_id: SourceId::new(parse_uuid(row.get::<_, String>(0)?)?),
                kind: row.get(1)?,
                display_label_pseudo: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Return enabled scopes for a source.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn enabled_scopes_for_source(&self, source_id: &SourceId) -> Result<Vec<ScopeRow>> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT s.scope_id, s.account_id, s.provider_key, s.display_label_pseudo, s.enabled \
             FROM source_scopes s \
             JOIN source_accounts a ON a.account_id = s.account_id \
             WHERE a.source_id = ?1 AND s.enabled = 1 \
             ORDER BY s.provider_key",
        )?;
        let sid = source_id.as_string();
        let rows = stmt.query_map(params![sid], |row| {
            Ok(ScopeRow {
                scope_id: ScopeId::new(parse_uuid(row.get::<_, String>(0)?)?),
                account_id: AccountId::new(parse_uuid(row.get::<_, String>(1)?)?),
                provider_key: row.get(2)?,
                display_label_pseudo: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
```

Note on `ObjectType` import: it is used by Task 3 below; including it here in the shared `use` block is fine (the compiler warns only on a fully-unused import — Task 3 uses it). If a check between tasks complains, add `#[allow(unused_imports)]` temporarily, removed by Task 3.

- [ ] **Step 4: Export new types**

In `crates/anno-knowledge-store/src/lib.rs`, extend the `control_store` re-export:
```rust
pub use control_store::{
    KnowledgeControlStore, LocalFolderRegistered, LocalFolderRegistration, ScopeRow, SourceRow,
    TestChunkInput,
};
```

- [ ] **Step 5: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: PASS including the 2 new CRUD tests.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-knowledge-store/src/control_store.rs crates/anno-knowledge-store/src/lib.rs
git commit -m "feat(knowledge-store): local folder source/account/scope CRUD"
```

---

## Task 3: Store — Object/Revision/Part/Chunk Commit + State Machine

**Files:**
- Modify: `crates/anno-knowledge-store/src/control_store.rs`
- Modify: `crates/anno-knowledge-store/src/lib.rs`

- [ ] **Step 1: Write failing commit + skip tests**

Add to the `#[cfg(test)] mod tests` block in `control_store.rs`:
```rust
    fn sample_chunk(object_id: ObjectId, revision_id: RevisionId, part_id: PartId, idx: u32, body: &str) -> CommitChunk {
        CommitChunk {
            chunk_id: ChunkId::from_parts(revision_id, part_id, idx),
            chunk_idx: idx,
            title_pseudo: Some("Doc FOLDER_1".to_string()),
            text_pseudo: body.to_string(),
            metadata_pseudo_json: "{\"path\":\"FOLDER_1\"}".to_string(),
            char_start: 0,
            char_end: body.len() as u32,
        }
    }

    fn commit_input(scope_id: ScopeId, account_id: AccountId, source_id: SourceId, hash: [u8; 32], body: &str)
        -> CommitObjectInput
    {
        let object_id = ObjectId::from_external(
            anno_knowledge_core::SourceKindForId::LocalFolder, "local", "scope", "C:/docs/a.txt",
        );
        let revision_id = RevisionId::from_parts(&object_id.as_string(), &hex32(&hash));
        let part_id = PartId::from_parts(&object_id.as_string(), "file_body");
        CommitObjectInput {
            object_id,
            source_id,
            account_id,
            scope_id,
            revision_id,
            part_id,
            external_id: "C:/docs/a.txt".to_string(),
            object_type: ObjectType::File,
            provider_version: hex32(&hash),
            title_pseudo: Some("Doc FOLDER_1".to_string()),
            metadata_pseudo_json: "{\"path\":\"FOLDER_1\"}".to_string(),
            source_kind: SourceKind::LocalFolder,
            chunks: vec![sample_chunk(object_id, revision_id, part_id, 0, body)],
        }
    }

    #[test]
    fn commit_object_then_search_and_skip_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(), source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(), provider_key: "C:/docs".into(),
            }).expect("register");

        let hash = [1u8; 32];
        let input = commit_input(reg.scope_id, reg.account_id, reg.source_id, hash, "le contrat FOLDER_1");

        // Not yet indexed at this content hash.
        assert!(!store.revision_is_fts_ready(&input.object_id, &input.provider_version).expect("check"));
        store.commit_object(&input).expect("commit");
        assert!(store.revision_is_fts_ready(&input.object_id, &input.provider_version).expect("check"));

        let hits = store
            .search_fast(&anno_knowledge_core::KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");
        assert_eq!(hits.len(), 1);

        let status = store.status().expect("status");
        assert_eq!(status.objects, 1);
        assert_eq!(status.chunks, 1);
    }

    #[test]
    fn recommit_with_new_revision_replaces_chunks_no_duplicates() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(), source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(), provider_key: "C:/docs".into(),
            }).expect("register");

        let v1 = commit_input(reg.scope_id, reg.account_id, reg.source_id, [1u8; 32], "version une FOLDER_1");
        store.commit_object(&v1).expect("commit v1");
        let v2 = commit_input(reg.scope_id, reg.account_id, reg.source_id, [2u8; 32], "version deux FOLDER_1");
        store.commit_object(&v2).expect("commit v2");

        // Same object -> still one object, one current chunk, no FTS duplicates.
        let status = store.status().expect("status");
        assert_eq!(status.objects, 1);
        assert_eq!(status.chunks, 1);
        let hits = store
            .search_fast(&anno_knowledge_core::KnowledgeSearchRequest::new("version").with_top_k(5))
            .expect("search");
        assert_eq!(hits.len(), 1);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: FAIL — `CommitObjectInput`, `CommitChunk`, `commit_object`, `revision_is_fts_ready`, `hex32` not defined.

- [ ] **Step 3: Add commit types and a hex helper**

In `control_store.rs`, extend the core import to add `ChunkId, ObjectId, PartId, RevisionId` (merge with the import added in Task 2) and add types near `TestChunkInput`:
```rust
/// One pseudonymized chunk to commit.
#[derive(Debug, Clone)]
pub struct CommitChunk {
    /// Chunk id.
    pub chunk_id: ChunkId,
    /// Chunk index within the part.
    pub chunk_idx: u32,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized body.
    pub text_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

/// All rows for committing one object revision in a single transaction.
#[derive(Debug, Clone)]
pub struct CommitObjectInput {
    /// Object id.
    pub object_id: ObjectId,
    /// Source id.
    pub source_id: SourceId,
    /// Account id.
    pub account_id: AccountId,
    /// Scope id.
    pub scope_id: ScopeId,
    /// Revision id (content-based).
    pub revision_id: RevisionId,
    /// Part id (single FileBody part).
    pub part_id: PartId,
    /// External id (canonical path).
    pub external_id: String,
    /// Object family.
    pub object_type: ObjectType,
    /// Provider version (hex of content hash).
    pub provider_version: String,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Source kind.
    pub source_kind: SourceKind,
    /// Pseudonymized chunks.
    pub chunks: Vec<CommitChunk>,
}

/// Lowercase hex of a 32-byte hash.
#[must_use]
pub fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
```

- [ ] **Step 4: Implement `commit_object` and `revision_is_fts_ready`**

Add to `impl KnowledgeControlStore`:
```rust
    /// True when the object already has an `fts_ready` revision at this provider version.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn revision_is_fts_ready(&self, object_id: &ObjectId, provider_version: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_objects o \
             JOIN knowledge_revisions r ON r.object_id = o.object_id \
             WHERE o.object_id = ?1 AND r.provider_version = ?2 AND o.state = 'fts_ready'",
            params![object_id.as_string(), provider_version],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Commit one object revision: upsert object/revision/part, replace chunks
    /// and FTS rows, set state to `fts_ready`. One transaction.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn commit_object(&self, input: &CommitObjectInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let source_kind = serde_json::to_value(input.source_kind)?
            .as_str().expect("source kind serializes to string").to_string();
        let object_type = serde_json::to_value(input.object_type)?
            .as_str().expect("object type serializes to string").to_string();
        let oid = input.object_id.as_string();
        let rid = input.revision_id.as_string();
        let pid = input.part_id.as_string();

        let mut conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO knowledge_objects \
             (object_id, source_id, account_id, scope_id, external_id, object_type, \
              title_pseudo, metadata_pseudo_json, source_url_policy, source_updated_at, state, last_error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, 'fts_ready', NULL) \
             ON CONFLICT(object_id) DO UPDATE SET \
               title_pseudo = excluded.title_pseudo, \
               metadata_pseudo_json = excluded.metadata_pseudo_json, \
               source_updated_at = excluded.source_updated_at, \
               state = 'fts_ready', last_error = NULL",
            params![oid, input.source_id.as_string(), input.account_id.as_string(),
                    input.scope_id.as_string(), input.external_id, object_type,
                    input.title_pseudo, input.metadata_pseudo_json, now],
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO knowledge_revisions \
             (revision_id, object_id, provider_version, observed_at) VALUES (?1, ?2, ?3, ?4)",
            params![rid, oid, input.provider_version, now],
        )?;

        tx.execute(
            "INSERT INTO knowledge_parts \
             (part_id, object_id, part_type, title_pseudo, metadata_pseudo_json, extracted_chars) \
             VALUES (?1, ?2, 'file_body', ?3, ?4, ?5) \
             ON CONFLICT(part_id) DO UPDATE SET \
               title_pseudo = excluded.title_pseudo, \
               metadata_pseudo_json = excluded.metadata_pseudo_json, \
               extracted_chars = excluded.extracted_chars",
            params![pid, oid, input.title_pseudo, input.metadata_pseudo_json,
                    input.chunks.iter().map(|c| c.text_pseudo.chars().count() as i64).sum::<i64>()],
        )?;

        // Replace prior chunks + FTS rows for this object.
        tx.execute("DELETE FROM knowledge_objects_fts WHERE object_id = ?1", params![oid])?;
        tx.execute("DELETE FROM knowledge_chunks WHERE object_id = ?1", params![oid])?;

        for c in &input.chunks {
            let cid = c.chunk_id.as_string();
            tx.execute(
                "INSERT INTO knowledge_chunks \
                 (chunk_id, object_id, revision_id, part_id, source_kind, object_type, title_pseudo, \
                  body_pseudo, metadata_pseudo_json, chunk_idx, char_start, char_end, indexed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![cid, oid, rid, pid, source_kind, object_type, c.title_pseudo,
                        c.text_pseudo, c.metadata_pseudo_json, c.chunk_idx, c.char_start, c.char_end, now],
            )?;
            tx.execute(
                "INSERT INTO knowledge_objects_fts \
                 (chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, body_pseudo, metadata_pseudo) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![cid, oid, rid, source_kind, object_type, c.title_pseudo, c.text_pseudo, c.metadata_pseudo_json],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 5: Export new types**

In `crates/anno-knowledge-store/src/lib.rs`, extend the `control_store` re-export to add `CommitChunk, CommitObjectInput, hex32`:
```rust
pub use control_store::{
    hex32, CommitChunk, CommitObjectInput, KnowledgeControlStore, LocalFolderRegistered,
    LocalFolderRegistration, ScopeRow, SourceRow, TestChunkInput,
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: PASS, including replacement (no FTS duplicates).

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-knowledge-store/src/control_store.rs crates/anno-knowledge-store/src/lib.rs
git commit -m "feat(knowledge-store): commit_object with revision-replace + fts-ready check"
```

---

## Task 4: Store — Forget Cascade + Per-State Status

**Files:**
- Modify: `crates/anno-knowledge-store/src/control_store.rs`

- [ ] **Step 1: Write failing forget test**

Add to the `#[cfg(test)] mod tests` block:
```rust
    #[test]
    fn forget_scope_removes_objects_chunks_and_fts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
        let reg = store
            .register_local_folder(LocalFolderRegistration {
                stable_key: "C:/docs".into(), source_label_pseudo: "FOLDER_1".into(),
                scope_label_pseudo: "FOLDER_1".into(), provider_key: "C:/docs".into(),
            }).expect("register");
        let input = commit_input(reg.scope_id, reg.account_id, reg.source_id, [1u8; 32], "le contrat FOLDER_1");
        store.commit_object(&input).expect("commit");
        assert_eq!(store.status().expect("status").objects, 1);

        let removed = store.forget_scope(&reg.scope_id).expect("forget");
        assert_eq!(removed, 1);

        let status = store.status().expect("status");
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        let hits = store
            .search_fast(&anno_knowledge_core::KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");
        assert!(hits.is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: FAIL — `forget_scope` not defined.

- [ ] **Step 3: Implement `forget_scope`**

Add to `impl KnowledgeControlStore`. FTS rows are deleted explicitly (the FTS virtual table is not covered by `ON DELETE CASCADE`):
```rust
    /// Delete all objects (and their chunks + FTS rows) under a scope.
    /// Returns the number of objects removed.
    ///
    /// # Errors
    /// Returns store errors on SQLite failure.
    pub fn forget_scope(&self, scope_id: &ScopeId) -> Result<u64> {
        let sid = scope_id.as_string();
        let mut conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let tx = conn.transaction()?;
        // Remove FTS rows first (no cascade on the virtual table).
        tx.execute(
            "DELETE FROM knowledge_objects_fts WHERE object_id IN \
             (SELECT object_id FROM knowledge_objects WHERE scope_id = ?1)",
            params![sid],
        )?;
        let removed = tx.execute(
            "DELETE FROM knowledge_objects WHERE scope_id = ?1",
            params![sid],
        )? as u64;
        // knowledge_chunks/revisions/parts cascade via ON DELETE CASCADE.
        tx.commit()?;
        Ok(removed)
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
```
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/anno-knowledge-store/src/control_store.rs
git commit -m "feat(knowledge-store): forget_scope cascade including FTS rows"
```

---

## Task 5: anno-rag — `Pipeline::pseudonymize_knowledge_object`

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Create: `crates/anno-rag/src/knowledge_privacy.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/lib.rs`

- [ ] **Step 1: Add the core dependency and widen visibility**

In `crates/anno-rag/Cargo.toml` `[dependencies]`, add:
```toml
anno-knowledge-core = { path = "../anno-knowledge-core" }
```

The new `pseudonymize_knowledge_object` impl lives in a sibling module
(`knowledge_privacy`), so it cannot see `Pipeline`'s private members. In
`crates/anno-rag/src/pipeline.rs`, widen two visibilities:

- the `vault` field (line ~64): `vault: Vault,` → `pub(crate) vault: Vault,`
- the detector accessor (line ~158): `fn detector_get_or_init(&self)` →
  `pub(crate) fn detector_get_or_init(&self)`

- [ ] **Step 2: Write the failing model-gated test**

Create `crates/anno-rag/src/knowledge_privacy.rs` with the test module first. The test is guarded: it returns early (passes) when no model directory is present, so CI without models stays green; locally with models it exercises the real detector + vault.
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AnnoRagConfig;

    fn models_present(cfg: &AnnoRagConfig) -> bool {
        cfg.models_cache().exists()
    }

    #[tokio::test]
    async fn pseudonymizes_chunks_title_and_metadata() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig { data_dir: dir.path().to_path_buf(), ..AnnoRagConfig::default() };
        if !models_present(&cfg) {
            eprintln!("skipping: no models dir at {}", cfg.models_cache().display());
            return;
        }
        // Pipeline::new(cfg, vault_key). Skip if construction fails (e.g. vault
        // not initialized in this env) — this test only asserts pseudonymization
        // behavior when a working pipeline is available.
        let pipeline = match Pipeline::new(cfg, [0u8; 32]).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping: pipeline unavailable: {e}");
                return;
            }
        };

        let input = PrivacyIndexInput {
            object_id: "obj-1".to_string(),
            revision_id: "rev-1".to_string(),
            part_id: "part-1".to_string(),
            title_raw: Some("Contrat Dupont.pdf".to_string()),
            metadata_raw: serde_json::json!({"path": "C:/clients/Dupont/contrat.pdf"}),
            chunks: vec![ExtractedChunkInput {
                idx: 0,
                text: "Le contrat de Jean Dupont prévoit 5000 euros.".to_string(),
                char_start: 0,
                char_end: 45,
            }],
        };

        let out = pipeline.pseudonymize_knowledge_object(input).await.expect("pseudo");
        assert_eq!(out.len(), 1);
        assert!(!out[0].text_pseudo.contains("Dupont"));
        assert!(out[0].title_pseudo.as_deref().map(|t| !t.contains("Dupont")).unwrap_or(true));
        assert!(!out[0].metadata_pseudo_json.contains("Dupont"));
    }
}
```

- [ ] **Step 3: Run test to verify it fails (or skips cleanly)**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: FAIL to compile — types not defined. (Once implemented, the test compiles and either runs or skips depending on models.)

- [ ] **Step 4: Implement the privacy API**

Prepend to `crates/anno-rag/src/knowledge_privacy.rs` (above the test module):
```rust
//! Narrow privacy entrypoint for the knowledge indexer.
//!
//! Reuses the existing detector + vault to pseudonymize knowledge-object text,
//! title, and metadata into chunks. No embedding, no legal enrichment, no
//! LanceDB. IDs are passed through as opaque strings so this module does not
//! depend on the knowledge ID newtypes.

use crate::pipeline::Pipeline;
use crate::Result;
use std::collections::HashMap;

/// One extracted (pre-pseudonymization) chunk.
#[derive(Debug, Clone)]
pub struct ExtractedChunkInput {
    /// Chunk index.
    pub idx: u32,
    /// Raw chunk text.
    pub text: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

/// Input to pseudonymize one knowledge object.
#[derive(Debug, Clone)]
pub struct PrivacyIndexInput {
    /// Opaque object id string.
    pub object_id: String,
    /// Opaque revision id string.
    pub revision_id: String,
    /// Opaque part id string.
    pub part_id: String,
    /// Raw title (file name).
    pub title_raw: Option<String>,
    /// Raw metadata JSON.
    pub metadata_raw: serde_json::Value,
    /// Extracted chunks.
    pub chunks: Vec<ExtractedChunkInput>,
}

/// One pseudonymized chunk produced by the privacy API.
#[derive(Debug, Clone)]
pub struct PseudonymizedChunk {
    /// Chunk index.
    pub chunk_idx: u32,
    /// Pseudonymized title (same for all chunks of the object).
    pub title_pseudo: Option<String>,
    /// Pseudonymized chunk body.
    pub text_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: String,
    /// Char start offset.
    pub char_start: u32,
    /// Char end offset.
    pub char_end: u32,
}

impl Pipeline {
    /// Pseudonymize a knowledge object's chunks, title, and metadata.
    ///
    /// Uses the PII subset only (legal labels are not requested). Loads the NER
    /// detector on demand. Does not embed.
    ///
    /// # Errors
    /// Returns detector or vault errors.
    pub async fn pseudonymize_knowledge_object(
        &self,
        input: PrivacyIndexInput,
    ) -> Result<Vec<PseudonymizedChunk>> {
        let detector = self.detector_get_or_init()?;
        let no_legal: Vec<crate::legal::LegalLabel> = Vec::new();
        let no_thresholds: HashMap<&'static str, f32> = HashMap::new();

        // Title: pseudonymize the file name.
        let title_pseudo = match &input.title_raw {
            Some(t) if !t.is_empty() => {
                let bundle = detector.detect_for_ingest(t, &no_legal, &no_thresholds)?;
                let (p, _map) = self.vault.pseudonymize_with_map(t, &bundle.pii).await?;
                Some(p)
            }
            _ => None,
        };

        // Metadata: pseudonymize the serialized JSON string.
        let metadata_raw_str = serde_json::to_string(&input.metadata_raw)
            .unwrap_or_else(|_| "{}".to_string());
        let metadata_pseudo_json = {
            let bundle = detector.detect_for_ingest(&metadata_raw_str, &no_legal, &no_thresholds)?;
            let (p, _map) = self.vault.pseudonymize_with_map(&metadata_raw_str, &bundle.pii).await?;
            p
        };

        let mut out = Vec::with_capacity(input.chunks.len());
        for chunk in &input.chunks {
            let bundle = detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
            let (text_pseudo, _map) = self.vault.pseudonymize_with_map(&chunk.text, &bundle.pii).await?;
            out.push(PseudonymizedChunk {
                chunk_idx: chunk.idx,
                title_pseudo: title_pseudo.clone(),
                text_pseudo,
                metadata_pseudo_json: metadata_pseudo_json.clone(),
                char_start: chunk.char_start,
                char_end: chunk.char_end,
            });
        }
        Ok(out)
    }
}
```

Note: confirm against `crates/anno-rag/src/legal/mod.rs` that `LegalLabel` is re-exported as `crate::legal::LegalLabel` and that `detector_get_or_init`, `self.detector`-free access via the returned detector, and `self.vault` are reachable from this module (they are used the same way in `pipeline.rs::ingest_one_counted`). If `detector_get_or_init` is private to `pipeline.rs`, change its visibility to `pub(crate)`.

- [ ] **Step 5: Register the module**

In `crates/anno-rag/src/lib.rs`, add with the other `pub mod` lines:
```rust
pub mod knowledge_privacy;
```

- [ ] **Step 6: Run the check then tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag
```
Expected: check PASS; test passes (runs with models, skips cleanly without).

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag/Cargo.toml crates/anno-rag/src/knowledge_privacy.rs crates/anno-rag/src/lib.rs
git commit -m "feat(rag): add Pipeline::pseudonymize_knowledge_object (no embed, no legal)"
```

---

## Task 6: `KnowledgeIndexer` Orchestration

**Files:**
- Modify: `crates/anno-rag-mcp/Cargo.toml`
- Create: `crates/anno-rag-mcp/src/indexer.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs` (add `mod indexer;`)

- [ ] **Step 1: Add the source dependency**

In `crates/anno-rag-mcp/Cargo.toml` `[dependencies]`, add:
```toml
anno-source-local = { path = "../anno-source-local" }
```

- [ ] **Step 2: Write the failing sync summary test**

Create `crates/anno-rag-mcp/src/indexer.rs` with the test module first. The end-to-end run is model-gated; a pure test covers the summary struct default:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_summary_starts_zeroed() {
        let s = SyncSummary::default();
        assert_eq!(s.seen, 0);
        assert_eq!(s.fts_ready, 0);
        assert_eq!(s.failed, 0);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: FAIL — `SyncSummary` not defined.

- [ ] **Step 4: Implement the indexer**

Prepend to `crates/anno-rag-mcp/src/indexer.rs`:
```rust
//! Knowledge sync orchestration: discovery -> extract -> pseudonymize -> store.

use anno_knowledge_core::{ObjectId, PartId, RevisionId, SourceKind, SourceKindForId};
use anno_knowledge_store::{hex32, CommitChunk, CommitObjectInput, KnowledgeControlStore, ScopeRow, SourceRow};
use anno_rag::config::AnnoRagConfig;
use anno_rag::knowledge_privacy::{ExtractedChunkInput, PrivacyIndexInput};
use anno_rag::pipeline::Pipeline;
use anno_source_local::{DiscoverBudget, LocalFolderSource};
use serde::Serialize;

/// Per-run result summary returned by `knowledge_sync`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSummary {
    /// Files discovered this run (after budget).
    pub seen: u64,
    /// Skipped because already `fts_ready` at the current content hash.
    pub skipped_unchanged: u64,
    /// Files extracted by Kreuzberg.
    pub extracted: u64,
    /// Objects pseudonymized.
    pub pseudonymized: u64,
    /// Objects written to FTS.
    pub fts_ready: u64,
    /// Objects removed because the file disappeared.
    pub forgotten: u64,
    /// Objects that failed this run.
    pub failed: u64,
    /// True when the budget truncated the walk (deletion reconciliation skipped).
    pub truncated: bool,
}

/// Sync one local-folder scope end to end.
///
/// # Errors
/// Returns a string error only for setup failures (store/scope access). Per-file
/// failures are counted in the summary, not returned.
pub async fn sync_local_scope(
    store: &KnowledgeControlStore,
    pipeline: &Pipeline,
    cfg: &AnnoRagConfig,
    source: &SourceRow,
    scope: &ScopeRow,
) -> Result<SyncSummary, String> {
    let mut summary = SyncSummary::default();
    let budget = DiscoverBudget::default();
    let src = LocalFolderSource::new(&scope.provider_key);
    let discovered = src.discover(&budget).map_err(|e| format!("discover: {e}"))?;
    summary.seen = discovered.len() as u64;
    summary.truncated = discovered.len() >= budget.max_files;

    for obj in &discovered {
        let object_id = ObjectId::from_external(
            SourceKindForId::LocalFolder,
            "local",
            &scope.provider_key,
            &obj.external_id,
        );
        let provider_version = hex32(&obj.content_hash);

        match store.revision_is_fts_ready(&object_id, &provider_version) {
            Ok(true) => {
                summary.skipped_unchanged += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "revision check failed");
                summary.failed += 1;
                continue;
            }
        }

        // Extract (Kreuzberg).
        let extracted = match anno_rag::ingest::extract(&obj.path, cfg).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "extraction failed");
                summary.failed += 1;
                continue;
            }
        };
        summary.extracted += 1;

        let revision_id = RevisionId::from_parts(&object_id.as_string(), &provider_version);
        let part_id = PartId::from_parts(&object_id.as_string(), "file_body");

        let privacy_input = PrivacyIndexInput {
            object_id: object_id.as_string(),
            revision_id: revision_id.as_string(),
            part_id: part_id.as_string(),
            title_raw: obj.title_raw.clone(),
            metadata_raw: obj.metadata_raw.clone(),
            chunks: extracted.chunks.iter().map(|c| ExtractedChunkInput {
                idx: c.idx,
                text: c.text.clone(),
                char_start: c.char_start,
                char_end: c.char_end,
            }).collect(),
        };

        let pseudo = match pipeline.pseudonymize_knowledge_object(privacy_input).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "pseudonymization failed");
                summary.failed += 1;
                continue;
            }
        };
        summary.pseudonymized += 1;

        let chunks: Vec<CommitChunk> = pseudo.iter().map(|p| CommitChunk {
            chunk_id: anno_knowledge_core::ChunkId::from_parts(revision_id, part_id, p.chunk_idx),
            chunk_idx: p.chunk_idx,
            title_pseudo: p.title_pseudo.clone(),
            text_pseudo: p.text_pseudo.clone(),
            metadata_pseudo_json: p.metadata_pseudo_json.clone(),
            char_start: p.char_start,
            char_end: p.char_end,
        }).collect();

        let commit = CommitObjectInput {
            object_id,
            source_id: source.source_id,
            account_id: scope.account_id,
            scope_id: scope.scope_id,
            revision_id,
            part_id,
            external_id: obj.external_id.clone(),
            object_type: obj.object_type,
            provider_version,
            title_pseudo: pseudo.first().and_then(|p| p.title_pseudo.clone()),
            metadata_pseudo_json: pseudo.first().map(|p| p.metadata_pseudo_json.clone()).unwrap_or_else(|| "{}".into()),
            source_kind: SourceKind::LocalFolder,
            chunks,
        };

        match store.commit_object(&commit) {
            Ok(()) => summary.fts_ready += 1,
            Err(e) => {
                tracing::warn!(error = %e, "commit failed");
                summary.failed += 1;
            }
        }
    }

    Ok(summary)
}
```

Note: deletion reconciliation (set absent files to `forgotten`) is intentionally deferred to keep the first orchestration slice focused; it runs only on a non-truncated walk (`!summary.truncated`). For the Phase 2 MVP the `forgotten` field stays 0. To implement it as an optional follow-up commit: add a `store.objects_under_scope(scope_id) -> Vec<(ObjectId, String /*external_id*/)>` listing method and a per-object `store.forget_object(object_id)` (mirror `forget_scope` but keyed on `object_id`), diff the stored external ids against the discovered set, and forget the missing ones only when the walk was complete. Keep it in its own commit.

- [ ] **Step 5: Register the module**

In `crates/anno-rag-mcp/src/lib.rs`, near the other module decls:
```rust
mod indexer;
```

- [ ] **Step 6: Run check + tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: PASS (the `SyncSummary` unit test).

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp/Cargo.toml crates/anno-rag-mcp/src/indexer.rs crates/anno-rag-mcp/src/lib.rs
git commit -m "feat(mcp): KnowledgeIndexer sync orchestration for local folders"
```

---

## Task 7: MCP Tools — add_local_folder / sync / forget + real sources/status/search

**Files:**
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Write failing health + real-sources tests**

In `crates/anno-rag-mcp/src/health.rs`, extend the existing tests module (or add it) so it asserts the new names:
```rust
    #[test]
    fn all_tool_names_includes_phase2_knowledge_tools() {
        let tools = all_tool_names();
        assert!(tools.contains(&"knowledge_add_local_folder".to_string()));
        assert!(tools.contains(&"knowledge_sync".to_string()));
        assert!(tools.contains(&"knowledge_forget".to_string()));
    }
```

In `crates/anno-rag-mcp/src/knowledge.rs` tests module, add a no-models test that `add_local_folder` then `sources` returns one entry:
```rust
    #[test]
    fn add_local_folder_then_sources_lists_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig { data_dir: dir.path().to_path_buf(), ..AnnoRagConfig::default() };
        let folder = dir.path().join("corpus");
        std::fs::create_dir_all(&folder).expect("mkdir");

        let service = KnowledgeService::open(&cfg).expect("service");
        let source_id = service.add_local_folder(&folder.display().to_string()).expect("add");
        assert!(!source_id.is_empty());

        let sources = service.sources();
        assert_eq!(sources.len(), 1);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: FAIL — names absent, `add_local_folder` not defined.

- [ ] **Step 3: Implement real service methods**

In `crates/anno-rag-mcp/src/knowledge.rs`, replace the Phase 1 placeholder `sources()` and add the new methods. Update imports:
```rust
use anno_knowledge_core::KnowledgeSearchRequest;
use crate::indexer::{sync_local_scope, SyncSummary};
use std::path::Path;
```

Add inside `impl KnowledgeService` (replacing the placeholder `sources`):
```rust
    /// List configured sources as JSON values.
    pub fn sources(&self) -> Vec<serde_json::Value> {
        self.store
            .list_sources()
            .unwrap_or_default()
            .into_iter()
            .map(|s| serde_json::json!({
                "source_id": s.source_id.as_string(),
                "kind": s.kind,
                "label": s.display_label_pseudo,
                "enabled": s.enabled,
            }))
            .collect()
    }

    /// Register a local folder as a source. Does not load models.
    ///
    /// # Errors
    /// Returns store errors.
    pub fn add_local_folder(&self, path: &str) -> anno_knowledge_store::Result<String> {
        let label = pseudo_folder_label(path);
        let reg = self.store.register_local_folder(
            anno_knowledge_store::LocalFolderRegistration {
                stable_key: path.to_string(),
                source_label_pseudo: label.clone(),
                scope_label_pseudo: label,
                provider_key: path.to_string(),
            },
        )?;
        Ok(reg.source_id.as_string())
    }

    /// Forget a source: remove all its scopes' objects/chunks/FTS rows.
    ///
    /// # Errors
    /// Returns store errors.
    pub fn forget_source(&self, source_id: &str) -> anno_knowledge_store::Result<u64> {
        let sid = anno_knowledge_core::SourceId::new(
            uuid::Uuid::parse_str(source_id)
                .map_err(|e| anno_knowledge_store::KnowledgeStoreError::Sqlite(
                    rusqlite::Error::ToSqlConversionFailure(Box::new(e))))?,
        );
        let mut removed = 0;
        for scope in self.store.enabled_scopes_for_source(&sid)? {
            removed += self.store.forget_scope(&scope.scope_id)?;
        }
        Ok(removed)
    }

    /// Run a bounded sync over all enabled scopes of a source (or all sources).
    /// Requires a pipeline reference for pseudonymization (loads NER on demand).
    ///
    /// # Errors
    /// Returns a string error on setup failure.
    pub async fn sync(
        &self,
        pipeline: &anno_rag::pipeline::Pipeline,
        cfg: &anno_rag::config::AnnoRagConfig,
        source_id: Option<&str>,
    ) -> Result<SyncSummary, String> {
        let sources = self.store.list_sources().map_err(|e| format!("list_sources: {e}"))?;
        let mut total = SyncSummary::default();
        for source in &sources {
            if let Some(want) = source_id {
                if source.source_id.as_string() != want {
                    continue;
                }
            }
            let scopes = self.store
                .enabled_scopes_for_source(&source.source_id)
                .map_err(|e| format!("scopes: {e}"))?;
            for scope in &scopes {
                let s = sync_local_scope(&self.store, pipeline, cfg, source, scope).await?;
                total.seen += s.seen;
                total.skipped_unchanged += s.skipped_unchanged;
                total.extracted += s.extracted;
                total.pseudonymized += s.pseudonymized;
                total.fts_ready += s.fts_ready;
                total.forgotten += s.forgotten;
                total.failed += s.failed;
                total.truncated |= s.truncated;
            }
        }
        Ok(total)
    }
```

Add a free helper at the bottom of `knowledge.rs` (a folder label that does not leak the full path — the path itself is pseudonymized later during indexing, but the source label should be coarse):
```rust
fn pseudo_folder_label(path: &str) -> String {
    let last = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("folder");
    format!("local:{last}")
}
```

Note: confirm `KnowledgeService` stores `store: KnowledgeControlStore` (Phase 1 field name). If Phase 1 named it differently, adjust `self.store` accordingly.

- [ ] **Step 4: Add the three MCP tools**

In `crates/anno-rag-mcp/src/lib.rs`, add params struct near the other params:
```rust
/// Parameters for `knowledge_add_local_folder`.
#[derive(Debug, Clone, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct KnowledgeAddFolderParams {
    /// Absolute path to the folder to index.
    pub path: String,
}

/// Parameters for `knowledge_sync`.
#[derive(Debug, Clone, Default, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct KnowledgeSyncParams {
    /// Optional source id; if omitted, all local-folder sources are synced.
    #[serde(default)]
    pub source_id: Option<String>,
}

/// Parameters for `knowledge_forget`.
#[derive(Debug, Clone, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct KnowledgeForgetParams {
    /// Source id to forget (all its scopes' content is removed).
    pub source_id: String,
}
```

Add these tool methods inside the existing tool `impl` block:
```rust
    /// Register a local folder as a knowledge source. Does not load models.
    #[tool(description = "Register a local folder as an Anno knowledge source. Does not load local ML models. Run knowledge_sync afterwards to index it.")]
    async fn knowledge_add_local_folder(&self, Parameters(p): Parameters<KnowledgeAddFolderParams>) -> String {
        let service = match self.knowledge().await {
            Ok(s) => s,
            Err(e) => return format!("Error: {e}"),
        };
        match service.add_local_folder(&p.path) {
            Ok(source_id) => serde_json::to_string_pretty(&serde_json::json!({"ok": true, "source_id": source_id}))
                .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Index local folder sources end to end (extract + pseudonymize + FTS write).
    #[tool(description = "Sync Anno local-folder knowledge sources: walk, extract, pseudonymize locally, and write pseudonymized FTS chunks. Loads the local NER model. Bounded per run; call again to resume large folders.")]
    async fn knowledge_sync(&self, Parameters(p): Parameters<KnowledgeSyncParams>) -> String {
        let service = match self.knowledge().await {
            Ok(s) => s,
            Err(e) => return format!("Error: {e}"),
        };
        // self.pipeline() returns &Pipeline (lazy-initialized). If it fails,
        // models/vault are not ready — report models_missing.
        let pipeline = match self.pipeline().await {
            Ok(pl) => pl,
            Err(_) => return serde_json::json!({
                "ok": false,
                "error": {"code": "models_missing",
                          "message": "Models are not available. Fast FTS search works on already-indexed content; indexing is paused.",
                          "next_action": "Run download_models or ask Anno to set up models."}
            }).to_string(),
        };
        match service.sync(pipeline, self.cfg.as_ref(), p.source_id.as_deref()).await {
            Ok(summary) => serde_json::to_string_pretty(&serde_json::json!({"ok": true, "summary": summary}))
                .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Forget a knowledge source and all its indexed content.
    #[tool(description = "Remove an Anno knowledge source and all its pseudonymized content from SQLite and FTS. Does not load local ML models.")]
    async fn knowledge_forget(&self, Parameters(p): Parameters<KnowledgeForgetParams>) -> String {
        let service = match self.knowledge().await {
            Ok(s) => s,
            Err(e) => return format!("Error: {e}"),
        };
        match service.forget_source(&p.source_id) {
            Ok(removed) => serde_json::to_string_pretty(&serde_json::json!({"ok": true, "removed_objects": removed}))
                .unwrap_or_else(|e| format!("Error: {e}")),
            Err(e) => format!("Error: {e}"),
        }
    }
```

Note: `self.pipeline()` returns `anno_rag::error::Result<&Pipeline>` (confirmed in `lib.rs:42`) and `self.cfg: Arc<AnnoRagConfig>` (confirmed `lib.rs:32`). Pass the borrow through directly — `service.sync(pipeline, self.cfg.as_ref(), ...)` — do not add another `&`.

- [ ] **Step 5: Advertise the tools**

In `crates/anno-rag-mcp/src/health.rs`, inside `all_tool_names()`, add after the Phase 1 knowledge entries:
```rust
        "knowledge_add_local_folder",
        "knowledge_sync",
        "knowledge_forget",
```

- [ ] **Step 6: Run tests**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: PASS (health names + add_local_folder/sources without models).

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp/src/knowledge.rs crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs
git commit -m "feat(mcp): knowledge_add_local_folder, knowledge_sync, knowledge_forget"
```

---

## Task 8: Verification

**Files:** none (verification only)

- [ ] **Step 1: Format**

Run:
```powershell
cargo fmt --check
```
Expected: PASS (run `cargo fmt` then re-check if it fails).

- [ ] **Step 2: Check the new + touched crates**

Run:
```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-source-local -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-knowledge-store -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check -Profile dev-fast
```
Expected: zero warnings, zero errors.

- [ ] **Step 3: Run targeted tests for new crates**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-source-local
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-knowledge-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: PASS.

- [ ] **Step 4: Confirm fast search + status work without models**

Run:
```powershell
Remove-Item Env:\ANNO_MODELS_DIR -ErrorAction SilentlyContinue
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-rag-mcp
```
Expected: PASS — `add_local_folder`, `sources`, `status`, and `knowledge_search(fast)` paths do not require models. (`knowledge_sync` returns `models_missing` when models are absent — that is correct, not a failure.)

- [ ] **Step 5: Detect changed scope**

Run:
```powershell
npx gitnexus detect-changes
```
Expected: changes limited to the new `anno-source-local` crate, `anno-knowledge-store`, the new `knowledge_privacy` module in `anno-rag`, and the MCP indexer/knowledge/health/lib files. No changes to `Store`, `Pipeline::ingest_folder`, `Pipeline::search`, `legal_ingest`, or `legal_search`.

- [ ] **Step 6: Re-index GitNexus (code changed)**

Run:
```powershell
npx gitnexus analyze
```

---

## Acceptance Criteria

- `knowledge_add_local_folder(path)` registers a source/account/scope without loading models.
- `knowledge_sync` indexes a fixture folder end to end and returns a correct summary; a second sync skips unchanged files.
- `knowledge_search(mode=fast)` returns pseudonymized hits from indexed files and never returns raw source text.
- `knowledge_forget(source_id)` removes the source's content from SQLite and FTS.
- `knowledge_status` reports real counts.
- No edits to `crates/anno-rag/src/store.rs`, `Pipeline::ingest_folder`, `Pipeline::search`, `legal_ingest`, or `legal_search`.
- `anno-source-local` depends only on `anno-knowledge-core` (+ `sha2`/`walkdir`/`chrono`/`serde_json`/`thiserror`) — not on `anno-rag`, MCP, SQLite, or Kreuzberg.
- `anno_health.available_tools` includes the three new tool names.
- Targeted crate tests pass; `npx gitnexus detect-changes` reports only expected files.

## Self-Review Against Spec

Covered:
- §5 discovery-only connector + orchestrating indexer (Tasks 1, 6).
- §6 deterministic content-based IDs + skip rule (Tasks 1, 3, 6).
- §7 synchronous bounded sync with states + resume (Tasks 3, 6).
- §8 narrow `pseudonymize_knowledge_object`, PII-only, no embed/legal (Task 5).
- §9 graceful degradation: `knowledge_sync` returns `models_missing` (Task 7).
- §10 M2 tool surface; real sources/status/search (Tasks 7; status/search from Phase 1).
- §11 per-object failure isolation via summary counters (Task 6).
- §12 test strategy (per-task tests; model-gated integration in Tasks 5-7).

Deferred (matches spec §14):
- Full per-object deletion reconciliation (`forgotten` counter) — scaffolded in Task 6 note; optional follow-up commit.
- Vectors / semantic / `knowledge_open` — Phase 3+.
