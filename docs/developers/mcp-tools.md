# MCP Tools

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, User, Admin, Compliance
Language: EN

`anno-rag mcp` exposes Hacienda capabilities over stdio MCP for Claude Desktop,
Cowork, and compatible local clients. Use the MCP client's installed tool schema
as the source of truth for exact tool availability and argument shapes.
`anno_health` is useful for version, build, vault, and core capability checks,
but it is not a complete inventory of every `review_*` method.

## Core Tools

| Tool | Purpose | Privacy behavior |
|---|---|---|
| `index` | Register a client folder as a corpus and run the requested profile. | Keeps corpus ids pseudonymous; corpus-scoped legal outputs are stored under Anno's data directory. |
| `sync_corpus` | Synchronize a selected corpus. Defaults to `knowledge_fast`; `legal_semantic` must be requested explicitly. | Refreshes only bound sources for the selected corpus and reports freshness instead of silently widening scope. |
| `search` | Search the indexed local corpus. | Pseudonymizes the query through the local vault and returns pseudonymized chunks plus freshness metadata. |
| `rehydrate` | Restore pseudonymized text for trusted local use. | Intentionally returns cleartext to the local MCP client after vault lookup. |
| `detect` | Dry-run PII detection on supplied text. | Processes cleartext locally and returns categories, confidence, and offsets without replacement. |
| `vault_stats` | Report vault mapping counts. | Returns aggregate counts, not original sensitive values. |
| `anno_init_vault` | Initialize vault state when an operator provides a managed secret. | Secret handling remains local; prefer OS keyring for normal local installs. |
| `anno_health` | Report version, build target, available tools, and vault state. | Side-effect-free; it does not validate or download model files. |
| `download_models` | Download local embedder and NER models in the background. | Writes model files locally and returns status/path metadata. |

## Privacy Vault Tools

| Tool | Purpose | Privacy behavior |
|---|---|---|
| `privacy_prepare_folder` | Create a local `vault` workspace with editable Word review documents, anonymized outputs, reports, and a manifest. | Returns generated paths, counts, and status metadata only. Cleartext stays in local working files. |
| `privacy_finalize_folder` | Read Word comments from a local `vault` workspace and regenerate anonymized documents after user edits. | Treats `à masquer` and `à garder` comments as local instructions; returns paths and aggregate counts only. |
| `privacy_status` | Report privacy workflow capabilities. | Does not load models and does not return document content. |

## Corpus Sync

### `sync_corpus`

Synchronizes a selected corpus. By default it refreshes the `knowledge_fast`
output only. Legal semantic refresh must be requested explicitly with
`outputs=["legal_semantic"]` or `outputs=["knowledge_fast","legal_semantic"]`.

Example payload:

```json
{
  "corpus_id": "00000000-0000-0000-0000-000000000000",
  "outputs": ["knowledge_fast"],
  "max_files": 25,
  "max_millis": 750
}
```

The response includes `freshness`, source counts, knowledge summary, legal
summary, and warnings. `freshness="fresh"` means the bounded sync completed
without failed files or truncation.

Search responses include corpus freshness metadata:

- `index_fresh=true` means the selected corpus was synced successfully.
- `index_fresh=false` means Anno answered from the existing index and the caller should consider `sync_corpus`.
- `sync.attempted=false` with `reason="models_not_loaded"` means Anno avoided a hidden model cold start.

## Memory Tools

| Tool | Purpose | Privacy behavior |
|---|---|---|
| `memory_save` | Save a long-term memory with kind/session metadata. | Default async mode stores raw text immediately, then enriches NER references later; sync mode stores tokenized text. |
| `memory_recall` | Recall memories by hybrid search. | Returns rehydrated plaintext to the trusted local caller. |
| `memory_graph_recall` | Traverse entity-linked memory context. | Uses local memory/entity references and returns local results. |
| `memory_invalidate` | Mark a memory invalid from a point in time. | Keeps auditability while excluding invalidated memories from normal recall. |
| `memory_forget` | Forget memory content for erasure workflows. | Removes or tombstones local memory data according to the store behavior. |
| `memory_list` | List memories with optional filters and pagination. | Returns local memory metadata/content visible to the trusted MCP client. |

## Legal Tools

Tools whose names begin with `legal_` cover legal document ingestion, filtered
legal search, graph and citation workflows, structured legal extraction,
mandatory-clause checks, prescription checks, risk review, and validation.

These tools reuse the local vault and RAG index. Search outputs are
pseudonymized; citation rehydration is a trusted local operation. Inspect the
tool schema exposed by the installed MCP client before automating exact
arguments.

For corpus-scoped indexing, generated legal anonymized files are stored under
Anno's data directory, not under the client source folder. Use the explicit
export workflow when generated anonymized files need to be copied to a
user-chosen destination.

## Review Tools

| Tool | Purpose | Response highlights |
|---|---|---|
| `review_create` | Create a tabular review and optionally materialize columns from a built-in template. | Returns `review_id`, review name, and `columns_loaded`. |
| `review_add_rows` | Add ingested document UUIDs as review rows. | Returns `rows_added`, `failed_doc_ids`, `extraction_started`, and `extraction_error`; starts extraction when rows were added. |
| `review_extract` | Start extraction for an existing review. | Returns row and column counts plus `extraction_started`; use `force_reextract=true` to rerun unlocked cells. |
| `review_get` | Read review state. | Returns columns, rows, latest cells, and `extraction_status` for polling background extraction. |
| `review_refine_cell` | Re-extract one cell with an extra instruction. | Writes a new cell version; locked cells are rejected until unlocked. |
| `review_set_cell` | Write a human override value to one cell. | Records a human-authored version and can lock it with `lock=true`. |
| `review_lock_cell` | Lock the latest cell value. | Prevents automatic extraction from overwriting the cell. |
| `review_unlock_cell` | Unlock a cell. | Allows future extraction or refinement to overwrite the cell. |
| `review_export` | Export the review as `csv`, `markdown`, or `xlsx`. | CSV/Markdown are returned in the tool response; XLSX requires an absolute `output_path`. |

Canonical MCP review workflow:

1. Create a review with `review_create`.
2. Add ingested document UUIDs with `review_add_rows`.
3. Call `review_extract` when `review_add_rows.extraction_started` is `false`, or when a rerun is needed.
4. Poll `review_get` and inspect `extraction_status.state` until it is `completed`, `completed_with_errors`, or `blocked`.
5. Correct cells with `review_refine_cell` for targeted re-extraction, or `review_set_cell` for a human override.
6. Lock verified cells with `review_lock_cell`; unlock them with `review_unlock_cell` before changing them again.
7. Export with `review_export`.

## Related Docs

- [Claude Desktop, Cowork, And Claude Code Setup](../getting-started/claude-desktop-cowork.md)
- [Memory](../user-guide/memory.md)
- [Tabular Review](../user-guide/tabular-review.md)
