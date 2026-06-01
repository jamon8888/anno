# Anno Local Knowledge Service Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the additive local Knowledge foundation: source-neutral core types, a SQLite/FTS store, and MCP `knowledge_*` tools that work without loading NER, embedder, reranker, or the existing LanceDB `Store`.

**Architecture:** This first slice keeps the existing `search`, `legal_*`, memory, vault, and tabular tools unchanged. It adds independent `anno-knowledge-core` and `anno-knowledge-store` crates, then wires a lazy `KnowledgeService` into `anno-rag-mcp` beside the existing lazy `Pipeline`. Fast keyword search is the only search mode implemented here; semantic vectors, local folder source sync, Outlook sync, and attachment extraction get separate follow-on plans after this foundation lands.

**Tech Stack:** Rust workspace, `serde`, `chrono`, `uuid`, `thiserror`, `rusqlite` with bundled SQLite and FTS5, `rmcp`, `schemars`, `tokio::sync::OnceCell`, existing `AnnoRagConfig`, GitNexus impact checks, targeted Cargo checks through `scripts/dev-fast.ps1`.

---

## Scope Boundary

The multi-source spec covers several independent subsystems: knowledge schema, MCP surface, local folder source, vector projection, Outlook/Microsoft auth, attachment extraction, and optional daemon mode. Implementing all of that in one coding pass would increase risk in the current Rust workspace and would force broad changes near `Pipeline` and `Store`.

This plan implements the first production slice:

- Add source-neutral knowledge domain types.
- Add a separate `knowledge.sqlite3` control and FTS store.
- Add an in-process `KnowledgeService` that opens SQLite without opening `Pipeline`.
- Add MCP tools: `knowledge_sources`, `knowledge_status`, and `knowledge_search`.
- Keep `knowledge_search` on Fast/FTS mode only.
- Update `anno_health.available_tools`.
- Prove with tests that knowledge status/search work without model directories.

The next implementation plans should be created after this one passes:

- Phase 2: local folder source connector and Kreuzberg extraction into the knowledge store.
- Phase 3: privacy pipeline split in `anno-rag` and LanceDB `knowledge_chunks_v1` vector projection.
- Phase 4: Outlook/Microsoft connector with PKCE, keyring token storage, Graph delta sync, and metadata-only attachments.

## Codebase Constraints

Do not modify the existing `crates/anno-rag/src/store.rs::Store` in this plan. GitNexus impact analysis already showed CRITICAL blast radius for broad `Store` changes. The new knowledge path must be additive.

Do not change `Pipeline::ingest_folder`, `Pipeline::search`, `legal_ingest`, or `legal_search` in this plan. Those are compatibility paths and stay stable.

Do not load local ML models from any of the new tools in this plan. Source/status/fast-search tools must never call `AnnoRagServer::pipeline()`.

Use the local Rust debug loop from `AGENTS.md`. Avoid broad `cargo build --workspace` and release builds.

## File Structure

Create:

- `crates/anno-knowledge-core/Cargo.toml` - crate metadata and lightweight dependencies.
- `crates/anno-knowledge-core/src/lib.rs` - public exports.
- `crates/anno-knowledge-core/src/error.rs` - domain errors.
- `crates/anno-knowledge-core/src/ids.rs` - typed UUID wrappers and deterministic ID builders.
- `crates/anno-knowledge-core/src/source.rs` - source/account/scope types and enums.
- `crates/anno-knowledge-core/src/object.rs` - object, part, revision, and state types.
- `crates/anno-knowledge-core/src/query.rs` - search mode and request/result types.
- `crates/anno-knowledge-core/src/status.rs` - status summary wire/domain types.
- `crates/anno-knowledge-store/Cargo.toml` - store crate metadata and SQLite dependencies.
- `crates/anno-knowledge-store/src/lib.rs` - public exports.
- `crates/anno-knowledge-store/src/error.rs` - store errors.
- `crates/anno-knowledge-store/src/migrations.rs` - schema creation and migration runner.
- `crates/anno-knowledge-store/src/fts_query.rs` - safe FTS query builder.
- `crates/anno-knowledge-store/src/control_store.rs` - SQLite connection wrapper and CRUD/search operations.
- `crates/anno-rag-mcp/src/knowledge.rs` - MCP-facing service facade and response builders.

Modify:

- `Cargo.toml` - add both new crates to `workspace.members`.
- `crates/anno-rag-mcp/Cargo.toml` - add `anno-knowledge-core` and `anno-knowledge-store`.
- `crates/anno-rag-mcp/src/lib.rs` - add lazy `knowledge` cell, helper, params, and three tools.
- `crates/anno-rag-mcp/src/health.rs` - add new tool names.

Test:

- `crates/anno-knowledge-core/src/ids.rs`
- `crates/anno-knowledge-core/src/query.rs`
- `crates/anno-knowledge-store/src/migrations.rs`
- `crates/anno-knowledge-store/src/fts_query.rs`
- `crates/anno-knowledge-store/src/control_store.rs`
- `crates/anno-rag-mcp/src/knowledge.rs`
- `crates/anno-rag-mcp/src/health.rs`

## Task 0: Pre-Flight And Impact Checks

**Files:**
- Read: `AGENTS.md`
- Read: `docs/superpowers/specs/2026-05-29-anno-local-knowledge-service-multisource-design.md`
- Read: `crates/anno-rag-mcp/src/lib.rs`
- Read: `crates/anno-rag-mcp/src/health.rs`
- Read: `Cargo.toml`

- [ ] **Step 1: Verify workspace status**

Run:

```powershell
git status --short --branch
```

Expected: note any unrelated untracked files and do not stage them.

- [ ] **Step 2: Verify GitNexus index**

Run:

```powershell
npx gitnexus status
```

Expected: the `anno` repo is indexed. If GitNexus says the index is stale, run:

```powershell
npx gitnexus analyze
```

- [ ] **Step 3: Run impact before editing MCP server symbols**

Run:

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
```

Expected: record direct callers and risk in the implementation notes. If risk is HIGH or CRITICAL, stop and warn the user before touching `AnnoRagServer`.

- [ ] **Step 4: Confirm the forbidden edit area**

Run:

```powershell
npx gitnexus impact --repo anno Store --direction upstream
```

Expected: CRITICAL or broad blast radius. Keep `crates/anno-rag/src/store.rs` out of this plan.

## Task 1: Workspace Crates And Core IDs

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/anno-knowledge-core/Cargo.toml`
- Create: `crates/anno-knowledge-core/src/lib.rs`
- Create: `crates/anno-knowledge-core/src/error.rs`
- Create: `crates/anno-knowledge-core/src/ids.rs`

- [ ] **Step 1: Write failing ID tests**

Create `crates/anno-knowledge-core/src/ids.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_is_stable_for_same_kind_and_key() {
        let first = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");
        let second = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");

        assert_eq!(first, second);
        assert_eq!(first.as_uuid().get_version_num(), 5);
    }

    #[test]
    fn object_id_changes_when_scope_changes() {
        let inbox = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "inbox",
            "immutable-message-id",
        );
        let sent = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "sent",
            "immutable-message-id",
        );

        assert_ne!(inbox, sent);
    }

    #[test]
    fn chunk_id_is_stable_for_revision_part_and_index() {
        let revision = RevisionId::from_parts("object-a", "version-a");
        let part = PartId::from_parts("object-a", "body");

        let first = ChunkId::from_parts(revision, part, 7);
        let second = ChunkId::from_parts(revision, part, 7);

        assert_eq!(first, second);
    }
}
```

