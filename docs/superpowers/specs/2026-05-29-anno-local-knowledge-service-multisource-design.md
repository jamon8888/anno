# Anno Local Knowledge Service Multi-Source Design

**Date:** 2026-05-29
**Status:** Draft for review
**Scope:** Architecture for extending the current Claude Desktop/Codex Anno MCP
plugin from local-folder RAG to a local-first, privacy-preserving knowledge
service. The first external source is Outlook, but the architecture must support
local folders, documents, attachments, Microsoft 365, Gmail, Slack, Notion, and
future sources without changing the privacy boundary.

This document supersedes the narrower Outlook-only direction in
`2026-05-26-anno-outlook-lance-mvp-design.md` for product architecture, while
keeping its useful Outlook sync details. It also refines
`2026-05-26-anno-knowledge-microsoft-design.md` for local-machine performance.

---

## 1. Goal

Today, Anno works as a Claude Desktop/Codex MCP plugin that indexes a local
folder chosen by the user:

```text
Claude Desktop
  -> anno-rag mcp
  -> local folder ingest
  -> kreuzberg extraction
  -> local PII detection
  -> local vault pseudonymization
  -> local LanceDB chunks
  -> Claude receives pseudonymized chunks
```

The target is to preserve this workflow and extend it:

```text
Claude Desktop / Codex
  -> anno-rag mcp
  -> Anno Local Knowledge Service
  -> local folder / Outlook / future sources
  -> local extraction + local PII pipeline
  -> local FTS + local vector index
  -> Claude receives pseudonymized snippets only
```

The product promise is:

- Raw source content can be seen by Anno and its local models.
- Remote Claude should receive only pseudonymized text.
- Existing folder ingest and legal RAG tools keep working.
- Multi-source search becomes available through new `knowledge_*` tools.
- The architecture runs well on a recent personal machine, not an enterprise
  server.

---

## 2. Current Codebase Baseline

The current codebase already provides the core privacy and local RAG primitives.

Relevant facts:

- `crates/anno-rag/src/pipeline.rs::Pipeline` is the current end-to-end engine:
  local detection, vault pseudonymization, embedding, LanceDB storage, memory,
  and legal projections.
- `Pipeline::ingest_folder` and `Pipeline::ingest_one` are local-file specific.
  They call `kreuzberg`, perform OCR budgeting, produce `.anon.md` outputs,
  enrich legal chunks, update the legal knowledge graph, and optimize indexes.
- `crates/anno-rag/src/store.rs::Store` owns the current LanceDB `chunks` and
  `memories` tables. Its `chunks` schema is file-centric:
  `source_path`, `folder_path`, `doc_id`, `chunk_idx`, `text_pseudo`, `vector`.
- `Store` has a large blast radius. GitNexus impact analysis reports CRITICAL
  risk for broad `Store` changes: 78 impacted symbols, 31 direct dependencies,
  13 modules. Therefore the existing `Store` must not be migrated in place.
- `crates/anno-rag-mcp/src/lib.rs::AnnoRagServer` is the MCP stdio facade. It
  already lazy-initializes `Pipeline` so the MCP server can start before models
  are downloaded.
- `anno_health`, `anno_init_vault`, and `download_models` show the right
  pattern: some MCP tools must work without loading detector or embedder models.
- `AnnoRagConfig` already centralizes local paths:
  `data_dir`, `vault_path()`, `index_path()`, `models_cache()`, and
  `outputs_dir()`.
- `kreuzberg` is already a workspace dependency with document, email, office,
  excel, xml, archive, chunking, and tokio runtime support.
- The shipped models are local. Current binary messaging states roughly
  embedder ~470 MiB and NER ~500 MiB. These must remain lazy and on-demand.

Design consequence:

The new system must be additive:

```text
existing search/legal tools -> existing Store/chunks
new knowledge tools         -> new KnowledgeControlStore + knowledge index
```

---

## 3. Trust Boundary

Anno is the local trust boundary.

Three data worlds must stay separate:

```text
1. Raw local world
   Source bytes, email bodies, attachments, extracted text.
   Visible only to Anno and local models.

2. Pseudonymized local world
   SQLite FTS, LanceDB vectors, snippets, metadata, audit status.
   This is the persistent search world.

3. Claude world
   Claude Desktop/Codex receives only pseudonymized snippets through MCP.
```

Rule:

```text
Any content from Outlook, files, or future sources must pass through Anno's
local PII pipeline before it is exposed through MCP search/open tools.
```

Native Claude connectors for Microsoft, Gmail, Slack, or similar sources cannot
be forced through Anno. They are separate connectors and should be considered
outside the Anno PII guarantee. Sensitive workflows should use Anno source
connectors instead.

---

## 4. Non-Goals

- Do not replace the existing local-folder `search`, `legal_ingest`, or
  `legal_search` behavior in the first implementation.
- Do not migrate the existing LanceDB `chunks` schema in place.
- Do not require a server database, Redis, external queue, or enterprise
  deployment.
- Do not load NER, embedder, or reranker models just to connect a source,
  list status, or sync metadata.
- Do not store raw email bodies or document text by default.
- Do not rely on Claude native connectors for privacy-sensitive source access.
- Do not request write/send/delete provider scopes in the first Microsoft
  implementation.
- Do not make reranking a default requirement for usable search.

---

## 5. Design Decision

Build a new **Anno Local Knowledge Service** as an internal service layer behind
the existing MCP binary.

The service is local-first and single-user by default:

```text
Claude Desktop / Codex
        |
        v
anno-rag-mcp
        |
        v
Anno Local Knowledge Service
        |
        +-- SourceManager
        |     +-- LocalFolderSource
        |     +-- MicrosoftOutlookSource
        |     +-- future Google/Slack/Notion sources
        |
        +-- KnowledgeControlStore (SQLite)
        +-- KnowledgeFtsIndex (SQLite FTS5)
        +-- KnowledgeVectorStore (LanceDB knowledge_chunks_v1)
        +-- KnowledgeIndexer
        +-- existing anno-rag Pipeline primitives
        +-- existing Vault/keyring
```

MVP deployment:

```text
anno-rag mcp
  -> runs KnowledgeService in-process
```

Future deployment, only if needed:

```text
anno-rag daemon
  -> long-lived local service

anno-rag mcp
  -> thin stdio bridge to daemon
```

The in-process path is simpler and matches the current `.mcpb`/Claude Desktop
model. The code boundaries must still make daemon extraction possible later,
because Claude Desktop and Codex may otherwise launch separate processes and
duplicate model memory.

---

## 6. Performance Position

The system targets recent local machines, not large servers.

Assumed baseline:

- 8 to 16 CPU cores.
- NVMe SSD.
- 16 to 32 GiB RAM.
- No always-on external database.
- Occasional large corpora: tens of thousands of emails/documents.
- User may have Claude Desktop, browser, office apps, and Anno running together.

The architecture must therefore separate:

```text
light control plane:
  source config, sync metadata, status, FTS, queue state

heavy ML plane:
  NER, embeddings, reranking
```

Required behavior:

- Idle MCP/service should remain lightweight.
- Source connection and status must work without downloaded models.
- Sync can fetch metadata and queue work without loading detector/embedder.
- Pseudonymization is required before any content becomes searchable.
- FTS search works as soon as pseudonymized text is written.
- Vector search is a background upgrade, not a blocker.
- Rerank is opt-in or query-mode dependent.

Target operating modes:

```text
fast:
  SQLite FTS + metadata filters, no embedder load.

normal:
  FTS + ready vectors, embedder loaded only when needed.

deep:
  FTS + vectors + local rerank. Slower and explicit.
```

---

## 7. Local Models Policy

All models that see cleartext must be local.

Allowed local cleartext readers:

- PII regex/pattern detector.
- Local NER model.
- Local document extractor.
- Optional local reranker, if configured for cleartext rerank.

Remote Claude should not receive cleartext source content.

Default indexing path:

```text
raw text
  -> local PII detection
  -> vault pseudonymization
  -> pseudonymized text
  -> SQLite FTS
  -> local embeddings over pseudonymized text
  -> LanceDB vector rows
```

