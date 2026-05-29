# Threat Model

Status: Experimental in v0.11.0-rc.11
Audience: Admin, Compliance, Developer
Language: EN

This Phase 1 threat model is a concise operating summary. Phase 2 should expand
it into a fuller asset, actor, control, and evidence model.

## Assets

| Asset | Why It Matters |
|---|---|
| Vault mappings and secrets | Rehydrate pseudonym tokens and enable subject-rights workflows. |
| Source documents | Original evidence and clear PII source material. |
| Pseudonymized indexes | Searchable local state that may still reveal sensitive context. |
| LanceDB memory and tabular review state | Persistent product data, including memories, review schemas, cells, locks, corrections, and outputs. |
| MCP and gateway config | Controls binary paths, data directories, auth, model cache, and provider routing. |
| Audit logs | Accountability evidence that may include sensitive metadata. |

## Risks And Mitigations

| Risk | Mitigation |
|---|---|
| Clear PII sent to a remote provider | Use local pseudonymization and gateway filtering; test representative prompts before enabling upstream calls. |
| Vault secret exposure | Keep secrets out of repos, logs, and chat; prefer keyring or managed secret injection; rotate after exposure. |
| Wrong or stale binary | Pin versioned binary paths, verify checksums, and run `anno-rag --version` during upgrades. |
| Broken backup | Back up vault, secrets, configs, source documents, and LanceDB state when memory or tabular review is used; run recovery tests on a clean host. |
| Overreliance on AI output | Require qualified human review, source citation checks, and correction workflows before regulated use. |

## Current Boundaries

- Local detection, OCR, vault, and rehydration can process cleartext inside the
  trusted local boundary.
- Search/legal retrieval outputs and optional upstream provider prompts should
  be tokenized where applicable; trusted local memory recall and rehydration can
  return cleartext to authorized local clients.
- Gateway auth is a no-op when `ANNO_GATEWAY_BEARER_TOKEN` is unset; use that
  mode only behind loopback or a private trusted boundary.
- Streaming `input_json_delta` tool-use frames fail closed in
  `v0.11.0-rc.11`.

## Planned Phase 2 Work

- Detailed asset inventory and data-flow diagrams.
- Actor model for local users, admins, MCP clients, gateway clients, and remote
  providers.
- Control matrix mapped to RGPD, AI Act governance, and enterprise deployment
  patterns.
- Incident and evidence collection checklist.

## Related Links

- [Privacy Model](privacy-model.md)
- [Audit Logging](audit-logging.md)
- [Deployment](../admins/deployment.md)
