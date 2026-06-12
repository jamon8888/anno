# MsgVault Knowledge Connector — Design

**Date:** 2026-06-12
**Status:** Draft for review
**Depends on:** Knowledge service Phase 1 (core/store), Phase 2 (local folder source), privacy gateway (`knowledge_privacy.rs`)

## Goal

Index a user's mail and chat archive — acquired and stored locally by
[msgvault](https://github.com/kenn-io/msgvault) (Go, MIT, SQLite) — into Anno's
pseudonymized knowledge index, so `knowledge_search` covers email, SMS, WhatsApp,
iMessage, and Messenger history alongside local documents, without raw mail text
ever reaching an agent.

## Decisions (settled during brainstorming)

| Decision | Choice |
|---|---|
| Integration boundary | **Direct read-only SQLite access** to `msgvault.db`. No sidecar, no HTTP, no FFI, no CLI invocation in the data path. |
| Acquisition | **Anno never spawns the msgvault binary.** The user runs `msgvault sync` on their own schedule; Anno consumes whatever is in the DB. |
| Content scope (v1) | **Email and chat messages** (all `message_type` values msgvault stores). |
| Attachments (v1) | **Included**, gated per scope by the existing `SyncPolicy.include_attachments` flag (default `false`). |
| Connector trait | **Deferred.** Dispatch on `SourceKind` in the indexer; extract a shared commit helper instead. Introduce a `SourceConnector` trait only when a third connector appears. |

## Architecture

```
Gmail / IMAP / WhatsApp / SMS exports
        │   (OAuth, History API — msgvault's job, outside Anno)
        ▼
msgvault sync ──writes──► ~/.msgvault/msgvault.db   (SQLite, WAL)
                          ~/.msgvault/attachments/{2-char}/{sha256}
                                       │
              anno-source-msgvault     │   rusqlite, READ ONLY, busy_timeout
                                       ▼
   discover (cursor on messages.id, budget-bounded)
        → chunk (inline text; Kreuzberg only for attachments)
        → pseudonymize (existing privacy gateway, NER)
        → commit (knowledge.sqlite3 control store + FTS)
```

Two decoupled loops. The acquisition loop is Go and entirely the user's;
the indexing loop is Rust, in-process, and reuses the existing
discover → pseudonymize → commit pipeline. Data access is microseconds per
message; the NER pass dominates cost, so the run budget
(`max_files` / `max_millis`) is spent where it matters.

WAL concurrency: readers never block the msgvault writer and see consistent
snapshots. Open with `SQLITE_OPEN_READ_ONLY` and a busy timeout. Never use
`immutable=1` (the file is live).

## Components

### 1. New crate: `crates/anno-source-msgvault`

Pure discovery + change detection, mirroring `anno-source-local`'s footprint:
depends only on `anno-knowledge-core`, `rusqlite` (bundled), `serde_json`,
`chrono`, `sha2`, `thiserror`. No models, no Kreuzberg, no knowledge-store
dependency.

Public API:

```rust
pub struct MsgVaultSource { /* conn: rusqlite read-only */ }

pub struct MsgVaultBudget { pub max_messages: usize }          // default 200

pub struct DiscoveredMessage {
    pub external_id: String,        // see Identity below
    pub object_type: ObjectType,    // Email | ChatMessage
    pub content_hash: [u8; 32],     // sha256(subject \x1f body_text)
    pub sent_at: DateTime<Utc>,
    pub title_raw: Option<String>,  // email subject / conversation title
    pub body_raw: String,           // message_bodies.body_text (may be empty)
    pub metadata_raw: serde_json::Value, // sender, recipients, labels, conversation, msg type
    pub attachments: Vec<DiscoveredAttachment>,
    pub msgvault_rowid: i64,        // cursor value
}

pub struct DiscoveredAttachment {
    pub filename_raw: Option<String>,
    pub mime_type: Option<String>,
    pub path: PathBuf,              // resolved under <home>/attachments/
    pub content_hash_hex: Option<String>,
    pub byte_size: Option<u64>,
}

impl MsgVaultSource {
    /// Open <home>/msgvault.db read-only and run the schema compatibility check.
    pub fn open(msgvault_home: &Path) -> Result<Self>;

    /// List msgvault accounts (`sources` rows) for registration.
    pub fn list_accounts(&self) -> Result<Vec<MsgVaultAccount>>;

    /// Messages with rowid > cursor for one msgvault account, ascending,
    /// excluding soft-deleted rows. Returns the new cursor (max rowid seen).
    pub fn discover(&self, account: i64, cursor: i64, budget: &MsgVaultBudget)
        -> Result<(Vec<DiscoveredMessage>, i64)>;

    /// Rowids at or below `cursor` soft-deleted in msgvault
    /// (deleted_at or deleted_from_source_at set) — for forget reconciliation.
    pub fn deleted_up_to(&self, account: i64, cursor: i64) -> Result<Vec<i64>>;
}
```

The discover query joins `messages ⋈ message_bodies ⋈ message_recipients
⋈ participants ⋈ message_labels ⋈ labels ⋈ conversations`, filtered by
`m.source_id = ?account AND m.id > ?cursor AND m.deleted_at IS NULL AND
m.deleted_from_source_at IS NULL`, ordered by `m.id`, limited by budget.

**Schema compatibility check** at `open()`: verify the tables and columns the
queries touch exist (via `pragma_table_info`). On mismatch return
`MsgVaultSourceError::IncompatibleSchema { detail }` so `knowledge_sync`
surfaces "msgvault version unsupported" instead of misreading data.
msgvault's schema is idempotent/append-only by design, so this should be rare.

### 2. Type additions: `anno-knowledge-core` (additive only)

- `SourceKind::MsgVault` (`source.rs`) and `SourceKindForId::MsgVault`
  (`"msgvault"`, `ids.rs`).
- `PartType::ChatBody` (`object.rs`) for chat message bodies.
  `ObjectType::ChatMessage`, `ScopeKind::MailFolder`, and
  `ScopeKind::Channel` already exist.

Identity mapping:

| Anno entity | Derivation |
|---|---|
| `SourceId` | `from_parts(MsgVault, canonical msgvault home path)` — one Anno source per msgvault installation |
| `AccountId` | `from_parts(source_id, "{source_type}:{identifier}")` — one per msgvault `sources` row |
| `ScopeId` | `from_parts(account_id, provider_key)`; one scope per account. `provider_key` = msgvault `sources.id` (as string). `ScopeKind::MailFolder` for email accounts, `Channel` for chat accounts |
| `external_id` (object) | `source_message_id` when non-null, else `"rowid:{messages.id}"` |
| `provider_version` | hex of `sha256(subject \x1f body_text)` — content-based, like the local-folder connector |
| `PartId` | `"email_body"` / `"chat_body"` / `"attachment:{attachments.id}"` |

Labels (Gmail) are carried in `metadata_raw`, not modeled as separate scopes
in v1 — a message belongs to many labels, and scope-per-label would duplicate
objects. The pseudonymized label list remains searchable via chunk metadata.

### 3. Store change: per-scope sync cursor (`anno-knowledge-store`)

New migration: `ALTER TABLE knowledge_scopes ADD COLUMN sync_cursor TEXT`
(nullable; local-folder scopes leave it NULL). Two accessors:
`scope_sync_cursor(scope_id) -> Option<String>` and
`set_scope_sync_cursor(scope_id, value)`. The msgvault connector stores the
high-water `messages.id` here, so each run reads only new rows. Content-hash
revision checks keep re-runs idempotent if the cursor is ever reset.

Also: generalize registration with `register_msgvault(reg) -> Registered`
(source + N accounts + N scopes in one call), alongside the existing
`register_local_folder` (unchanged).

### 4. Indexer: `sync_msgvault_scope` (`anno-rag-mcp/src/indexer.rs`)

Per enabled msgvault scope:

1. `MsgVaultSource::open(home)`; on `IncompatibleSchema`, fail the source with
   a clear message (counts as setup error, not per-object failure).
2. Load cursor; `discover(account, cursor, budget)`.
3. Per message:
   - **Body part:** chunk `body_raw` with the new inline chunker (below);
     empty bodies still index the subject/metadata. Part type
     `EmailBody` or `ChatBody` by object type. Chat messages are typically
     one chunk.
   - **Attachment parts** (only if `scope.sync_policy.include_attachments`):
     for each attachment whose `mime_type`/extension Kreuzberg supports, run
     the existing `ingest::extract(path)` and add `AttachmentBody` chunks.
     Per-attachment failures are logged and counted, never fatal.
   - Pseudonymize all chunks via `pipeline.pseudonymize_knowledge_object`
     (one `PrivacyIndexInput` per part) and `commit_object`.
4. Advance and persist the cursor only after the batch commits.
5. **Deletion reconciliation:** `deleted_up_to(account, cursor)` → map rowids
   to `ObjectId`s → `forget_object` (GDPR-consistent with msgvault's own
   deletion staging). Counted as `forgotten` in `SyncSummary`.
6. Respect `max_millis` between messages, as `sync_local_scope` does.

Dispatch in `KnowledgeService::sync` on `source.kind`:
`LocalFolder → sync_local_scope`, `MsgVault → sync_msgvault_scope`. The
extract-tail shared by both (pseudonymize-input assembly + commit) is factored
into a private helper; no trait.

### 5. Inline chunking: `anno-rag/src/ingest.rs`

New public function `chunk_text(text: &str, cfg: &AnnoRagConfig)
-> Vec<ExtractedChunk>` reusing the existing Kreuzberg `chunking_config`
(markdown chunker, `chunk_max_chars`, `chunk_overlap`). `extract()` is
refactored to call it; behavior of the file path is unchanged.

### 6. MCP surface: `anno-rag-mcp`

- **New tool `knowledge_add_msgvault`** — params:
  `{ path?: string }` (msgvault home; default `~/.msgvault`, honoring
  `MSGVAULT_HOME`). Opens the DB read-only, runs the schema check, registers
  source + accounts + scopes, returns
  `{ source_id, accounts: [{ label, kind, scope_id }] }`. Does not load models.
  Display labels are pseudonymized (`msgvault_<uuid5-prefix>` pattern, same
  scheme as `pseudo_folder_label`); account emails/phone numbers never appear
  in labels.
- `knowledge_sync`, `knowledge_sources`, `knowledge_status`,
  `knowledge_search`, `knowledge_forget` work unchanged through the new kind.
- `health.rs` tool list gains the new tool name.
- Path policy: the msgvault home must pass the same filesystem allowlist
  validation used by `validate_sync_source_paths`.
- CLI parity: mirror `knowledge_add_msgvault` in `anno-rag-bin` per the
  existing CLI-parity convention.

## Privacy

- Raw subjects, bodies, participant names/addresses, label names, and
  conversation titles exist **only in memory** between discover and
  pseudonymize. Nothing raw is written to `knowledge.sqlite3` or logs
  (per `.claude/rules/privacy.md`: no transcripts, no matter text in logs).
- Source/account/scope display labels are pseudonymized at registration.
- `metadata_raw` (sender, recipients, labels, conversation title, message
  type, timestamps) goes through the privacy gateway like local-folder
  metadata; only `metadata_pseudo` is persisted.
- `forget_source` / deletion reconciliation provide GDPR Art. 17 erasure,
  consistent with the rest of the knowledge service.
- Anno opens msgvault data strictly read-only; it never mutates or deletes
  the user's archive.

## Error handling

| Failure | Behavior |
|---|---|
| msgvault DB missing / locked beyond busy timeout | Setup error on the source; other sources still sync |
| Schema check fails | `IncompatibleSchema` setup error with the offending table/column named |
| Empty `body_text` | Index subject + metadata only (common for media-only chats) |
| Attachment file missing on disk / extraction fails | Log, count in `failed`, continue |
| Pseudonymization failure | Count in `failed`, continue (same as local folder) |
| Budget/time exhausted mid-run | `truncated = true`; cursor persists progress; next run resumes |

## Testing

Fixture-based, no msgvault binary required: tests create a minimal
`msgvault.db` from an embedded subset of msgvault's `schema.sql` and insert
synthetic rows.

- `anno-source-msgvault` unit tests: discover ordering/cursor/budget; chat vs
  email object typing; soft-deleted exclusion; `deleted_up_to`;
  attachment path resolution; schema-check failure on a doctored DB;
  external-id fallback when `source_message_id` is NULL.
- Store tests: `sync_cursor` migration + round-trip on fresh and existing DBs;
  `register_msgvault` idempotency.
- Indexer integration test (temp dirs, test pipeline): end-to-end sync of a
  fixture DB → FTS hits; pseudonymization leak assertions (no raw names,
  emails, or label text in `knowledge.sqlite3`); deletion reconciliation;
  cursor resume across two runs.
- MCP tests: `knowledge_add_msgvault` registration + label leak checks;
  health tool list.
- Local loop: `scripts/test-local.ps1 -Package anno-source-msgvault`, then
  targeted packages per crate touched (never workspace-wide).

## Out of scope (v1)

- Spawning or supervising the msgvault binary (acquisition stays external).
- Reading `message_raw` MIME or msgvault's own FTS/vector tables.
- Scope-per-label modeling; reactions; participants as first-class entities.
- Semantic/vector projection of knowledge chunks (Phase 3 concern, applies
  to all sources uniformly).
- Postgres-backed msgvault stores (SQLite only).

## Risks

- **Schema drift across msgvault releases** — mitigated by the startup
  compatibility check, content-hash revisions (safe re-index), and pinning
  tested msgvault versions in the connector's docs. Low likelihood:
  upstream schema is deliberately idempotent.
- **Very large archives (500k+ messages)** — initial indexing takes many
  budgeted runs by design; cursor makes progress monotonic. NER throughput is
  the limit, unchanged by this connector.
- **Chat noise in search results** — millions of short chat chunks may dilute
  FTS ranking. v1 accepts this (user chose full scope); `knowledge_search`
  source filtering already exists if it becomes a problem.
