# anno Knowledge Layer + Microsoft 365 Source Design

**Date:** 2026-05-26
**Status:** Draft for second review after codebase counter-review
**Scope:** Architecture for turning Hacienda/anno into a private local knowledge
index fed by Microsoft 365, starting with Outlook and expanding to the wider
suite. This is a design document only. It does not implement crates or change
runtime behavior.

---

## 1. Goal

Extend the current `anno-rag` system from "ingest a local folder" to "maintain a
private, local, continuously refreshed knowledge index" for high-value sources:

- Outlook mail first.
- Then OneDrive and SharePoint files.
- Then Calendar and Contacts.
- Later Notion, Gmail, Slack, local folders, or other sources through the same
  source abstraction.

The result should feel simple to a Claude Desktop user:

1. Install the `.mcpb` extension.
2. Ask Claude to connect Microsoft 365.
3. Sign in through Microsoft in the browser.
4. Ask questions over mail, files, meetings, and contacts.

The local machine remains the privacy boundary. Remote LLMs should receive only
pseudonymized chunks unless the user explicitly calls local rehydration.

---

## 2. Current Anno Baseline

The existing codebase already owns the hard privacy/RAG primitives:

- `anno-rag::Pipeline` orchestrates detect -> pseudonymize -> embed -> store.
- `anno-rag::Vault` wraps the `cloakpipe` encrypted vault and OS keyring based
  key derivation.
- `anno-rag::Store` owns LanceDB `chunks` and `memories`.
- `anno-rag-mcp` exposes Claude Desktop tools over stdio and already supports
  lazy pipeline startup.
- `ingest_folder` and `ingest_one` are local-file specific entrypoints.
- Memory is already bi-temporal enough to support recall, invalidation, entity
  refs, and graph expansion.
- Legal enrichment is already a second projection beside the generic chunk
  store.

The new work should not replace these pieces. It should add a source-agnostic
knowledge layer that feeds the existing pipeline.

---

## 3. Non-Goals

- Do not build a generic Microsoft 365 client UI.
- Do not request write/send/delete permissions in the first release.
- Do not make Claude Desktop talk directly to Microsoft source content.
- Do not store Microsoft refresh tokens in `claude_desktop_config.json`.
- Do not vendor or link AGPL code such as Bichon.
- Do not use webhooks for the local MVP. Polling delta links are simpler and
  better suited to a local desktop extension.
- Do not re-index entire mailboxes or drives after the first sync.

---

## 4. Design Decision

Build a new source-agnostic layer named **anno Knowledge Layer**, using
`kenn-io/msgvault` as the reference architecture for local-first sync/search
shape:

```text
Microsoft 365 / local folders / future SaaS sources
        |
        v
anno-source-* crates
        |
        v
anno-knowledge-core
        |
        v
anno-rag Pipeline
        |
        v
Vault + LanceDB + memory + MCP
```

`anno-rag` remains the privacy/RAG engine. The knowledge layer is responsible for
auth, sync, object versioning, source metadata, queues, and provenance.

This avoids an Outlook-only architecture while still letting Microsoft 365 be
the first serious source suite.

The important import from `msgvault` is architectural, not code-level:

- SQLite is the local source of truth for accounts, cursors, objects, jobs, and
  operational state.
- Search/index structures are derived from that source of truth and can be
  rebuilt.
- Sync runs are explicit, resumable, and observable.
- Attachments use content-addressed storage when persisted.
- MCP/API surfaces talk through a query abstraction instead of reaching into
  storage internals.
- Vector search has generation state, pending work, and stale/building/ready
  semantics.

Do not clone `msgvault` literally. It is Go, it stores full raw MIME by default,
and its Microsoft path is currently OAuth + IMAP XOAUTH2 oriented. Anno needs a
Rust implementation, a stricter privacy boundary, and Microsoft Graph for
suite-wide Outlook, OneDrive, SharePoint, Calendar, and Contacts support.

---

## 5. Crate Layout

Recommended crates:

```text
crates/
  anno-knowledge-core/
    src/
      lib.rs
      ids.rs
      object.rs
      query.rs
      source.rs
      sync.rs
      job.rs
      attachment_cache.rs
      acl.rs
      error.rs

  anno-source-microsoft/
    src/
      lib.rs
      auth.rs
      graph.rs
      account.rs
      outlook.rs
      drive.rs
      calendar.rs
      contacts.rs
      throttle.rs
      transform.rs

  anno-rag/
    existing modules
    plus source-agnostic ingest entrypoints

  anno-rag-mcp/
    existing MCP server
    plus knowledge and microsoft tools
```

