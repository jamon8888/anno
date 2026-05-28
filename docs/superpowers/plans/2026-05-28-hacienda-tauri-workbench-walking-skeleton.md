# Hacienda Tauri Workbench Walking Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first usable Tauri desktop skeleton for Hacienda: a local app shell that opens a client folder, scans text documents read-only, creates anonymized editable working documents, and exposes the flow through a quiet legal-workbench UI.

**Architecture:** Add a testable Rust core crate (`hacienda-workbench-core`) for matter metadata, folder ingestion, regex PII anonymization via `anno-rag`, revisioned working documents, and append-only audit events. Add a Tauri v2 app (`apps/hacienda-workbench`) that invokes the core through commands and renders an operational dashboard, matter view, and document atelier. This plan intentionally limits Phase 1 to local folders and text/Markdown files; broad PDF/DOCX/XLSX/PPTX/email/OCR, SharePoint/OneDrive, GLiNER2/LoRA profiles, signed installers, and updater/resource packs get separate plans.

**Tech Stack:** Rust 1.95, Tauri v2, Vite + React + TypeScript, lucide-react, SQLite through `rusqlite`, `anno-rag::detect::detect_patterns`, `anno-rag::vault::Vault`, Tauri dialog plugin, existing `scripts/dev-fast.ps1` local Rust loop.

---

## Source References

This plan follows the approved spec:

- `docs/superpowers/specs/2026-05-28-hacienda-tauri-client-workbench-design.md`

Current Tauri references checked while writing this plan:

- Tauri project creation and dev commands: https://v2.tauri.app/start/create-project/
- Tauri Rust command invocation from frontend: https://v2.tauri.app/develop/calling-rust/
- Tauri dialog plugin for folder selection: https://v2.tauri.app/plugin/dialog/

## Scope

Implement only the Phase 1 walking skeleton:

- Tauri app shell.
- Embedded Rust engine boundary.
- Local folder ingestion.
- Normalized text document model.
- Basic model-free PII detection and anonymized working document generation.
- SQLite metadata store.
- Separate encrypted vault file through existing `anno-rag::vault`.
- Basic append-only audit log.
- UI for dashboard, matter details, document atelier, save revision.

Do not implement in this plan:

- faithful source-format editing;
- PDF/DOCX/XLSX/PPTX/email/OCR extraction;
- SharePoint/OneDrive;
- network folder hardening beyond normal filesystem paths;
- GLiNER2/LoRA profile selection;
- legal workflow checklist runtime;
- packaging/signing/updater/resource packs.

## File Structure

- Modify `Cargo.toml`
  - Add `crates/hacienda-workbench-core`.
  - Add `apps/hacienda-workbench/src-tauri`.

- Create `crates/hacienda-workbench-core/Cargo.toml`
  - Rust core dependencies.

- Create `crates/hacienda-workbench-core/src/lib.rs`
  - Public module exports.

- Create `crates/hacienda-workbench-core/src/error.rs`
  - Core error and `Result`.

- Create `crates/hacienda-workbench-core/src/model.rs`
  - Serializable API/domain types shared with Tauri.

- Create `crates/hacienda-workbench-core/src/store.rs`
  - SQLite schema and persistence.

- Create `crates/hacienda-workbench-core/src/ingest.rs`
  - Local text/Markdown folder scanner.

- Create `crates/hacienda-workbench-core/src/anonymize.rs`
  - Model-free PII detection + vault pseudonymization.

- Create `crates/hacienda-workbench-core/src/engine.rs`
  - High-level app service used by Tauri commands.

- Create `apps/hacienda-workbench/package.json`
  - App-local frontend scripts and dependencies.

- Create `apps/hacienda-workbench/index.html`
- Create `apps/hacienda-workbench/tsconfig.json`
- Create `apps/hacienda-workbench/vite.config.ts`
- Create `apps/hacienda-workbench/src/main.tsx`
- Create `apps/hacienda-workbench/src/App.tsx`
- Create `apps/hacienda-workbench/src/api.ts`
- Create `apps/hacienda-workbench/src/styles.css`
  - React/Vite frontend.

- Create `apps/hacienda-workbench/src-tauri/Cargo.toml`
- Create `apps/hacienda-workbench/src-tauri/build.rs`
- Create `apps/hacienda-workbench/src-tauri/tauri.conf.json`
- Create `apps/hacienda-workbench/src-tauri/capabilities/default.json`
- Create `apps/hacienda-workbench/src-tauri/src/main.rs`
- Create `apps/hacienda-workbench/src-tauri/src/lib.rs`
- Create `apps/hacienda-workbench/src-tauri/src/commands.rs`
  - Tauri app and command boundary.

## Preflight

- [ ] **Step 1: Confirm repo status and GitNexus freshness**

Run:

```powershell
npx gitnexus status
git status --short
```

Expected:

```text
Status: ✅ up-to-date
```

If GitNexus is stale:

```powershell
npx gitnexus analyze
```

At the time this plan was written, unrelated local changes existed and must not be reverted or staged by accident:

```text
.config/hakari.toml
crates/anno-rag-tabular/src/schema/column.rs
workspace-hack/Cargo.toml
docs/superpowers/plans/2026-05-27-anno-tabular-local-legal-extraction-quality.md
docs/superpowers/specs/2026-05-27-anno-tabular-local-legal-extraction-quality-design.md
```

- [ ] **Step 2: Run impact checks before reusing existing symbols**

Run:

```powershell
npx gitnexus impact detect_patterns
npx gitnexus impact Vault
npx gitnexus impact derive_key
```

Expected: no HIGH or CRITICAL risk for calling these APIs from a new crate. If GitNexus reports HIGH or CRITICAL, stop and summarize the blast radius before coding.