- [ ] **Step 2: Add core crate and workspace member**

Modify root `Cargo.toml`:

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
    "crates/anno-knowledge-core",
    "workspace-hack",
]
```

Create `crates/anno-knowledge-core/Cargo.toml`:

```toml
[package]
name = "anno-knowledge-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Source-neutral knowledge domain types for Anno local knowledge indexing."

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }

[lints]
workspace = true
```

Create `crates/anno-knowledge-core/src/lib.rs`:

```rust
//! Source-neutral domain types for Anno's local knowledge service.

pub mod error;
pub mod ids;

pub use error::{KnowledgeCoreError, Result};
pub use ids::{
    AccountId, ChunkId, ObjectId, PartId, RevisionId, ScopeId, SourceId, SourceKindForId,
};
```

Create `crates/anno-knowledge-core/src/error.rs`:

```rust
//! Error types for source-neutral knowledge domain code.

/// Result type for knowledge core operations.
pub type Result<T> = std::result::Result<T, KnowledgeCoreError>;

/// Source-neutral domain errors.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeCoreError {
    /// A required stable provider key or namespace component was empty.
    #[error("{field} must not be empty")]
    EmptyStablePart {
        /// Name of the missing field.
        field: &'static str,
    },
}
```

- [ ] **Step 3: Run ID tests and verify they fail**

Run:

```powershell
cargo test -p anno-knowledge-core ids --no-default-features
```

Expected: FAIL because ID types and `SourceKindForId` are not implemented yet.

- [ ] **Step 4: Implement typed deterministic IDs**

Replace `crates/anno-knowledge-core/src/ids.rs` with:

```rust
//! Typed deterministic UUID identifiers for knowledge entities.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const KNOWLEDGE_NAMESPACE: Uuid = Uuid::from_u128(0x67a2_4aa5_6f8d_4f88_9c53_3d6f_0f42_9b21);

/// Minimal source kind enum used by deterministic ID builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKindForId {
    /// Local folder source.
    LocalFolder,
    /// Microsoft Outlook mail source.
    MicrosoftOutlook,
    /// Microsoft OneDrive source.
    MicrosoftOneDrive,
    /// Microsoft SharePoint source.
    MicrosoftSharePoint,
    /// Gmail source.
    Gmail,
    /// Google Drive source.
    GoogleDrive,
    /// Slack source.
    Slack,
    /// Notion source.
    Notion,
}

impl SourceKindForId {
    fn as_stable_str(self) -> &'static str {
        match self {
            Self::LocalFolder => "local_folder",
            Self::MicrosoftOutlook => "microsoft_outlook",
            Self::MicrosoftOneDrive => "microsoft_onedrive",
            Self::MicrosoftSharePoint => "microsoft_sharepoint",
            Self::Gmail => "gmail",
            Self::GoogleDrive => "google_drive",
            Self::Slack => "slack",
            Self::Notion => "notion",
        }
    }
}

macro_rules! typed_id {
    ($name:ident) => {
        #[doc = concat!("Typed UUID wrapper for ", stringify!($name), ".")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Create from an existing UUID.
            #[must_use]
            pub const fn new(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Borrow the underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }

            /// Return the canonical UUID string.
            #[must_use]
            pub fn as_string(self) -> String {
                self.0.to_string()
            }
        }
    };
}

typed_id!(SourceId);
typed_id!(AccountId);
typed_id!(ScopeId);
typed_id!(ObjectId);
typed_id!(PartId);
typed_id!(RevisionId);
typed_id!(ChunkId);

fn stable_uuid(parts: &[&str]) -> Uuid {
    let stable = parts.join("\u{1f}");
    Uuid::new_v5(&KNOWLEDGE_NAMESPACE, stable.as_bytes())
}

impl SourceId {
    /// Build a stable source id from source kind and provider/local key.
    #[must_use]
    pub fn from_parts(kind: SourceKindForId, stable_key: &str) -> Self {
        Self(stable_uuid(&["source", kind.as_stable_str(), stable_key]))
    }
}

impl AccountId {
    /// Build a stable account id from source and provider subject.
    #[must_use]
    pub fn from_parts(source_id: SourceId, provider_subject: &str) -> Self {
        Self(stable_uuid(&[
            "account",
            &source_id.as_string(),
            provider_subject,
        ]))
    }
}

impl ScopeId {
    /// Build a stable scope id from account and provider scope key.
    #[must_use]
    pub fn from_parts(account_id: AccountId, provider_key: &str) -> Self {
        Self(stable_uuid(&["scope", &account_id.as_string(), provider_key]))
    }
}

impl ObjectId {
    /// Build a stable object id from source kind, account key, scope key, and external id.
    #[must_use]
    pub fn from_external(
        kind: SourceKindForId,
        account_key: &str,
        scope_key: &str,
        external_id: &str,
    ) -> Self {
        Self(stable_uuid(&[
            "object",
            kind.as_stable_str(),
            account_key,
            scope_key,
            external_id,
        ]))
    }
}

impl PartId {
    /// Build a stable part id from object key and part key.
    #[must_use]
    pub fn from_parts(object_key: &str, part_key: &str) -> Self {
        Self(stable_uuid(&["part", object_key, part_key]))
    }
}

impl RevisionId {
    /// Build a stable revision id from object key and provider version.
    #[must_use]
    pub fn from_parts(object_key: &str, provider_version: &str) -> Self {
        Self(stable_uuid(&["revision", object_key, provider_version]))
    }
}