`anno-source-microsoft` depends on `anno-knowledge-core`, but not on the vault.
It fetches provider data and returns normalized raw objects at the process
boundary only. `anno-knowledge-core` owns source metadata, cursors, ids, and
jobs. `anno-rag` depends on `anno-knowledge-core` to ingest normalized items.
`anno-rag-mcp` orchestrates user-facing tools.

---

## 6. Source Abstraction

Each connector implements a small interface:

```rust
trait KnowledgeSource {
    async fn connect(&self, account: SourceAccount) -> Result<ConnectResult>;
    async fn list_scopes(&self, account_id: AccountId) -> Result<Vec<SourceScope>>;
    async fn sync(&self, request: SyncRequest) -> Result<SyncBatch>;
    async fn fetch_object(&self, object_ref: ObjectRef) -> Result<KnowledgeObject>;
}
```

Important boundary: source crates fetch and normalize data. They do not
pseudonymize, embed, rehydrate, or decide what Claude sees.

The source output is always a `KnowledgeObject` plus optional parts and sync
state.

---

## 7. Core Data Model

### 7.1 KnowledgeSource

One installed source kind.

```rust
KnowledgeSource {
    source_id: SourceId,
    kind: SourceKind,          // microsoft, local_files, notion, gmail
    display_name: String,
    created_at: DateTime<Utc>,
}
```

### 7.2 SourceAccount

One connected account or tenant.

```rust
SourceAccount {
    account_id: AccountId,
    source_id: SourceId,
    provider_subject: String,  // Microsoft oid/sub, not an email if avoidable
    tenant_id: Option<String>,
    display_name_pseudo: String,
    scopes_granted: Vec<String>,
    auth_state: AuthState,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
}
```

OAuth refresh tokens stay in the OS keyring and are referenced by `account_id`.
They are not stored in LanceDB or config files.

`display_name_pseudo` is not produced by the source connector. The connector may
return a raw display name in a transient connect result; the knowledge layer
persists only a pseudonymized or redacted label. This keeps source crates free of
vault responsibilities while still preventing account metadata from becoming a
cleartext side channel.

### 7.3 KnowledgeObject

One logical item: email, file, calendar event, contact, Notion page, etc.

```rust
KnowledgeObject {
    object_id: ObjectId,
    source_id: SourceId,
    account_id: AccountId,
    external_id: String,
    object_type: ObjectType,   // email, file, event, contact, page
    title: String,
    body_markdown: String,
    metadata_json: serde_json::Value,
    acl_snapshot: AclSnapshot,
    source_url: Option<String>,
    content_hash: [u8; 32],
    updated_at: DateTime<Utc>,
    deleted: bool,
}
```

`title` and `body_markdown` are raw only inside the source-to-pipeline boundary.
The persisted searchable form is pseudonymized.

### 7.4 KnowledgePart

Large objects split into logical parts before chunking.

```rust
KnowledgePart {
    part_id: PartId,
    object_id: ObjectId,
    part_type: PartType,       // email_body, attachment, file_body, event_notes
    name: Option<String>,
    mime_type: Option<String>,
    body_markdown: String,
    byte_size: u64,
    content_hash: [u8; 32],
}
```

Attachments and files use the existing `kreuzberg` extraction path where
possible.

### 7.5 KnowledgeRevision

Every indexed version is explicit.

```rust
KnowledgeRevision {
    revision_id: RevisionId,
    object_id: ObjectId,
    source_cursor_id: CursorId,
    content_hash: [u8; 32],
    indexed_at: DateTime<Utc>,
    superseded_at: Option<DateTime<Utc>>,
}
```

This gives rollback/debug visibility and prevents silent duplicate indexing.

### 7.6 SyncCursor

Opaque provider state.

```rust
SyncCursor {
    cursor_id: CursorId,
    account_id: AccountId,
    scope_key: String,         // mail folder id, drive id, calendar id
    cursor_kind: CursorKind,   // microsoft_delta_link, local_watermark
    cursor_value_encrypted: Vec<u8>,
    last_success_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}
```

Microsoft delta links are opaque and must be reused exactly as returned.

### 7.7 IndexJob

Sync and indexing are decoupled.

```rust
IndexJob {
    job_id: JobId,
    account_id: AccountId,
    object_id: ObjectId,
    revision_id: Option<RevisionId>,
    priority: JobPriority,
    state: JobState,           // pending, running, failed, done, skipped
    attempts: u32,
    not_before: DateTime<Utc>,
    error: Option<String>,
}
```

