# Privacy Gateway User Guide

Status: Available in v0.11.0-rc.11
Audience: User, Developer, Integrator, Admin
Language: Bilingual

The Privacy Gateway is a local Anthropic-compatible HTTP proxy. It lets clients
that cannot call MCP directly use a familiar `/v1/messages` API while Hacienda
keeps cleartext PII inside the local boundary.

Utilisez le gateway quand une application sait parler a une API compatible
Anthropic, mais ne peut pas invoquer directement les outils MCP locaux.

## When To Use It

Use the gateway for:

- Cowork or internal clients that route through an Anthropic-compatible API;
- integrations that need HTTP instead of stdio MCP;
- remote provider calls where prompts must be tokenized first;
- controlled enterprise deployments with bearer auth and audit logging.

Use MCP directly when the client can launch `anno-rag mcp` and needs local tools
such as `search`, `rehydrate`, `memory_*`, or `review_*`.

## Request Flow

```text
Client
  -> POST /v1/messages to local anno-privacy-gateway
  -> gateway detects and tokenizes outbound prompt content
  -> gateway forwards tokenized request to Anthropic-compatible upstream
  -> upstream returns tokenized assistant output
  -> gateway rehydrates known tokens locally when auto_rehydrate is enabled
  -> client receives the local response
```

The remote provider should receive tokenized text, not the original names,
emails, phone numbers, IBANs, or other detected values. Rehydration happens
locally before the response is returned to the trusted client.

## Privacy Modes

Provider-router models expose the privacy mode in the model id:

- `:pseudonymized` is the default regulated mode. The gateway tokenizes request
  content before the upstream call and rehydrates known tokens locally.
- `:cleartext-dpa` sends cleartext only to a provider with `dpa_verified=true`
  and only when the catalog sets `allow_cleartext_dpa=true`.
- `:cleartext-local` is accepted only for provider id `local` or loopback local
  endpoints.

Cleartext DPA mode is intentionally opt-in and writes content-free audit
metadata. It does not store prompt or file contents in logs. Use it only for
providers where the deployment has accepted the provider DPA and configured the
catalog accordingly.

## Files And Document Blocks

The gateway supports Claude/Cowork document blocks for local uploaded files and
inline base64 documents:

- `source.type = "file"` is accepted only for gateway ids that start with
  `anno_file_`.
- `source.type = "base64"` is extracted locally before provider routing.
- `source.type = "url"` is rejected. Fetch remote URLs yourself, upload the file
  to the gateway, then reference the returned `anno_file_*` id.

For `:pseudonymized` models, document text sent upstream uses the pseudonymized
derivative. For `:cleartext-dpa` models, document text is sent in cleartext only
when the selected provider has `dpa_verified=true` and the deployment enables
`allow_cleartext_dpa=true`.

Set `ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT=false` to prevent local cleartext text
retention after upload. In that mode, `:cleartext-dpa` requests that reference
stored files are rejected because no cleartext derivative is available.

## Streaming

Streaming is accepted only when `ANNO_GATEWAY_STREAMING=enabled` is configured.
When streaming privacy mode uses buffered scanning, the gateway buffers response
deltas, waits for safe flush points, rescans the accumulated text, redacts fresh
PII if needed, and rehydrates known tokens before emitting events.

This buffering matters because PII can be split across streaming chunks. The
gateway avoids flushing partial cleartext fragments that could become a detected
entity only after the next delta arrives.

In legacy upstream mode, streaming `input_json_delta` tool-use frames fail
closed with an unsupported-delta stream error. In provider-router mode, the
gateway converts OpenAI-compatible stream chunks to Anthropic-compatible frames
and buffers complete tool JSON before emitting safe `input_json_delta` payloads.

## Authentication

`/health` is public so operators can check liveness.

When `ANNO_GATEWAY_BEARER_TOKEN` is unset, gateway auth is a no-op and routes
under `/v1/*` remain usable behind the loopback or network boundary. When the
token is set, `/v1/*` routes require bearer auth. Send:

```text
Authorization: Bearer <token>
```

Security guidance: deployments exposed beyond loopback should always configure
`ANNO_GATEWAY_BEARER_TOKEN` or place the gateway behind an approved
authentication layer. Do not rely on the no-op auth mode outside a trusted local
or private network boundary.

## Operator Notes

Common environment variables:

| Variable | Purpose |
|---|---|
| `ANNO_GATEWAY_LISTEN` | Local listen address, default `127.0.0.1:3000`. |
| `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE` | Anthropic-compatible upstream base URL. |
| `ANNO_GATEWAY_PROVIDER_CATALOG` | TOML provider catalog path for Mistral, Scaleway, OVHcloud, or local OpenAI-compatible routing. |
| `ANNO_GATEWAY_BEARER_TOKEN` | Bearer token for protected `/v1/*` routes. |
| `ANNO_GATEWAY_VAULT_PATH` | Persistent local gateway vault path. |
| `ANNO_GATEWAY_VAULT_KEY_HEX` | 32-byte vault key in hex; must be paired with vault path. |
| `ANNO_GATEWAY_FILE_STORE_DIR` | Local directory for file metadata and text derivatives. |
| `ANNO_GATEWAY_FILE_MAX_BYTES` | Maximum accepted upload size in bytes. |
| `ANNO_GATEWAY_FILE_RETAIN_RAW` | Set to `true` only when operators intentionally retain raw upload bytes. |
| `ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT` | Set to `false` to avoid retaining cleartext extracted text after upload. |
| `ANNO_GATEWAY_STREAMING` | Set to `enabled`, `true`, or `1` to accept streaming. |
| `ANNO_GATEWAY_AUDIT_DIR` | Directory for persistent JSONL audit files. |
| `ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX` | HMAC key for audit signatures. |

## Limits

- The gateway is an HTTP privacy boundary, not a full replacement for MCP tools.
- Image blocks are rejected in the strict regulated profile.
- If persistent vault settings are incomplete, startup fails.
- If `ANNO_GATEWAY_BEARER_TOKEN` is unset, `/v1/*` auth is disabled by design;
  this is acceptable only behind a trusted loopback or network boundary.

## Related Links

- [Gateway API](../developers/gateway-api.md)
- [Operations](../admins/operations.md)
- [Audit Logging](../security-compliance/audit-logging.md)
