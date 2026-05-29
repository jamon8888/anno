# Environment Variables

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin
Language: EN

Environment variables define the local storage, model cache, memory privacy
mode, and gateway boundary. Set them through the operating system, service
manager, MCP client config, or secret manager used by the deployment.

## RAG And MCP

| Variable | Purpose | Notes |
|---|---|---|
| `ANNO_NO_DOWNLOADS` | Disable new model downloads where supported. | Use after models are already cached for offline or controlled-network runs. |
| `ANNO_RAG_VAULT_PASSPHRASE` | Derive the local RAG vault key from an admin-managed passphrase. | Prefer the OS keyring for desktop installs. Never commit, log, or paste this value. |
| `ANNO_RAG_DATA_DIR` | Override the local RAG data directory. | Holds `vault.enc`, `index.lance`, default `models`, and `outputs`. |
| `ANNO_MODELS_DIR` | Point model loaders at a populated model cache. | Usually set to the path printed by `anno-rag download-models`. |
| `ANNO_RAG_MEMORY_NER_MODE` | Control memory enrichment privacy mode. | Values include `async`, `sync`, and `disabled`; default `async` stores raw text immediately before background enrichment, while `sync` uses the inline tokenizing path where available. |

## Gateway

| Variable | Purpose | Notes |
|---|---|---|
| `ANNO_GATEWAY_BEARER_TOKEN` | Bearer token for protected `/v1/*` routes. | If unset, gateway auth is a no-op; only use that mode on loopback or another trusted private boundary. |
| `ANNO_GATEWAY_LISTEN` | Gateway listen address. | Default is `127.0.0.1:3000`. |
| `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE` | Anthropic-compatible upstream base URL. | Default local upstream is `http://127.0.0.1:3100`. |
| `ANNO_GATEWAY_PROVIDER_PROFILE` | Label the gateway provider/audit profile. | Useful for routing, audit review, and deployment diagnostics. |
| `ANNO_GATEWAY_STREAMING` | Enable or disable `stream=true` requests. | Set to `enabled`, `true`, or `1` to accept streaming. |
| `ANNO_GATEWAY_STREAM_PRIVACY` | Privacy mode for streamed response text. | Default is buffered scan; `token_rehydrate_only` skips fresh PII scan and only rehydrates known tokens. |
| `ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS` | Maximum buffered streaming text before a privacy scan flush. | Tune only after testing latency and leakage behavior. |
| `ANNO_GATEWAY_STREAM_MAX_BUFFER_MS` | Maximum buffering time before a streaming privacy scan flush. | Tune with `ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS`. |
| `ANNO_GATEWAY_AUDIT_DIR` | Directory for persistent gateway JSONL audit files. | Built-in audit is narrow; use HTTP logs/tracing for full route visibility. |
| `ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX` | Hex HMAC key for daily audit signature files. | Required when `ANNO_GATEWAY_AUDIT_DIR` is set. Keep separate from vault keys. |
| `ANNO_GATEWAY_VAULT_PATH` | Persistent gateway vault path. | Set with `ANNO_GATEWAY_VAULT_KEY_HEX`; otherwise the gateway uses ephemeral vault state. |
| `ANNO_GATEWAY_VAULT_KEY_HEX` | 32-byte gateway vault key encoded as 64 hex characters. | Secret material. Never commit, log, or reuse as an audit HMAC key. |

## Security Notes

- Do not commit secrets in config files, examples, shell history, screenshots,
  or issue comments.
- Do not log vault passphrases, gateway bearer tokens, vault keys, or audit
  HMAC keys.
- If the gateway is reachable outside loopback or a tightly controlled private
  network, set `ANNO_GATEWAY_BEARER_TOKEN` or put the gateway behind an
  approved authenticated proxy.
- `ANNO_RAG_MEMORY_NER_MODE` affects privacy behavior. Validate the chosen mode
  before storing sensitive memory content.

## Related Links

- [Configuration](../developers/configuration.md)
- [Privacy Model](../security-compliance/privacy-model.md)