This is what lets the app stay responsive when a mailbox or drive is huge.

---

## 8. Store Model

Use two storage planes:

1. **Authoritative plane: SQLite** for source accounts, cursors, objects,
   revisions, jobs, FTS metadata/body search, and audit state.
2. **Semantic data plane: LanceDB** for vector searchable pseudonymized chunks.
3. **Optional derived analytics plane: DuckDB/Parquet**, later, for large
   aggregate questions over mail/file metadata.

This split follows the codebase reality: the current LanceDB `Store` is optimized
for `chunks` and `memories`, while Microsoft Graph delta sync needs transactional
state transitions. The workspace already has `rusqlite` with the bundled SQLite
feature, so this does not introduce a new runtime service.

This also mirrors the best part of `msgvault`: one durable local database as
truth, specialized indexes/caches as rebuildable projections.

### 8.1 SQLite Authoritative Plane

Create a small `KnowledgeControlStore` backed by
`cfg.data_dir.join("knowledge.sqlite3")`.

Tables:

```text
source_accounts
sync_scopes
sync_cursors
knowledge_objects
knowledge_revisions
index_jobs
source_audit
sync_runs
sync_checkpoints
knowledge_objects_fts
index_generations
pending_index_jobs
```

The control store owns Graph delta cursor commits. A sync batch must be committed
as:

```text
BEGIN IMMEDIATE
  upsert changed objects
  insert revisions
  enqueue index jobs
  mark deleted objects as tombstoned
  update pending cursor -> committed cursor
COMMIT
```

Never advance a Microsoft `deltaLink` outside the same transaction that persists
the corresponding object/revision/job changes. On crash, the next sync either
replays from the last committed cursor or resumes a bounded recovery path.

Set `journal_mode=WAL`, `busy_timeout`, and `synchronous=NORMAL` on this
database. Microsoft sync and indexing are both long-running local workloads; WAL
keeps reads responsive while a worker commits batches.

### 8.2 FTS5 Projection

Add an SQLite FTS5 table for fast keyword search and metadata filters:

```text
knowledge_objects_fts(
  object_id UNINDEXED,
  revision_id UNINDEXED,
  title_pseudo,
  body_pseudo,
  sender_pseudo,
  recipients_pseudo,
  source_label_pseudo,
  tokenize='unicode61 remove_diacritics 1'
)
```

Use it as the cheap first-stage keyword signal for large mailboxes. LanceDB
hybrid search is still useful, but FTS5 gives predictable local performance for
queries such as sender, subject, dates, folder, and exact terms without loading
embedding models. Search can then fuse:

```text
SQLite FTS5 BM25 hits
  + LanceDB vector hits
  + metadata/date/source filters from SQLite
  -> reciprocal-rank fusion
  -> hydrated pseudonymized results
```

This is the main `msgvault` lesson for performance: do not force every local
mail query through embeddings. A huge mailbox needs a cheap lexical path.

### 8.3 LanceDB Data Plane

Do **not** extend the existing `chunks` table in the Microsoft MVP. The current
table has a fixed file-centric schema and an idempotence contract based on
`(doc_id, chunk_idx)`. Changing it in place would require a LanceDB schema
migration and would risk the existing legal ingest path.

Keep existing tables unchanged:

```text
chunks                 existing local-file/legal chunks
memories               existing memory table
legal_chunk_enrichment existing legal projection
```

Add a new table:

```text
knowledge_chunks_v1
```

Initial `knowledge_chunks_v1` columns:

```text
chunk_id              FixedSizeBinary(16)
object_id             FixedSizeBinary(16)
revision_id           FixedSizeBinary(16)
part_id               FixedSizeBinary(16), nullable
source_id             FixedSizeBinary(16)
account_id            FixedSizeBinary(16)
object_type           Utf8
title_pseudo          Utf8
text_pseudo           Utf8
source_updated_at     Timestamp(Microsecond)
source_url_pseudo     Utf8, nullable
acl_hash              FixedSizeBinary(32)
metadata_json_pseudo  Utf8
text_hash             FixedSizeBinary(32)
vector                FixedSizeList<Float32>(embed_dim)
indexed_at            Timestamp(Microsecond)
```

Search tools:

- Existing `search` continues to query `chunks`.
- New `knowledge_search` runs through `KnowledgeQueryEngine`, using SQLite
  FTS5 for lexical/filter-first retrieval and `knowledge_chunks_v1` for semantic
  vector retrieval.
- Local-file ingest may dual-write to `knowledge_chunks_v1` later, after the
  Microsoft path is stable.

### 8.4 Index Generations and Queue Claims