Embedding should use pseudonymized text by default. This keeps vector indexes
rebuildable without retaining raw source bodies.

Rerank has two modes:

```text
pseudo_rerank:
  rerank pseudonymized passages only.

trusted_local_rerank:
  rehydrate top candidates locally, rerank in cleartext locally, return only
  pseudonymized snippets to Claude.
```

The default is `pseudo_rerank` or no rerank. `trusted_local_rerank` must be
explicitly marked local-only in config and documentation.

---

## 8. Crate Layout

Recommended additive crates:

```text
crates/
  anno-knowledge-core/
    src/
      lib.rs
      ids.rs
      object.rs
      part.rs
      source.rs
      sync.rs
      job.rs
      query.rs
      status.rs
      error.rs

  anno-knowledge-store/
    src/
      lib.rs
      control_store.rs
      fts.rs
      vector_store.rs
      schema.rs
      migrations.rs
      error.rs

  anno-source-local/
    src/
      lib.rs
      folder.rs
      transform.rs

  anno-source-microsoft/
    src/
      lib.rs
      auth.rs
      graph.rs
      account.rs
      outlook.rs
      drive.rs
      throttle.rs
      transform.rs

  anno-rag/
    existing modules
    plus narrow privacy/indexer entrypoints

  anno-rag-mcp/
    existing MCP facade
    plus knowledge tools
```

Dependency direction:

```text
anno-knowledge-core
  no dependency on anno-rag, MCP, Microsoft, LanceDB, or SQLite

anno-knowledge-store
  depends on anno-knowledge-core, rusqlite, lancedb

anno-source-*
  depends on anno-knowledge-core

anno-rag
  depends on anno-knowledge-core only when adding privacy/indexer entrypoints

anno-rag-mcp
  depends on anno-rag, anno-knowledge-store, source crates, and service layer
```

If crate count feels heavy during early implementation, `anno-knowledge-store`
can start as a module under `anno-rag` or `anno-rag-mcp`, but public boundaries
should still match the crates above.

---

## 9. Core Types

### 9.1 SourceKind

```rust
enum SourceKind {
    LocalFolder,
    MicrosoftOutlook,
    MicrosoftOneDrive,
    MicrosoftSharePoint,
    Gmail,
    GoogleDrive,
    Slack,
    Notion,
}
```

### 9.2 KnowledgeSource

One configured source integration.

```rust
struct KnowledgeSource {
    source_id: SourceId,
    kind: SourceKind,
    display_label_pseudo: String,
    created_at: DateTime<Utc>,
    enabled: bool,
}
```

### 9.3 SourceAccount

One account or local identity.

```rust
struct SourceAccount {
    account_id: AccountId,
    source_id: SourceId,
    provider_subject: String,
    tenant_id: Option<String>,
    display_label_pseudo: String,
    scopes_granted: Vec<String>,
    auth_ref: Option<String>,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
}
```

Provider tokens stay in the OS keyring. SQLite stores only references and
pseudonymized labels.

### 9.4 SourceScope

One selectable area inside a source.

```rust
struct SourceScope {
    scope_id: ScopeId,
    account_id: AccountId,
    kind: ScopeKind,
    provider_key: String,
    display_label_pseudo: String,
    sync_policy: SyncPolicy,
    enabled: bool,
}
```

Examples:

- Local folder path.
- Outlook folder id.
- OneDrive drive root or selected folder.
- SharePoint selected site.
- Gmail label.

### 9.5 KnowledgeObject

One logical source object.

```rust
struct KnowledgeObject {
    object_id: ObjectId,
    source_id: SourceId,
    account_id: AccountId,
    scope_id: ScopeId,
    external_id: String,
    object_type: ObjectType,
    title_raw: Option<String>,
    metadata_raw: serde_json::Value,
    source_url: Option<String>,
    source_updated_at: DateTime<Utc>,
    content_hash: [u8; 32],
    deleted: bool,
}
```

`title_raw` and `metadata_raw` are transient until pseudonymized. Persistent
columns should store pseudonymized title and metadata.

