# Gateway API

Status: Available in v0.11.0-rc.11
Audience: Developer, Integrator, Admin
Language: EN

`anno-privacy-gateway` is an Anthropic-compatible HTTP gateway for clients that
cannot use MCP. It accepts Anthropic-style HTTP requests, tokenizes cleartext
before forwarding prompts to the configured upstream provider, and rehydrates
known tokens before returning responses to the trusted caller when configured.

## Main Behavior

- Provides `/health` for operational checks.
- Proxies `/v1/messages` to the configured Anthropic-compatible upstream.
- Proxies `/v1/models` without privacy transforms.
- Rejects unsupported file endpoints rather than passing file content through.
- Tokenizes outbound request text through the local gateway vault.
- Rehydrates known pseudonyms in responses when auto-rehydration is enabled.
- Supports data-subject lookup, erasure, and export routes over the gateway
  vault.
- Streaming is opt-in by environment configuration and applies the configured
  stream privacy policy.
- In `v0.11.0-rc.11`, streaming `input_json_delta` tool-use frames fail closed
  with an unsupported-delta error instead of being forwarded unsafely.

## Data Subject Routes

| Route | Purpose |
|---|---|
| `POST /v1/subjects/find` | Find vault mappings for an original sensitive value or pseudo-token. |
| `POST /v1/subjects/forget` | Remove a matching subject mapping and return an erasure receipt. |
| `GET /v1/subjects/{subject_ref}/export?format=json|csv` | Export matching subject mappings in JSON or CSV format. |

Subject lookup and export routes can return original sensitive values from the
gateway vault. Treat them as trusted-client operations and protect them with
bearer auth or a loopback/private network boundary.

## Authentication

`/health` is public.

When `ANNO_GATEWAY_BEARER_TOKEN` is set, protected `/v1/*` routes require:

```http
Authorization: Bearer <token>
```

When `ANNO_GATEWAY_BEARER_TOKEN` is unset, bearer authentication is a no-op.
In that mode, bind the gateway to loopback or place it behind a trusted network
boundary. Do not expose an unauthenticated gateway on a public interface.

## Related Docs

- [Privacy Gateway](../user-guide/privacy-gateway.md)
- [Configuration](configuration.md)
- [Audit Logging](../security-compliance/audit-logging.md)