impl ChunkId {
    /// Build a stable chunk id from revision, part, and chunk index.
    #[must_use]
    pub fn from_parts(revision_id: RevisionId, part_id: PartId, chunk_idx: u32) -> Self {
        Self(stable_uuid(&[
            "chunk",
            &revision_id.as_string(),
            &part_id.as_string(),
            &chunk_idx.to_string(),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_is_stable_for_same_kind_and_key() {
        let first = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");
        let second = SourceId::from_parts(SourceKindForId::MicrosoftOutlook, "tenant-a:user-a");

        assert_eq!(first, second);
        assert_eq!(first.as_uuid().get_version_num(), 5);
    }

    #[test]
    fn object_id_changes_when_scope_changes() {
        let inbox = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "inbox",
            "immutable-message-id",
        );
        let sent = ObjectId::from_external(
            SourceKindForId::MicrosoftOutlook,
            "account-a",
            "sent",
            "immutable-message-id",
        );

        assert_ne!(inbox, sent);
    }

    #[test]
    fn chunk_id_is_stable_for_revision_part_and_index() {
        let revision = RevisionId::from_parts("object-a", "version-a");
        let part = PartId::from_parts("object-a", "body");

        let first = ChunkId::from_parts(revision, part, 7);
        let second = ChunkId::from_parts(revision, part, 7);

        assert_eq!(first, second);
    }
}
```

- [ ] **Step 5: Run ID tests and verify they pass**

Run:

```powershell
cargo test -p anno-knowledge-core ids --no-default-features
```

Expected: PASS for the three ID tests.

- [ ] **Step 6: Commit Task 1**

Run:

```powershell
git add Cargo.toml crates/anno-knowledge-core
git commit -m "feat: add knowledge core identifiers"
```

## Task 2: Core Domain Types

**Files:**
- Modify: `crates/anno-knowledge-core/src/lib.rs`
- Create: `crates/anno-knowledge-core/src/source.rs`
- Create: `crates/anno-knowledge-core/src/object.rs`
- Create: `crates/anno-knowledge-core/src/query.rs`
- Create: `crates/anno-knowledge-core/src/status.rs`

- [ ] **Step 1: Write source and query tests**

Create `crates/anno-knowledge-core/src/query.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_search_mode_is_fast() {
        let request = KnowledgeSearchRequest::new("contrat dupont");

        assert_eq!(request.mode, KnowledgeSearchMode::Fast);
        assert_eq!(request.top_k, 10);
    }

    #[test]
    fn top_k_is_clamped_to_local_machine_budget() {
        let request = KnowledgeSearchRequest::new("contrat").with_top_k(500);

        assert_eq!(request.top_k, 50);
    }
}
```

- [ ] **Step 2: Run query tests and verify they fail**

Run:

```powershell
cargo test -p anno-knowledge-core query --no-default-features
```

Expected: FAIL because query types are not implemented yet.

- [ ] **Step 3: Implement source types**

Create `crates/anno-knowledge-core/src/source.rs`:

```rust
//! Source, account, and scope domain types.

use crate::ids::{AccountId, ScopeId, SourceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported knowledge source families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Local folder chosen by the user.
    LocalFolder,
    /// Microsoft Outlook mail.
    MicrosoftOutlook,
    /// Microsoft OneDrive files.
    MicrosoftOneDrive,
    /// Microsoft SharePoint files.
    MicrosoftSharePoint,
    /// Gmail mail.
    Gmail,
    /// Google Drive files.
    GoogleDrive,
    /// Slack messages and files.
    Slack,
    /// Notion pages.
    Notion,
}

/// Configured source integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSource {
    /// Stable source id.
    pub source_id: SourceId,
    /// Source family.
    pub kind: SourceKind,
    /// Pseudonymized display label.
    pub display_label_pseudo: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Whether this source is enabled for sync/search.
    pub enabled: bool,
}

/// Account or local identity inside a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAccount {
    /// Stable account id.
    pub account_id: AccountId,
    /// Parent source id.
    pub source_id: SourceId,
    /// Provider subject or local identity key. This must not be returned to Claude.
    pub provider_subject: String,
    /// Optional Microsoft tenant id or equivalent provider tenant.
    pub tenant_id: Option<String>,
    /// Pseudonymized account label.
    pub display_label_pseudo: String,
    /// Granted provider scopes.
    pub scopes_granted: Vec<String>,
    /// OS keyring reference. Token values are never stored here.
    pub auth_ref: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last successful provider observation timestamp.
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// Scope type selected inside a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// Local filesystem folder.
    LocalFolder,
    /// Outlook mail folder.
    MailFolder,
    /// Cloud drive folder.
    DriveFolder,
    /// Chat channel.
    Channel,
    /// Page tree or workspace section.
    PageTree,
}

/// Sync policy for a selected scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPolicy {
    /// Whether sync is enabled.
    pub enabled: bool,
    /// Maximum provider pages per sync run.
    pub max_pages_per_run: u32,
    /// Whether attachment/file bodies may be extracted.
    pub include_attachments: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_pages_per_run: 5,
            include_attachments: false,
        }
    }
}

/// Selectable area inside a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceScope {
    /// Stable scope id.
    pub scope_id: ScopeId,
    /// Parent account id.
    pub account_id: AccountId,
    /// Scope family.
    pub kind: ScopeKind,
    /// Provider folder id, local folder path key, or equivalent stable key.
    pub provider_key: String,
    /// Pseudonymized display label.
    pub display_label_pseudo: String,
    /// Sync budget and attachment policy.
    pub sync_policy: SyncPolicy,
    /// Whether this scope participates in sync/search.
    pub enabled: bool,
}
```

- [ ] **Step 4: Implement object and state types**

Create `crates/anno-knowledge-core/src/object.rs`:

```rust
//! Knowledge object, part, revision, and state types.

use crate::ids::{AccountId, ObjectId, PartId, RevisionId, ScopeId, SourceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Logical object type from a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// Email message.
    Email,
    /// Standalone file or document.
    File,
    /// Chat message.
    ChatMessage,
    /// Web/page object.
    Page,
}

/// Extracted part type inside an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartType {
    /// Email body text.
    EmailBody,
    /// Attachment or document body.
    AttachmentBody,
    /// Local/cloud file body.
    FileBody,
    /// Metadata-only part.
    Metadata,
}

/// Current processing state for an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectState {
    /// Metadata known, content not yet extracted.
    Discovered,
    /// Extracted text is waiting for privacy processing.
    ExtractedPendingPrivacy,
    /// Pseudonymized chunks are stored in SQLite FTS.
    Pseudonymized,
    /// Vector projection has been created.
    VectorIndexed,
    /// Processing deferred because a local budget was exceeded.
    DeferredBudget,
    /// Processing failed and can be retried.
    FailedRetryable,
    /// Processing failed permanently for current config.
    FailedPermanent,
    /// Object was forgotten locally.
    Forgotten,
}

/// One logical source object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeObject {
    /// Stable object id.
    pub object_id: ObjectId,
    /// Parent source id.
    pub source_id: SourceId,
    /// Parent account id.
    pub account_id: AccountId,
    /// Parent scope id.
    pub scope_id: ScopeId,
    /// Provider id or local file id.
    pub external_id: String,
    /// Object family.
    pub object_type: ObjectType,
    /// Raw title for local processing only. Do not return through MCP.
    pub title_raw: Option<String>,
    /// Raw provider metadata for local processing only. Do not return through MCP.
    pub metadata_raw: serde_json::Value,
    /// Provider URL or local URI. Do not return through MCP until policy allows it.
    pub source_url: Option<String>,
    /// Provider update timestamp.
    pub source_updated_at: DateTime<Utc>,
    /// Current processing state.
    pub state: ObjectState,
}

/// One extracted part of a source object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePart {
    /// Stable part id.
    pub part_id: PartId,
    /// Parent object id.
    pub object_id: ObjectId,
    /// Part family.
    pub part_type: PartType,
    /// Pseudonymized display title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized metadata.
    pub metadata_pseudo: serde_json::Value,
    /// Extracted text length before pseudonymization.
    pub extracted_chars: u32,
}

/// Object revision based on provider version or content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRevision {
    /// Stable revision id.
    pub revision_id: RevisionId,
    /// Parent object id.
    pub object_id: ObjectId,
    /// Provider change token or content version.
    pub provider_version: String,
    /// Timestamp when this revision was observed locally.
    pub observed_at: DateTime<Utc>,
}
```

- [ ] **Step 5: Implement query and status types**

Create `crates/anno-knowledge-core/src/query.rs`:

```rust
//! Query types for the knowledge search surface.

use crate::ids::{ChunkId, ObjectId, RevisionId};
use crate::object::ObjectType;
use crate::source::SourceKind;
use serde::{Deserialize, Serialize};

/// Search execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSearchMode {
    /// SQLite FTS only. Must not load models.
    Fast,
    /// Vector search. Implemented after vector projection lands.
    Semantic,
    /// Hybrid search and optional local rerank. Implemented after vector projection lands.
    Deep,
}

/// Search request used by service code and MCP params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSearchRequest {
    /// User query. May contain PII; phase 1 only uses it for local FTS matching.
    pub query: String,
    /// Search mode.
    pub mode: KnowledgeSearchMode,
    /// Maximum result count.
    pub top_k: usize,
}

impl KnowledgeSearchRequest {
    /// Create a request with local-machine defaults.
    #[must_use]
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            mode: KnowledgeSearchMode::Fast,
            top_k: 10,
        }
    }

    /// Set and clamp top-k for local response size.
    #[must_use]
    pub fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k.clamp(1, 50);
        self
    }
}