### 9.6 KnowledgePart

One textual or binary part of an object.

```rust
struct KnowledgePart {
    part_id: PartId,
    object_id: ObjectId,
    part_type: PartType,
    name_raw: Option<String>,
    mime_type: Option<String>,
    body_raw: PartBody,
    byte_size: u64,
    content_hash: [u8; 32],
}
```

`PartBody`:

```rust
enum PartBody {
    Text(String),
    Bytes(Vec<u8>),
    Deferred(DeferredPartRef),
}
```

Kreuzberg handles `Bytes` for supported document types. Email body text can be
normalized directly before chunking.

### 9.7 ObjectType and PartType

```rust
enum ObjectType {
    LocalFile,
    Email,
    Attachment,
    DriveFile,
    CalendarEvent,
    Contact,
    ChatMessage,
    Page,
}

enum PartType {
    FileBody,
    EmailBody,
    AttachmentBody,
    EventNotes,
    ChatText,
    MetadataSummary,
}
```

### 9.8 KnowledgeRevision

```rust
struct KnowledgeRevision {
    revision_id: RevisionId,
    object_id: ObjectId,
    source_cursor_id: Option<CursorId>,
    provider_version: Option<String>,
    content_hash: [u8; 32],
    indexed_at: Option<DateTime<Utc>>,
    superseded_at: Option<DateTime<Utc>>,
}
```

### 9.9 Object State

Every object/revision should expose a state:

```text
seen
fetched
extracted
pseudonymized
fts_ready
vector_pending
vector_ready
failed
forgotten
```

These states let `knowledge_status` report useful progress:

```text
Outlook Inbox:
  12,430 seen
  12,430 pseudonymized
  12,430 FTS ready
  8,100 vector ready
  42 failed extraction
```

---

## 10. Deterministic IDs

Use UUIDv5 for stable IDs:

```text
source_id   = UUIDv5("source-kind:stable-source-name")
account_id  = UUIDv5(source_id + provider_subject + tenant_id)
scope_id    = UUIDv5(account_id + provider_scope_key)
object_id   = UUIDv5(scope_id + external_immutable_id)
revision_id = UUIDv5(object_id + content_hash + provider_version)
part_id     = UUIDv5(object_id + provider_part_id_or_part_type)
chunk_id    = UUIDv5(revision_id + part_id + chunk_idx)
```

For local files:

```text
external_immutable_id = canonical absolute path or source root relative path
provider_version      = file mtime + size + content hash
```

For Outlook:

```text
external_immutable_id = Microsoft Graph immutable id when available
provider_version      = changeKey or lastModifiedDateTime + content hash
```

---

## 11. Storage Model

Use three local storage planes.

### 11.1 Control Plane: SQLite

Path:

```text
cfg.data_dir / "knowledge.sqlite3"
```

SQLite is authoritative for:

- sources;
- accounts;
- scopes;
- provider cursors;
- objects;
- revisions;
- object states;
- FTS text;
- sync runs;
- index jobs;
- failure/retry state;
- forget/disconnect state.