Model the semantic index as generations:

```text
index_generations(
  generation_id,
  model,
  dimension,
  fingerprint,
  started_at,
  seeded_at,
  completed_at,
  activated_at,
  state,              -- building, active, failed, retired
  chunk_count
)
```

`fingerprint` must include model id, embedding dimension, chunking policy, and
pseudonymization/index schema version. If any of those change, old vectors are
stale rather than silently mixed with new vectors.

Index workers should claim work with a token:

```text
pending_index_jobs(
  generation_id,
  job_id,
  claimed_at,
  claim_token,
  attempts,
  last_error
)
```

The worker pattern is:

```text
claim batch -> embed/upsert -> complete claimed jobs
             -> release or reclaim stale claims on failure/crash
```

This directly addresses large local mailboxes: work can run in bounded batches,
resume after crashes, and avoid duplicate indexing when multiple workers are
eventually allowed.

### 8.5 Attachment Cache

Use content-addressed storage only when a user enables attachment/file caching:

```text
attachments/{first-two-hex}/{sha256}
```

Persist metadata in SQLite:

```text
knowledge_attachments(
  attachment_id,
  object_id,
  part_id,
  filename_pseudo,
  mime_type,
  byte_size,
  sha256,
  storage_path,
  encryption_version,
  indexed_at
)
```

Default behavior for Outlook MVP: persist attachment metadata, but do not keep
raw attachment bytes unless needed for extraction and within budget. If raw
bytes are cached, encrypt them using the local vault/keyring boundary. This is
stricter than `msgvault`, which archives raw MIME as a product goal.

### 8.6 Query Engine Boundary

Introduce a `KnowledgeQueryEngine` trait in `anno-knowledge-core`:

```rust
trait KnowledgeQueryEngine {
    async fn search(&self, params: KnowledgeSearchParams) -> Result<KnowledgeSearchResult>;
    async fn open(&self, object_id: ObjectId) -> Result<Option<KnowledgeObjectView>>;
    async fn status(&self) -> Result<KnowledgeStatus>;
    async fn stats(&self, filter: KnowledgeStatsFilter) -> Result<KnowledgeStats>;
}
```

`anno-rag-mcp` should depend on this trait, not directly on SQLite or LanceDB.
That copies the successful `msgvault` boundary where MCP, HTTP, and TUI use a
query engine abstraction. It also makes it possible to add a faster derived
engine later without changing MCP tools.

---

## 9. Pipeline Migration

Today:

```text
ingest_folder -> ingest_one(path)
  -> kreuzberg extract
  -> detect
  -> vault pseudonymize
  -> embed
  -> Store::upsert(ChunkRecord)
```

Target:

```text
KnowledgeObject + KnowledgePart
  -> normalize/chunk
  -> detect
  -> vault pseudonymize
  -> embed
  -> Store::upsert_knowledge_chunks(...)
```

Add a source-agnostic pipeline method:

```rust
Pipeline::ingest_knowledge_object(object: KnowledgeObject) -> Result<IngestReport>
```

The first implementation must be a **parallel path**, not a rewrite of
`ingest_folder`. Current local ingest also owns OCR budgeting, deletion by
`source_path`, `.anon.md` output, legal enrichment, legal graph writes, and
post-ingest index optimization. Re-routing it during the Microsoft MVP would
raise unnecessary regression risk.

Phase 1 target:

```text
Microsoft/local test object -> ingest_knowledge_object -> knowledge_chunks_v1
existing ingest_folder      -> unchanged -> chunks + legal projections
```

Later, once `knowledge_chunks_v1` is stable, local file ingestion can dual-write:

```text
local file adapter -> KnowledgeObject -> ingest_knowledge_object
existing ingest_one path remains the source of .anon.md and legal enrichment
```

This preserves existing CLI/MCP behavior while making Microsoft 365 first-class.

### 9.1 Deterministic Ids

Define source ids before implementation:

```text
source_id   = UUIDv5("source-kind:name")
account_id  = UUIDv5(source_id, provider_subject + tenant_id)
object_id   = UUIDv5(source_id + account_id + external_immutable_id)
revision_id = UUIDv5(object_id + content_hash + provider_version)
part_id     = UUIDv5(object_id + provider_part_id)
chunk_id    = UUIDv5(revision_id + part_id + chunk_idx)
```

For Outlook, `external_immutable_id` should use Microsoft Graph immutable IDs
when available. `provider_version` should use `changeKey`,
`lastModifiedDateTime`, or the normalized content hash when the provider does
not expose a stable revision marker.

---