/// One pseudonymized search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeSearchHit {
    /// Matching chunk id.
    pub chunk_id: ChunkId,
    /// Parent object id.
    pub object_id: ObjectId,
    /// Parent revision id.
    pub revision_id: RevisionId,
    /// Source kind.
    pub source_kind: SourceKind,
    /// Object type.
    pub object_type: ObjectType,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized snippet.
    pub snippet_pseudo: String,
    /// FTS or semantic score.
    pub score: f32,
}
```

Create `crates/anno-knowledge-core/src/status.rs`:

```rust
//! Status summaries for the knowledge service.

use serde::{Deserialize, Serialize};

/// User-visible local knowledge status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeStatus {
    /// Number of configured sources.
    pub sources: u64,
    /// Number of configured accounts.
    pub accounts: u64,
    /// Number of configured scopes.
    pub scopes: u64,
    /// Number of discovered objects.
    pub objects: u64,
    /// Number of pseudonymized chunks in FTS.
    pub chunks: u64,
    /// Number of failed objects/jobs.
    pub failures: u64,
    /// Whether the service opened without model loading.
    pub models_loaded: bool,
}
```

- [ ] **Step 6: Export modules**

Modify `crates/anno-knowledge-core/src/lib.rs`:

```rust
//! Source-neutral domain types for Anno's local knowledge service.

pub mod error;
pub mod ids;
pub mod object;
pub mod query;
pub mod source;
pub mod status;

pub use error::{KnowledgeCoreError, Result};
pub use ids::{
    AccountId, ChunkId, ObjectId, PartId, RevisionId, ScopeId, SourceId, SourceKindForId,
};
pub use object::{
    KnowledgeObject, KnowledgePart, KnowledgeRevision, ObjectState, ObjectType, PartType,
};
pub use query::{KnowledgeSearchHit, KnowledgeSearchMode, KnowledgeSearchRequest};
pub use source::{KnowledgeSource, ScopeKind, SourceAccount, SourceKind, SourceScope, SyncPolicy};
pub use status::KnowledgeStatus;
```

- [ ] **Step 7: Run core tests**

Run:

```powershell
cargo test -p anno-knowledge-core --no-default-features
```

Expected: PASS.

- [ ] **Step 8: Commit Task 2**

Run:

```powershell
git add crates/anno-knowledge-core
git commit -m "feat: add knowledge core domain types"
```

## Task 3: SQLite Store Migrations

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/anno-knowledge-store/Cargo.toml`
- Create: `crates/anno-knowledge-store/src/lib.rs`
- Create: `crates/anno-knowledge-store/src/error.rs`
- Create: `crates/anno-knowledge-store/src/migrations.rs`

- [ ] **Step 1: Add store crate to workspace**

Modify root `Cargo.toml`:

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
    "crates/anno-knowledge-core",
    "crates/anno-knowledge-store",
    "workspace-hack",
]
```

Create `crates/anno-knowledge-store/Cargo.toml`:

```toml
[package]
name = "anno-knowledge-store"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "SQLite control and FTS store for Anno local knowledge indexing."

[dependencies]
anno-knowledge-core = { path = "../anno-knowledge-core" }
chrono = { workspace = true }
rusqlite = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }

[dev-dependencies]
tempfile = "3.27.0"

[lints]
workspace = true
```

Create `crates/anno-knowledge-store/src/lib.rs`:

```rust
//! SQLite-backed local knowledge control and FTS store.

pub mod error;
pub mod migrations;

pub use error::{KnowledgeStoreError, Result};
```

Create `crates/anno-knowledge-store/src/error.rs`:

```rust
//! Error types for the knowledge store.

/// Store result type.
pub type Result<T> = std::result::Result<T, KnowledgeStoreError>;

