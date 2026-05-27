# anno Outlook + Lance MVP Design

**Date:** 2026-05-26
**Status:** Draft
**Scope:** Narrow MVP for Outlook-only local knowledge indexing in Claude
Desktop. This replaces the broader Microsoft 365 suite scope for the first
implementation plan.

---

## 1. Decision

Use a **Lance-first retrieval plane** for Outlook content, backed by a small
SQLite sync ledger.

```text
Microsoft Graph Outlook
  -> SQLite sync ledger              stable cursors, runs, job state
  -> anno privacy pipeline           detect, pseudonymize, embed
  -> LanceDB outlook_chunks_v1        FTS + vector + scalar filters
  -> MCP knowledge_search/outlook_*   Claude Desktop tools
```

LanceDB should own search speed. SQLite should own transactional sync
correctness. Do not build a generic Microsoft suite connector in the MVP.

---

## 2. Why Lance Helps Here

The current broader design used SQLite FTS5 plus LanceDB vectors. For Outlook
only, that can be simplified.

Lance/LanceDB can store pseudonymized text, vectors, metadata, and indexes in
one table. It supports:

- BM25 full-text search on string columns.
- Vector indexes for semantic search.
- Hybrid search with FTS + vector reranking.
- Scalar indexes for metadata filters such as folder, received date, account,
  read status, and attachment flag.
- Columnar `select` so search can return summaries without reading full body
  columns.
- Table versioning, schema evolution, compaction, and rebuildable indexes.

This removes one merge layer from the hot search path:

```text
Old proposed path:
  SQLite FTS candidates + LanceDB vector candidates -> custom fusion -> hydrate

Outlook MVP path:
  LanceDB hybrid search + scalar filters -> hydrate selected columns
```

That is likely faster to build and good enough for large local Outlook archives,
as long as ingestion batches and `optimize()` are controlled.

---

## 3. Why Not Lance-Only

Do not store sync state only in Lance.

Outlook sync needs small, frequent, transactional state changes:

- OAuth account records and token references.
- Folder configuration.
- Graph delta cursors.
- Sync run status.
- Crash recovery checkpoints.
- Index job claims and retries.

Lance table versions and compaction are optimized for analytical/search data,
not for queue-style state with frequent tiny updates. Lance also keeps old
versions until cleanup, and deletes are not immediate. That is useful for
lineage, but it is a privacy and operational concern for source cursors,
disconnect, and erasure.

SQLite remains the local source of truth for sync state. LanceDB is the
searchable projection.

---

## 4. Outlook-Only Data Model

### 4.1 SQLite Ledger

Create `outlook.sqlite3` under the existing Anno data directory.

Tables:

```text
outlook_accounts
outlook_folders
outlook_delta_cursors
outlook_messages
outlook_sync_runs
outlook_index_jobs
outlook_source_audit
```

Use WAL mode, `busy_timeout`, and `BEGIN IMMEDIATE` for cursor commits.

Cursor commit rule:

```text
BEGIN IMMEDIATE
  upsert message metadata
  insert/update message revision hash
  enqueue index jobs
  mark deleted messages tombstoned
  promote pending deltaLink to committed deltaLink
COMMIT
```

Never advance a Graph delta cursor unless the corresponding message metadata
and index jobs are durably recorded.

### 4.2 LanceDB Table

Add one dedicated table:

```text
outlook_chunks_v1
```

One row per pseudonymized chunk:

```text
chunk_id              FixedSizeBinary(16)
message_id            FixedSizeBinary(16)
revision_id           FixedSizeBinary(16)
account_id            FixedSizeBinary(16)
folder_id             Utf8
conversation_id_hash  FixedSizeBinary(32)
internet_id_hash      FixedSizeBinary(32), nullable
chunk_idx             Int32
subject_pseudo        Utf8
text_pseudo           Utf8
participants_pseudo   Utf8
received_at           Timestamp(Microsecond)
sent_at               Timestamp(Microsecond), nullable
has_attachments       Boolean
is_read               Boolean
importance            Utf8, nullable
metadata_json_pseudo  Utf8
vector                FixedSizeList<Float32>(embed_dim)
indexed_at            Timestamp(Microsecond)
```

Indexes:

```text
FTS:    subject_pseudo, text_pseudo, participants_pseudo
Vector: vector using dot distance on normalized 384-dim e5 embeddings
BTree:  received_at, message_id, revision_id, internet_id_hash
Bitmap: account_id, folder_id, has_attachments, is_read, importance
```

Vector index default should be benchmarked instead of hard-coded. Current
`anno-rag` uses `IVF_HNSW_SQ` for the generic chunk table, but Outlook searches
will commonly carry folder/date/account filters. LanceDB's vector-index
guidance says filtered workloads may see higher latency variance with
HNSW-backed IVF indexes. For Outlook, benchmark:

```text
candidate 1: IVF_PQ      likely first default for filtered Outlook search
candidate 2: IVF_RQ      good compression candidate; 384 is divisible by 8
candidate 3: IVF_HNSW_SQ baseline from existing Anno chunk search
```