SQLite settings:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;
```

Core tables:

```text
knowledge_sources
source_accounts
source_scopes
sync_cursors
knowledge_objects
knowledge_revisions
knowledge_parts
knowledge_chunks
knowledge_objects_fts
sync_runs
index_jobs
source_audit
schema_migrations
```

### 11.2 FTS Plane: SQLite FTS5

FTS is the first search path. It must work without model loading.

```sql
CREATE VIRTUAL TABLE knowledge_objects_fts USING fts5(
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
```

FTS row granularity should be chunk-level, not whole-object only, to return
focused snippets to Claude.

### 11.3 Vector Plane: LanceDB

Path:

```text
cfg.index_path() / "knowledge_chunks_v1"
```

Do not modify the existing `chunks` table.

`knowledge_chunks_v1` schema:

```text
chunk_id              FixedSizeBinary(16)
object_id             FixedSizeBinary(16)
revision_id           FixedSizeBinary(16)
part_id               FixedSizeBinary(16)
source_id             FixedSizeBinary(16)
account_id            FixedSizeBinary(16), nullable for local-only source
scope_id              FixedSizeBinary(16)
source_kind           Utf8
object_type           Utf8
title_pseudo          Utf8
text_hash             FixedSizeBinary(32)
acl_hash              FixedSizeBinary(32), nullable
source_updated_at     Timestamp(Microsecond)
indexed_at            Timestamp(Microsecond)
embedding_model       Utf8
embedding_fingerprint Utf8
vector                FixedSizeList(Float32, embed_dim)
```

Do not store long `text_pseudo` in LanceDB by default for the knowledge path.
SQLite already stores pseudonymized chunk text for FTS and snippet hydration.
LanceDB should stay a vector projection with enough metadata for filters and
joins.

Exception:

Short `title_pseudo` is acceptable in LanceDB because it improves result
inspection and is already pseudonymized.

---

## 12. Extraction Path

Kreuzberg becomes the shared document extraction layer.

Local documents:

```text
local file bytes
  -> kreuzberg extract
  -> KnowledgePart(FileBody)
```

Outlook messages:

```text
Graph message body HTML/text
  -> HTML cleanup / markdown normalization
  -> KnowledgePart(EmailBody)

Graph attachment bytes, when enabled
  -> kreuzberg extract
  -> KnowledgePart(AttachmentBody)
```

SharePoint/OneDrive files:

```text
driveItem bytes
  -> kreuzberg extract
  -> KnowledgePart(FileBody)
```

Budgets:

```text
max_file_bytes
max_extracted_chars
max_pages
max_ocr_seconds_per_run
max_archive_depth
max_archive_entries
allowed_mime_types
attachment_enabled
```

If a part exceeds budget:

- store metadata and status;
- do not index content;
- report in `knowledge_status`;
- allow user to opt in later.

---

## 13. Privacy Pipeline

The existing `Pipeline` remains the source of privacy primitives. New knowledge
code should not duplicate vault or model loading.

Add a narrow internal API, not a rewrite of `ingest_folder`:

```rust
struct PrivacyIndexInput {
    object: KnowledgeObject,
    parts: Vec<KnowledgePart>,
}

struct PseudonymizedChunk {
    chunk_id: ChunkId,
    object_id: ObjectId,
    revision_id: RevisionId,
    part_id: PartId,
    chunk_idx: u32,
    title_pseudo: String,
    text_pseudo: String,
    metadata_json_pseudo: String,
    token_refs: Vec<TokenRef>,
}

impl Pipeline {
    async fn pseudonymize_knowledge_object(
        &self,
        input: PrivacyIndexInput,
    ) -> Result<Vec<PseudonymizedChunk>>;

    async fn embed_pseudonymized_chunks(
        &self,
        chunks: &[PseudonymizedChunk],
    ) -> Result<Vec<Vec<f32>>>;
}
```

Why split pseudonymize and embed:

- Pseudonymization is mandatory before any persistent searchable text.
- Embedding is optional and can lag behind.
- FTS can be ready while vectors are pending.
- Models can be loaded only when the index worker needs them.

This also avoids calling the existing monolithic `ingest_one_counted`, which is
local-file and legal-enrichment specific.

---

## 14. Indexing Lifecycle

Sync and indexing must be decoupled.

```text
source sync worker:
  list/fetch changed objects
  extract raw text/parts within budget
  write object/revision metadata
  enqueue privacy job

privacy worker:
  claim object/revision
  run local NER + vault pseudonymization
  write SQLite chunks + FTS rows
  enqueue vector job

vector worker:
  claim pseudonymized chunks
  load embedder only when needed
  embed batch
  upsert LanceDB knowledge_chunks_v1
  mark vector_ready
```

Minimum viable implementation may combine source sync and privacy worker in one
process, but the state model should keep them distinct.

Job claim fields:

```text
job_id
job_type              -- sync, privacy, vector
source_id
object_id
revision_id
state                 -- pending, running, done, failed, skipped
priority
attempts
not_before
claimed_at
claim_token
last_error
```

Only one ML worker should run by default on a laptop. It may batch work:

```text
privacy batch: 8-32 objects or bounded by chars
vector batch: 32-128 chunks, tuned by memory and latency
```

---

## 15. Search Semantics

MCP search modes:

```rust
enum KnowledgeSearchMode {
    Auto,
    Fast,
    Semantic,
    Deep,
}
```

Mode behavior:

```text
Fast:
  SQLite FTS + metadata filters only.
  Never loads embedder.

Semantic:
  Pseudonymize query locally.
  Embed query locally.
  Search LanceDB vectors plus FTS.
  If vectors are missing, fall back to Fast with status warning.

Deep:
  Semantic search plus local rerank if available.
  Does not return cleartext to Claude.

Auto:
  Fast when models are absent or index lag is high.
  Semantic when vectors are ready and embedder is warm or acceptable to load.
```

Result shape:

```rust
struct KnowledgeHit {
    chunk_id: ChunkId,
    object_id: ObjectId,
    source_kind: SourceKind,
    object_type: ObjectType,
    title_pseudo: String,
    snippet_pseudo: String,
    source_label_pseudo: String,
    source_updated_at: DateTime<Utc>,
    score: f32,
    score_source: ScoreSource,
    vector_ready: bool,
    provenance: Provenance,
}
```

`knowledge_open(object_id)` returns pseudonymized title, metadata, and selected
chunk context. It must not fetch or return raw source content by default.

Rehydration remains explicit:

```text
rehydrate(text)
legal_rehydrate_citation(...)
future knowledge_rehydrate(...)
```

---

## 16. MCP Surface

Keep existing tools:

```text
search
rehydrate
detect
vault_stats
anno_health
download_models
memory_*
legal_*
review_*
```

Add generic knowledge tools:

```text
knowledge_sources
knowledge_add_local_folder
knowledge_configure_source
knowledge_sync
knowledge_status
knowledge_search
knowledge_open
knowledge_forget
knowledge_disconnect
```

Add Microsoft convenience tools:

```text
microsoft_connect
microsoft_accounts
microsoft_configure_sources
microsoft_sync
microsoft_status
microsoft_disconnect
```

Rules:

- Source/account/status tools must not call `AnnoRagServer::pipeline()`.
- `knowledge_search(mode=fast)` must not load embedder.
- `knowledge_sync` returns quickly after starting bounded work.
- Long-running background jobs report through `knowledge_status`.
- `anno_health.available_tools` must be updated when adding tools.
- Tool responses should be JSON strings consistent with current MCP style.

Update `AnnoRagServer` shape:

```rust
pub struct AnnoRagServer {
    pipeline: Arc<OnceCell<Arc<Pipeline>>>,
    knowledge: Arc<OnceCell<Arc<KnowledgeService>>>,
    cfg: Arc<AnnoRagConfig>,
    key: [u8; 32],
    tabular_storage: Arc<OnceCell<Arc<StorageHandle>>>,
    tool_router: ToolRouter<Self>,
}
```

`KnowledgeService` must be able to open SQLite and list status without opening
`Pipeline`.

---

## 17. Local Folder Migration

The current folder workflow remains the compatibility path.

Phase 1:

```text
legal_ingest/search -> unchanged
search              -> unchanged
knowledge_*         -> new, initially empty or local-folder opt-in
```

Phase 2 local folder source:

```text
knowledge_add_local_folder(path)
knowledge_sync(source_id)
knowledge_search(query)
```

Recommended first implementation:

- Implement local folder as a real `SourceConnector`.
- Reuse `kreuzberg` extraction.
- Write to `knowledge.sqlite3` and `knowledge_chunks_v1`.
- Do not dual-write from `Pipeline::ingest_folder` in the first code step.

Later compatibility enhancement:

```text
Pipeline::ingest_folder
  -> existing chunks/legal projections
  -> optional knowledge dual-write
```

This order validates the knowledge architecture without increasing the risk of
the existing legal ingest path.

---

## 18. Outlook Pilot

Outlook is the first external source.

Auth:

```text
Authorization code + PKCE
loopback callback
openid profile email offline_access User.Read Mail.Read
refresh token in OS keyring
access token in memory only
```

Default scope:

```text
Inbox and Sent
initial window: 90 days
attachments: metadata only
page budget per run
read-only permissions
```

Sync:

```text
list folders
configure selected folders
initial delta per folder
store committed deltaLink encrypted
follow nextLink until page budget
commit object metadata and cursor in one SQLite transaction
enqueue privacy/vector jobs
```

Message normalization:

```text
object_type: Email
title: subject
part: EmailBody
metadata:
  message_id
  internet_message_id hash/pseudo
  conversation_id hash
  folder_id
  sender pseudo
  recipients pseudo
  cc pseudo
  received_at
  sent_at
  has_attachments
```

Attachments:

- Store metadata in MVP.
- Fetch and extract only when user opts in.
- Use Kreuzberg under size/type/OCR/archive budgets.

---

## 19. Security

Secrets:

- No client secrets in binary.
- OAuth refresh tokens in OS keyring.
- Access tokens in memory only.
- Cursor values encrypted at rest.
- Vault key remains sourced as today: OS keyring or managed passphrase path.

Logging:

- Never log raw email body, file text, extracted text, access token, refresh
  token, delta link, or raw attachment bytes.
- Log counts, ids, source kind, state, error class, and durations.

Permissions:

- Microsoft MVP asks for read-only scopes.
- No `Mail.Send`, `Mail.ReadWrite`, or file write scopes.
- SharePoint broad scopes are delayed or enterprise-gated.

Forget/disconnect:

```text
knowledge_forget(source/account/object)
  -> tombstone or delete SQLite objects/chunks
  -> delete LanceDB vector rows
  -> clear jobs/cursors
  -> remove keyring token if account disconnect
  -> trigger orphan vault token cleanup where safe
  -> report compaction status
```

Claude exposure:

- `knowledge_search` and `knowledge_open` return pseudonymized data.
- Rehydration is a separate local operation.
- Native Claude source connectors are documented as outside Anno's privacy
  guarantee.

---

## 20. Performance Rules

General:

- No broad workspace/server model.
- Use bounded batches everywhere.
- Keep transactions short.
- Prefer append/upsert and background compaction.
- Avoid reading full bodies when list/search result columns suffice.
- Never allow unbounded MCP scans.

SQLite:

- Use indexes on source/account/scope/object/state/date.
- FTS is chunk-level.
- Use pagination for status and object lists.
- Keep raw text out of persistent tables.

LanceDB:

- Store vector projection, not source of truth.
- Upsert vectors in batches.
- Build vector indexes after bulk insert or at idle.
- Track vector lag in SQLite status.
- Keep `knowledge_chunks_v1` separate from `chunks`.

Models:

- NER and embedder load on demand.
- One ML worker by default.
- Batch embeddings.
- Rerank opt-in.
- FTS continues to work when models are unavailable.

Operational targets:

```text
MCP idle before model load: under 150 MiB RSS target
source status/connect: no model load
fast search warm FTS: tens of ms on typical corpus
semantic search warm: sub-second target for local corpus
first model-backed query: may pay cold-load cost
indexing: background, progress visible, resumable
```

---

## 21. Error Handling

Error categories:

```text
auth_required
auth_revoked
tenant_blocked
throttled
cursor_expired
unsupported_file_type
budget_exceeded
extraction_failed
pii_detection_failed
pseudonymization_failed
models_missing
embedding_failed
vector_store_failed
fts_store_failed
```

MCP tools should return user-actionable JSON:

```json
{
  "ok": false,
  "error": {
    "code": "models_missing",
    "message": "Models are not downloaded. Fast FTS search is available; semantic indexing is paused.",
    "next_action": "Run download_models or ask Anno to set up models."
  }
}
```

Provider sync errors should not block existing indexed search. Failed objects
remain visible in status with retry controls.

---

## 22. Test Strategy

Unit tests:

- Deterministic ID generation.
- Object/revision hashing.
- Source state transitions.
- SQLite migrations.
- FTS insert/delete/update.
- Cursor encryption/decryption.
- Redaction of logged errors.
- Budget decisions for documents and attachments.

Integration tests:

- Local folder source over fixtures.
- Kreuzberg extraction path into `KnowledgePart`.
- Pseudonymization writes FTS rows before vector rows.
- `knowledge_search(mode=fast)` works without model directories.
- Vector worker resumes after crash/stale claim.
- `knowledge_forget` removes SQLite rows and LanceDB rows.
- Mock Graph Outlook delta with `nextLink`, `deltaLink`, deleted messages,
  throttling, and cursor expiry.
- MCP `knowledge_status` works before `Pipeline` initialization.

E2E tests:

- Existing `search` and `legal_ingest` still work unchanged.
- Add local folder through `knowledge_add_local_folder`, sync, fast search,
  then semantic search when models exist.
- Outlook fake provider sync, restart MCP server, search returns pseudo hits.
- Disconnect account, search no longer returns those hits.

Performance tests:

- 10k local/document chunks.
- 10k synthetic email messages.
- 100k synthetic email messages.
- FTS-only p50/p95.
- Vector-ready p50/p95.
- Indexing throughput at batch sizes 32, 64, 128.
- RSS before and after model load.

---

## 23. Implementation Roadmap

### Phase 1 - Knowledge Core and Store

- Add `anno-knowledge-core` types.
- Add `anno-knowledge-store` or equivalent module with SQLite migrations.
- Add `KnowledgeControlStore`.
- Add chunk-level FTS table.
- Add status API.
- Add tests for IDs, migrations, FTS, and state transitions.

### Phase 2 - MCP Skeleton

- Add `KnowledgeService` lazy cell to `AnnoRagServer`.
- Add `knowledge_sources`, `knowledge_status`, and `knowledge_search`.
- `knowledge_search(mode=fast)` uses SQLite only.
- Update `anno_health.available_tools`.
- Ensure these tools work without model directories.

### Phase 3 - Local Folder Source

- Add `anno-source-local`.
- Implement `knowledge_add_local_folder`.
- Use Kreuzberg extraction into `KnowledgePart`.
- Run local PII pseudonymization.
- Write pseudonymized chunks to SQLite FTS.
- Optional vector jobs remain pending when models are missing.

### Phase 4 - Vector Projection

- Add `knowledge_chunks_v1` LanceDB table.
- Add vector worker and batch embedding.
- Add semantic search fusion with FTS.
- Track vector lag and index stats in `knowledge_status`.

### Phase 5 - Outlook Pilot

- Add `anno-source-microsoft`.
- Implement PKCE auth and keyring token storage.
- Add folder listing/configuration.
- Add Outlook delta sync for Inbox/Sent.
- Normalize email bodies to knowledge objects/parts.
- Add `microsoft_*` MCP tools.

### Phase 6 - Attachments and More Sources

- Add Outlook attachment extraction under budgets.
- Add OneDrive/SharePoint file source using Kreuzberg.
- Add Gmail/Google Drive or Slack only after Microsoft and local folder paths
  prove the source abstraction.

### Phase 7 - Optional Daemon

- Add `anno-rag daemon` only if duplicated MCP processes become a real issue.
- `anno-rag mcp` can then become a stdio bridge to the daemon.
- Keep in-process MCP path as a fallback.

---

## 24. Approval Gate

Implementation should proceed only if these decisions are accepted:

- Additive architecture; no migration of existing `Store/chunks`.
- Existing folder and legal workflows remain stable.
- New knowledge path uses SQLite FTS first and LanceDB vectors second.
- Raw source text is transient by default.
- All cleartext processing models are local.
- Claude receives pseudonymized snippets only.
- `knowledge_search(mode=fast)` works without model loading.
- Local folder source is implemented before or alongside Outlook to validate
  multi-source architecture.
- Outlook uses Anno's own Microsoft connector, not Claude's native connector.
- Rerank is local and opt-in.
- The first implementation runs in-process behind `anno-rag mcp`; daemon is a
  later optimization.

After this spec is approved, the next artifact should be a TDD implementation
plan starting with `anno-knowledge-core`, SQLite migrations, and MCP skeleton
tools that do not initialize `Pipeline`.