/// Errors from SQLite-backed knowledge storage.
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeStoreError {
    /// SQLite failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// Filesystem IO failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

- [ ] **Step 2: Write migration tests**

Create `crates/anno-knowledge-store/src/migrations.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_create_expected_tables_and_fts() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type IN ('table', 'view') AND name LIKE 'knowledge_%' \
                 ORDER BY name",
            )
            .expect("prepare");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect");

        assert!(names.contains(&"knowledge_sources".to_string()));
        assert!(names.contains(&"knowledge_objects".to_string()));
        assert!(names.contains(&"knowledge_chunks".to_string()));
        assert!(names.contains(&"knowledge_objects_fts".to_string()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("first migrate");
        run_migrations(&conn).expect("second migrate");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, 1);
    }
}
```

- [ ] **Step 3: Run migration tests and verify they fail**

Run:

```powershell
cargo test -p anno-knowledge-store migrations --no-default-features
```

Expected: FAIL because `run_migrations` is not implemented.

- [ ] **Step 4: Implement schema migration**

Replace `crates/anno-knowledge-store/src/migrations.rs` with:

```rust
//! SQLite schema migrations for knowledge control and FTS data.

use crate::Result;
use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 1;

/// Apply all schema migrations.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        migrate_v1(conn)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS knowledge_sources (
            source_id TEXT PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL,
            display_label_pseudo TEXT NOT NULL,
            created_at TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS source_accounts (
            account_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT NOT NULL REFERENCES knowledge_sources(source_id) ON DELETE CASCADE,
            provider_subject TEXT NOT NULL,
            tenant_id TEXT,
            display_label_pseudo TEXT NOT NULL,
            scopes_granted_json TEXT NOT NULL,
            auth_ref TEXT,
            created_at TEXT NOT NULL,
            last_seen_at TEXT
        );

        CREATE TABLE IF NOT EXISTS source_scopes (
            scope_id TEXT PRIMARY KEY NOT NULL,
            account_id TEXT NOT NULL REFERENCES source_accounts(account_id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            provider_key TEXT NOT NULL,
            display_label_pseudo TEXT NOT NULL,
            sync_policy_json TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS knowledge_objects (
            object_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            scope_id TEXT NOT NULL,
            external_id TEXT NOT NULL,
            object_type TEXT NOT NULL,
            title_pseudo TEXT,
            metadata_pseudo_json TEXT NOT NULL,
            source_url_policy TEXT,
            source_updated_at TEXT NOT NULL,
            state TEXT NOT NULL,
            last_error TEXT,
            UNIQUE(source_id, account_id, scope_id, external_id)
        );

        CREATE TABLE IF NOT EXISTS knowledge_revisions (
            revision_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            provider_version TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            UNIQUE(object_id, provider_version)
        );

        CREATE TABLE IF NOT EXISTS knowledge_parts (
            part_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            part_type TEXT NOT NULL,
            title_pseudo TEXT,
            metadata_pseudo_json TEXT NOT NULL,
            extracted_chars INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_chunks (
            chunk_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT NOT NULL REFERENCES knowledge_objects(object_id) ON DELETE CASCADE,
            revision_id TEXT NOT NULL REFERENCES knowledge_revisions(revision_id) ON DELETE CASCADE,
            part_id TEXT NOT NULL REFERENCES knowledge_parts(part_id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            object_type TEXT NOT NULL,
            title_pseudo TEXT,
            body_pseudo TEXT NOT NULL,
            metadata_pseudo_json TEXT NOT NULL,
            chunk_idx INTEGER NOT NULL,
            char_start INTEGER NOT NULL,
            char_end INTEGER NOT NULL,
            indexed_at TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_objects_fts USING fts5(
            chunk_id UNINDEXED,
            object_id UNINDEXED,
            revision_id UNINDEXED,
            source_kind UNINDEXED,
            object_type UNINDEXED,
            title_pseudo,
            body_pseudo,
            metadata_pseudo,
            tokenize = 'unicode61 remove_diacritics 1'
        );

        CREATE TABLE IF NOT EXISTS sync_runs (
            sync_run_id TEXT PRIMARY KEY NOT NULL,
            source_id TEXT,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            status TEXT NOT NULL,
            objects_seen INTEGER NOT NULL DEFAULT 0,
            objects_changed INTEGER NOT NULL DEFAULT 0,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS index_jobs (
            job_id TEXT PRIMARY KEY NOT NULL,
            object_id TEXT,
            job_type TEXT NOT NULL,
            status TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            not_before TEXT,
            last_error TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_object
            ON knowledge_chunks(object_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_revision
            ON knowledge_chunks(revision_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_objects_state
            ON knowledge_objects(state);
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_create_expected_tables_and_fts() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("migrate");

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type IN ('table', 'view') AND name LIKE 'knowledge_%' \
                 ORDER BY name",
            )
            .expect("prepare");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("collect");

        assert!(names.contains(&"knowledge_sources".to_string()));
        assert!(names.contains(&"knowledge_objects".to_string()));
        assert!(names.contains(&"knowledge_chunks".to_string()));
        assert!(names.contains(&"knowledge_objects_fts".to_string()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().expect("open memory db");

        run_migrations(&conn).expect("first migrate");
        run_migrations(&conn).expect("second migrate");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, 1);
    }
}
```

- [ ] **Step 5: Run store migration tests**

Run:

```powershell
cargo test -p anno-knowledge-store migrations --no-default-features
```

Expected: PASS.

- [ ] **Step 6: Commit Task 3**

Run:

```powershell
git add Cargo.toml crates/anno-knowledge-store
git commit -m "feat: add knowledge sqlite migrations"
```

## Task 4: FTS Query Builder

**Files:**
- Modify: `crates/anno-knowledge-store/src/lib.rs`
- Create: `crates/anno-knowledge-store/src/fts_query.rs`

- [ ] **Step 1: Write failing FTS query tests**

Create `crates/anno-knowledge-store/src/fts_query.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_quotes_terms_and_escapes_quotes() {
        let query = build_fts_query("contrat \"Dupont\" 2026").expect("query");

        assert_eq!(query, "\"contrat\" AND \"Dupont\" AND \"2026\"");
    }

    #[test]
    fn query_drops_only_punctuation_input() {
        assert_eq!(build_fts_query("!? /"), None);
    }

    #[test]
    fn query_caps_term_count() {
        let query = build_fts_query("a b c d e f g h i j k l").expect("query");

        assert_eq!(
            query,
            "\"a\" AND \"b\" AND \"c\" AND \"d\" AND \"e\" AND \"f\" AND \"g\" AND \"h\""
        );
    }
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```powershell
cargo test -p anno-knowledge-store fts_query --no-default-features
```

Expected: FAIL because `build_fts_query` is not implemented.

- [ ] **Step 3: Implement query builder**

Replace `crates/anno-knowledge-store/src/fts_query.rs` with:

```rust
//! FTS5 query normalization for local user input.

const MAX_FTS_TERMS: usize = 8;

/// Convert user input into a conservative FTS5 expression.
#[must_use]
pub fn build_fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .filter_map(|raw| {
            let trimmed = raw.trim_matches(|c: char| c == '\'' || c == '-');
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", trimmed.replace('"', "\"\"")))
            }
        })
        .take(MAX_FTS_TERMS)
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_quotes_terms_and_escapes_quotes() {
        let query = build_fts_query("contrat \"Dupont\" 2026").expect("query");

        assert_eq!(query, "\"contrat\" AND \"Dupont\" AND \"2026\"");
    }

    #[test]
    fn query_drops_only_punctuation_input() {
        assert_eq!(build_fts_query("!? /"), None);
    }

    #[test]
    fn query_caps_term_count() {
        let query = build_fts_query("a b c d e f g h i j k l").expect("query");

        assert_eq!(
            query,
            "\"a\" AND \"b\" AND \"c\" AND \"d\" AND \"e\" AND \"f\" AND \"g\" AND \"h\""
        );
    }
}
```

Modify `crates/anno-knowledge-store/src/lib.rs`:

```rust
//! SQLite-backed local knowledge control and FTS store.

pub mod error;
pub mod fts_query;
pub mod migrations;

pub use error::{KnowledgeStoreError, Result};
```

- [ ] **Step 4: Run FTS query tests**

Run:

```powershell
cargo test -p anno-knowledge-store fts_query --no-default-features
```

Expected: PASS.

- [ ] **Step 5: Commit Task 4**

Run:

```powershell
git add crates/anno-knowledge-store/src/lib.rs crates/anno-knowledge-store/src/fts_query.rs
git commit -m "feat: add knowledge fts query builder"
```

## Task 5: Control Store Open, Status, And Fast Search

**Files:**
- Modify: `crates/anno-knowledge-store/src/lib.rs`
- Create: `crates/anno-knowledge-store/src/control_store.rs`

- [ ] **Step 1: Write failing control store tests**

Create `crates/anno-knowledge-store/src/control_store.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anno_knowledge_core::{
        ChunkId, KnowledgeSearchRequest, ObjectId, PartId, RevisionId, SourceKind,
    };

    #[test]
    fn open_creates_database_and_empty_status() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");

        let store = KnowledgeControlStore::open(&db_path).expect("open store");
        let status = store.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_returns_pseudonymized_chunks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");
        let store = KnowledgeControlStore::open(&db_path).expect("open store");

        let object_id = ObjectId::from_external(
            anno_knowledge_core::SourceKindForId::LocalFolder,
            "local",
            "folder-a",
            "file-a",
        );
        let revision_id = RevisionId::from_parts("file-a", "v1");
        let part_id = PartId::from_parts("file-a", "body");
        let chunk_id = ChunkId::from_parts(revision_id, part_id, 0);

        store
            .insert_test_chunk(TestChunkInput {
                chunk_id,
                object_id,
                revision_id,
                part_id,
                source_kind: SourceKind::LocalFolder,
                object_type: anno_knowledge_core::ObjectType::File,
                title_pseudo: Some("Document PERSON_1".to_string()),
                body_pseudo: "Le contrat de PERSON_1 contient CLAUSE_1.".to_string(),
                metadata_pseudo_json: serde_json::json!({"path": "FILE_1"}),
            })
            .expect("insert chunk");

        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, chunk_id);
        assert!(hits[0].snippet_pseudo.contains("contrat"));
        assert!(!hits[0].snippet_pseudo.contains("Dupont"));
    }
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```powershell
cargo test -p anno-knowledge-store control_store --no-default-features
```

Expected: FAIL because `KnowledgeControlStore` is not implemented.

- [ ] **Step 3: Implement control store**

Replace `crates/anno-knowledge-store/src/control_store.rs` with:

```rust
//! SQLite control store and FTS search operations.

use crate::fts_query::build_fts_query;
use crate::migrations::run_migrations;
use crate::Result;
use anno_knowledge_core::{
    ChunkId, KnowledgeSearchHit, KnowledgeSearchRequest, KnowledgeStatus, ObjectId, ObjectType,
    PartId, RevisionId, SourceKind,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// Synchronous SQLite store. MCP service code keeps calls short and local.
pub struct KnowledgeControlStore {
    conn: Mutex<Connection>,
}

/// Input used by tests and later source/indexer code to store a pseudonymized chunk.
#[derive(Debug, Clone)]
pub struct TestChunkInput {
    /// Chunk id.
    pub chunk_id: ChunkId,
    /// Object id.
    pub object_id: ObjectId,
    /// Revision id.
    pub revision_id: RevisionId,
    /// Part id.
    pub part_id: PartId,
    /// Source family.
    pub source_kind: SourceKind,
    /// Object family.
    pub object_type: ObjectType,
    /// Pseudonymized title.
    pub title_pseudo: Option<String>,
    /// Pseudonymized body chunk.
    pub body_pseudo: String,
    /// Pseudonymized metadata JSON.
    pub metadata_pseudo_json: serde_json::Value,
}

impl KnowledgeControlStore {
    /// Open or create the knowledge SQLite database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Return local status without loading ML models.
    pub fn status(&self) -> Result<KnowledgeStatus> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        Ok(KnowledgeStatus {
            sources: count(&conn, "knowledge_sources")?,
            accounts: count(&conn, "source_accounts")?,
            scopes: count(&conn, "source_scopes")?,
            objects: count(&conn, "knowledge_objects")?,
            chunks: count(&conn, "knowledge_chunks")?,
            failures: count_failed_objects(&conn)?,
            models_loaded: false,
        })
    }

    /// Insert one pseudonymized chunk. This is public for the first slice and will be reused
    /// by source/indexer code in later phases.
    pub fn insert_test_chunk(&self, input: TestChunkInput) -> Result<()> {
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let now = Utc::now().to_rfc3339();
        let metadata = serde_json::to_string(&input.metadata_pseudo_json)?;
        let title_pseudo = input.title_pseudo.clone();
        let body_pseudo = input.body_pseudo.clone();
        let body_len = body_pseudo.chars().count() as u32;
        let chunk_id = input.chunk_id.as_string();
        let object_id = input.object_id.as_string();
        let revision_id = input.revision_id.as_string();
        let part_id = input.part_id.as_string();
        let source_kind = serde_json::to_value(input.source_kind)?
            .as_str()
            .expect("source kind serializes to string")
            .to_string();
        let object_type = serde_json::to_value(input.object_type)?
            .as_str()
            .expect("object type serializes to string")
            .to_string();

        conn.execute(
            "INSERT OR IGNORE INTO knowledge_objects \
             (object_id, source_id, account_id, scope_id, external_id, object_type, \
              title_pseudo, metadata_pseudo_json, source_url_policy, source_updated_at, state) \
             VALUES (?1, 'test-source', 'test-account', 'test-scope', ?1, ?2, ?3, ?4, NULL, ?5, 'pseudonymized')",
            params![
                &object_id,
                &object_type,
                &title_pseudo,
                &metadata,
                &now,
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_revisions \
             (revision_id, object_id, provider_version, observed_at) VALUES (?1, ?2, 'test', ?3)",
            params![&revision_id, &object_id, &now],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_parts \
             (part_id, object_id, part_type, title_pseudo, metadata_pseudo_json, extracted_chars) \
             VALUES (?1, ?2, 'file_body', ?3, ?4, ?5)",
            params![
                &part_id,
                &object_id,
                &title_pseudo,
                &metadata,
                body_len
            ],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO knowledge_chunks \
             (chunk_id, object_id, revision_id, part_id, source_kind, object_type, title_pseudo, \
             body_pseudo, metadata_pseudo_json, chunk_idx, char_start, char_end, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, ?10, ?11)",
            params![
                &chunk_id,
                &object_id,
                &revision_id,
                &part_id,
                &source_kind,
                &object_type,
                &title_pseudo,
                &body_pseudo,
                &metadata,
                body_len,
                &now,
            ],
        )?;
        conn.execute(
            "INSERT INTO knowledge_objects_fts \
             (chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, body_pseudo, metadata_pseudo) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &chunk_id,
                &object_id,
                &revision_id,
                &source_kind,
                &object_type,
                &title_pseudo,
                &body_pseudo,
                &metadata,
            ],
        )?;
        Ok(())
    }

    /// Run local FTS search. This must not load models.
    pub fn search_fast(&self, request: &KnowledgeSearchRequest) -> Result<Vec<KnowledgeSearchHit>> {
        let Some(fts_query) = build_fts_query(&request.query) else {
            return Ok(Vec::new());
        };
        let conn = self.conn.lock().expect("knowledge sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT chunk_id, object_id, revision_id, source_kind, object_type, title_pseudo, \
                    snippet(knowledge_objects_fts, 6, '[', ']', '...', 20) AS snippet, \
                    bm25(knowledge_objects_fts) AS score \
             FROM knowledge_objects_fts \
             WHERE knowledge_objects_fts MATCH ?1 \
             ORDER BY score \
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, request.top_k as i64], |row| {
            let source_kind_text: String = row.get(3)?;
            let object_type_text: String = row.get(4)?;
            Ok(KnowledgeSearchHit {
                chunk_id: ChunkId::new(parse_uuid(row.get::<_, String>(0)?)?),
                object_id: ObjectId::new(parse_uuid(row.get::<_, String>(1)?)?),
                revision_id: RevisionId::new(parse_uuid(row.get::<_, String>(2)?)?),
                source_kind: parse_source_kind(&source_kind_text)?,
                object_type: parse_object_type(&object_type_text)?,
                title_pseudo: row.get(5)?,
                snippet_pseudo: row.get(6)?,
                score: row.get::<_, f64>(7)? as f32,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

fn count(conn: &Connection, table: &str) -> Result<u64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count as u64)
}

fn count_failed_objects(conn: &Connection) -> Result<u64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM knowledge_objects WHERE state IN ('failed_retryable', 'failed_permanent')",
        [],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn parse_uuid(value: String) -> std::result::Result<uuid::Uuid, rusqlite::Error> {
    uuid::Uuid::parse_str(&value)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_source_kind(value: &str) -> std::result::Result<SourceKind, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_object_type(value: &str) -> std::result::Result<ObjectType, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_knowledge_core::{
        ChunkId, KnowledgeSearchRequest, ObjectId, PartId, RevisionId, SourceKind,
    };

    #[test]
    fn open_creates_database_and_empty_status() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");

        let store = KnowledgeControlStore::open(&db_path).expect("open store");
        let status = store.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert_eq!(status.chunks, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_returns_pseudonymized_chunks() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("knowledge.sqlite3");
        let store = KnowledgeControlStore::open(&db_path).expect("open store");

        let object_id = ObjectId::from_external(
            anno_knowledge_core::SourceKindForId::LocalFolder,
            "local",
            "folder-a",
            "file-a",
        );
        let revision_id = RevisionId::from_parts("file-a", "v1");
        let part_id = PartId::from_parts("file-a", "body");
        let chunk_id = ChunkId::from_parts(revision_id, part_id, 0);

        store
            .insert_test_chunk(TestChunkInput {
                chunk_id,
                object_id,
                revision_id,
                part_id,
                source_kind: SourceKind::LocalFolder,
                object_type: anno_knowledge_core::ObjectType::File,
                title_pseudo: Some("Document PERSON_1".to_string()),
                body_pseudo: "Le contrat de PERSON_1 contient CLAUSE_1.".to_string(),
                metadata_pseudo_json: serde_json::json!({"path": "FILE_1"}),
            })
            .expect("insert chunk");

        let hits = store
            .search_fast(&KnowledgeSearchRequest::new("contrat").with_top_k(5))
            .expect("search");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, chunk_id);
        assert!(hits[0].snippet_pseudo.contains("contrat"));
        assert!(!hits[0].snippet_pseudo.contains("Dupont"));
    }
}
```

