# Product Architecture

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin, Compliance
Language: EN

Hacienda separates trusted local cleartext processing from tokenized indexing,
retrieval, and optional remote model reasoning. Cleartext PII may be processed
by local extraction, OCR, detection, and explicit rehydration flows; search
indexes and upstream LLM providers should receive tokenized content.

## High-Level Flow

```text
Documents
  -> text extraction and optional OCR
  -> PII detection
  -> local vault pseudonymization
  -> embeddings and LanceDB index
  -> MCP or CLI search
  -> optional remote LLM reasoning over tokenized content
  -> local rehydration from the vault
```

## Components

| Component | Responsibility | Local data handled |
|---|---|---|
| `anno` | Core extraction, NER, PII detection, and text preparation. | Input text, detected spans, labels, confidence scores. |
| `anno-cli` | CLI wrapper around extraction and automation tasks. | Local files and command output. |
| `anno-rag` | Ingestion, pseudonymization, embeddings, LanceDB index, vault, memory, MCP tools. | Documents, pseudonymized chunks, embeddings, vault mappings, memory records. |
| `anno-privacy-gateway` | Anthropic-compatible privacy boundary for HTTP clients. | Requests, tokenized upstream payloads, audit records. |
| `anno-rag-tabular` | Schema-driven legal review extraction and verification. | Review schemas, rows, cells, citations, verification status. |
| Claude Desktop / Cowork | MCP client that invokes local tools. | Tool calls and returned tokenized or rehydrated content. |
| Remote LLM provider | Optional reasoning endpoint. | Tokenized prompts only, when enabled by the operator. |

## Trust Boundaries

| Boundary | Cleartext PII allowed? | Notes |
|---|---:|---|
| Local document ingestion | Yes | Cleartext exists before detection and tokenization. |
| Local vault | Yes | Stores reversible mappings; protect with OS keyring, passphrase policy, and disk encryption. |
| Local LanceDB index | No by design | Indexes pseudonymized chunks and vectors. |
| MCP client boundary | Depends on tool | Search returns tokenized evidence; rehydration intentionally returns cleartext locally. |
| Privacy Gateway upstream boundary | No | Outbound provider calls should receive tokenized content. |
| Audit logs | No by design | Logs should record operational events without leaking cleartext secrets or PII. |

## Operational Notes

- Keep the vault and LanceDB directory on encrypted storage for enterprise use.
- Treat rehydration as a privileged local operation.
- Do not configure a remote provider unless the workflow accepts tokenized
  remote reasoning.
- Validate `anno_health` and gateway health checks after upgrades.

## Related Docs

- [Privacy Model](../security-compliance/privacy-model.md)
- [MCP Tools](../developers/mcp-tools.md)
- [Gateway API](../developers/gateway-api.md)
- [File Layout](../reference/file-layout.md)
