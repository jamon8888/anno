# MCP Corpus Control Plane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a corpus control plane so Anno MCP can index multiple client folders while keeping search, legal extraction, tabular review, and forget operations scoped to the selected client corpus.

**Architecture:** Add `anno-corpus-core` for shared typed ids, root normalization, document instance identity, and guard types. Add `anno-corpus-store` as a dedicated SQLite control store under `AnnoRagConfig.data_dir`, then wire MCP, knowledge, legal, and tabular operations through an `EffectiveCorpus` resolver before backend access. Keep backend storage changes scoped: use the corpus store as authority, add corpus-qualified legal document ids, and avoid raw path leakage in MCP responses.

**Tech Stack:** Rust 2021, Cargo workspace, rusqlite, uuid v5/v7, serde, LanceDB 0.29, rmcp, Tokio, existing `scripts/dev-fast.ps1` targeted build loop.

---

## Scope Check

This is one cross-cutting plan because each subsystem depends on the same control-plane contract. Execute it in order. Tasks 1-3 create the corpus foundation. Tasks 4-8 wire one backend area at a time. Task 9 adds symmetric deletion. Task 10 proves the full MCP behavior.

Use a clean worktree for implementation. The current workspace may already contain documentation edits and unrelated deleted files; do not revert unrelated changes.

## File Map

Create:

- `crates/anno-corpus-core/Cargo.toml` - lightweight corpus domain crate.
- `crates/anno-corpus-core/src/lib.rs` - public exports.
- `crates/anno-corpus-core/src/ids.rs` - `CorpusId`, content id, document instance id generation.
- `crates/anno-corpus-core/src/root.rs` - root normalization, pseudonymous labels, overlap detection.
- `crates/anno-corpus-core/src/model.rs` - shared domain structs and enums.
- `crates/anno-corpus-core/src/guard.rs` - `EffectiveCorpus` and resolver-facing error types.
- `crates/anno-corpus-store/Cargo.toml` - SQLite corpus registry crate.
- `crates/anno-corpus-store/src/lib.rs` - public exports.
- `crates/anno-corpus-store/src/error.rs` - store error type.
- `crates/anno-corpus-store/src/migrations.rs` - schema creation.
- `crates/anno-corpus-store/src/store.rs` - registry CRUD, bindings, documents, health.
- `crates/anno-rag-mcp/src/corpus.rs` - MCP-facing corpus service and wire models.

Modify:

- `Cargo.toml` - workspace members and dependencies.
- `crates/anno-rag/Cargo.toml` - add `anno-corpus-core`.
- `crates/anno-rag-mcp/Cargo.toml` - add `anno-corpus-core` and `anno-corpus-store`.
- `crates/anno-knowledge-core/src/query.rs` - add source/scope filter and source identity in hits.
- `crates/anno-knowledge-store/src/control_store.rs` - filter FTS through `knowledge_objects`.
- `crates/anno-rag-mcp/src/knowledge.rs` - use normalized source keys and scoped search requests.
- `crates/anno-rag-mcp/src/lib.rs` - add corpus params, resolver calls, tools, sanitized hits, scoped index/search/forget/review flows.
- `crates/anno-rag-mcp/src/health.rs` - add `corpus_list`, `corpus_get`, `corpus_health`.
- `crates/anno-rag/src/pipeline.rs` - add scoped ingest and corpus-qualified legal document ids.
- `crates/anno-rag/src/store.rs` - add doc/chunk filtering helpers for corpus document ids.
- `crates/anno-rag/src/legal/store.rs` - compose legal business filters with corpus doc/chunk candidate sets.
- `crates/anno-rag/src/legal/kg.rs` - add typed case-file query methods.
- `crates/anno-rag/src/legal/extract.rs` - replace raw `cypher` in case-file extraction.
- `crates/anno-rag-tabular/src/storage/rows.rs` - add delete helper for review rows.
- `crates/anno-rag-tabular/src/storage/cells.rs` - add delete helper for review cells.

Do not change:

- Claude Desktop config.
- Release packaging.
- UI code.

---

### Task 0: Baseline Verification For Existing Pdfium Fix

**Files:**
- Modify only if missing: `Cargo.toml`
- Test: targeted Cargo commands

- [ ] **Step 1: Verify `kreuzberg` has bundled Pdfium**

Check `Cargo.toml` contains:

```toml
kreuzberg       = { version = "=4.9.7", default-features = false, features = ["pdf", "bundled-pdfium", "office", "html", "email", "excel", "xml", "archives", "tokio-runtime", "chunking"] }
```

- [ ] **Step 2: Run the feature check**

Run:

```powershell
cargo tree -p anno-rag-bin -e features --prefix none | rg "bundled-pdfium|pdfium-render"
```

Expected: output contains `bundled-pdfium` or a `pdfium-render` bundled feature edge.