Modify `crates/anno-knowledge-store/src/lib.rs`:

```rust
//! SQLite-backed local knowledge control and FTS store.

pub mod control_store;
pub mod error;
pub mod fts_query;
pub mod migrations;

pub use control_store::{KnowledgeControlStore, TestChunkInput};
pub use error::{KnowledgeStoreError, Result};
```

- [ ] **Step 4: Run store tests**

Run:

```powershell
cargo test -p anno-knowledge-store --no-default-features
```

Expected: PASS.

- [ ] **Step 5: Commit Task 5**

Run:

```powershell
git add crates/anno-knowledge-store
git commit -m "feat: add knowledge control store fast search"
```

## Task 6: MCP Knowledge Service Facade

**Files:**
- Modify: `crates/anno-rag-mcp/Cargo.toml`
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Create: `crates/anno-rag-mcp/src/knowledge.rs`

- [ ] **Step 1: Add MCP dependencies**

Modify `crates/anno-rag-mcp/Cargo.toml`:

```toml
[dependencies]
anno-rag         = { path = "../anno-rag" }
anno-rag-tabular = { path = "../anno-rag-tabular" }
anno-knowledge-core = { path = "../anno-knowledge-core" }
anno-knowledge-store = { path = "../anno-knowledge-store" }
```

Keep the existing dependencies below this block unchanged.

- [ ] **Step 2: Write facade tests**

Create `crates/anno-rag-mcp/src/knowledge.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    #[test]
    fn service_status_opens_empty_store_without_models() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let status = service.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_empty_store_returns_empty_hits() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let result = service.search(KnowledgeSearchParams {
            query: "contrat".to_string(),
            top_k: 5,
            mode: None,
        }).expect("search");

        assert_eq!(result.mode, "fast");
        assert!(result.hits.is_empty());
    }
}
```

- [ ] **Step 3: Run tests and verify failure**

Run:

```powershell
cargo test -p anno-rag-mcp knowledge --no-default-features
```

Expected: FAIL because `KnowledgeService` and params are not implemented.

- [ ] **Step 4: Implement facade**

Replace `crates/anno-rag-mcp/src/knowledge.rs` with:

```rust
//! MCP-facing facade for local knowledge tools.

use anno_knowledge_core::{KnowledgeSearchMode, KnowledgeSearchRequest, KnowledgeStatus};
use anno_knowledge_store::KnowledgeControlStore;
use anno_rag::config::AnnoRagConfig;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Parameters for `knowledge_search`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct KnowledgeSearchParams {
    /// User query. Phase 1 uses local FTS only and returns pseudonymized snippets.
    pub query: String,
    /// Maximum result count.
    #[serde(default = "default_knowledge_top_k")]
    pub top_k: usize,
    /// Search mode. Phase 1 accepts only `fast`.
    #[serde(default)]
    pub mode: Option<String>,
}

fn default_knowledge_top_k() -> usize {
    10
}

/// Result for `knowledge_search`.
#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeSearchResponse {
    /// Effective search mode.
    pub mode: String,
    /// Pseudonymized hits.
    pub hits: Vec<anno_knowledge_core::KnowledgeSearchHit>,
}

/// Local knowledge service. Opens SQLite only; does not load `Pipeline`.
pub struct KnowledgeService {
    store: KnowledgeControlStore,
}

impl KnowledgeService {
    /// Open the local knowledge service from Anno config.
    pub fn open(cfg: &AnnoRagConfig) -> anno_knowledge_store::Result<Self> {
        let path = knowledge_db_path(cfg);
        Ok(Self {
            store: KnowledgeControlStore::open(path)?,
        })
    }

    /// Return local status without loading ML models.
    pub fn status(&self) -> anno_knowledge_store::Result<KnowledgeStatus> {
        self.store.status()
    }

    /// Return configured sources. Phase 1 returns an empty list until source CRUD lands.
    pub fn sources(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Search the local knowledge FTS index.
    pub fn search(
        &self,
        params: KnowledgeSearchParams,
    ) -> anno_knowledge_store::Result<KnowledgeSearchResponse> {
        let mode = params.mode.as_deref().unwrap_or("fast");
        if mode != "fast" {
            return Ok(KnowledgeSearchResponse {
                mode: "fast".to_string(),
                hits: Vec::new(),
            });
        }

        let request = KnowledgeSearchRequest::new(params.query).with_top_k(params.top_k);
        let hits = match request.mode {
            KnowledgeSearchMode::Fast => self.store.search_fast(&request)?,
            KnowledgeSearchMode::Semantic | KnowledgeSearchMode::Deep => Vec::new(),
        };
        Ok(KnowledgeSearchResponse {
            mode: "fast".to_string(),
            hits,
        })
    }
}

fn knowledge_db_path(cfg: &AnnoRagConfig) -> PathBuf {
    cfg.data_dir.join("knowledge.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anno_rag::config::AnnoRagConfig;

    #[test]
    fn service_status_opens_empty_store_without_models() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let status = service.status().expect("status");

        assert_eq!(status.sources, 0);
        assert_eq!(status.objects, 0);
        assert!(!status.models_loaded);
    }

    #[test]
    fn fast_search_empty_store_returns_empty_hits() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cfg = AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..AnnoRagConfig::default()
        };

        let service = KnowledgeService::open(&cfg).expect("service");
        let result = service
            .search(KnowledgeSearchParams {
                query: "contrat".to_string(),
                top_k: 5,
                mode: None,
            })
            .expect("search");

        assert_eq!(result.mode, "fast");
        assert!(result.hits.is_empty());
    }
}
```

Modify the top of `crates/anno-rag-mcp/src/lib.rs`:

```rust
pub mod health;
pub mod knowledge;
pub mod tabular;
```

- [ ] **Step 5: Run facade tests**

Run:

```powershell
cargo test -p anno-rag-mcp knowledge --no-default-features
```

Expected: PASS.

- [ ] **Step 6: Commit Task 6**

Run:

```powershell
git add crates/anno-rag-mcp/Cargo.toml crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/knowledge.rs
git commit -m "feat: add knowledge mcp service facade"
```

## Task 7: Wire Lazy Knowledge Service Into MCP Tools

**Files:**
- Modify: `crates/anno-rag-mcp/src/lib.rs`
- Modify: `crates/anno-rag-mcp/src/health.rs`

- [ ] **Step 1: Run impact before editing `AnnoRagServer`**

Run:

```powershell
npx gitnexus impact --repo anno AnnoRagServer --direction upstream
```

Expected: record risk. If HIGH or CRITICAL, warn the user before editing.

- [ ] **Step 2: Write health test for new tools**

Add this test to `crates/anno-rag-mcp/src/health.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tool_names_includes_knowledge_tools() {
        let tools = all_tool_names();

        assert!(tools.contains(&"knowledge_sources".to_string()));
        assert!(tools.contains(&"knowledge_status".to_string()));
        assert!(tools.contains(&"knowledge_search".to_string()));
    }
}
```

- [ ] **Step 3: Run health test and verify failure**

Run:

```powershell
cargo test -p anno-rag-mcp all_tool_names_includes_knowledge_tools --no-default-features
```

Expected: FAIL because the names are not yet listed.

- [ ] **Step 4: Add lazy service cell to `AnnoRagServer`**

Modify `crates/anno-rag-mcp/src/lib.rs` imports:

```rust
use crate::knowledge::{KnowledgeSearchParams, KnowledgeService};
```

Modify `AnnoRagServer`:

```rust
#[derive(Clone)]
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    knowledge: Arc<OnceCell<Arc<KnowledgeService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    tabular_storage: Arc<OnceCell<Arc<anno_rag_tabular::storage::StorageHandle>>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}
```

Add helper inside `impl AnnoRagServer`:

```rust
async fn knowledge(&self) -> anno_knowledge_store::Result<&KnowledgeService> {
    self.knowledge
        .get_or_try_init(|| {
            let cfg = Arc::clone(&self.cfg);
            async move { KnowledgeService::open(&cfg).map(Arc::new) }
        })
        .await
        .map(|arc| arc.as_ref())
}
```

Modify both constructors:

```rust
knowledge: Arc::new(OnceCell::new()),
```

- [ ] **Step 5: Add three MCP tools without calling `pipeline()`**

Add these methods inside the existing tool `impl` in `crates/anno-rag-mcp/src/lib.rs`:

```rust
/// List configured knowledge sources. Phase 1 returns an empty list until source CRUD lands.
#[tool(
    description = "List configured Anno knowledge sources. Does not load local ML models."
)]
async fn knowledge_sources(&self) -> String {
    let service = match self.knowledge().await {
        Ok(service) => service,
        Err(e) => return format!("Error: {e}"),
    };
    serde_json::to_string_pretty(&service.sources()).unwrap_or_else(|e| format!("Error: {e}"))
}

/// Local knowledge status. Does not load local ML models.
#[tool(
    description = "Return local Anno knowledge status for sources, objects, chunks, and failures. Does not load local ML models."
)]
async fn knowledge_status(&self) -> String {
    let service = match self.knowledge().await {
        Ok(service) => service,
        Err(e) => return format!("Error: {e}"),
    };
    match service.status() {
        Ok(status) => serde_json::to_string_pretty(&status).unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    }
}

/// Search the local knowledge FTS index. Phase 1 supports fast mode only.
#[tool(
    description = "Search Anno's local multi-source knowledge index. Phase 1 supports fast SQLite FTS mode and returns pseudonymized snippets only."
)]
async fn knowledge_search(&self, Parameters(p): Parameters<KnowledgeSearchParams>) -> String {
    let service = match self.knowledge().await {
        Ok(service) => service,
        Err(e) => return format!("Error: {e}"),
    };
    match service.search(p) {
        Ok(result) => serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("Error: {e}")),
        Err(e) => format!("Error: {e}"),
    }
}
```

- [ ] **Step 6: Update advertised tools**

Modify `crates/anno-rag-mcp/src/health.rs` inside `all_tool_names()` after engine management tools:

```rust
        // Local multi-source knowledge
        "knowledge_sources",
        "knowledge_status",
        "knowledge_search",
```

- [ ] **Step 7: Run MCP tests**

Run:

```powershell
cargo test -p anno-rag-mcp all_tool_names_includes_knowledge_tools --no-default-features
cargo test -p anno-rag-mcp knowledge --no-default-features
```

Expected: PASS.

- [ ] **Step 8: Commit Task 7**

Run:

```powershell
git add crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs
git commit -m "feat: expose knowledge mcp tools"
```

## Task 8: Targeted Verification

**Files:**
- Read: `scripts/dev-fast.ps1`
- Read: changed files from this plan

- [ ] **Step 1: Format**

Run:

```powershell
cargo fmt --check
```

Expected: PASS. If it fails, run `cargo fmt`, then repeat `cargo fmt --check`.

- [ ] **Step 2: Run new crate tests**

Run:

```powershell
cargo test -p anno-knowledge-core --no-default-features
cargo test -p anno-knowledge-store --no-default-features
```

Expected: PASS.

- [ ] **Step 3: Run MCP-focused tests**

Run:

```powershell
cargo test -p anno-rag-mcp knowledge --no-default-features
cargo test -p anno-rag-mcp all_tool_names_includes_knowledge_tools --no-default-features
```

Expected: PASS.

- [ ] **Step 4: Run targeted Rust check**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-rag-mcp -Mode check
```

Expected: PASS for `anno-rag-mcp` and its local dependencies.

- [ ] **Step 5: Check that models were not required**

Run the MCP tests with a temporary data directory and no `ANNO_MODELS_DIR`:

```powershell
Remove-Item Env:\ANNO_MODELS_DIR -ErrorAction SilentlyContinue
cargo test -p anno-rag-mcp service_status_opens_empty_store_without_models --no-default-features
```

Expected: PASS. This confirms the knowledge status path does not call `Pipeline::new`.

- [ ] **Step 6: Detect changed symbols and files**

Run:

```powershell
npx gitnexus detect-changes
```

Expected: changes are limited to new knowledge crates and MCP service wiring. No existing `Store`, legal, memory, or current folder ingest flows should appear as modified behavior.

- [ ] **Step 7: Review diff before final commit**

Run:

```powershell
git diff --stat HEAD
git diff -- Cargo.toml crates/anno-knowledge-core crates/anno-knowledge-store crates/anno-rag-mcp/Cargo.toml crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/src/knowledge.rs
```

Expected: diff shows only the files in this plan.

- [ ] **Step 8: Final commit**

Run:

```powershell
git add Cargo.toml crates/anno-knowledge-core crates/anno-knowledge-store crates/anno-rag-mcp/Cargo.toml crates/anno-rag-mcp/src/lib.rs crates/anno-rag-mcp/src/health.rs crates/anno-rag-mcp/src/knowledge.rs
git commit -m "feat: add local knowledge service foundation"
```

## Acceptance Criteria

- Existing `search`, `legal_*`, memory, and tabular tools still compile without behavior changes.
- `knowledge_sources` returns a JSON array without loading `Pipeline`.
- `knowledge_status` opens `cfg.data_dir / "knowledge.sqlite3"` and returns counts without model loading.
- `knowledge_search` supports `mode = fast` and returns pseudonymized FTS hits from SQLite.
- `knowledge_search` with unsupported modes returns an empty fast response or a structured error without loading embedder/reranker.
- `anno_health.available_tools` includes the three new knowledge tools.
- No code in this plan edits `crates/anno-rag/src/store.rs`.
- No code in this plan edits `Pipeline::ingest_folder`, `Pipeline::search`, `legal_ingest`, or `legal_search`.
- Targeted tests pass.
- `npx gitnexus detect-changes` reports only expected knowledge/MCP files.

## Self-Review Against Spec

Covered by this plan:

- Additive architecture beside existing local-folder and legal paths.
- Separate Knowledge Core and Knowledge Store boundaries.
- SQLite `knowledge.sqlite3` as local control and FTS plane.
- Fast search as the first always-on path.
- MCP tools that do not load local ML models.
- `anno_health.available_tools` update.
- Local-machine performance posture: SQLite first, no always-hot vector daemon.
- Avoidance of `Store` migration and existing LanceDB `chunks` changes.

Deferred by design to separate plans:

- Local folder source connector and Kreuzberg extraction into knowledge objects.
- PII pseudonymization split APIs in `anno-rag`.
- LanceDB `knowledge_chunks_v1` vector projection.
- Outlook/Microsoft auth and Graph delta sync.
- Attachment body extraction budgets.
- Optional daemon split for sharing model memory across MCP clients.

Red-flag scan:

- No `Store` schema migration.
- No model-loading status path.
- No external provider token storage in SQLite.
- No raw text returned through MCP.
- No broad release or workspace build required for verification.
