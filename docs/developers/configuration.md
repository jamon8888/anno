# Configuration

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin
Language: EN

Hacienda configuration is split by runtime boundary: MCP server launch, local
RAG storage, vault secrets, model cache, and HTTP gateway settings. Prefer
explicit environment variables and absolute paths in integrations.

Memory configuration is part of the privacy boundary. `ANNO_RAG_MEMORY_NER_MODE`
controls whether memory enrichment runs in the default asynchronous path or a
synchronous tokenizing path where available. Validate the configured mode before
storing sensitive memory content.

## MCP Server

Claude Desktop, Cowork, and other MCP clients should launch the installed
`anno-rag` binary directly:

```json
{
  "mcpServers": {
    "anno-rag": {
      "command": "C:\\Users\\you\\Tools\\hacienda\\anno-rag.exe",
      "args": ["mcp"],
      "env": {
        "ANNO_MODELS_DIR": "C:\\Users\\you\\.anno-rag\\models"
      }
    }
  }
}
```

Use an absolute `command` path. Keep `args` as `["mcp"]`. Add only the
environment variables required by the deployment.

## RAG Storage

`anno-rag` stores local state under `ANNO_RAG_DATA_DIR` when set, otherwise
under the default user data directory:

| Path | Contents |
|---|---|
| `vault.enc` | Encrypted token mapping vault. |
| `index.lance` | LanceDB corpus, memory, legal, and tabular review data. |
| `models` | Default model cache when `ANNO_MODELS_DIR` is not set. |
| `outputs` | Pseudonymized copies produced by ingest workflows. |

Use one data directory per user, tenant, or test environment. Do not share a
mutable data directory across unrelated trust boundaries.

## Vault

Prefer the OS keyring for normal local installs. This is the default path used
by the CLI and MCP server.

Use `ANNO_RAG_VAULT_PASSPHRASE` only for managed deployments where an admin
controls the secret through an approved environment or secrets system. Do not
commit it, log it, paste it into chat, or store it in plain text config files.

Gateway vault settings are separate from `anno-rag`:

| Variable | Purpose |
|---|---|
| `ANNO_GATEWAY_VAULT_PATH` | Persistent gateway vault file. |
| `ANNO_GATEWAY_VAULT_KEY_HEX` | 32-byte gateway vault key encoded as hex. |

Set gateway vault path and key together when using persistent gateway state.

## Model Cache

Install local RAG models with:

```bash
anno-rag download-models
```

The command prints the model path. Set `ANNO_MODELS_DIR` to that path when the
models are outside the default data directory or when launching from an MCP
client that needs explicit environment configuration.

## Gateway

The gateway reads environment variables at startup. Common settings:

| Variable | Purpose |
|---|---|
| `ANNO_GATEWAY_LISTEN` | Listen address, default `127.0.0.1:3000`. |
| `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE` | Anthropic-compatible upstream base URL. |
| `ANNO_GATEWAY_PROVIDER_PROFILE` | Audit/routing profile label. |
| `ANNO_GATEWAY_BEARER_TOKEN` | Bearer token for protected `/v1/*` routes. |
| `ANNO_GATEWAY_STREAMING` | Enables streaming when set to `enabled`, `true`, or `1`. |
| `ANNO_GATEWAY_STREAM_PRIVACY` | Streaming privacy mode. |
| `ANNO_GATEWAY_AUDIT_DIR` | Persistent audit register directory. |
| `ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX` | HMAC key for audit signature files. |

Set `ANNO_GATEWAY_BEARER_TOKEN` whenever the gateway is reachable outside a
strictly controlled loopback or private boundary.

## Agent Harness

The repo-local agent harness configures Claude Code and Codex for Anno
development. It provides safety hooks, targeted Rust checks, GitNexus-first
exploration, changelog generation, PR review, docs generation, crate dependency
mapping, CLI feature parity checks, and compact agent context generation.

Dry-run setup:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
```

Status:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1
```

Run fixture tests:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

## Related Docs

- [Environment Variables](../reference/environment-variables.md)
- [File Layout](../reference/file-layout.md)
- [Model Cache](../reference/model-cache.md)