- [ ] **Step 3: Run targeted build**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-bin -Mode build
```

Expected: exit code 0.

- [ ] **Step 4: Commit**

Only commit this task if it changed files:

```powershell
git add Cargo.toml
git commit -m "fix: bundle pdfium for local mcp ingestion"
```

---

### Task 1: Add `anno-corpus-core`

**Files:**
- Create: `crates/anno-corpus-core/Cargo.toml`
- Create: `crates/anno-corpus-core/src/lib.rs`
- Create: `crates/anno-corpus-core/src/ids.rs`
- Create: `crates/anno-corpus-core/src/root.rs`
- Create: `crates/anno-corpus-core/src/model.rs`
- Create: `crates/anno-corpus-core/src/guard.rs`
- Modify: `Cargo.toml`
- Test: `crates/anno-corpus-core/src/*`

- [ ] **Step 1: Add workspace member and dependency**

In root `Cargo.toml`, add the member:

```toml
"crates/anno-corpus-core",
```

Add the workspace dependency:

```toml
anno-corpus-core = { path = "crates/anno-corpus-core" }
```

- [ ] **Step 2: Create crate manifest**

Create `crates/anno-corpus-core/Cargo.toml`:

```toml
[package]
name = "anno-corpus-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true
documentation.workspace = true
description = "Shared corpus identity and scoping domain types for Anno."

[dependencies]
serde = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
sha2 = { workspace = true }
```

- [ ] **Step 3: Write failing id/root tests**

Create `crates/anno-corpus-core/src/ids.rs` with tests first:

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const CORPUS_NAMESPACE: Uuid = Uuid::from_u128(0x3a17_0c2f_9db3_4b42_b73f_7be4_f565_4a01);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CorpusId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentInstanceId(Uuid);

impl CorpusId {
    #[must_use]
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    #[must_use]
    pub fn as_string(self) -> String {
        self.0.to_string()
    }

    #[must_use]
    pub fn from_normalized_root(normalized_root: &str) -> Self {
        Self(stable_uuid(&["corpus", normalized_root]))
    }
}

impl ContentId {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Self(hex_lower(&hasher.finalize()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl DocumentInstanceId {
    #[must_use]
    pub const fn new(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    #[must_use]
    pub fn as_string(self) -> String {
        self.0.to_string()
    }

    #[must_use]
    pub fn from_parts(corpus_id: CorpusId, normalized_relative_path: &str, content_id: &ContentId) -> Self {
        Self(stable_uuid(&[
            "document",
            &corpus_id.as_string(),
            normalized_relative_path,
            content_id.as_str(),
        ]))
    }
}

fn stable_uuid(parts: &[&str]) -> Uuid {
    Uuid::new_v5(&CORPUS_NAMESPACE, parts.join("\u{1f}").as_bytes())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_id_is_stable_for_normalized_root() {
        let a = CorpusId::from_normalized_root("c:/clients/acme");
        let b = CorpusId::from_normalized_root("c:/clients/acme");
        assert_eq!(a, b);
    }

    #[test]
    fn document_instance_id_changes_by_corpus() {
        let content = ContentId::from_bytes(b"same file");
        let a = CorpusId::from_normalized_root("c:/clients/a");
        let b = CorpusId::from_normalized_root("c:/clients/b");
        let doc_a = DocumentInstanceId::from_parts(a, "contract.pdf", &content);
        let doc_b = DocumentInstanceId::from_parts(b, "contract.pdf", &content);
        assert_ne!(doc_a, doc_b);
        assert_eq!(content, ContentId::from_bytes(b"same file"));
    }
}
```

Create `crates/anno-corpus-core/src/root.rs` with tests first:

```rust
use crate::ids::CorpusId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RootError {
    #[error("corpus root path is empty")]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusRoot {
    pub raw_path: String,
    pub normalized_path: String,
    pub label_pseudo: String,
}

impl CorpusRoot {
    pub fn from_raw(raw_path: impl Into<String>) -> Result<Self, RootError> {
        let raw_path = raw_path.into();
        let normalized_path = normalize_path(&raw_path)?;
        let corpus_id = CorpusId::from_normalized_root(&normalized_path);
        Ok(Self {
            raw_path,
            normalized_path,
            label_pseudo: format!("corpus_{}", &corpus_id.as_string()[..12]),
        })
    }
}

pub fn normalize_path(path: &str) -> Result<String, RootError> {
    let replaced = path.replace('\\', "/");
    let trimmed = replaced.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(RootError::Empty);
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub fn roots_overlap(a: &str, b: &str) -> bool {
    fn inside(child: &str, parent: &str) -> bool {
        child == parent || child.strip_prefix(parent).is_some_and(|suffix| suffix.starts_with('/'))
    }
    inside(a, b) || inside(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_collapses_windows_variants() {
        assert_eq!(
            normalize_path("C:\\Users\\Client\\Matter\\").unwrap(),
            "c:/users/client/matter"
        );
    }

    #[test]
    fn roots_overlap_detects_nested_paths() {
        assert!(roots_overlap("c:/clients/acme", "c:/clients/acme/sub"));
        assert!(!roots_overlap("c:/clients/acme", "c:/clients/beta"));
    }
}
```

- [ ] **Step 4: Add model and guard types**

Create `crates/anno-corpus-core/src/model.rs`:

```rust
use crate::ids::{CorpusId, DocumentInstanceId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusBindingKind {
    KnowledgeSource,
    LegalFolder,
    LegalDocument,
    TabularReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorpusProfile {
    Knowledge,
    Legal,
    Tabular,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusSummary {
    pub corpus_id: CorpusId,
    pub label_pseudo: String,
    pub profiles: Vec<CorpusProfile>,
    pub health: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusDocumentRef {
    pub corpus_id: CorpusId,
    pub document_id: DocumentInstanceId,
    pub source_path_hash: String,
    pub relative_path_hash: Option<String>,
    pub content_id: String,
}
```

Create `crates/anno-corpus-core/src/guard.rs`:

```rust
use crate::ids::CorpusId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectiveCorpus {
    Single(CorpusId),
    CrossCorpus,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CorpusGuardError {
    #[error("index a folder before using this tool")]
    NoCorpus,
    #[error("corpus_id is required because multiple corpora are indexed")]
    CorpusRequired,
    #[error("unknown corpus_id: {0}")]
    UnknownCorpus(String),
}
```

Create `crates/anno-corpus-core/src/lib.rs`:

```rust
pub mod guard;
pub mod ids;
pub mod model;
pub mod root;

pub use guard::{CorpusGuardError, EffectiveCorpus};
pub use ids::{ContentId, CorpusId, DocumentInstanceId};
pub use model::{CorpusBindingKind, CorpusDocumentRef, CorpusProfile, CorpusSummary};
pub use root::{normalize_path, roots_overlap, CorpusRoot, RootError};
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-corpus-core
```

Expected: all `anno-corpus-core` tests pass.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml crates/anno-corpus-core
git commit -m "feat: add corpus core domain types"
```

---

### Task 2: Add `anno-corpus-store`

**Files:**
- Create: `crates/anno-corpus-store/Cargo.toml`
- Create: `crates/anno-corpus-store/src/lib.rs`
- Create: `crates/anno-corpus-store/src/error.rs`
- Create: `crates/anno-corpus-store/src/migrations.rs`
- Create: `crates/anno-corpus-store/src/store.rs`
- Modify: `Cargo.toml`
- Test: `crates/anno-corpus-store/src/store.rs`

- [ ] **Step 1: Add workspace member and dependencies**

In root `Cargo.toml`, add:

```toml
"crates/anno-corpus-store",
```

Add workspace dependency:

```toml
anno-corpus-store = { path = "crates/anno-corpus-store" }
```

- [ ] **Step 2: Create crate manifest**

Create `crates/anno-corpus-store/Cargo.toml`:

```toml
[package]
name = "anno-corpus-store"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true
documentation.workspace = true
description = "SQLite corpus registry for Anno local MCP scoping."

[dependencies]
anno-corpus-core = { workspace = true }
chrono = { workspace = true }
rusqlite = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write migration and store tests**

Create `crates/anno-corpus-store/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Root(#[from] anno_corpus_core::RootError),
    #[error("corpus root overlaps existing corpus {corpus_id}")]
    Overlap { corpus_id: String },
    #[error("unknown corpus_id: {0}")]
    UnknownCorpus(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

Create `crates/anno-corpus-store/src/migrations.rs`:

```rust
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS corpora (
            corpus_id TEXT PRIMARY KEY,
            normalized_root TEXT NOT NULL UNIQUE,
            raw_root TEXT NOT NULL,
            label_pseudo TEXT NOT NULL,
            profiles_json TEXT NOT NULL,
            health TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS corpus_bindings (
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            binding_kind TEXT NOT NULL,
            binding_id TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (corpus_id, binding_kind, binding_id)
        );

        CREATE TABLE IF NOT EXISTS corpus_documents (
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            document_id TEXT NOT NULL,
            backend_kind TEXT NOT NULL,
            source_path_hash TEXT NOT NULL,
            relative_path_hash TEXT,
            content_id TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (corpus_id, document_id, backend_kind)
        );

        CREATE TABLE IF NOT EXISTS corpus_index_runs (
            run_id TEXT PRIMARY KEY,
            corpus_id TEXT NOT NULL REFERENCES corpora(corpus_id) ON DELETE CASCADE,
            profile TEXT NOT NULL,
            status TEXT NOT NULL,
            counters_json TEXT NOT NULL,
            failures_json TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_corpus_bindings_lookup
            ON corpus_bindings(binding_kind, binding_id);
        CREATE INDEX IF NOT EXISTS idx_corpus_documents_doc
            ON corpus_documents(document_id);
        ",
    )
}
```

Create `crates/anno-corpus-store/src/lib.rs`:

```rust
pub mod error;
pub mod migrations;
pub mod store;

pub use error::{Error, Result};
pub use store::{CorpusBindingRow, CorpusDocumentRow, CorpusStore, RegisterCorpusResult};
```

- [ ] **Step 4: Implement store API**

Create `crates/anno-corpus-store/src/store.rs` with these public methods:

```rust
use crate::error::{Error, Result};
use anno_corpus_core::{
    roots_overlap, ContentId, CorpusBindingKind, CorpusId, CorpusProfile, CorpusRoot,
    DocumentInstanceId,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterCorpusResult {
    pub corpus_id: CorpusId,
    pub normalized_root: String,
    pub label_pseudo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusBindingRow {
    pub corpus_id: CorpusId,
    pub binding_kind: CorpusBindingKind,
    pub binding_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusDocumentRow {
    pub corpus_id: CorpusId,
    pub document_id: DocumentInstanceId,
    pub backend_kind: String,
    pub content_id: String,
}

pub struct CorpusStore {
    conn: Mutex<Connection>,
}

impl CorpusStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        crate::migrations::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn register_root(&self, raw_root: &str, profiles: &[CorpusProfile]) -> Result<RegisterCorpusResult> {
        let root = CorpusRoot::from_raw(raw_root)?;
        let corpus_id = CorpusId::from_normalized_root(&root.normalized_path);
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let mut stmt = conn.prepare("SELECT corpus_id, normalized_root FROM corpora")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (existing_id, existing_root) = row?;
            if existing_root != root.normalized_path && roots_overlap(&existing_root, &root.normalized_path) {
                return Err(Error::Overlap { corpus_id: existing_id });
            }
        }
        let now = Utc::now().to_rfc3339();
        let profiles_json = serde_json::to_string(profiles)?;
        conn.execute(
            "INSERT INTO corpora
             (corpus_id, normalized_root, raw_root, label_pseudo, profiles_json, health, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'ok', ?6, ?6)
             ON CONFLICT(corpus_id) DO UPDATE SET
                raw_root = excluded.raw_root,
                profiles_json = excluded.profiles_json,
                updated_at = excluded.updated_at",
            params![
                corpus_id.as_string(),
                root.normalized_path,
                root.raw_path,
                root.label_pseudo,
                profiles_json,
                now,
            ],
        )?;
        Ok(RegisterCorpusResult {
            corpus_id,
            normalized_root: root.normalized_path,
            label_pseudo: root.label_pseudo,
        })
    }

    pub fn add_binding(
        &self,
        corpus_id: CorpusId,
        kind: CorpusBindingKind,
        binding_id: &str,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        conn.execute(
            "INSERT OR REPLACE INTO corpus_bindings
             (corpus_id, binding_kind, binding_id, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                corpus_id.as_string(),
                binding_kind_text(kind),
                binding_id,
                serde_json::to_string(metadata)?,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn add_document(
        &self,
        corpus_id: CorpusId,
        document_id: DocumentInstanceId,
        backend_kind: &str,
        source_path: &str,
        relative_path: Option<&str>,
        content_id: &ContentId,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        conn.execute(
            "INSERT OR REPLACE INTO corpus_documents
             (corpus_id, document_id, backend_kind, source_path_hash, relative_path_hash, content_id, metadata_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                corpus_id.as_string(),
                document_id.as_string(),
                backend_kind,
                sha256_hex(source_path.as_bytes()),
                relative_path.map(|p| sha256_hex(p.as_bytes())),
                content_id.as_str(),
                serde_json::to_string(metadata)?,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn corpus_count(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM corpora", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn document_ids_for_corpus(&self, corpus_id: CorpusId, backend_kind: &str) -> Result<Vec<uuid::Uuid>> {
        let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
        ensure_corpus_exists(&conn, corpus_id)?;
        let mut stmt = conn.prepare(
            "SELECT document_id FROM corpus_documents
             WHERE corpus_id = ?1 AND backend_kind = ?2
             ORDER BY document_id",
        )?;
        let rows = stmt.query_map(params![corpus_id.as_string(), backend_kind], |row| {
            let value: String = row.get(0)?;
            uuid::Uuid::parse_str(&value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn ensure_corpus_exists(conn: &Connection, corpus_id: CorpusId) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM corpora WHERE corpus_id = ?1",
        params![corpus_id.as_string()],
        |row| row.get(0),
    )?;
    if count == 0 {
        return Err(Error::UnknownCorpus(corpus_id.as_string()));
    }
    Ok(())
}

fn binding_kind_text(kind: CorpusBindingKind) -> &'static str {
    match kind {
        CorpusBindingKind::KnowledgeSource => "knowledge_source",
        CorpusBindingKind::LegalFolder => "legal_folder",
        CorpusBindingKind::LegalDocument => "legal_document",
        CorpusBindingKind::TabularReview => "tabular_review",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 5: Add store tests**

Append tests to `store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_create_empty_store() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CorpusStore::open(dir.path().join("corpus.sqlite3")).expect("open");
        assert_eq!(store.corpus_count().expect("count"), 0);
    }

    #[test]
    fn register_root_is_stable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CorpusStore::open(dir.path().join("corpus.sqlite3")).expect("open");
        let a = store
            .register_root("C:\\Clients\\Acme\\", &[CorpusProfile::All])
            .expect("register a");
        let b = store
            .register_root("c:/clients/acme", &[CorpusProfile::All])
            .expect("register b");
        assert_eq!(a.corpus_id, b.corpus_id);
        assert_eq!(store.corpus_count().expect("count"), 1);
    }

    #[test]
    fn register_root_rejects_overlap() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CorpusStore::open(dir.path().join("corpus.sqlite3")).expect("open");
        store
            .register_root("c:/clients/acme", &[CorpusProfile::All])
            .expect("register");
        let err = store
            .register_root("c:/clients/acme/matter", &[CorpusProfile::All])
            .expect_err("overlap must fail");
        assert!(matches!(err, Error::Overlap { .. }));
    }
}
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p anno-corpus-store
```

Expected: all `anno-corpus-store` tests pass.

- [ ] **Step 7: Commit**

```powershell
git add Cargo.toml crates/anno-corpus-store
git commit -m "feat: add corpus registry store"
```

---

### Task 3: Add MCP Corpus Service And Tools

**Files:**
- Modify: `crates/anno-rag-mcp/Cargo.toml`
- Create: `crates/anno-rag-mcp/src/corpus.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`
- Test: `crates/anno-rag-mcp/src/corpus.rs`, `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Add MCP dependencies**

In `crates/anno-rag-mcp/Cargo.toml`, add:

```toml
anno-corpus-core  = { workspace = true }
anno-corpus-store = { workspace = true }
```

- [ ] **Step 2: Create corpus service**

Create `crates/anno-rag-mcp/src/corpus.rs`:

```rust
use anno_corpus_core::{CorpusId, CorpusProfile};
use anno_corpus_store::{CorpusStore, RegisterCorpusResult};
use anno_rag::config::AnnoRagConfig;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct CorpusWire {
    pub corpus_id: String,
    pub label: String,
    pub health: String,
}

pub struct CorpusService {
    store: CorpusStore,
}

impl CorpusService {
    pub fn open(cfg: &AnnoRagConfig) -> anno_corpus_store::Result<Self> {
        Ok(Self {
            store: CorpusStore::open(corpus_db_path(cfg))?,
        })
    }

    pub fn register_index_root(&self, path: &str, profile: &str) -> anno_corpus_store::Result<RegisterCorpusResult> {
        let profiles = match profile {
            "general" => vec![CorpusProfile::Knowledge],
            "legal" => vec![CorpusProfile::Legal],
            "all" => vec![CorpusProfile::All],
            _ => vec![CorpusProfile::All],
        };
        self.store.register_root(path, &profiles)
    }

    pub fn store(&self) -> &CorpusStore {
        &self.store
    }
}

pub fn corpus_db_path(cfg: &AnnoRagConfig) -> PathBuf {
    cfg.data_dir.join("corpus.sqlite3")
}

pub fn parse_corpus_id(value: &str) -> Result<CorpusId, String> {
    uuid::Uuid::parse_str(value)
        .map(CorpusId::new)
        .map_err(|e| format!("bad corpus_id: {e}"))
}
```

- [ ] **Step 3: Wire lazy service in MCP server**

In `crates/anno-rag-mcp/src/lib.rs`, add module:

```rust
pub mod corpus;
```

Add a field to `AnnoRagServer`:

```rust
corpus: Arc<tokio::sync::OnceCell<crate::corpus::CorpusService>>,
```

Initialize it in both constructors:

```rust
corpus: Arc::new(tokio::sync::OnceCell::new()),
```

Add method:

```rust
async fn corpus(&self) -> Result<&crate::corpus::CorpusService, String> {
    self.corpus
        .get_or_try_init(|| async {
            crate::corpus::CorpusService::open(self.cfg.as_ref()).map_err(|e| e.to_string())
        })
        .await
}
```

- [ ] **Step 4: Add corpus tool handlers**

Add these MCP tool methods in the same impl block as other tools:

```rust
#[tool(description = "List indexed client corpora without exposing raw filesystem paths.")]
async fn corpus_list(&self) -> String {
    match self.corpus().await {
        Ok(service) => {
            let count = match service.store().corpus_count() {
                Ok(count) => count,
                Err(e) => return format!("Error: {e}"),
            };
            serde_json::json!({
                "ok": true,
                "count": count
            })
            .to_string()
        }
        Err(e) => format!("Error: {e}"),
    }
}

#[tool(description = "Return one corpus health summary by corpus_id.")]
async fn corpus_health(&self, Parameters(p): Parameters<CorpusGetParams>) -> String {
    match crate::corpus::parse_corpus_id(&p.corpus_id) {
        Ok(corpus_id) => serde_json::json!({
            "ok": true,
            "corpus_id": corpus_id.as_string(),
            "health": "ok"
        })
        .to_string(),
        Err(e) => format!("Error: {e}"),
    }
}
```

Add parameter type:

```rust
#[derive(Debug, Clone, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct CorpusGetParams {
    pub corpus_id: String,
}
```

This first handler returns minimal data. Task 9 will enrich it with binding health.

- [ ] **Step 5: Add tool names to health**

In `crates/anno-rag-mcp/src/health.rs`, add these names near unified tools:

```rust
"corpus_list",
"corpus_get",
"corpus_health",
```

- [ ] **Step 6: Add tests**

Add tests in `lib.rs` test module:

```rust
#[tokio::test]
async fn corpus_service_opens_under_data_dir() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let out = server.corpus_list().await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["ok"], true);
    assert_eq!(v["count"], 0);
    assert!(dir.path().join("corpus.sqlite3").exists());
}
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p anno-rag-mcp corpus
```

Expected: corpus-related MCP tests pass.

- [ ] **Step 8: Commit**

```powershell
git add crates/anno-rag-mcp Cargo.toml
git commit -m "feat: expose corpus registry tools"
```

---

### Task 4: Add Corpus-Qualified Legal Document Identity

**Files:**
- Modify: `crates/anno-rag/Cargo.toml`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Test: `crates/anno-rag/src/pipeline.rs`

- [ ] **Step 1: Add `anno-corpus-core` to `anno-rag`**

In `crates/anno-rag/Cargo.toml`, add:

```toml
anno-corpus-core = { workspace = true }
```

- [ ] **Step 2: Write failing duplicate-content test**

Add test in `crates/anno-rag/src/pipeline.rs` near existing `doc_uuid` tests:

```rust
#[test]
fn scoped_doc_uuid_distinguishes_same_bytes_across_corpora() {
    use anno_corpus_core::{ContentId, CorpusId};

    let bytes = b"same contract";
    let content = ContentId::from_bytes(bytes);
    let corpus_a = CorpusId::from_normalized_root("c:/clients/a");
    let corpus_b = CorpusId::from_normalized_root("c:/clients/b");

    let doc_a = scoped_doc_uuid(corpus_a, "contract.pdf", &content);
    let doc_b = scoped_doc_uuid(corpus_b, "contract.pdf", &content);

    assert_ne!(doc_a, doc_b);
    assert_eq!(doc_uuid(bytes), doc_uuid(bytes));
}
```

Expected before implementation: FAIL because `scoped_doc_uuid` does not exist.

- [ ] **Step 3: Implement scoped id helper and ingest context**

Add near `doc_uuid`:

```rust
use anno_corpus_core::{ContentId, CorpusId, DocumentInstanceId};

#[derive(Debug, Clone)]
pub struct LegalIngestScope {
    pub corpus_id: CorpusId,
    pub root: std::path::PathBuf,
}

#[must_use]
pub(crate) fn scoped_doc_uuid(
    corpus_id: CorpusId,
    normalized_relative_path: &str,
    content_id: &ContentId,
) -> Uuid {
    DocumentInstanceId::from_parts(corpus_id, normalized_relative_path, content_id).as_uuid()
}

fn normalized_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}
```

Change `ingest_one_counted` signature:

```rust
async fn ingest_one_counted(
    &self,
    path: &Path,
    output_dir: &Path,
    cfg: &AnnoRagConfig,
    scope: Option<&LegalIngestScope>,
) -> Result<IngestOutcome>
```

Replace doc id calculation:

```rust
let content_id = ContentId::from_bytes(&file_bytes);
let doc_id = match scope {
    Some(scope) => {
        let relative = normalized_relative_path(&scope.root, path);
        scoped_doc_uuid(scope.corpus_id, &relative, &content_id)
    }
    None => doc_uuid(&file_bytes),
};
```

Update current call sites:

```rust
self.ingest_one_counted(path, output_dir, &self.cfg, None).await?;
```

and:

```rust
match self.ingest_one_counted(&p, output_dir, &doc_cfg, None).await {
```

- [ ] **Step 4: Add scoped folder ingest**

Add method:

```rust
pub async fn ingest_folder_scoped(
    &self,
    folder: &Path,
    recursive: bool,
    output_dir: &Path,
    scope: LegalIngestScope,
) -> Result<usize> {
    self.ingest_folder_with_scope(folder, recursive, output_dir, Some(scope)).await
}
```

Refactor existing `ingest_folder` body into private:

```rust
async fn ingest_folder_with_scope(
    &self,
    folder: &Path,
    recursive: bool,
    output_dir: &Path,
    scope: Option<LegalIngestScope>,
) -> Result<usize>
```

Inside its file loop, call:

```rust
match self
    .ingest_one_counted(&p, output_dir, &doc_cfg, scope.as_ref())
    .await
{
```

Keep public `ingest_folder` as:

```rust
pub async fn ingest_folder(
    &self,
    folder: &Path,
    recursive: bool,
    output_dir: &Path,
) -> Result<usize> {
    self.ingest_folder_with_scope(folder, recursive, output_dir, None).await
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-rag scoped_doc_uuid_distinguishes_same_bytes_across_corpora
```

Expected: test passes.

Run targeted check:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag -Mode check
```

Expected: exit code 0.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag Cargo.toml
git commit -m "feat: qualify legal document ids by corpus"
```

---

### Task 5: Wire `index` To Register Corpus Bindings

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/corpus.rs`
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing `index` response test**

Add test:

```rust
#[tokio::test]
async fn index_general_returns_corpus_id() {
    let dir = tempfile::tempdir().expect("temp dir");
    let corpus_dir = dir.path().join("corpus");
    std::fs::create_dir_all(&corpus_dir).expect("corpus dir");
    std::fs::write(corpus_dir.join("note.txt"), "Bonjour index").expect("write corpus file");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().join("data"),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let out = server
        .index_impl_routing(IndexParams {
            path: corpus_dir.to_string_lossy().into_owned(),
            profile: "general".to_string(),
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert!(v["corpus_id"].as_str().is_some(), "{out}");
}
```

Expected before implementation: FAIL because response has no `corpus_id`.

- [ ] **Step 2: Register corpus at start of `index_impl_routing`**

At the top of `index_impl_routing`, after profile validation:

```rust
let corpus = match self.corpus().await {
    Ok(service) => match service.register_index_root(&p.path, &p.profile) {
        Ok(corpus) => corpus,
        Err(e) => {
            return serde_json::json!({
                "ok": false,
                "error": format!("corpus register: {e}")
            })
            .to_string();
        }
    },
    Err(e) => {
        return serde_json::json!({
            "ok": false,
            "error": format!("corpus service: {e}")
        })
        .to_string();
    }
};
```

Include in response:

```rust
"corpus_id": corpus.corpus_id.as_string(),
"corpus_label": corpus.label_pseudo,
```

- [ ] **Step 3: Bind knowledge source id**

After `knowledge_add_local_folder_impl` returns `source_id`, add:

```rust
if let Ok(service) = self.corpus().await {
    if let Err(e) = service.store().add_binding(
        corpus.corpus_id,
        anno_corpus_core::CorpusBindingKind::KnowledgeSource,
        &source_id,
        &serde_json::json!({"profile": p.profile}),
    ) {
        errors.push(format!("corpus knowledge binding: {e}"));
    }
}
```

- [ ] **Step 4: Bind legal folder and call scoped ingest**

Change `legal_ingest_impl` to accept optional corpus:

```rust
async fn legal_ingest_impl(
    &self,
    p: LegalIngestParams,
    corpus_id: Option<anno_corpus_core::CorpusId>,
) -> Result<serde_json::Value, String>
```

In `index_impl_routing`, call:

```rust
self.legal_ingest_impl(
    LegalIngestParams {
        folder: p.path.clone(),
        recursive: true,
    },
    Some(corpus.corpus_id),
)
.await
```

In `legal_ingest_impl`, use scoped ingest when `corpus_id` is present:

```rust
let ingest_result = if let Some(corpus_id) = corpus_id {
    pipeline
        .ingest_folder_scoped(
            folder,
            p.recursive,
            &out,
            anno_rag::pipeline::LegalIngestScope {
                corpus_id,
                root: folder.to_path_buf(),
            },
        )
        .await
} else {
    pipeline.ingest_folder(folder, p.recursive, &out).await
};
```

After success, bind the legal folder:

```rust
if let Some(corpus_id) = corpus_id {
    if let Ok(service) = self.corpus().await {
        service
            .store()
            .add_binding(
                corpus_id,
                anno_corpus_core::CorpusBindingKind::LegalFolder,
                &legal_folder_id(&p.folder),
                &serde_json::json!({"label": legal_folder_id(&p.folder)}),
            )
            .map_err(|e| e.to_string())?;
    }
}
```

- [ ] **Step 5: Keep direct `legal_ingest` compatible**

Change the tool handler to call:

```rust
match self.legal_ingest_impl(p, None).await {
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p anno-rag-mcp index_general_returns_corpus_id
```

Expected: test passes.

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: exit code 0.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp
git commit -m "feat: register corpus during mcp index"
```

---

### Task 6: Scope Knowledge Search

**Files:**
- Modify: `crates/anno-knowledge-core/src/query.rs`
- Modify: `crates/anno-knowledge-store/src/control_store.rs`
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Test: `crates/anno-knowledge-store/src/control_store.rs`

- [ ] **Step 1: Write failing FTS scope test**

Add a test in `control_store.rs` that creates two sources, indexes one object per source, and searches with a source filter:

```rust
#[test]
fn search_fast_filters_by_source_id() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = KnowledgeControlStore::open(dir.path().join("knowledge.sqlite3")).expect("open");
    let a = store
        .register_local_folder(LocalFolderRegistration {
            stable_key: "c:/clients/a".into(),
            source_label_pseudo: "a".into(),
            scope_label_pseudo: "a".into(),
            provider_key: "c:/clients/a".into(),
        })
        .expect("register a");
    let b = store
        .register_local_folder(LocalFolderRegistration {
            stable_key: "c:/clients/b".into(),
            source_label_pseudo: "b".into(),
            scope_label_pseudo: "b".into(),
            provider_key: "c:/clients/b".into(),
        })
        .expect("register b");
    let input_a = commit_input(
        a.scope_id,
        a.account_id,
        a.source_id,
        [1u8; 32],
        "Contrat Alpha",
    );
    let input_b = commit_input(
        b.scope_id,
        b.account_id,
        b.source_id,
        [2u8; 32],
        "Contrat Beta",
    );
    store.commit_object(&input_a).expect("commit a");
    store.commit_object(&input_b).expect("commit b");

    let hits = store
        .search_fast(
            &KnowledgeSearchRequest::new("contrat")
                .with_top_k(10)
                .with_source_ids(vec![a.source_id]),
        )
        .expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, a.source_id);
}
```

- [ ] **Step 2: Extend request and hit**

In `query.rs`, import ids:

```rust
use crate::ids::{ChunkId, ObjectId, RevisionId, ScopeId, SourceId};
```

Add fields:

```rust
pub source_ids: Vec<SourceId>,
pub scope_ids: Vec<ScopeId>,
```

Initialize them in `new`:

```rust
source_ids: Vec::new(),
scope_ids: Vec::new(),
```

Add builders:

```rust
#[must_use]
pub fn with_source_ids(mut self, source_ids: Vec<SourceId>) -> Self {
    self.source_ids = source_ids;
    self
}

#[must_use]
pub fn with_scope_ids(mut self, scope_ids: Vec<ScopeId>) -> Self {
    self.scope_ids = scope_ids;
    self
}
```

Add hit fields:

```rust
pub source_id: SourceId,
pub scope_id: ScopeId,
```

- [ ] **Step 3: Update FTS SQL**

In `KnowledgeControlStore::search_fast`, replace direct FTS query with a join:

```sql
SELECT f.chunk_id, f.object_id, f.revision_id, f.source_kind, f.object_type, f.title_pseudo,
       snippet(knowledge_objects_fts, 6, '[', ']', '[snip]', 20) AS snippet,
       bm25(knowledge_objects_fts) AS score,
       o.source_id,
       o.scope_id
FROM knowledge_objects_fts f
JOIN knowledge_objects o ON o.object_id = f.object_id
WHERE knowledge_objects_fts MATCH ?1
```

Append filters when present:

```rust
if !request.source_ids.is_empty() {
    sql.push_str(" AND o.source_id IN (");
    sql.push_str(&repeat_vars(request.source_ids.len()));
    sql.push(')');
}
if !request.scope_ids.is_empty() {
    sql.push_str(" AND o.scope_id IN (");
    sql.push_str(&repeat_vars(request.scope_ids.len()));
    sql.push(')');
}
sql.push_str(" ORDER BY score LIMIT ?");
```

Use `rusqlite::params_from_iter` to bind query, ids, and top_k.

- [ ] **Step 4: Return source identity in hits**

Map row columns:

```rust
source_id: SourceId::new(parse_uuid(row.get::<_, String>(8)?)?),
scope_id: ScopeId::new(parse_uuid(row.get::<_, String>(9)?)?),
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-knowledge-store search_fast_filters_by_source_id
```

Expected: test passes.

Run:

```powershell
cargo test -p anno-knowledge-core
```

Expected: query type tests pass.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-knowledge-core crates/anno-knowledge-store
git commit -m "feat: scope knowledge fts by source"
```

---

### Task 7: Add `EffectiveCorpus` Guard And Sanitize MCP Search

**Files:**
- Modify: `crates/anno-rag-mcp/src/corpus.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/knowledge.rs`
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [ ] **Step 1: Write failing multi-corpus refusal test**

Add test:

```rust
#[tokio::test]
async fn search_requires_corpus_when_multiple_exist() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let corpus = server.corpus().await.expect("corpus");
    corpus.register_index_root("c:/clients/a", "all").expect("a");
    corpus.register_index_root("c:/clients/b", "all").expect("b");

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".into(),
            top_k: 5,
            mode: Some("fast".into()),
            scope: Some("knowledge".into()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: false,
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("corpus_id is required"));
}
```

- [ ] **Step 2: Add params**

Extend `SearchUnifiedParams`:

```rust
#[serde(default)]
pub corpus_id: Option<String>,
#[serde(default)]
pub allow_cross_corpus: bool,
```

Extend `crate::knowledge::KnowledgeSearchParams`:

```rust
#[serde(default)]
pub corpus_id: Option<String>,
#[serde(default)]
pub allow_cross_corpus: bool,
```

- [ ] **Step 3: Implement resolver**

In `corpus.rs`, add:

```rust
use anno_corpus_core::{CorpusGuardError, EffectiveCorpus};

impl CorpusService {
    pub fn resolve_effective(
        &self,
        corpus_id: Option<&str>,
        allow_cross_corpus: bool,
    ) -> Result<EffectiveCorpus, CorpusGuardError> {
        let count = self.store.corpus_count().map_err(|_| CorpusGuardError::NoCorpus)?;
        if let Some(value) = corpus_id {
            let parsed = parse_corpus_id(value).map_err(|_| CorpusGuardError::UnknownCorpus(value.to_string()))?;
            return Ok(EffectiveCorpus::Single(parsed));
        }
        if allow_cross_corpus {
            return Ok(EffectiveCorpus::CrossCorpus);
        }
        match count {
            0 => Err(CorpusGuardError::NoCorpus),
            1 => {
                let one = self.store.single_corpus_id().map_err(|_| CorpusGuardError::NoCorpus)?;
                Ok(EffectiveCorpus::Single(one))
            }
            _ => Err(CorpusGuardError::CorpusRequired),
        }
    }
}
```

Add `single_corpus_id()` to `CorpusStore`.

- [ ] **Step 4: Guard unified search**

At start of `search_impl_routing`, resolve corpus:

```rust
let effective = match self.corpus().await {
    Ok(service) => match service.resolve_effective(p.corpus_id.as_deref(), p.allow_cross_corpus) {
        Ok(effective) => effective,
        Err(e) => {
            return serde_json::json!({
                "ok": false,
                "error": e.to_string()
            })
            .to_string();
        }
    },
    Err(e) => {
        return serde_json::json!({
            "ok": false,
            "error": e
        })
        .to_string();
    }
};
```

- [ ] **Step 5: Sanitize legacy search wire**

Replace `SearchHitWire` fields:

```rust
struct SearchHitWire {
    doc_id: String,
    chunk_id: String,
    corpus_id: Option<String>,
    document_label: Option<String>,
    chunk_idx: u32,
    text_pseudo: String,
    page: Option<u32>,
    char_start: u32,
    char_end: u32,
    score: f32,
}
```

Do not include `source_path` or `folder_path`.

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p anno-rag-mcp search_requires_corpus_when_multiple_exist
```

Expected: test passes.

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: exit code 0.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp crates/anno-corpus-store
git commit -m "feat: guard mcp search by effective corpus"
```

---

### Task 8: Scope Legal Search And Case-File Extraction

**Files:**
- Modify: `crates/anno-rag/src/store.rs`
- Modify: `crates/anno-rag/src/pipeline.rs`
- Modify: `crates/anno-rag/src/legal/store.rs`
- Modify: `crates/anno-rag/src/legal/kg.rs`
- Modify: `crates/anno-rag/src/legal/extract.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Test: `crates/anno-rag/src/store.rs`, `crates/anno-rag/src/legal/extract.rs`, `crates/anno-rag-mcp/src/lib.rs`

- [x] **Step 1: Add store helper test**

In `store.rs`, add:

```rust
#[tokio::test]
#[ignore = "opens LanceDB (~30s); run with --ignored"]
async fn search_filtered_to_docs_uses_only_allowed_doc_ids() {
    let (_dir, cfg) = fresh_cfg(8);
    let store = Store::open(&cfg).await.expect("open");
    let a = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"a");
    let b = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b");
    let mk = |doc_id: uuid::Uuid, text: &str| ChunkRecord {
        doc_id,
        source_path: format!("{doc_id}.txt"),
        folder_path: "corpus".into(),
        chunk_idx: 0,
        text_pseudo: text.into(),
        page: None,
        char_start: 0,
        char_end: text.len() as u32,
        vector: vec![0.0; 8],
    };
    store
        .upsert(vec![mk(a, "Alpha contrat"), mk(b, "Beta contrat")])
        .await
        .expect("upsert");
    let hits = store
        .search_filtered_to_docs("contrat", &[0.0; 8], 10, &[a])
        .await
        .expect("search");
    assert!(hits.iter().all(|hit| hit.doc_id == a));
}
```

- [x] **Step 2: Implement `search_filtered_to_docs`**

Add method:

```rust
pub async fn search_filtered_to_docs(
    &self,
    query_text: &str,
    query_vec: &[f32],
    k: usize,
    allowed_doc_ids: &[Uuid],
) -> Result<Vec<SearchHit>> {
    if allowed_doc_ids.is_empty() {
        return Ok(Vec::new());
    }
    let chunk_ids = self.chunk_ids_for_docs(allowed_doc_ids).await?;
    self.search_filtered_to_chunks(query_text, query_vec, k, &chunk_ids).await
}
```

Add `chunk_ids_for_docs`:

```rust
pub async fn chunk_ids_for_docs(&self, doc_ids: &[Uuid]) -> Result<Vec<Uuid>> {
    if doc_ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = doc_id_filter_sql(doc_ids);
    let stream = self
        .tbl
        .query()
        .select(lancedb::query::Select::columns(&["chunk_id"]))
        .only_if(filter)
        .execute()
        .await?;
    let batches: Vec<RecordBatch> = stream.try_collect().await?;
    let mut out = Vec::new();
    for batch in &batches {
        let arr = get_col::<FixedSizeBinaryArray>(batch, "chunk_id")?;
        for idx in 0..batch.num_rows() {
            out.push(uuid_from_fsb(arr, idx)?);
        }
    }
    Ok(out)
}
```

- [x] **Step 3: Add scoped legal search in pipeline**

Add method:

```rust
pub async fn legal_search_scoped(
    &self,
    query: &str,
    top_k: usize,
    filters: crate::legal::types::LegalSearchFilters,
    allowed_doc_ids: &[uuid::Uuid],
) -> Result<Vec<crate::legal::types::LegalSearchHit>> {
    let entities = self.detector_get_or_init()?.detect(query)?;
    let pseudo_q = self.vault.pseudonymize(query, &entities).await?;
    let qv = self.embedder().await?.embed_query(&pseudo_q)?;
    let corpus_chunk_ids = self.store.chunk_ids_for_docs(allowed_doc_ids).await?;
    let allowed = if filters.has_any_filter() {
        let business = self
            .legal_store
            .filter_chunk_ids(&filters, top_k.saturating_mul(20).max(100))
            .await?;
        intersect_uuids(&corpus_chunk_ids, &business)
    } else {
        corpus_chunk_ids
    };
    let chunk_hits = self
        .store
        .search_filtered_to_chunks(&pseudo_q, &qv, top_k, &allowed)
        .await?;
    Ok(chunk_hits
        .into_iter()
        .map(|h| crate::legal::types::LegalSearchHit {
            chunk_id: h.chunk_id,
            doc_id: h.doc_id,
            text_pseudo: h.text_pseudo,
            score: h.score,
            enrichment: None,
        })
        .collect())
}
```

Add helper:

```rust
fn intersect_uuids(a: &[uuid::Uuid], b: &[uuid::Uuid]) -> Vec<uuid::Uuid> {
    let b: std::collections::BTreeSet<_> = b.iter().copied().collect();
    a.iter().copied().filter(|id| b.contains(id)).collect()
}
```

- [x] **Step 4: Replace raw Cypher case-file extraction**

In `legal/kg.rs`, add trait methods to `LegalKnowledgeGraph`:

```rust
async fn case_file_documents(&self, dossier_id: &str) -> Result<Vec<std::collections::HashMap<String, String>>>;
async fn case_file_parties(&self, dossier_id: &str) -> Result<Vec<std::collections::HashMap<String, String>>>;
async fn case_file_events(&self, dossier_id: &str) -> Result<Vec<std::collections::HashMap<String, String>>>;
```

Implement them for SQLite using parameterized SQL against existing node/edge tables. Use the same returned keys currently expected by `extract_case_file`: `doc_id`, `doc_type`, `value`, `role`, `kind`, `event_date`, `cid`.

In `legal/extract.rs`, replace the raw `kg.cypher` document, party, and event calls with:

```rust
let doc_rows = kg.case_file_documents(dossier_id).await?;
let party_rows = kg.case_file_parties(dossier_id).await?;
let event_rows = kg.case_file_events(dossier_id).await?;
```

- [x] **Step 5: Wire legal search through corpus docs**

In MCP `legal_search_impl`, when `EffectiveCorpus::Single(corpus_id)` is present:

```rust
let doc_ids = self
    .corpus()
    .await?
    .store()
    .document_ids_for_corpus(corpus_id, "legal")
    .map_err(|e| e.to_string())?;
pipeline
    .legal_search_scoped(&p.query, p.top_k, filters, &doc_ids)
    .await
```

For `EffectiveCorpus::CrossCorpus`, keep existing `pipeline.legal_search`.

- [x] **Step 6: Run tests**

Fast verification used during implementation:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-rag --lib
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-rag-mcp --lib
cargo test -p anno-corpus-core
cargo test -p anno-corpus-store
```

The ignored LanceDB test was added but not run in the fast loop. `cargo test -p anno-rag extract_case_file --lib` was attempted and stopped after 5 minutes without output to avoid a cold-cache-style harness build.

Run:

```powershell
cargo test -p anno-rag search_filtered_to_docs_uses_only_allowed_doc_ids -- --ignored
cargo test -p anno-rag extract_case_file
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: all commands exit 0.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag crates/anno-rag-mcp
git commit -m "feat: scope legal search by corpus"
```

---

### Task 9: Bind And Guard Tabular Reviews

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/tabular/chunk_source.rs`
- Modify: `crates/anno-rag-tabular/src/storage/rows.rs`
- Modify: `crates/anno-rag-tabular/src/storage/cells.rs`
- Test: `crates/anno-rag-mcp/src/lib.rs`, `crates/anno-rag-tabular/src/storage/rows.rs`, `crates/anno-rag-tabular/src/storage/cells.rs`

- [x] **Step 1: Add `corpus_id` to review create params**

Extend `ReviewCreateParams`:

```rust
#[serde(default)]
pub corpus_id: Option<String>,
```

- [x] **Step 2: Write failing cross-corpus add rows test**

Add MCP test:

```rust
#[tokio::test]
async fn review_add_rows_rejects_doc_outside_review_corpus() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let corpus_service = server.corpus().await.expect("corpus");
    let a = corpus_service.register_index_root("c:/clients/a", "all").expect("a");
    let b = corpus_service.register_index_root("c:/clients/b", "all").expect("b");
    let doc_b = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"b-doc");
    corpus_service
        .store()
        .add_document(
            b.corpus_id,
            anno_corpus_core::DocumentInstanceId::new(doc_b),
            "legal",
            "c:/clients/b/doc.pdf",
            Some("doc.pdf"),
            &anno_corpus_core::ContentId::from_bytes(b"doc"),
            &serde_json::json!({}),
        )
        .expect("doc b");

    let created = server
        .create_review_from_params(ReviewCreateParams {
            name: "A review".into(),
            template_id: None,
            scope_folder: None,
            corpus_id: Some(a.corpus_id.as_string()),
        })
        .await
        .expect("review");

    let out = server
        .review_add_rows(Parameters(ReviewAddRowsParams {
            review_id: created.review_id,
            doc_ids: vec![doc_b.to_string()],
            force_reextract: false,
        }))
        .await;
    assert!(out.contains("outside review corpus") || out.contains(&doc_b.to_string()));
}
```

- [x] **Step 3: Bind review to corpus**

After review creation succeeds:

```rust
if let Some(corpus_id) = p.corpus_id.as_deref() {
    let corpus_id = crate::corpus::parse_corpus_id(corpus_id)?;
    self.corpus()
        .await?
        .store()
        .add_binding(
            corpus_id,
            anno_corpus_core::CorpusBindingKind::TabularReview,
            &review_id.0.to_string(),
            &serde_json::json!({"name": p.name}),
        )
        .map_err(|e| e.to_string())?;
}
```

Make `corpus_id` required when multiple corpora exist by calling `resolve_effective` in `create_review_from_params`.

- [x] **Step 4: Validate doc ids in `review_add_rows`**

After loading the review, find its corpus binding:

```rust
let review_corpus = self
    .corpus()
    .await
    .map_err(|e| e.to_string())?
    .store()
    .corpus_for_binding(anno_corpus_core::CorpusBindingKind::TabularReview, &review_id.0.to_string())
    .map_err(|e| e.to_string())?;
```

Filter parsed doc ids:

```rust
let allowed_docs = self
    .corpus()
    .await
    .map_err(|e| e.to_string())?
    .store()
    .document_ids_for_corpus(review_corpus, "legal")
    .map_err(|e| e.to_string())?;
let allowed: std::collections::BTreeSet<_> = allowed_docs.into_iter().collect();
let parsed = parse_review_doc_ids(&p.doc_ids);
let mut failed = parsed.failed;
let valid_in_corpus: Vec<_> = parsed
    .valid
    .into_iter()
    .filter(|doc_id| {
        let ok = allowed.contains(doc_id);
        if !ok {
            failed.push(format!("{doc_id}: outside review corpus"));
        }
        ok
    })
    .collect();
```

Pass `valid_in_corpus` into `filter_ingested_doc_ids`.

- [x] **Step 5: Add tabular delete helpers**

In `rows.rs`:

```rust
pub async fn delete_for_review(&self, review_id: ReviewId) -> Result<()> {
    let hex = uuid_to_filter_lit(review_id.0);
    self.tbl.delete(&format!("review_id = X'{hex}'")).await?;
    Ok(())
}
```

In `cells.rs`:

```rust
pub async fn delete_for_review(&self, review_id: ReviewId) -> Result<()> {
    let hex = uuid_to_filter_lit(review_id.0);
    self.tbl.delete(&format!("review_id = X'{hex}'")).await?;
    Ok(())
}
```

Add tests in both files proving only matching review rows are removed.

- [x] **Step 6: Run tests**

Fast verification used during implementation:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-rag-tabular --lib
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-corpus-store
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-rag-mcp --lib
cargo test -p anno-corpus-store binding_and_document_round_trip
```

`cargo test -p anno-rag-tabular delete_for_review` was attempted and stopped after 5 minutes without output to avoid a heavy LanceDB test harness run.

Run:

```powershell
cargo test -p anno-rag-tabular delete_for_review
cargo test -p anno-rag-mcp review_add_rows_rejects_doc_outside_review_corpus
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```powershell
git add crates/anno-rag-mcp crates/anno-rag-tabular
git commit -m "feat: bind tabular reviews to corpus"
```

---

### Task 10: Implement Corpus-Aware Forget And Source Compatibility

**Files:**
- Modify: `crates/anno-corpus-store/src/store.rs`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/corpus.rs`
- Test: `crates/anno-rag-mcp/src/lib.rs`

- [x] **Step 1: Add binding lookup/delete APIs**

Add to `CorpusStore`:

```rust
pub fn bindings_for_corpus(&self, corpus_id: CorpusId) -> Result<Vec<CorpusBindingRow>> {
    let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
    ensure_corpus_exists(&conn, corpus_id)?;
    let mut stmt = conn.prepare(
        "SELECT binding_kind, binding_id FROM corpus_bindings
         WHERE corpus_id = ?1 ORDER BY binding_kind, binding_id",
    )?;
    let rows = stmt.query_map(params![corpus_id.as_string()], |row| {
        Ok(CorpusBindingRow {
            corpus_id,
            binding_kind: parse_binding_kind(&row.get::<_, String>(0)?),
            binding_id: row.get(1)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_corpus_registry_rows(&self, corpus_id: CorpusId) -> Result<()> {
    let conn = self.conn.lock().expect("corpus sqlite mutex poisoned");
    conn.execute(
        "DELETE FROM corpora WHERE corpus_id = ?1",
        params![corpus_id.as_string()],
    )?;
    Ok(())
}
```

Implement `parse_binding_kind`.

- [x] **Step 2: Write failing forget test**

Add MCP test:

```rust
#[tokio::test]
async fn forget_corpus_reports_all_backend_buckets() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let server = AnnoRagServer::new_lazy(cfg, [0u8; 32]);
    let corpus = server
        .corpus()
        .await
        .expect("corpus")
        .register_index_root("c:/clients/a", "all")
        .expect("register");
    let out = server
        .forget_impl_routing(ForgetParams {
            target: corpus.corpus_id.as_string(),
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["ok"], true);
    assert!(v["removed"].get("knowledge_objects").is_some());
    assert!(v["removed"].get("legal_chunks").is_some());
    assert!(v["removed"].get("tabular_reviews").is_some());
}
```

- [x] **Step 3: Route UUID target as corpus first**

At the top of `forget_impl_routing`, before the legacy UUID knowledge branch:

```rust
if let Ok(uuid) = uuid::Uuid::parse_str(&p.target) {
    let corpus_id = anno_corpus_core::CorpusId::new(uuid);
    if let Ok(service) = self.corpus().await {
        if service.store().corpus_exists(corpus_id).unwrap_or(false) {
            return self.forget_corpus(corpus_id).await;
        }
    }
}
```

Add helper:

```rust
async fn forget_corpus(&self, corpus_id: anno_corpus_core::CorpusId) -> String {
    let mut knowledge_removed = 0u64;
    let mut legal_removed = 0u64;
    let mut tabular_reviews = 0u64;
    let mut errors = Vec::<String>::new();

    let service = match self.corpus().await {
        Ok(service) => service,
        Err(e) => return format!("Error: {e}"),
    };
    let bindings = match service.store().bindings_for_corpus(corpus_id) {
        Ok(bindings) => bindings,
        Err(e) => return format!("Error: {e}"),
    };

    for binding in bindings {
        match binding.binding_kind {
            anno_corpus_core::CorpusBindingKind::KnowledgeSource => {
                match self.knowledge_forget_impl(KnowledgeForgetParams { source_id: binding.binding_id }).await {
                    Ok(n) => knowledge_removed += n,
                    Err(e) => errors.push(format!("knowledge forget: {e}")),
                }
            }
            anno_corpus_core::CorpusBindingKind::LegalFolder => {
                if let Some(pipeline) = self.pipeline_arc() {
                    match self.resolve_legal_folder_id(&pipeline, &binding.binding_id).await {
                        Ok(Some(path)) => match pipeline.forget_legal_folder_path(&path).await {
                            Ok(n) => legal_removed += n,
                            Err(e) => errors.push(format!("legal forget: {e}")),
                        },
                        Ok(None) => {}
                        Err(e) => errors.push(format!("legal resolve: {e}")),
                    }
                }
            }
            anno_corpus_core::CorpusBindingKind::TabularReview => {
                match self.forget_tabular_review(&binding.binding_id).await {
                    Ok(()) => tabular_reviews += 1,
                    Err(e) => errors.push(format!("tabular forget: {e}")),
                }
            }
            anno_corpus_core::CorpusBindingKind::LegalDocument => {}
        }
    }

    if errors.is_empty() {
        if let Err(e) = service.store().delete_corpus_registry_rows(corpus_id) {
            errors.push(format!("corpus registry delete: {e}"));
        }
    }

    serde_json::json!({
        "ok": errors.is_empty(),
        "removed": {
            "knowledge_objects": knowledge_removed,
            "legal_chunks": legal_removed,
            "tabular_reviews": tabular_reviews
        },
        "errors": if errors.is_empty() { serde_json::Value::Null } else { serde_json::json!(errors) }
    })
    .to_string()
}
```

- [x] **Step 4: Add tabular cascade helper**

Add:

```rust
async fn forget_tabular_review(&self, review_id: &str) -> Result<(), String> {
    let review_uuid = uuid::Uuid::parse_str(review_id).map_err(|e| e.to_string())?;
    let review_id = anno_rag_tabular::ReviewId(review_uuid);
    let ts = self.tabular_storage().await.map_err(|e| e.to_string())?;
    ts.cells.delete_for_review(review_id).await.map_err(|e| e.to_string())?;
    ts.rows.delete_for_review(review_id).await.map_err(|e| e.to_string())?;
    ts.columns.delete_for_review(review_id).await.map_err(|e| e.to_string())?;
    ts.reviews.delete(review_id).await.map_err(|e| e.to_string())?;
    Ok(())
}
```

- [x] **Step 5: Run tests**

Fast verification used during implementation:

```powershell
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-corpus-store
$env:CARGO_BUILD_JOBS='1'; cargo check -p anno-rag-mcp --lib
cargo test -p anno-corpus-store binding_and_document_round_trip
```

The MCP forget test was added but not run in the fast loop because previous `anno-rag-mcp` test harness runs were too heavy in this session.

Run:

```powershell
cargo test -p anno-rag-mcp forget_corpus_reports_all_backend_buckets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-corpus-store crates/anno-rag-mcp
git commit -m "feat: forget corpus across backends"
```

---

### Task 11: Sustained MCP Integration Tests

**Files:**
- Create or modify: `crates/anno-rag-mcp/tests/corpus_scope.rs`
- Modify: `crates/anno-rag-mcp/Cargo.toml` if test dependencies are missing

Implementation note: the routing helpers targeted by this task are currently `pub(crate)`, and running `index_impl_routing` in an external integration test would also exercise heavier indexing paths. The fast-loop adaptation strengthens the existing internal MCP test `search_requires_corpus_when_multiple_exist` to verify refusal without leaking raw corpus paths.

- [ ] **Step 1: Add two-corpus test fixture**

Create `crates/anno-rag-mcp/tests/corpus_scope.rs` with helpers:

```rust
use anno_rag::config::AnnoRagConfig;
use anno_rag_mcp::{AnnoRagServer, IndexParams, SearchUnifiedParams};

fn test_config(root: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: root.join("data"),
        ..Default::default()
    }
}

fn write_fixture(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let a = root.join("client-a");
    let b = root.join("client-b");
    std::fs::create_dir_all(&a).expect("client a");
    std::fs::create_dir_all(&b).expect("client b");
    std::fs::write(a.join("alpha.txt"), "Contrat Alpha exclusif").expect("write a");
    std::fs::write(b.join("beta.txt"), "Contrat Beta exclusif").expect("write b");
    std::fs::write(a.join("same.txt"), "Document commun").expect("same a");
    std::fs::write(b.join("same.txt"), "Document commun").expect("same b");
    (a, b)
}
```

- [ ] **Step 2: Add refusal and raw path tests**

Add:

```rust
#[tokio::test]
async fn unscoped_search_refuses_after_two_corpora() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (a, b) = write_fixture(dir.path());
    let server = AnnoRagServer::new_lazy(test_config(dir.path()), [0u8; 32]);
    let out_a = server
        .index_impl_routing(IndexParams {
            path: a.to_string_lossy().into_owned(),
            profile: "general".into(),
        })
        .await;
    let out_b = server
        .index_impl_routing(IndexParams {
            path: b.to_string_lossy().into_owned(),
            profile: "general".into(),
        })
        .await;
    assert!(out_a.contains("corpus_id"));
    assert!(out_b.contains("corpus_id"));

    let out = server
        .search_impl_routing(SearchUnifiedParams {
            query: "contrat".into(),
            top_k: 5,
            mode: Some("fast".into()),
            scope: Some("knowledge".into()),
            filters: None,
            corpus_id: None,
            allow_cross_corpus: false,
        })
        .await;
    let v: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(v["ok"], false);
    assert!(!out.contains(&a.to_string_lossy().to_string()));
    assert!(!out.contains(&b.to_string_lossy().to_string()));
}
```

- [ ] **Step 3: Add same-content document test**

Add:

```rust
#[tokio::test]
async fn byte_identical_documents_remain_corpus_specific() {
    let dir = tempfile::tempdir().expect("temp dir");
    let corpus_a = anno_corpus_core::CorpusId::from_normalized_root("c:/clients/a");
    let corpus_b = anno_corpus_core::CorpusId::from_normalized_root("c:/clients/b");
    let content = anno_corpus_core::ContentId::from_bytes(b"Document commun");
    let doc_a = anno_corpus_core::DocumentInstanceId::from_parts(corpus_a, "same.txt", &content);
    let doc_b = anno_corpus_core::DocumentInstanceId::from_parts(corpus_b, "same.txt", &content);
    assert_ne!(doc_a, doc_b);
    drop(dir);
}
```

- [ ] **Step 4: Run integration tests**

Run:

```powershell
cargo test -p anno-rag-mcp --test corpus_scope
```

Expected: all tests pass.

- [ ] **Step 5: Run targeted package checks**

Run:

```powershell
cargo test -p anno-corpus-core
cargo test -p anno-corpus-store
cargo test -p anno-knowledge-core
cargo test -p anno-knowledge-store
cargo test -p anno-rag-tabular delete_for_review
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add crates/anno-rag-mcp/tests crates/anno-rag-mcp/Cargo.toml
git commit -m "test: cover mcp corpus scoping"
```

---

## Review Checklist

- [ ] `index(path, profile)` returns `corpus_id`.
- [ ] Multiple corpora make unscoped sensitive tools refuse.
- [ ] `allow_cross_corpus=true` is explicit and auditable.
- [ ] Knowledge FTS filters through `source_id` or `scope_id`.
- [ ] Legal search never falls back to global search for a selected corpus.
- [ ] Byte-identical files in two corpora produce distinct document instance ids.
- [ ] MCP search responses contain no raw `source_path`, `folder_path`, indexed root, or absolute path.
- [ ] Tabular reviews bind to a corpus and reject foreign docs.
- [ ] `forget(corpus_id)` removes knowledge, legal, and tabular bindings.
- [ ] `legal_extract_case_file` uses typed graph methods, not raw Cypher.
- [ ] Direct legacy tools remain callable but share the same guards where they expose client data.

## Final Verification

Run:

```powershell
cargo test -p anno-corpus-core
cargo test -p anno-corpus-store
cargo test -p anno-knowledge-core
cargo test -p anno-knowledge-store
cargo test -p anno-rag-tabular delete_for_review
cargo test -p anno-rag extract_case_file
cargo test -p anno-rag-mcp corpus
cargo test -p anno-rag-mcp --test corpus_scope
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: all commands exit 0.

Do not run broad `cargo build --workspace` unless the targeted loop is clean and a release verification explicitly requires it.
