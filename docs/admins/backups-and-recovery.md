# Backups And Recovery

Status: Available in v0.11.0-rc.11
Audience: Admin, Compliance
Language: EN

Backups must preserve the data needed to reconstruct trusted local state.
Pure RAG chunks and vector indexes can be rebuilt when the vault, vault secret,
source documents, and configuration are intact. The LanceDB store can also hold
non-rebuildable product state such as memories, review schemas, cells, locks,
corrections, and exports; treat that state as persistent data, not cache.

## Backup Priority

| Item | Priority | Why It Matters |
|---|---|---|
| Vault file | Critical | Contains token mappings required for rehydration and subject-rights workflows. |
| Vault secret or keyring material | Critical | Required to unlock the vault; protect separately from the vault file. |
| Source documents | Critical | Required to rebuild indexes and verify evidence. |
| MCP and gateway config | High | Captures binary paths, data directories, model paths, and gateway settings. |
| LanceDB RAG chunks/vector indexes | Medium | Speeds recovery when the deployment uses only rebuildable corpus indexes. |
| LanceDB memory/tabular review state | High | Stores user/admin-created state such as memories, review schemas, cells, locks, and corrections. |
| Model cache | Low | Large and rebuildable with `anno-rag download-models`. |

Do not store clear vault passphrases in plain-text backup manifests. Use an
approved password manager, keyring export process, or secret manager.

## Backup Policy

- Back up before every release upgrade or binary path migration.
- Keep vault backups and vault secrets under separate access controls.
- Include source documents or the controlled document system export used for
  indexing.
- Record `ANNO_RAG_DATA_DIR`, `ANNO_MODELS_DIR`, gateway env vars, and MCP
  command paths.
- Store audit logs according to the organization's retention policy.

## Recovery Test

1. Restore the MCP or gateway configuration to a clean test host.
2. Restore the vault file and the corresponding vault secret or keyring entry.
3. Restore source documents, or confirm the controlled source repository is
   reachable.
4. Restore LanceDB when memory or tabular review state is used. If the
   deployment only uses rebuildable RAG indexes, rebuild by ingesting source
   documents again when no LanceDB backup is available.
5. Run `anno_health` through the target MCP client.
6. Run a sample search against a restored corpus.
7. Rehydrate one known token locally and confirm the result is expected.
8. Run a tokenized remote or gateway workflow and confirm no remote provider
   receives clear PII.

## Recovery Limits

If the vault secret is lost, existing tokens cannot be reliably rehydrated from
the encrypted vault. If the source documents are lost, indexes may still contain
pseudonymized text, but they are not a substitute for an evidence-grade source
record.

If LanceDB state is lost, RAG chunks may be rebuilt, but memory rows, review
schemas, cell locks, corrections, and review outputs may be lost unless they
also exist in another system of record.

## Related Links

- [Operations](operations.md)
- [Privacy Model](../security-compliance/privacy-model.md)
- [Configuration](../developers/configuration.md)
