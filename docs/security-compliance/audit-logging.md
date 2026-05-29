# Audit Logging

Status: Available in v0.11.0-rc.11
Audience: Admin, Compliance, Developer
Language: EN

Audit logging supports review of privacy-sensitive workflows. In Phase 1,
coverage depends on the runtime path and configuration; treat the logs as
operational evidence, not as a complete compliance system by themselves.

## What To Track

| Area | Events To Review |
|---|---|
| Gateway persistent JSONL audit | Built-in audit events emitted by the configured gateway audit writer. |
| HTTP/tracing/operator logs | Protected route access, tokenized upstream calls, streaming errors, auth boundary changes, and request lifecycle events when collected by the deployment. |
| Subject rights | Subject find, forget, and export activity. |
| Memory | Save, invalidate, recall, and forget workflows where audit data is emitted or logged. |
| Vault and rehydration | Rehydration-sensitive operations where supported, especially subject export or local cleartext restoration. |
| Legal workflows | Ingest, search, citation rehydration, validation, extraction, and review audit events where emitted. |

Gateway persistent audit output is controlled by `ANNO_GATEWAY_AUDIT_DIR` and
related audit settings, but it is narrower than full HTTP access logging. Use
external HTTP logs, tracing, reverse-proxy logs, or platform telemetry when the
deployment must review route access, auth failures, streaming errors, or every
upstream request. Legal RAG helpers also emit structured tracing events for
several sensitive workflows.

## Operational Review

- Do not log clear secrets, vault passphrases, bearer tokens, or unnecessary
  clear PII.
- Restrict audit log access because logs can reveal workflow timing, subject
  references, and operational metadata.
- Review subject export and rehydration activity as trusted cleartext paths.
- Preserve logs during incident review and release rollback windows.
- Test log collection after changing gateway or service configuration.

## Limits

- Not every CLI or MCP operation has identical persistent audit coverage in
  `v0.11.0-rc.11`.
- Built-in gateway JSONL audit is not a complete access log for every protected
  route, streaming event, or auth boundary change.
- Audit logs can support accountability, but they do not replace access control,
  backup protection, or human review.

## Related Links

- [Privacy Model](privacy-model.md)
- [RGPD](rgpd.md)
- [Operations](../admins/operations.md)