## 10. Microsoft 365 Source

### 10.1 Auth

Use Microsoft identity platform with authorization code + PKCE:

- Public client.
- No client secret in the binary.
- Browser-based login.
- Local loopback callback: `http://127.0.0.1:{port}/callback`.
- Refresh token stored in OS keyring.
- Access tokens kept in memory only.

First release can ship a Hacienda app registration. Enterprise mode lets admins
provide their own `client_id` and tenant policy.

`msgvault` is useful here because its Microsoft OAuth flow handles PKCE, state,
nonce, tenant-specific validation, token refresh timeouts, secure token files,
and redacted auth URLs. Anno should port those security properties to Rust while
changing the token sink to OS keyring and the source APIs to Graph.

### 10.2 Scopes by Feature

Ask for scopes lazily, not all at install time.

```text
Base sign-in: openid profile email offline_access User.Read
Outlook:      Mail.Read
Calendar:     Calendars.Read
Contacts:     Contacts.Read
OneDrive:     Files.Read
SharePoint:   Sites.Read.All or selected-site strategy, enterprise-gated
```

Avoid `Mail.ReadWrite`, `Mail.Send`, `Files.ReadWrite.All`, or broad admin
permissions in the consumer MVP.

### 10.3 Graph Client Choice

Recommended first implementation:

- Use a small Anno wrapper around Graph REST with `reqwest`, retry, throttle,
  paging, and delta-link handling.
- Use `graph-rs-sdk` as reference or dependency only where it saves real code.

Reasoning:

- Microsoft does not currently list Rust among official Microsoft Graph SDK
  languages.
- `graph-rs-sdk` is MIT and covers Graph broadly, including OAuth, paging, and
  delta links, but Anno should not expose its API through the whole codebase.
- `onedrive-api` is MIT and useful as a specialized reference for drive sync.

Do not base the Microsoft suite connector on IMAP. IMAP can cover Outlook mail,
but it cannot cover OneDrive, SharePoint, Calendar, Contacts, Graph delta links,
or tenant/site permission diagnostics. Graph is the right product boundary even
if the OAuth flow borrows heavily from `msgvault`.

### 10.4 Outlook

Outlook is the first production source.

```text
connect account
  -> list folders
  -> choose default set: Inbox, Sent, Archive, selected user folders
  -> initial messages/delta per folder
  -> store deltaLink per folder
  -> subsequent polling uses saved deltaLink
```

Normalize each message:

```text
object_type: email
title: subject
body_markdown: cleaned HTML/text body
metadata:
  message_id
  internet_message_id
  conversation_id
  folder_id
  sender
  recipients
  cc
  received_at
  sent_at
  has_attachments
source_url: Outlook web URL when available
```

Attachments are phase 2 for Outlook. The first sync should store attachment
metadata, then index selected attachments under size/type budgets.

### 10.5 OneDrive and SharePoint

Use drive delta sync.

```text
drive root delta
  -> changed driveItems
  -> skip folders after recording hierarchy
  -> fetch supported files
  -> extract with kreuzberg
  -> KnowledgeObject(file)
```

Default file policy:

- Include `.docx`, `.pdf`, `.txt`, `.md`, `.html`, `.pptx`, `.xlsx`.
- Skip videos, archives, binaries, and huge images.
- Size cap default: 25 MB per file for MVP.
- OCR only when enabled and within budget.

SharePoint is more sensitive than OneDrive because `Sites.Read.All` can be broad.
Make it an explicit enterprise feature or require the user/admin to select sites.

### 10.6 Calendar and Contacts

Calendar and contacts should feed context and graph relationships, not just
long-form chunks.

Calendar event object:

```text
title, body/description, attendees, organizer, start/end, location, meeting URL
```

Contact object:

```text
display name, company, role, email domains, phone labels
```

Contacts can improve entity canonicalization and graph expansion, but cleartext
contact details still go through the vault boundary.

---

## 11. Sync Architecture

### 11.1 Decouple Sync from Indexing

Sync is network-bound and should be fast. Indexing is CPU/model-bound and can be
slow. Keep them separate:

```text
sync worker
  -> fetch changed objects
  -> hash content
  -> write object/revision metadata transactionally in SQLite
  -> enqueue index jobs

index worker
  -> process jobs by priority
  -> privacy pipeline
  -> LanceDB upsert into knowledge_chunks_v1
```

This lets `microsoft_status` report that changes have been seen even if indexing
is still catching up.

Use `sync_runs` for every user-triggered or scheduled run:

```text
sync_runs(
  run_id,
  source_id,
  account_id,
  scope_key,
  sync_type,          -- initial, incremental, repair
  started_at,
  completed_at,
  status,             -- running, completed, failed, cancelled
  objects_seen,
  objects_added,
  objects_updated,
  objects_deleted,
  errors_count,
  cursor_before,
  cursor_after,
  error_message
)
```

For a huge mailbox, this table matters operationally: it lets
`microsoft_status` show whether the system is blocked on Microsoft throttling,
model indexing, expired cursors, or a bad object.

### 11.2 Initial Sync

Initial sync must be bounded:

```text
Default Outlook window: 180 days
Default mail folders: Inbox, Sent, Archive
Default drive scope: user's OneDrive recent/selected folders
Default max changed items per run: configurable, e.g. 1,000
```

Claude should be able to ask for broader history explicitly:

```text
"Index all Outlook mail from 2024"
"Add the SharePoint Legal site"
"Index attachments under 10 MB"
```

### 11.3 Incremental Sync

Each scope has a cursor:

```text
Outlook folder -> messages deltaLink
Drive root     -> driveItem deltaLink
Calendar       -> event deltaLink
Contacts       -> contacts deltaLink
```

On each run:

1. Load cursor.
2. Call provider delta endpoint.
3. Follow `nextLink` until page budget or completion.
4. Store the received `deltaLink` as pending state.
5. Enqueue changed objects.
6. Tombstone deleted objects.
7. Promote the pending `deltaLink` to committed state in the same SQLite
   transaction as the object/revision/job writes.

If a cursor expires or fails permanently, fall back to a bounded resync of that
scope, not a global reset.

Unlike `msgvault`'s Gmail incremental flow, do not advance a committed cursor
when changed objects failed to persist. It is acceptable to advance past objects
that failed only during embedding, because those jobs remain queued locally. It
is not acceptable to advance past objects that were never recorded in SQLite.

### 11.4 Scheduling

In the Claude Desktop MVP, use local polling:

```text
foreground sync: user-triggered MCP call
background sync: every 30-60 minutes while MCP server is alive
startup sync: lightweight, page-budgeted
```

Enterprise later:

- Graph webhooks.
- Relay service if the machine has no public callback URL.
- Admin consent and tenant-wide policy.

---

## 12. Retrieval Semantics

`knowledge_search` should search all indexed sources unless filtered:

```rust
KnowledgeSearchParams {
    query: String,
    top_k: usize,
    sources: Option<Vec<SourceKind>>,
    object_types: Option<Vec<ObjectType>>,
    accounts: Option<Vec<AccountId>>,
    time_range: Option<TimeRange>,
    include_deleted: bool,
    rerank: bool,
}
```

Search result:

```rust
KnowledgeHit {
    object_id: ObjectId,
    revision_id: RevisionId,
    object_type: ObjectType,
    title_pseudo: String,
    snippet_pseudo: String,
    source_label: String,
    source_updated_at: DateTime<Utc>,
    score: f32,
    provenance: Provenance,
}
```

`knowledge_open(object_id)` returns source metadata and pseudonymized context,
not cleartext by default. `rehydrate` remains the local opt-in boundary.

Ranking policy:

1. If embeddings are unavailable or stale, run SQLite FTS5 + metadata search.
2. If embeddings are active, run hybrid FTS5 + LanceDB vector search.
3. If the user asks filter-only questions, avoid embeddings and use SQLite.
4. Hydrate summaries in bulk from SQLite, then attach chunk snippets from
   LanceDB. Avoid per-hit object fetch loops.

This keeps first answer latency low on large mailboxes and avoids requiring the
model cache for basic Outlook search/status workflows.

---

## 13. MCP Surface

Add tools without breaking existing `search` and `memory_*` tools:

```text
knowledge_search
knowledge_open
knowledge_sources
knowledge_status
knowledge_forget
knowledge_refresh

microsoft_connect
microsoft_accounts
microsoft_configure_sources
microsoft_sync
microsoft_status
microsoft_disconnect
```

Tool behavior:

- `microsoft_connect` starts PKCE login and returns concise user instructions.
- `microsoft_configure_sources` lets Claude select mail folders, drives, sites,
  date windows, and attachment budgets.
- `microsoft_sync` starts a bounded background job and returns immediately.
- `microsoft_status` reports sync lag, cursor state, queue depth, failures, and
  last successful sync per scope.
- `knowledge_forget` supports source/account/object level erasure and triggers
  vault orphan cleanup.