## Task 1: Create Core Crate and Domain Model

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/hacienda-workbench-core/Cargo.toml`
- Create: `crates/hacienda-workbench-core/src/lib.rs`
- Create: `crates/hacienda-workbench-core/src/error.rs`
- Create: `crates/hacienda-workbench-core/src/model.rs`

- [ ] **Step 1: Write model tests**

Create `crates/hacienda-workbench-core/src/model.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceKind {
    LocalFolder,
    NetworkFolder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentStatus {
    SourceOnly,
    Anonymized,
    Edited,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatterSummary {
    pub id: Uuid,
    pub name: String,
    pub source_root: PathBuf,
    pub source_kind: SourceKind,
    pub document_count: u32,
    pub anonymized_count: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceDocument {
    pub id: Uuid,
    pub matter_id: Uuid,
    pub source_path: PathBuf,
    pub relative_path: String,
    pub sha256: String,
    pub byte_len: u64,
    pub status: DocumentStatus,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PiiSpan {
    pub category: String,
    pub token: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub confidence_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkingDocument {
    pub id: Uuid,
    pub matter_id: Uuid,
    pub source_document_id: Uuid,
    pub title: String,
    pub revision: u32,
    pub anonymized_text: String,
    pub pii_spans: Vec<PiiSpan>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatterDetail {
    pub matter: MatterSummary,
    pub documents: Vec<SourceDocument>,
    pub working_documents: Vec<WorkingDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateMatterRequest {
    pub name: String,
    pub source_root: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matter_summary_round_trips_json() {
        let summary = MatterSummary {
            id: Uuid::now_v7(),
            name: "Dossier Acme".into(),
            source_root: PathBuf::from("C:/cabinet/acme"),
            source_kind: SourceKind::LocalFolder,
            document_count: 3,
            anonymized_count: 2,
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&summary).expect("serialize");
        let back: MatterSummary = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(summary.id, back.id);
        assert_eq!(summary.name, back.name);
        assert_eq!(summary.source_kind, back.source_kind);
    }

    #[test]
    fn working_document_keeps_anonymized_text_only() {
        let doc = WorkingDocument {
            id: Uuid::now_v7(),
            matter_id: Uuid::now_v7(),
            source_document_id: Uuid::now_v7(),
            title: "contrat.md".into(),
            revision: 1,
            anonymized_text: "Contact EMAIL_1".into(),
            pii_spans: vec![PiiSpan {
                category: "Email".into(),
                token: "EMAIL_1".into(),
                byte_start: 8,
                byte_end: 15,
                confidence_percent: 100,
            }],
            updated_at: Utc::now(),
        };

        assert!(doc.anonymized_text.contains("EMAIL_1"));
        assert!(!doc.anonymized_text.contains("@"));
    }
}
```

- [ ] **Step 2: Add crate skeleton and workspace member**

Modify root `Cargo.toml` workspace members:

```toml
members = [
    "crates/anno",
    "crates/anno-eval",
    "crates/anno-cli",
    "crates/anno-rag",
    "crates/anno-rag-bin",
    "crates/anno-rag-mcp",
    "crates/anno-rag-tabular",
    "crates/anno-privacy-gateway",
    "crates/hacienda-workbench-core",
    "workspace-hack",
]
```

Create `crates/hacienda-workbench-core/Cargo.toml`:

```toml
[package]
name = "hacienda-workbench-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Local desktop workbench engine for Hacienda."

[dependencies]
anno-rag = { path = "../anno-rag" }
cloakpipe-core = { path = "../../vendor/cloakpipe/crates/cloakpipe-core" }
chrono = { workspace = true }
dirs = { workspace = true }
rusqlite = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
uuid = { workspace = true }
walkdir = { workspace = true }
workspace-hack = { version = "0.1", path = "../../workspace-hack" }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

Create `crates/hacienda-workbench-core/src/error.rs`:

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("workspace path is not valid UTF-8: {0}")]
    NonUtf8Path(String),

    #[error("unsupported source file extension: {0}")]
    UnsupportedExtension(String),

    #[error("matter not found: {0}")]
    MatterNotFound(uuid::Uuid),

    #[error("document not found: {0}")]
    DocumentNotFound(uuid::Uuid),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    AnnoRag(#[from] anno_rag::Error),
}
```

Create `crates/hacienda-workbench-core/src/lib.rs`:

```rust
pub mod anonymize;
pub mod engine;
pub mod error;
pub mod ingest;
pub mod model;
pub mod store;

pub use engine::WorkbenchEngine;
pub use error::{Error, Result};
pub use model::{
    CreateMatterRequest, DocumentStatus, MatterDetail, MatterSummary, PiiSpan, SourceDocument,
    SourceKind, WorkingDocument,
};
```

Create empty modules so the crate compiles during Task 1:

```powershell
New-Item -ItemType File crates\hacienda-workbench-core\src\anonymize.rs
New-Item -ItemType File crates\hacienda-workbench-core\src\engine.rs
New-Item -ItemType File crates\hacienda-workbench-core\src\ingest.rs
New-Item -ItemType File crates\hacienda-workbench-core\src\store.rs
```

The empty modules are temporary compile units; later tasks replace them with tested code.

- [ ] **Step 3: Run model tests**

Run:

```powershell
cargo test -p hacienda-workbench-core model --lib
```

Expected: PASS.

- [ ] **Step 4: Commit Task 1**

```powershell
git add -- Cargo.toml crates/hacienda-workbench-core
git commit -m "feat(workbench): add core crate model"
```

## Task 2: Add SQLite Store and Audit Log

**Files:**
- Modify: `crates/hacienda-workbench-core/src/store.rs`

- [ ] **Step 1: Write store tests**

Replace `crates/hacienda-workbench-core/src/store.rs` with:

```rust
use crate::model::{DocumentStatus, MatterSummary, SourceDocument, SourceKind, WorkingDocument};
use crate::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct WorkbenchStore {
    conn: Connection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_creates_and_lists_matter() {
        let store = WorkbenchStore::open_in_memory().expect("store");
        let matter = store
            .create_matter("Dossier Acme", Path::new("C:/cabinet/acme"), SourceKind::LocalFolder)
            .expect("matter");

        let matters = store.list_matters().expect("list");
        assert_eq!(matters.len(), 1);
        assert_eq!(matters[0].id, matter.id);
        assert_eq!(matters[0].document_count, 0);
    }

    #[test]
    fn store_upserts_source_and_working_document() {
        let store = WorkbenchStore::open_in_memory().expect("store");
        let matter = store
            .create_matter("Dossier Acme", Path::new("C:/cabinet/acme"), SourceKind::LocalFolder)
            .expect("matter");
        let source = SourceDocument {
            id: Uuid::now_v7(),
            matter_id: matter.id,
            source_path: PathBuf::from("C:/cabinet/acme/contrat.md"),
            relative_path: "contrat.md".into(),
            sha256: "abc".into(),
            byte_len: 42,
            status: DocumentStatus::Anonymized,
            updated_at: Utc::now(),
        };
        store.upsert_source_document(&source).expect("source");

        let working = WorkingDocument {
            id: Uuid::now_v7(),
            matter_id: matter.id,
            source_document_id: source.id,
            title: "contrat.md".into(),
            revision: 1,
            anonymized_text: "Contact EMAIL_1".into(),
            pii_spans: Vec::new(),
            updated_at: Utc::now(),
        };
        store.upsert_working_document(&working).expect("working");

        let detail = store.matter_detail(matter.id).expect("detail");
        assert_eq!(detail.documents.len(), 1);
        assert_eq!(detail.working_documents.len(), 1);
        assert_eq!(detail.matter.document_count, 1);
        assert_eq!(detail.matter.anonymized_count, 1);
    }
}
```

- [ ] **Step 2: Run store tests to verify failure**

Run:

```powershell
cargo test -p hacienda-workbench-core store --lib
```

Expected: FAIL because store methods are not implemented yet.

- [ ] **Step 3: Implement SQLite schema and methods**

Add this implementation above the test module in `store.rs`, keeping the imports and tests:

```rust
use crate::model::{MatterDetail, PiiSpan};

impl WorkbenchStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS matters (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              source_root TEXT NOT NULL,
              source_kind TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS source_documents (
              id TEXT PRIMARY KEY,
              matter_id TEXT NOT NULL,
              source_path TEXT NOT NULL,
              relative_path TEXT NOT NULL,
              sha256 TEXT NOT NULL,
              byte_len INTEGER NOT NULL,
              status TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY(matter_id) REFERENCES matters(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS working_documents (
              id TEXT PRIMARY KEY,
              matter_id TEXT NOT NULL,
              source_document_id TEXT NOT NULL,
              title TEXT NOT NULL,
              revision INTEGER NOT NULL,
              anonymized_text TEXT NOT NULL,
              pii_spans_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              FOREIGN KEY(matter_id) REFERENCES matters(id) ON DELETE CASCADE,
              FOREIGN KEY(source_document_id) REFERENCES source_documents(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS audit_events (
              id TEXT PRIMARY KEY,
              matter_id TEXT,
              event_type TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn create_matter(
        &self,
        name: &str,
        source_root: &Path,
        source_kind: SourceKind,
    ) -> Result<MatterSummary> {
        let now = Utc::now();
        let matter = MatterSummary {
            id: Uuid::now_v7(),
            name: name.to_string(),
            source_root: source_root.to_path_buf(),
            source_kind,
            document_count: 0,
            anonymized_count: 0,
            updated_at: now,
        };
        let source_root = matter.source_root.to_string_lossy().to_string();
        self.conn.execute(
            "INSERT INTO matters (id, name, source_root, source_kind, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                matter.id.to_string(),
                &matter.name,
                &source_root,
                source_kind_key(&matter.source_kind),
                matter.updated_at.to_rfc3339(),
            ],
        )?;
        self.append_audit(Some(matter.id), "matter.created", serde_json::json!({"name": name}))?;
        Ok(matter)
    }

    pub fn list_matters(&self) -> Result<Vec<MatterSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT m.id, m.name, m.source_root, m.source_kind, m.updated_at,
                   COUNT(d.id) AS document_count,
                   SUM(CASE WHEN d.status IN ('anonymized', 'edited') THEN 1 ELSE 0 END) AS anonymized_count
            FROM matters m
            LEFT JOIN source_documents d ON d.matter_id = m.id
            GROUP BY m.id
            ORDER BY m.updated_at DESC
            "#,
        )?;
        let rows = stmt.query_map([], matter_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn matter_detail(&self, matter_id: Uuid) -> Result<MatterDetail> {
        let matter = self
            .list_matters()?
            .into_iter()
            .find(|m| m.id == matter_id)
            .ok_or(crate::Error::MatterNotFound(matter_id))?;
        Ok(MatterDetail {
            matter,
            documents: self.documents_for_matter(matter_id)?,
            working_documents: self.working_documents_for_matter(matter_id)?,
        })
    }

    pub fn upsert_source_document(&self, doc: &SourceDocument) -> Result<()> {
        let source_path = doc.source_path.to_string_lossy().to_string();
        self.conn.execute(
            r#"
            INSERT INTO source_documents
              (id, matter_id, source_path, relative_path, sha256, byte_len, status, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
              sha256 = excluded.sha256,
              byte_len = excluded.byte_len,
              status = excluded.status,
              updated_at = excluded.updated_at
            "#,
            params![
                doc.id.to_string(),
                doc.matter_id.to_string(),
                &source_path,
                &doc.relative_path,
                &doc.sha256,
                i64::try_from(doc.byte_len).unwrap_or(i64::MAX),
                status_key(&doc.status),
                doc.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_working_document(&self, doc: &WorkingDocument) -> Result<()> {
        let spans = serde_json::to_string(&doc.pii_spans)?;
        self.conn.execute(
            r#"
            INSERT INTO working_documents
              (id, matter_id, source_document_id, title, revision, anonymized_text, pii_spans_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
              revision = excluded.revision,
              anonymized_text = excluded.anonymized_text,
              pii_spans_json = excluded.pii_spans_json,
              updated_at = excluded.updated_at
            "#,
            params![
                doc.id.to_string(),
                doc.matter_id.to_string(),
                doc.source_document_id.to_string(),
                &doc.title,
                i64::from(doc.revision),
                &doc.anonymized_text,
                &spans,
                doc.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn append_audit(
        &self,
        matter_id: Option<Uuid>,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit_events (id, matter_id, event_type, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::now_v7().to_string(),
                matter_id.map(|id| id.to_string()),
                event_type,
                payload.to_string(),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn documents_for_matter(&self, matter_id: Uuid) -> Result<Vec<SourceDocument>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, matter_id, source_path, relative_path, sha256, byte_len, status, updated_at FROM source_documents WHERE matter_id = ?1 ORDER BY relative_path",
        )?;
        let rows = stmt.query_map(params![matter_id.to_string()], source_doc_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn working_documents_for_matter(&self, matter_id: Uuid) -> Result<Vec<WorkingDocument>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, matter_id, source_document_id, title, revision, anonymized_text, pii_spans_json, updated_at FROM working_documents WHERE matter_id = ?1 ORDER BY title",
        )?;
        let rows = stmt.query_map(params![matter_id.to_string()], working_doc_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn matter_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MatterSummary> {
    Ok(MatterSummary {
        id: parse_uuid(row.get::<_, String>(0)?),
        name: row.get(1)?,
        source_root: PathBuf::from(row.get::<_, String>(2)?),
        source_kind: source_kind_from_key(&row.get::<_, String>(3)?),
        updated_at: parse_time(row.get::<_, String>(4)?),
        document_count: u32::try_from(row.get::<_, i64>(5)?).unwrap_or(u32::MAX),
        anonymized_count: u32::try_from(row.get::<_, i64>(6)?.max(0)).unwrap_or(u32::MAX),
    })
}

fn source_doc_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceDocument> {
    Ok(SourceDocument {
        id: parse_uuid(row.get::<_, String>(0)?),
        matter_id: parse_uuid(row.get::<_, String>(1)?),
        source_path: PathBuf::from(row.get::<_, String>(2)?),
        relative_path: row.get(3)?,
        sha256: row.get(4)?,
        byte_len: u64::try_from(row.get::<_, i64>(5)?).unwrap_or(u64::MAX),
        status: status_from_key(&row.get::<_, String>(6)?),
        updated_at: parse_time(row.get::<_, String>(7)?),
    })
}

fn working_doc_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkingDocument> {
    let spans_json: String = row.get(6)?;
    let pii_spans: Vec<PiiSpan> = serde_json::from_str(&spans_json).unwrap_or_default();
    Ok(WorkingDocument {
        id: parse_uuid(row.get::<_, String>(0)?),
        matter_id: parse_uuid(row.get::<_, String>(1)?),
        source_document_id: parse_uuid(row.get::<_, String>(2)?),
        title: row.get(3)?,
        revision: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(u32::MAX),
        anonymized_text: row.get(5)?,
        pii_spans,
        updated_at: parse_time(row.get::<_, String>(7)?),
    })
}

fn parse_uuid(s: String) -> Uuid {
    Uuid::parse_str(&s).unwrap_or_else(|_| Uuid::nil())
}

fn parse_time(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn source_kind_key(kind: &SourceKind) -> &'static str {
    match kind {
        SourceKind::LocalFolder => "local_folder",
        SourceKind::NetworkFolder => "network_folder",
    }
}

fn source_kind_from_key(kind: &str) -> SourceKind {
    match kind {
        "network_folder" => SourceKind::NetworkFolder,
        _ => SourceKind::LocalFolder,
    }
}

fn status_key(status: &DocumentStatus) -> &'static str {
    match status {
        DocumentStatus::SourceOnly => "source_only",
        DocumentStatus::Anonymized => "anonymized",
        DocumentStatus::Edited => "edited",
        DocumentStatus::Error => "error",
    }
}

fn status_from_key(status: &str) -> DocumentStatus {
    match status {
        "anonymized" => DocumentStatus::Anonymized,
        "edited" => DocumentStatus::Edited,
        "error" => DocumentStatus::Error,
        _ => DocumentStatus::SourceOnly,
    }
}
```

- [ ] **Step 4: Run store tests**

Run:

```powershell
cargo test -p hacienda-workbench-core store --lib
```

Expected: PASS.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- crates/hacienda-workbench-core/src/store.rs
git commit -m "feat(workbench): add sqlite matter store"
```

## Task 3: Add Local Text Folder Ingestion

**Files:**
- Modify: `crates/hacienda-workbench-core/src/ingest.rs`

- [ ] **Step 1: Write ingestion tests**

Replace `crates/hacienda-workbench-core/src/ingest.rs` with:

```rust
use crate::model::{DocumentStatus, SourceDocument};
use crate::Result;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct IngestedTextDocument {
    pub source: SourceDocument,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_folder_reads_text_and_markdown_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("contrat.md"), "Contact claire@example.com").expect("md");
        std::fs::write(dir.path().join("notes.txt"), "Tel 06 12 34 56 78").expect("txt");
        std::fs::write(dir.path().join("scan.pdf"), "%PDF").expect("pdf");

        let docs = scan_text_folder(Uuid::now_v7(), dir.path()).expect("scan");
        let rels: Vec<String> = docs.iter().map(|d| d.source.relative_path.clone()).collect();

        assert_eq!(docs.len(), 2);
        assert!(rels.contains(&"contrat.md".to_string()));
        assert!(rels.contains(&"notes.txt".to_string()));
        assert!(!rels.contains(&"scan.pdf".to_string()));
    }

    #[test]
    fn scan_folder_hashes_content_without_modifying_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("contrat.md");
        std::fs::write(&path, "Contact claire@example.com").expect("write");
        let before = std::fs::read_to_string(&path).expect("before");

        let docs = scan_text_folder(Uuid::now_v7(), dir.path()).expect("scan");
        let after = std::fs::read_to_string(&path).expect("after");

        assert_eq!(before, after);
        assert_eq!(docs[0].source.byte_len, before.len() as u64);
        assert_eq!(docs[0].source.sha256.len(), 64);
        assert!(matches!(docs[0].source.status, DocumentStatus::SourceOnly));
    }
}
```

- [ ] **Step 2: Run ingestion tests to verify failure**

Run:

```powershell
cargo test -p hacienda-workbench-core ingest --lib
```

Expected: FAIL because `scan_text_folder` is not implemented.

- [ ] **Step 3: Implement scanner**

Add this implementation above the test module in `ingest.rs`:

```rust
pub fn scan_text_folder(matter_id: Uuid, root: &Path) -> Result<Vec<IngestedTextDocument>> {
    let mut docs = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if !is_supported_text(path) {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        let bytes = text.as_bytes();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        docs.push(IngestedTextDocument {
            source: SourceDocument {
                id: Uuid::new_v5(&matter_id, relative_path.as_bytes()),
                matter_id,
                source_path: PathBuf::from(path),
                relative_path,
                sha256: sha256_hex(bytes),
                byte_len: bytes.len() as u64,
                status: DocumentStatus::SourceOnly,
                updated_at: Utc::now(),
            },
            text,
        });
    }
    docs.sort_by(|a, b| a.source.relative_path.cmp(&b.source.relative_path));
    Ok(docs)
}

fn is_supported_text(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "txt" | "md" | "markdown"))
        .unwrap_or(false)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}
```

- [ ] **Step 4: Run ingestion tests**

Run:

```powershell
cargo test -p hacienda-workbench-core ingest --lib
```

Expected: PASS.

- [ ] **Step 5: Commit Task 3**

```powershell
git add -- crates/hacienda-workbench-core/src/ingest.rs
git commit -m "feat(workbench): scan local text folders"
```

## Task 4: Add Basic Anonymization

**Files:**
- Modify: `crates/hacienda-workbench-core/src/anonymize.rs`

- [ ] **Step 1: Write anonymization tests**

Replace `crates/hacienda-workbench-core/src/anonymize.rs` with:

```rust
use crate::model::PiiSpan;
use crate::Result;
use anno_rag::detect::detect_patterns;
use anno_rag::vault::Vault;
use cloakpipe_core::EntityCategory;

#[derive(Debug, Clone)]
pub struct AnonymizedText {
    pub text: String,
    pub spans: Vec<PiiSpan>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn anonymize_text_replaces_email_and_keeps_token_span() {
        let vault = Vault::ephemeral_for_test();
        let out = anonymize_text("Contact claire@example.com", &vault)
            .await
            .expect("anonymize");

        assert!(!out.text.contains("claire@example.com"));
        assert!(out.text.contains("EMAIL_1"));
        assert_eq!(out.spans[0].category, "Email");
        assert_eq!(out.spans[0].token, "EMAIL_1");
    }

    #[tokio::test]
    async fn anonymize_text_preserves_non_pii_text() {
        let vault = Vault::ephemeral_for_test();
        let out = anonymize_text("Aucune donnee sensible ici.", &vault)
            .await
            .expect("anonymize");

        assert_eq!(out.text, "Aucune donnee sensible ici.");
        assert!(out.spans.is_empty());
    }
}
```

- [ ] **Step 2: Run anonymization tests to verify failure**

Run:

```powershell
cargo test -p hacienda-workbench-core anonymize --lib
```

Expected: FAIL because `anonymize_text` is not implemented.

- [ ] **Step 3: Implement model-free anonymization**

Add this implementation above the test module:

```rust
pub async fn anonymize_text(text: &str, vault: &Vault) -> Result<AnonymizedText> {
    let entities = detect_patterns(text);
    if entities.is_empty() {
        return Ok(AnonymizedText {
            text: text.to_string(),
            spans: Vec::new(),
        });
    }

    let pseudo = vault.pseudonymize(text, &entities).await?;
    let mut spans = Vec::new();
    for entity in &entities {
        let token = token_for_entity(&pseudo, &category_key(&entity.category));
        if let Some((token, start, end)) = token {
            spans.push(PiiSpan {
                category: category_key(&entity.category),
                token,
                byte_start: start as u32,
                byte_end: end as u32,
                confidence_percent: (entity.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
            });
        }
    }

    Ok(AnonymizedText { text: pseudo, spans })
}

fn category_key(category: &EntityCategory) -> String {
    match category {
        EntityCategory::Person => "Person".into(),
        EntityCategory::Organization => "Organization".into(),
        EntityCategory::Location => "Location".into(),
        EntityCategory::Email => "Email".into(),
        EntityCategory::PhoneNumber => "PhoneNumber".into(),
        EntityCategory::Custom(value) => value.clone(),
        other => format!("{other:?}"),
    }
}

fn token_prefix(category: &str) -> String {
    match category {
        "Email" => "EMAIL_".into(),
        "PhoneNumber" => "PHONE_NUMBER_".into(),
        "Person" => "PERSON_".into(),
        "Organization" => "ORGANIZATION_".into(),
        "Location" => "LOCATION_".into(),
        other => format!("{}_", other.to_ascii_uppercase()),
    }
}

fn token_for_entity(text: &str, category: &str) -> Option<(String, usize, usize)> {
    let prefix = token_prefix(category);
    let start = text.find(&prefix)?;
    let end = text[start..]
        .find(|c: char| !(c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()))
        .map(|offset| start + offset)
        .unwrap_or(text.len());
    Some((text[start..end].to_string(), start, end))
}
```

- [ ] **Step 4: Run anonymization tests**

Run:

```powershell
cargo test -p hacienda-workbench-core anonymize --lib
```

Expected: PASS.

- [ ] **Step 5: Commit Task 4**

```powershell
git add -- crates/hacienda-workbench-core/src/anonymize.rs
git commit -m "feat(workbench): anonymize text documents"
```

## Task 5: Add Workbench Engine

**Files:**
- Modify: `crates/hacienda-workbench-core/src/engine.rs`

- [ ] **Step 1: Write engine tests**

Replace `crates/hacienda-workbench-core/src/engine.rs` with:

```rust
use crate::anonymize::anonymize_text;
use crate::ingest::scan_text_folder;
use crate::model::{CreateMatterRequest, DocumentStatus, MatterDetail, MatterSummary, SourceKind, WorkingDocument};
use crate::store::WorkbenchStore;
use crate::Result;
use anno_rag::vault::{derive_key, Vault};
use chrono::Utc;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct WorkbenchEngine {
    store: WorkbenchStore,
    vault: Vault,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn engine_creates_matter_from_folder() {
        let root = tempfile::tempdir().expect("root");
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::write(root.path().join("contrat.md"), "Contact claire@example.com").expect("doc");

        let engine = WorkbenchEngine::open_for_test(workspace.path()).expect("engine");
        let matter = engine
            .create_matter_from_folder(CreateMatterRequest {
                name: "Acme".into(),
                source_root: root.path().to_path_buf(),
            })
            .await
            .expect("create");

        assert_eq!(matter.document_count, 1);
        assert_eq!(matter.anonymized_count, 1);

        let detail = engine.matter_detail(matter.id).expect("detail");
        assert_eq!(detail.documents.len(), 1);
        assert_eq!(detail.working_documents.len(), 1);
        assert!(detail.working_documents[0].anonymized_text.contains("EMAIL_1"));
    }

    #[tokio::test]
    async fn engine_saves_document_revision_without_source_mutation() {
        let root = tempfile::tempdir().expect("root");
        let workspace = tempfile::tempdir().expect("workspace");
        let source = root.path().join("contrat.md");
        std::fs::write(&source, "Contact claire@example.com").expect("doc");

        let engine = WorkbenchEngine::open_for_test(workspace.path()).expect("engine");
        let matter = engine
            .create_matter_from_folder(CreateMatterRequest {
                name: "Acme".into(),
                source_root: root.path().to_path_buf(),
            })
            .await
            .expect("create");
        let detail = engine.matter_detail(matter.id).expect("detail");
        let doc = detail.working_documents[0].clone();

        let updated = engine
            .save_working_document(doc.id, "Contact EMAIL_1\nNote revisee.".into())
            .expect("save");

        assert_eq!(updated.revision, 2);
        assert_eq!(std::fs::read_to_string(source).expect("source"), "Contact claire@example.com");
    }
}
```

- [ ] **Step 2: Run engine tests to verify failure**

Run:

```powershell
cargo test -p hacienda-workbench-core engine --lib
```

Expected: FAIL because engine methods are not implemented.

- [ ] **Step 3: Implement engine**

Add this implementation above the test module:

```rust
impl WorkbenchEngine {
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let root = workspace_root.as_ref();
        std::fs::create_dir_all(root)?;
        let store = WorkbenchStore::open(&root.join("workbench.sqlite"))?;
        let key = derive_key()?;
        let vault = Vault::open(&root.join("pii-vault.enc"), key)?;
        Ok(Self { store, vault })
    }

    #[cfg(test)]
    pub fn open_for_test(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let root = workspace_root.as_ref();
        std::fs::create_dir_all(root)?;
        Ok(Self {
            store: WorkbenchStore::open(&root.join("workbench.sqlite"))?,
            vault: Vault::ephemeral_for_test(),
        })
    }

    pub fn list_matters(&self) -> Result<Vec<MatterSummary>> {
        self.store.list_matters()
    }

    pub fn matter_detail(&self, matter_id: Uuid) -> Result<MatterDetail> {
        self.store.matter_detail(matter_id)
    }

    pub async fn create_matter_from_folder(
        &self,
        request: CreateMatterRequest,
    ) -> Result<MatterSummary> {
        let matter = self
            .store
            .create_matter(&request.name, &request.source_root, SourceKind::LocalFolder)?;
        let docs = scan_text_folder(matter.id, &request.source_root)?;

        for ingested in docs {
            let anonymized = anonymize_text(&ingested.text, &self.vault).await?;
            let source = crate::model::SourceDocument {
                status: DocumentStatus::Anonymized,
                ..ingested.source
            };
            self.store.upsert_source_document(&source)?;
            self.store.upsert_working_document(&WorkingDocument {
                id: Uuid::new_v5(&matter.id, source.relative_path.as_bytes()),
                matter_id: matter.id,
                source_document_id: source.id,
                title: source.relative_path.clone(),
                revision: 1,
                anonymized_text: anonymized.text,
                pii_spans: anonymized.spans,
                updated_at: Utc::now(),
            })?;
        }

        self.store
            .append_audit(Some(matter.id), "matter.ingested", serde_json::json!({"source_root": request.source_root}))?;
        self.store
            .matter_detail(matter.id)
            .map(|detail| detail.matter)
    }

    pub fn save_working_document(
        &self,
        working_document_id: Uuid,
        anonymized_text: String,
    ) -> Result<WorkingDocument> {
        let mut found = None;
        for matter in self.store.list_matters()? {
            let detail = self.store.matter_detail(matter.id)?;
            if let Some(doc) = detail
                .working_documents
                .into_iter()
                .find(|doc| doc.id == working_document_id)
            {
                found = Some(doc);
                break;
            }
        }
        let mut doc = found.ok_or(crate::Error::DocumentNotFound(working_document_id))?;
        doc.revision += 1;
        doc.anonymized_text = anonymized_text;
        doc.updated_at = Utc::now();
        self.store.upsert_working_document(&doc)?;
        self.store
            .append_audit(Some(doc.matter_id), "working_document.edited", serde_json::json!({"working_document_id": doc.id}))?;
        Ok(doc)
    }
}
```

- [ ] **Step 4: Run engine tests**

Run:

```powershell
cargo test -p hacienda-workbench-core engine --lib
```

Expected: PASS.

- [ ] **Step 5: Run full core tests and check**

Run:

```powershell
cargo test -p hacienda-workbench-core
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package hacienda-workbench-core -Mode check
```

Expected: PASS.

- [ ] **Step 6: Commit Task 5**

```powershell
git add -- crates/hacienda-workbench-core/src/engine.rs
git commit -m "feat(workbench): add local folder engine"
```

## Task 6: Scaffold Tauri App and Commands

**Files:**
- Modify: `Cargo.toml`
- Create: `apps/hacienda-workbench/package.json`
- Create: `apps/hacienda-workbench/index.html`
- Create: `apps/hacienda-workbench/tsconfig.json`
- Create: `apps/hacienda-workbench/vite.config.ts`
- Create: `apps/hacienda-workbench/src-tauri/Cargo.toml`
- Create: `apps/hacienda-workbench/src-tauri/build.rs`
- Create: `apps/hacienda-workbench/src-tauri/tauri.conf.json`
- Create: `apps/hacienda-workbench/src-tauri/capabilities/default.json`
- Create: `apps/hacienda-workbench/src-tauri/src/main.rs`
- Create: `apps/hacienda-workbench/src-tauri/src/lib.rs`
- Create: `apps/hacienda-workbench/src-tauri/src/commands.rs`

- [ ] **Step 1: Add Tauri workspace member and package files**

Add this workspace member in root `Cargo.toml`:

```toml
    "apps/hacienda-workbench/src-tauri",
```

Create `apps/hacienda-workbench/package.json`:

```json
{
  "name": "hacienda-workbench",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@tauri-apps/plugin-dialog": "^2.0.0",
    "lucide-react": "^0.468.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^5.0.0",
    "typescript": "^5.7.0",
    "vite": "^7.0.0"
  }
}
```

Create `apps/hacienda-workbench/index.html`:

```html
<!doctype html>
<html lang="fr">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Hacienda Workbench</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Create `apps/hacienda-workbench/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2022"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src"],
  "references": []
}
```

Create `apps/hacienda-workbench/vite.config.ts`:

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
```

- [ ] **Step 2: Create Tauri Rust crate**

Create `apps/hacienda-workbench/src-tauri/Cargo.toml`:

```toml
[package]
name = "hacienda-workbench-tauri"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Tauri desktop shell for Hacienda Workbench."

[lib]
name = "hacienda_workbench_tauri_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[[bin]]
name = "hacienda-workbench"
path = "src/main.rs"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
hacienda-workbench-core = { path = "../../../crates/hacienda-workbench-core" }
dirs = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tokio = { workspace = true }
uuid = { workspace = true }
workspace-hack = { version = "0.1", path = "../../../workspace-hack" }

[lints]
workspace = true
```

Create `apps/hacienda-workbench/src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

Create `apps/hacienda-workbench/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Hacienda Workbench",
  "version": "0.1.0",
  "identifier": "com.arc.hacienda.workbench",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Hacienda Workbench",
        "width": 1280,
        "height": 820,
        "minWidth": 1080,
        "minHeight": 720
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": ["msi", "dmg"],
    "icon": []
  }
}
```

Create `apps/hacienda-workbench/src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default desktop capabilities for Hacienda Workbench",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default"]
}
```

Create `apps/hacienda-workbench/src-tauri/src/main.rs`:

```rust
fn main() {
    hacienda_workbench_tauri_lib::run()
}
```

Create `apps/hacienda-workbench/src-tauri/src/lib.rs`:

```rust
mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_matters,
            commands::create_matter_from_folder,
            commands::matter_detail,
            commands::save_working_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hacienda Workbench");
}
```

Create `apps/hacienda-workbench/src-tauri/src/commands.rs`:

```rust
use hacienda_workbench_core::{
    CreateMatterRequest, MatterDetail, MatterSummary, WorkbenchEngine, WorkingDocument,
};
use std::path::PathBuf;
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct AppState {
    engine: Mutex<WorkbenchEngine>,
}

impl AppState {
    pub fn new() -> Self {
        let root = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Hacienda")
            .join("Workbench");
        let engine = WorkbenchEngine::open(root).expect("workbench engine opens");
        Self {
            engine: Mutex::new(engine),
        }
    }
}

#[tauri::command]
pub async fn list_matters(state: State<'_, AppState>) -> Result<Vec<MatterSummary>, String> {
    let engine = state.engine.lock().await;
    engine.list_matters().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_matter_from_folder(
    state: State<'_, AppState>,
    name: String,
    source_root: String,
) -> Result<MatterSummary, String> {
    let request = CreateMatterRequest {
        name,
        source_root: PathBuf::from(source_root),
    };
    let engine = state.engine.lock().await;
    engine
        .create_matter_from_folder(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn matter_detail(state: State<'_, AppState>, matter_id: String) -> Result<MatterDetail, String> {
    let id = Uuid::parse_str(&matter_id).map_err(|e| e.to_string())?;
    let engine = state.engine.lock().await;
    engine.matter_detail(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_working_document(
    state: State<'_, AppState>,
    working_document_id: String,
    anonymized_text: String,
) -> Result<WorkingDocument, String> {
    let id = Uuid::parse_str(&working_document_id).map_err(|e| e.to_string())?;
    let engine = state.engine.lock().await;
    engine
        .save_working_document(id, anonymized_text)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Check Tauri Rust crate**

Run:

```powershell
cargo check -p hacienda-workbench-tauri
```

Expected: PASS. If `workspace-hack` drift appears, run the repository’s established hakari workflow and stage only the expected hakari files.

- [ ] **Step 4: Commit Task 6**

```powershell
git add -- Cargo.toml apps/hacienda-workbench
git commit -m "feat(workbench): scaffold tauri app commands"
```

## Task 7: Add Frontend API and Operational UI

**Files:**
- Create: `apps/hacienda-workbench/src/api.ts`
- Create: `apps/hacienda-workbench/src/main.tsx`
- Create: `apps/hacienda-workbench/src/App.tsx`
- Create: `apps/hacienda-workbench/src/styles.css`

- [ ] **Step 1: Create typed Tauri API wrapper**

Create `apps/hacienda-workbench/src/api.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export type DocumentStatus = "SourceOnly" | "Anonymized" | "Edited" | "Error";

export interface MatterSummary {
  id: string;
  name: string;
  source_root: string;
  source_kind: "LocalFolder" | "NetworkFolder";
  document_count: number;
  anonymized_count: number;
  updated_at: string;
}

export interface SourceDocument {
  id: string;
  matter_id: string;
  source_path: string;
  relative_path: string;
  sha256: string;
  byte_len: number;
  status: DocumentStatus;
  updated_at: string;
}

export interface PiiSpan {
  category: string;
  token: string;
  byte_start: number;
  byte_end: number;
  confidence_percent: number;
}

export interface WorkingDocument {
  id: string;
  matter_id: string;
  source_document_id: string;
  title: string;
  revision: number;
  anonymized_text: string;
  pii_spans: PiiSpan[];
  updated_at: string;
}

export interface MatterDetail {
  matter: MatterSummary;
  documents: SourceDocument[];
  working_documents: WorkingDocument[];
}

export function listMatters(): Promise<MatterSummary[]> {
  return invoke("list_matters");
}

export function createMatterFromFolder(name: string, sourceRoot: string): Promise<MatterSummary> {
  return invoke("create_matter_from_folder", { name, sourceRoot });
}

export function matterDetail(matterId: string): Promise<MatterDetail> {
  return invoke("matter_detail", { matterId });
}

export function saveWorkingDocument(
  workingDocumentId: string,
  anonymizedText: string,
): Promise<WorkingDocument> {
  return invoke("save_working_document", { workingDocumentId, anonymizedText });
}
```

- [ ] **Step 2: Create React entry point**

Create `apps/hacienda-workbench/src/main.tsx`:

```tsx
import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

- [ ] **Step 3: Create app UI**

Create `apps/hacienda-workbench/src/App.tsx`:

```tsx
import { open } from "@tauri-apps/plugin-dialog";
import { FileText, FolderOpen, Save, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  createMatterFromFolder,
  listMatters,
  matterDetail,
  saveWorkingDocument,
  type MatterDetail,
  type MatterSummary,
  type WorkingDocument,
} from "./api";

export function App() {
  const [matters, setMatters] = useState<MatterSummary[]>([]);
  const [detail, setDetail] = useState<MatterDetail | null>(null);
  const [selectedDocId, setSelectedDocId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState("Pret");

  useEffect(() => {
    void refreshMatters();
  }, []);

  const selectedDoc = useMemo(
    () => detail?.working_documents.find((doc) => doc.id === selectedDocId) ?? null,
    [detail, selectedDocId],
  );

  async function refreshMatters() {
    const rows = await listMatters();
    setMatters(rows);
  }

  async function openMatter(matter: MatterSummary) {
    const next = await matterDetail(matter.id);
    setDetail(next);
    const first = next.working_documents[0] ?? null;
    setSelectedDocId(first?.id ?? null);
    setDraft(first?.anonymized_text ?? "");
  }

  async function addFolderMatter() {
    const picked = await open({ directory: true, multiple: false });
    if (!picked || Array.isArray(picked)) return;
    const sourceRoot = String(picked);
    const name = sourceRoot.split(/[\\/]/).filter(Boolean).pop() ?? "Nouveau dossier";
    setStatus("Ingestion en cours");
    const matter = await createMatterFromFolder(name, sourceRoot);
    await refreshMatters();
    await openMatter(matter);
    setStatus("Dossier ingere");
  }

  function selectWorkingDocument(doc: WorkingDocument) {
    setSelectedDocId(doc.id);
    setDraft(doc.anonymized_text);
  }

  async function saveDraft() {
    if (!selectedDoc) return;
    const saved = await saveWorkingDocument(selectedDoc.id, draft);
    if (detail) {
      setDetail({
        ...detail,
        working_documents: detail.working_documents.map((doc) =>
          doc.id === saved.id ? saved : doc,
        ),
      });
    }
    setStatus("Revision sauvegardee");
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <ShieldCheck size={24} />
          <div>
            <strong>Hacienda</strong>
            <span>Workbench local</span>
          </div>
        </div>
        <button className="primary" onClick={addFolderMatter}>
          <FolderOpen size={16} />
          Ajouter un dossier
        </button>
        <div className="matter-list">
          {matters.map((matter) => (
            <button key={matter.id} className="matter-row" onClick={() => void openMatter(matter)}>
              <strong>{matter.name}</strong>
              <span>{matter.anonymized_count}/{matter.document_count} anonymises</span>
            </button>
          ))}
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>{detail?.matter.name ?? "Tableau de bord cabinet"}</h1>
            <p>{detail?.matter.source_root ?? "Ajoutez un dossier client local pour commencer."}</p>
          </div>
          <span className="status">{status}</span>
        </header>

        <div className="content-grid">
          <section className="document-list">
            <h2>Documents</h2>
            {detail?.working_documents.map((doc) => (
              <button
                key={doc.id}
                className={doc.id === selectedDocId ? "doc-row active" : "doc-row"}
                onClick={() => selectWorkingDocument(doc)}
              >
                <FileText size={16} />
                <span>{doc.title}</span>
                <small>rev {doc.revision}</small>
              </button>
            ))}
          </section>

          <section className="editor-panel">
            <div className="editor-header">
              <div>
                <h2>{selectedDoc?.title ?? "Document anonymise"}</h2>
                <p>{selectedDoc ? `${selectedDoc.pii_spans.length} PII detectees` : "Aucun document selectionne"}</p>
              </div>
              <button className="secondary" onClick={saveDraft} disabled={!selectedDoc}>
                <Save size={16} />
                Sauvegarder
              </button>
            </div>
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
            />
          </section>

          <section className="pii-panel">
            <h2>PII</h2>
            {selectedDoc?.pii_spans.map((span) => (
              <div key={`${span.token}-${span.byte_start}`} className="pii-row">
                <strong>{span.token}</strong>
                <span>{span.category}</span>
                <small>{span.confidence_percent}%</small>
              </div>
            ))}
          </section>
        </div>
      </section>
    </main>
  );
}
```

- [ ] **Step 4: Add CSS**

Create `apps/hacienda-workbench/src/styles.css`:

```css
:root {
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  color: #17201b;
  background: #f4f6f5;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
}

button {
  font: inherit;
}

.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 300px minmax(0, 1fr);
}

.sidebar {
  border-right: 1px solid #d8dfda;
  background: #ffffff;
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.brand span,
.topbar p,
.editor-header p,
.matter-row span,
.doc-row small,
.pii-row small {
  color: #66736b;
}

.primary,
.secondary,
.matter-row,
.doc-row {
  border: 1px solid #cbd6cf;
  background: #ffffff;
  border-radius: 8px;
  min-height: 40px;
}

.primary,
.secondary {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 9px 12px;
  cursor: pointer;
}

.primary {
  color: #ffffff;
  background: #22543d;
  border-color: #22543d;
}

.secondary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.matter-list,
.document-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.matter-row,
.doc-row {
  width: 100%;
  text-align: left;
  padding: 10px;
  display: grid;
  gap: 4px;
  cursor: pointer;
}

.doc-row {
  grid-template-columns: 18px 1fr auto;
  align-items: center;
}

.doc-row.active {
  border-color: #22543d;
  background: #edf6f1;
}

.workspace {
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.topbar {
  min-height: 84px;
  padding: 18px 22px;
  border-bottom: 1px solid #d8dfda;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  background: #fbfcfb;
}

h1,
h2,
p {
  margin: 0;
}

h1 {
  font-size: 22px;
}

h2 {
  font-size: 15px;
}

.status {
  border: 1px solid #cbd6cf;
  border-radius: 999px;
  padding: 6px 10px;
  background: #ffffff;
  white-space: nowrap;
}

.content-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr) 260px;
}

.document-list,
.pii-panel,
.editor-panel {
  padding: 16px;
  min-height: 0;
}

.document-list,
.pii-panel {
  border-right: 1px solid #d8dfda;
  background: #fbfcfb;
}

.pii-panel {
  border-right: 0;
  border-left: 1px solid #d8dfda;
}

.editor-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

textarea {
  width: 100%;
  flex: 1;
  min-height: 420px;
  resize: none;
  border: 1px solid #cbd6cf;
  border-radius: 8px;
  padding: 14px;
  font: 14px/1.55 ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
  color: #17201b;
}

.pii-row {
  border: 1px solid #d8dfda;
  border-radius: 8px;
  padding: 10px;
  display: grid;
  gap: 3px;
  margin-top: 8px;
  background: #ffffff;
}
```

- [ ] **Step 5: Run frontend build**

Run:

```powershell
Set-Location apps\hacienda-workbench
npm install
npm run build
Set-Location ..\..
```

Expected: PASS. If npm creates `package-lock.json`, stage it with the app.

- [ ] **Step 6: Commit Task 7**

```powershell
git add -- apps/hacienda-workbench
git commit -m "feat(workbench): add tauri frontend shell"
```

## Task 8: Verify Full Walking Skeleton

**Files:**
- No new files unless fixes are needed.

- [ ] **Step 1: Run Rust tests and checks**

Run:

```powershell
cargo test -p hacienda-workbench-core
cargo check -p hacienda-workbench-tauri
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package hacienda-workbench-core -Mode check
```

Expected: PASS.

- [ ] **Step 2: Run frontend build**

Run:

```powershell
Set-Location apps\hacienda-workbench
npm run build
Set-Location ..\..
```

Expected: PASS.

- [ ] **Step 3: Run Tauri dev smoke**

Run:

```powershell
Set-Location apps\hacienda-workbench
npm run tauri dev
```

Expected: a desktop window opens. Manually verify:

```text
The app opens on the operational dashboard.
"Ajouter un dossier" opens a native folder picker.
Selecting a folder with .txt or .md files creates a matter.
The document list shows scanned text documents.
The editor displays anonymized content.
Email or phone values are tokenized.
Saving creates a new revision.
The source file content on disk is unchanged.
```

Stop the dev server after the smoke test. Do not leave the process running.

- [ ] **Step 4: Verify scope**

Run:

```powershell
git diff --name-status HEAD~8..HEAD
npx gitnexus status
```

Expected changed paths are limited to:

```text
Cargo.toml
crates/hacienda-workbench-core/**
apps/hacienda-workbench/**
workspace-hack/Cargo.toml    # only if hakari generation was required
.config/hakari.toml          # only if hakari generation was required
```

- [ ] **Step 5: Refresh GitNexus after final commit**

After the final implementation commit:

```powershell
npx gitnexus analyze
```

Expected: index refresh succeeds.

## Follow-Up Plans

Write separate implementation plans for:

- broad format ingestion via Kreuzberg/OCR;
- persistent encrypted vault UX and unlock/lock states;
- GLiNER2/LoRA extraction profiles;
- assisted legal workflow templates;
- SharePoint/OneDrive connector;
- signed installer and Tauri updater/resource packs;
- Playwright/browser visual checks once the UI grows beyond the walking skeleton.
