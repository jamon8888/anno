# Changelog

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin, Compliance
Language: EN

The current documented candidate is `v0.11.0-rc.11`.

Release page: [v0.11.0-rc.11](https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11)

## v0.11.0-rc.11 Documentation Scope

This documentation set covers the product areas available or explicitly
documented for the release candidate:

| Area | Notes |
|---|---|
| Release binaries | GitHub release archives for installing `anno-rag` and `anno-privacy-gateway`; checksums should be verified before use. |
| Claude Desktop/Cowork MCP | `anno-rag mcp` is the local stdio MCP server path for desktop agents. |
| Local RAG and vault | Local ingest/search, pseudonymized outputs, encrypted token vault, and rehydration through trusted local operations. |
| Memory | MCP memory workflows, including the default async enrichment privacy caveat and `ANNO_RAG_MEMORY_NER_MODE`. |
| Privacy gateway | Anthropic-compatible HTTP boundary with tokenized upstream calls, bearer-token support when configured, streaming opt-in, and audit settings. |
| Streaming/tool-use limitation | Streaming tool-use `input_json_delta` frames fail closed in `v0.11.0-rc.11`; agentic clients should validate streaming workflows before production use. |
| Legal tabular review | Review creation, document-row attachment, extraction, locking/refinement workflows, and reliable CLI exports for `xlsx` and `md`/`markdown`. |

## Release Operations

Use the release page and local `--version` output together when diagnosing
installations. If a machine runs an older binary, follow the upgrade and
rollback procedure before changing user data directories or MCP config.

## Related Links

- [GitHub Release](https://github.com/jamon8888/anno/releases/tag/v0.11.0-rc.11)
- [Installation](../getting-started/installation.md)
- [Release Management](../admins/release-management.md)
