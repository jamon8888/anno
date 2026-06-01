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
| `search` | Search the indexed local corpus. | Pseudonymizes the query through the local vault and returns pseudonymized chunks. |
| `rehydrate` | Restore pseudonymized text for trusted local use. | Intentionally returns cleartext to the local MCP client after vault lookup. |
| `detect` | Dry-run PII detection on supplied text. | Processes cleartext locally and returns categories, confidence, and offsets without replacement. |
| `vault_stats` | Report vault mapping counts. | Returns aggregate counts, not original sensitive values. |
| `anno_init_vault` | Initialize vault state when an operator provides a managed secret. | Secret handling remains local; prefer OS keyring for normal local installs. |
| `anno_health` | Report version, build target, available tools, and vault state. | Side-effect-free; it does not validate or download model files. |
| `download_models` | Download local embedder and NER models in the background. | Writes model files locally and returns status/path metadata. |

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

## Review Tools

Tools whose names begin with `review_` cover common tabular review workflows:
creating a review, adding ingested document IDs, starting extraction through row
addition where supported, reading review state, refining or overriding cells,
locking cells, and exporting results.

The MCP review surface is not a one-to-one mirror of every CLI subcommand. Use
the installed MCP tool schema as the source of truth for available review tools
and arguments.

## Related Docs

- [Claude Desktop And Cowork Setup](../getting-started/claude-desktop-cowork.md)
- [Memory](../user-guide/memory.md)
- [Tabular Review](../user-guide/tabular-review.md)