Implementation constraint: Microsoft account/source/status tools must not call
`AnnoRagServer::pipeline()`. The existing pipeline lazy initializer refuses to
load if model directories are missing, but users must be able to connect
Microsoft 365 immediately after installing the extension.

`AnnoRagServer` should hold two lazy subsystems:

```text
pipeline: OnceCell<Arc<Pipeline>>       // vault + LanceDB + models
sources:  OnceCell<Arc<SourceManager>>  // SQLite + auth + Graph clients
```

`microsoft_connect`, `microsoft_accounts`, `microsoft_configure_sources`,
`microsoft_sync`, and `microsoft_status` use `SourceManager`. Index workers call
`Pipeline` only when model-backed indexing is required. If models are absent,
sync can still discover changes and queue jobs, while status reports
`indexing_blocked: models_not_downloaded`.

Also update the hardcoded MCP health tool list whenever adding new tools, so
`anno_health.available_tools` stays accurate.

---

## 14. Installation UX

The preferred user flow:

```text
Install Hacienda .mcpb
  -> Claude Desktop shows connector
  -> user asks "connect Microsoft 365"
  -> browser opens Microsoft login
  -> keyring stores refresh token
  -> Claude receives connected account summary
  -> user chooses sources and sync depth
```

No manual JSON edits for most users.

Advanced/enterprise config:

```text
ANNO_MICROSOFT_CLIENT_ID
ANNO_MICROSOFT_TENANT
ANNO_MICROSOFT_AUTHORITY
ANNO_MICROSOFT_DISABLE_CONSUMER_ACCOUNTS
ANNO_MICROSOFT_PROXY
```

These are optional and should be exposed through `.mcpb` user config only when
needed.

---

## 15. Privacy and Security

Rules:

- Raw Microsoft content is transient unless explicitly cached in encrypted form.
- Searchable chunks are pseudonymized.
- OAuth refresh tokens live in OS keyring.
- Delta links and source cursors are encrypted at rest.
- Source account display names are pseudonymized before persistence.
- Deleting a source removes objects, revisions, chunks, cursors, queued jobs,
  and orphan vault tokens.
- Errors must not log raw email body, file body, access token, refresh token, or
  delta link.
- All source APIs are read-only in the MVP.

Threats to design against:

- Tenant blocks user consent.
- Refresh token revoked.
- Microsoft throttling.
- Delta cursor expiry.
- Huge mailbox/drive.
- Attachment zip bomb or unsupported file.
- SharePoint permission broadening.
- User asks Claude to rehydrate sensitive results without noticing.

Mitigations:

- Admin/BYO app mode.
- Retry with backoff and `Retry-After`.
- Page budgets and resumable sync.
- File type and size allowlist.
- Explicit source configuration.
- Rehydration kept as local, visible, separate tool.

---

## 16. Dependency and License Position

Recommended:

- `reqwest`, `oauth2`, `keyring`, `serde`, `tokio`, `tracing`: standard Rust
  dependencies already aligned with the workspace style.
- `graph-rs-sdk`: MIT, broad Microsoft Graph coverage. Use as reference or
  controlled dependency behind `anno-source-microsoft::graph`.
- `onedrive-api`: MIT, useful OneDrive/Graph reference.
- `kreuzberg`: reuse existing document extraction path.

Avoid:

- Bichon code reuse: AGPL-3.0, not compatible with Hacienda's permissive
  distribution goals.
- GPL OneDrive sync projects.
- Python/Node connector runtime in the shipped desktop path unless used only for
  prototyping.

The long-term product should be a single Rust binary/extension path.

---

## 17. Phased Roadmap

### Phase 1 - Knowledge Core

- Add `anno-knowledge-core` id and object types.
- Add `KnowledgeControlStore` over SQLite for accounts, scopes, cursors,
  objects, revisions, jobs, FTS5 projection, sync runs, index generations, and
  audit.
- Define deterministic `source_id`, `account_id`, `object_id`, `revision_id`,
  `part_id`, and `chunk_id`.
- Add `KnowledgeQueryEngine` trait so MCP does not couple to storage internals.
- Do not modify local-file ingest.

### Phase 2 - Knowledge Data Plane

- Add LanceDB `knowledge_chunks_v1`.
- Add index generation and pending job claim/reclaim semantics.
- Add `Pipeline::ingest_knowledge_object` as a parallel path.
- Add `knowledge_search` over SQLite FTS5 + `knowledge_chunks_v1`.
- Keep existing `search`, `legal_ingest`, `legal_search`, and local file ingest
  unchanged.

### Phase 3 - SourceManager + Microsoft Auth

