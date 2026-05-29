# Deployment

Status: Available in v0.11.0-rc.11
Audience: Admin, Integrator
Language: EN

Hacienda deployment in Phase 1 is designed for local desktops and controlled
internal environments. Treat the binary, vault, model cache, and configuration
as part of the same trust boundary.

## Supported Patterns

| Pattern | Use When | Boundary |
|---|---|---|
| Local desktop MCP | Claude Desktop or Cowork can launch `anno-rag mcp`. | User workstation. |
| Local CLI | Operators run ingest, search, vault, or review commands directly. | User or admin shell. |
| Internal gateway | HTTP clients need an Anthropic-compatible `/v1/messages` API. | Loopback or private network. |

Avoid exposing the gateway on a public interface. If the gateway is reachable
beyond loopback, configure `ANNO_GATEWAY_BEARER_TOKEN` or place it behind an
approved authentication layer.

## Baseline Requirements

| Requirement | Guidance |
|---|---|
| Stable binary path | Extract the release archive to a versioned folder and point MCP configs to an absolute `anno-rag` path. |
| Writable vault and index dirs | Set or confirm `ANNO_RAG_DATA_DIR`; the process needs write access for `vault.enc`, LanceDB, outputs, and local state. |
| Model cache | Run `anno-rag download-models` once and set `ANNO_MODELS_DIR` when the cache is outside the default data directory. |
| Secrets handling | Prefer the OS keyring for desktop installs; use `ANNO_RAG_VAULT_PASSPHRASE` only through managed secret injection. |
| Checksum verification | Verify `SHA256SUMS.txt` before extracting release assets. |

## Deployment Checklist

1. Download the platform archive and checksum file.
2. Verify the checksum before extraction.
3. Extract to a stable local or managed tools directory.
4. Configure absolute binary paths in Claude Desktop, Cowork, service units, or scripts.
5. Set only the environment variables required by the deployment.
6. Run `anno-rag --version` and `anno-rag mcp` through the target client.
7. For gateway deployments, check `/health` and confirm bearer auth behavior.

## Related Links

- [Installation](../getting-started/installation.md)
- [Configuration](../developers/configuration.md)
- [Release Management](release-management.md)