The existing `chunks` table remains untouched.

---

## 5. Sync Flow

### 5.1 Connect

`outlook_connect` uses Microsoft authorization code + PKCE:

```text
openid profile email offline_access User.Read Mail.Read
```

Refresh tokens live in OS keyring. SQLite stores only token references and
pseudonymized account labels.

### 5.2 Configure

Default MVP scope:

```text
Folders: Inbox and Sent
Initial window: 90 days
Attachments: metadata only
Body: message body only, no raw MIME cache
Page budget: bounded per run
```

The user can later opt into Archive, older history, and attachments.

### 5.3 Initial Sync

Fetch Outlook pages from Graph, normalize each message, and record metadata in
SQLite. Indexing is a separate queue:

```text
sync worker:
  Graph page -> SQLite ledger -> index jobs

index worker:
  claim jobs -> pseudonymize -> embed -> LanceDB upsert
```

During initial sync, batch LanceDB writes. Build FTS/vector/scalar indexes after
the first bulk load instead of rebuilding after every page.

### 5.4 Incremental Sync

Each folder has a delta cursor. Incremental sync:

```text
load committed deltaLink
fetch changed/deleted messages
write changes and jobs transactionally
commit new deltaLink
index pending jobs in batches
```

If a message fails during embedding, keep the cursor committed because the
message metadata and job exist locally. If a message fails before SQLite commit,
do not advance the cursor.

---

## 6. Search Flow

`outlook_search` should use LanceDB directly:

```rust
OutlookSearchParams {
    query: String,
    top_k: usize,
    folder_ids: Option<Vec<String>>,
    account_ids: Option<Vec<AccountId>>,
    received_after: Option<DateTime<Utc>>,
    received_before: Option<DateTime<Utc>>,
    has_attachments: Option<bool>,
    mode: SearchMode, // auto, fts, vector, hybrid
}
```

Default mode:

```text
models unavailable -> FTS only
models ready       -> hybrid FTS + vector
filter-only query  -> scalar/filter query, no embedding
```

Filtering policy:

```text
folder/account/date filters -> prefilter by default
highly selective filters    -> consider exact/bypass-vector path or higher nprobes
postfilter                  -> only when semantic similarity is more important
                               than guaranteeing `limit` matching rows
```

Return selected columns only:

```text
message_id, revision_id, subject_pseudo, snippet_pseudo,
participants_pseudo, received_at, folder_id, score
```

Use `outlook_open(message_id)` for pseudonymized details. Cleartext
rehydration remains a separate explicit local action.

---

## 7. Performance Rules

These rules are verified against the LanceDB docs for FTS, scalar indexes,
vector indexes, filtering, reindexing, and table updates.

### 7.1 Index Coverage and Optimize

Rows added after FTS/scalar/vector index creation remain searchable, but LanceDB
uses flat/exhaustive search over the unindexed tail. That preserves recall but
increases latency as the tail grows.

Therefore:

- Batch Graph fetches and LanceDB upserts.
- Avoid embedding during the sync transaction.
- Build FTS/scalar/vector indexes after the initial bulk ingest, not after each
  page.
- Run `table.optimize()` periodically because it updates indexes, compacts
  fragments, and prunes old files according to the retention window.
- Track `index_stats(...).num_unindexed_rows` per index and expose it through
  `outlook_status`.
- Treat `num_unindexed_rows > 0` as a latency warning, not a correctness failure.
- Use `fast_search` only for commands where stale results are acceptable,
  because it intentionally ignores unindexed rows.
- Use `wait_for_index(...)` only in tests/admin flows; continuous Outlook writes
  can keep the fully indexed state from stabilizing before timeout.

Initial cadence:

```text
After initial sync: optimize once after bulk load completes
Steady state:      optimize after ~20 modification batches or ~100k row changes
Status warning:    report unindexed rows and last optimize time
```

### 7.2 Scalar Filters

LanceDB docs recommend scalar indexes for filtered search. Use:

```text
BTree:  received_at, message_id, revision_id, internet_id_hash
Bitmap: account_id, folder_id, has_attachments, is_read, importance
```

Query rules:

- Always apply account/folder/date filters via `.where(...)`.
- Keep filter expressions simple and use exact column names.
- Use prefilter by default because Outlook filters are part of the result
  contract.
- Always set `limit`; never allow unbounded scans from MCP tools.

### 7.3 Vector Index and Distance

Anno's embedder produces L2-normalized 384-dimensional multilingual-e5-small
vectors. LanceDB recommends `dot` for normalized vectors, so Outlook vector
indexes and vector searches should use `dot`, not the default `l2`.

Do not tune `nprobes` first. LanceDB auto-tunes it by default. Only raise
`nprobes`, `ef`, or `refine_factor` after a recall benchmark shows a real loss.
Use `refine_factor` for quality-sensitive queries where quantized ANN distance
needs full-vector reranking.

### 7.4 Hybrid Search

Use LanceDB hybrid search rather than a custom SQLite/Lance fusion path:

```text
query_type: hybrid
vector: normalized e5 query embedding
text: pseudonymized query text
reranker: RRFReranker
where: account/folder/date filters with prefilter=true
limit: explicit top_k
select: summary columns only
```

LanceDB applies a hybrid query's `where(...)` filter to both the vector and FTS
halves. Use `explain_plan`/`analyze_plan` in performance tests to verify the
filter is pushed into the query instead of becoming a late `FilterExec`.

### 7.5 Column and Storage Discipline

- Use `select` to avoid reading `text_pseudo` for result lists.
- Compact LanceDB after heavy import/delete phases.
- Keep deleted/disconnected source cleanup explicit because Lance versions can
  retain old data until cleanup.
- After `outlook_disconnect` or source-level forget, run delete plus optimize
  with an aggressive retention window only if the user explicitly requests
  immediate local disk cleanup.
- Expect temporary disk growth during compaction because new compacted files are
  written before old versions are pruned.

### 7.6 Benchmarks Before Locking Defaults

Create synthetic Outlook corpora at:

```text
10k messages
100k messages
500k messages
```

Benchmark:

```text
FTS-only subject/body queries
hybrid query with no filter
hybrid query with folder filter
hybrid query with date + folder filter
filter-only query
delete/disconnect cleanup
initial bulk optimize time
steady-state optimize time
```

Index worker batch starts at 32-128 messages depending on embedder latency.
Graph page budgets should stay small enough that sync can report progress and
resume from SQLite checkpoints.

---

## 8. MCP Surface

Outlook-only tools:

```text
outlook_connect
outlook_accounts
outlook_configure
outlook_sync
outlook_status
outlook_search
outlook_open
outlook_disconnect
```

Do not add `knowledge_sources`, SharePoint, OneDrive, Calendar, or Contacts in
the MVP. Keep the tool list small and predictable for Claude Desktop.

`outlook_connect`, `outlook_accounts`, `outlook_configure`,
`outlook_sync`, and `outlook_status` must not initialize the model-backed
`Pipeline`. Users must be able to connect Outlook before models are downloaded.

---

## 9. Implementation Phases

### Phase 1 - Outlook Ledger

- Add Outlook account/folder/cursor/sync-run SQLite store.
- Add deterministic Outlook ids.
- Add fake Graph test provider.
- Add `outlook_status` from SQLite only.

### Phase 2 - Lance Outlook Table

- Add `outlook_chunks_v1`.
- Add FTS/scalar/vector index builders.
- Use dot distance for normalized e5 vectors.
- Benchmark `IVF_PQ`, `IVF_RQ`, and `IVF_HNSW_SQ` under Outlook filters before
  choosing the default.
- Add `outlook_search` in FTS-only mode.
- Add optimize/index coverage status using `num_unindexed_rows`.

### Phase 3 - Privacy Pipeline Entry

- Add `Pipeline::ingest_outlook_message`.
- Pseudonymize subject/body/participants.
- Embed chunks when models are available.
- Upsert batches into `outlook_chunks_v1`.

### Phase 4 - Graph Outlook MVP

- Implement PKCE auth and `Mail.Read`.
- Initial folder sync for Inbox/Sent over 90 days.
- Incremental delta sync per folder.
- Retry and throttling with bounded page budgets.

### Phase 5 - Product Hardening

- Disconnect/forget with Lance cleanup.
- Attachment metadata.
- Archive folder opt-in.
- Larger history windows.
- Benchmarks on synthetic 10k/100k/500k message mailboxes.

---

## 10. Open Questions

1. Should the default initial window be 30, 90, or 180 days?
2. Should Sent be enabled by default, or only Inbox?
3. Should `outlook_search` be separate from future `knowledge_search`, or become
   the first backend behind `knowledge_search`?
4. Should LanceDB `fast_search` be allowed for user-facing results when the
   unindexed tail is non-zero?
5. Should raw message bodies ever be cached encrypted, or always re-fetched from
   Graph for cleartext rehydration?

---

## 11. Approval Gate

Proceed to implementation planning only if these decisions hold:

- Outlook-only MVP.
- LanceDB owns the search projection.
- SQLite owns sync correctness and queue state.
- No OneDrive, SharePoint, Calendar, Contacts, Notion, or Gmail in this plan.
- No raw email cache by default.
- Existing local/legal ingest tables remain untouched.

---

## 12. LanceDB Documentation Checked

- [Lance format](https://docs.lancedb.com/lance)
- [Full-text search](https://docs.lancedb.com/search/full-text-search)
- [Scalar indexes](https://docs.lancedb.com/indexing/scalar-index)
- [Vector search](https://docs.lancedb.com/search/vector-search)
- [Vector indexes](https://docs.lancedb.com/indexing/vector-index)
- [Hybrid search](https://docs.lancedb.com/search/hybrid-search)
- [Metadata filtering](https://docs.lancedb.com/search/filtering)
- [Reindexing and optimize](https://docs.lancedb.com/indexing/reindexing)
- [Updating, merge insert, and delete](https://docs.lancedb.com/tables/update)
