# File Layout

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin, Compliance
Language: EN

Exact paths vary by operating system, archive extraction location, service
manager, and environment variables. Treat this page as a map of data
categories, not a fixed filesystem contract.

## Path Categories

| Category | Typical Location | Contents | Backup Priority |
|---|---|---|---|
| Release binaries | Extracted release archive or managed install directory | `anno-rag`, `anno-privacy-gateway`, checksums, and supporting files. | Medium; reinstall from the release archive when checksums are available. |
| Vault | `ANNO_RAG_DATA_DIR/vault.enc` or the default user data directory | Encrypted RAG token mappings used for rehydration and subject workflows. | Critical. |
| Vault secret material | OS keyring, secret manager, or `ANNO_RAG_VAULT_PASSPHRASE` source | Key material required to unlock `vault.enc`. | Critical; store separately from the vault file. |
| LanceDB RAG indexes | `ANNO_RAG_DATA_DIR/index.lance` | Corpus chunks, vectors, and search indexes. | Medium when only rebuildable RAG indexes are used. |
| LanceDB memory/tabular state | `ANNO_RAG_DATA_DIR/index.lance` | Memory rows, review schemas, cells, locks, corrections, and review state. | High or critical when memory or tabular review is used; this is persistent product state. |
| Model cache | `ANNO_MODELS_DIR` or `ANNO_RAG_DATA_DIR/models` | Local embedder and NER model files. | Low; large and rebuildable with `anno-rag download-models`. |
| Pseudonymized outputs | `ANNO_RAG_DATA_DIR/outputs` or CLI `--output` path | Pseudonymized markdown/text outputs from ingest workflows. | Project-dependent; keep with the evidence record when used for review. |
| Source documents | User-selected corpus folders or document management export | Original documents, contracts, filings, and evidence. | Critical; required to rebuild RAG indexes and verify citations. |
| MCP config | Claude Desktop/Cowork/client config directory | Absolute command path, `args: ["mcp"]`, and selected environment variables. | High; needed to recreate integrations. |
| Audit logs | `ANNO_GATEWAY_AUDIT_DIR` | Gateway JSONL audit files and signature files. | Compliance-dependent; follow the retention policy. |

## Backup Boundary

RAG chunks and vector indexes can be rebuilt from source documents when the
vault, vault secret, and configuration are intact. Memory and tabular review
state stored in LanceDB is different: it may contain user-created rows,
schemas, cells, locks, corrections, and review outputs that cannot be rebuilt
from source documents alone.

Back up `index.lance` whenever memory or tabular review is enabled. If a
deployment uses only rebuildable corpus search, losing `index.lance` is usually
a performance and reindexing incident rather than a permanent data loss event.

## Related Links

- [Backups And Recovery](../admins/backups-and-recovery.md)
- [Model Cache](model-cache.md)
