# Operations

Status: Available in v0.11.0-rc.11
Audience: Admin
Language: EN

Operate Hacienda as a local privacy-sensitive system. Routine checks should
confirm the binary, MCP server, model cache, gateway, and documentation remain
consistent with the deployed release.

## Routine Checks

| Check | Command Or Tool | Expected Result |
|---|---|---|
| MCP health | `anno_health` through the MCP client | Version, build, vault, and core capability metadata. |
| Binary version | `anno-rag --version` | Installed binary matches the intended release. |
| Model availability | `anno-rag download-models` | Models are present or downloaded; the command prints the model path. |
| Gateway liveness | `GET /health` | HTTP success from the local or internal gateway. |
| Docs integrity | `python scripts/docs_audit.py` | Relative documentation links pass, except known in-progress Phase 1 pages. |

`anno_health` is side-effect-free and does not validate model files. Use
`download-models` or an actual ingest/search workflow for model readiness.

## Operational Rules

- Keep vault secrets out of logs, shell history, repository files, and MCP
  config files.
- Back up vault, source documents, configuration, and managed secrets before
  upgrades.
- Treat pure RAG chunks/vector indexes and model caches as rebuildable when
  source documents, vault, and secrets are intact.
- Back up LanceDB as persistent product state when memory or tabular review is
  used; those rows can include user-created memories, review schemas, cells,
  locks, corrections, and outputs that are not recreated by re-ingesting source
  documents.
- Review gateway and subject-rights audit logs where configured.
- Pin binary paths in MCP and service configs; do not rely on transient download
  folders.
- Use one data directory per user, tenant, matter, or test environment.

## Incident Triggers

Start an incident review when:

- a vault secret is exposed;
- a gateway is reachable without the intended network or bearer boundary;
- a remote provider receives clear PII unexpectedly;
- a backup cannot be restored;
- a user relies on AI output without human review in a regulated workflow.

## Related Links

- [Backups And Recovery](backups-and-recovery.md)
- [Audit Logging](../security-compliance/audit-logging.md)
- [File Layout](../reference/file-layout.md)