- Implement PKCE browser flow.
- Store refresh tokens in OS keyring.
- Add `SourceManager` independent from `Pipeline`.
- Add `microsoft_connect`, `microsoft_accounts`, `microsoft_disconnect`.
- Support Hacienda default app registration plus BYO client id.

### Phase 4 - Outlook MVP

- List mail folders.
- Delta sync selected folders.
- Normalize mail to `KnowledgeObject(email)`.
- Queue messages transactionally in SQLite.
- Index messages into `knowledge_chunks_v1` when models are available.
- Add `microsoft_sync`, `microsoft_status`, `knowledge_search`.

### Phase 5 - Attachments and Files

- Fetch selected Outlook attachments.
- Add encrypted content-addressed cache for attachments when enabled.
- Add OneDrive personal drive delta sync.
- Extract supported file types with `kreuzberg`.
- Enforce file size, MIME, OCR, and page budgets.

### Phase 6 - SharePoint, Calendar, Contacts

- Add SharePoint selected-site support.
- Add calendar event context.
- Add contact/entity canonicalization hints.
- Extend graph recall across mail, files, events, and people.

### Phase 7 - Local File Unification

- Add optional dual-write from local file ingest to `knowledge_chunks_v1`.
- Keep `.anon.md` output and legal enrichment owned by the existing ingest path.
- Promote `knowledge_search` in user docs once local files and Microsoft 365
  both land in the knowledge index.

### Phase 8 - Enterprise Sync

- Optional Graph webhook support.
- Admin consent documentation.
- Tenant policy diagnostics.
- Audit exports for connected sources and erasure actions.

---

## 18. Test Strategy

Unit tests:

- Object hashing and revision logic.
- Cursor encryption/decryption.
- SQLite transaction recovery for cursor/job commits.
- Scope parsing and source configuration.
- Retry/throttle behavior.
- Token redaction in errors/logs.

Integration tests:

- Mock Graph delta endpoints with `nextLink`, `deltaLink`, deletion facets, and
  throttling.
- Outlook message HTML -> markdown normalization.
- Drive file extraction budget handling.
- Queue resume after crash.
- `knowledge_forget` removes chunks and orphan vault tokens.
- `microsoft_connect` and `microsoft_status` work without downloaded models.

E2E tests:

- Claude Desktop MCP handshake still lists existing tools.
- `microsoft_connect` can be exercised with a fake provider.
- Sync -> restart -> search returns pseudonymized hits.
- Disconnect -> search no longer returns disconnected account content.

Manual enterprise tests:

- Tenant with user consent allowed.
- Tenant with user consent blocked.
- BYO app registration.
- Revoked refresh token.

---

## 19. Open Questions

1. Should Hacienda ship one default Microsoft app registration, or require BYO
   client id for early private beta?
2. Should the first Outlook sync default to 90 days or 180 days?
3. Should Sent and Archive be enabled by default, or only Inbox?
4. Should attachments be opt-in per folder, per MIME type, or global?
5. Should SharePoint be delayed until selected-site permissions are fully
   designed?
6. Should `knowledge_search` replace `search` in user docs, while keeping
   `search` as compatibility alias?
7. Should `knowledge_chunks_v1` eventually absorb local file chunks, or should
   the legacy `chunks` table remain a permanent compatibility path?
8. Should a DuckDB/Parquet analytics cache be planned from the start for mailbox
   statistics, or delayed until Outlook search/indexing works reliably?
9. Should raw email bodies ever be cached encrypted for reprocessing/export, or
   should Anno always re-fetch from Microsoft when cleartext is needed?

---

## 20. Approval Gate

This spec is ready for review if the following decisions are acceptable:

- Build a generic `anno-knowledge-core`, not an Outlook-only crate.
- Use SQLite as the authoritative knowledge store.
- Add SQLite FTS5 as the first-stage lexical search path for huge mailboxes.
- Add `knowledge_chunks_v1` instead of migrating `chunks` in the Microsoft MVP.
- Add explicit index generations and resumable pending job claims.
- Use a `KnowledgeQueryEngine` boundary for MCP/API retrieval.
- Start Microsoft 365 with Outlook read-only delta sync.
- Keep `anno-rag` as the privacy/RAG engine.
- Keep Microsoft source/account/status tools independent from model-backed
  `Pipeline` initialization.
- Use local polling delta links before webhooks.
- Keep OAuth tokens in OS keyring and keep source cursors encrypted.
- Avoid Bichon/GPL code reuse.

After approval, the next artifact should be an implementation plan with small
TDD-ready tasks, starting with `anno-knowledge-core` and the pipeline entrypoint
refactor.
