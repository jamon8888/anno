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
- Accepts local file uploads, extracts text locally, and returns metadata-only
  `anno_file_*` references.
- Tokenizes outbound request text through the local gateway vault.
- Rehydrates known pseudonyms in responses when auto-rehydration is enabled.
- Supports data-subject lookup, erasure, and export routes over the gateway
  vault.
- Streaming is opt-in by environment configuration and applies the configured
  stream privacy policy.
- Without a provider catalog, legacy streaming `input_json_delta` tool-use
  frames fail closed with an unsupported-delta error instead of being forwarded
  unsafely.

## Provider Catalog

Set `ANNO_GATEWAY_PROVIDER_CATALOG` to enable provider-router mode. Without this
variable, the gateway keeps the legacy `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE`
proxy behavior.

Example:

```toml
allow_cleartext_dpa = true

[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "mistral-large-latest", upstream = "mistral-large-latest" }]

[[providers]]
id = "local"
kind = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
api_key_env = ""
dpa_verified = false
allowed_privacy_modes = ["pseudonymized", "cleartext_local"]
models = [{ id = "llama-local", upstream = "llama3.1" }]
```

Visible model ids include the provider and privacy mode:

```text
anno/mistral/mistral-large-latest:pseudonymized
anno/mistral/mistral-large-latest:cleartext-dpa
anno/local/llama-local:cleartext-local
```

Provider API keys are read from the named environment variable at request time.
Do not put secret values in the catalog file.

## File Ingress

`POST /v1/files` accepts a multipart `file` field. The gateway extracts text
locally, stores an `anno_file_*` reference, and returns metadata only:

```json
{
  "id": "anno_file_018f4fd0f70a7e26b6b0c4d4ec0a09b0",
  "object": "file",
  "filename": "contract.pdf",
  "bytes": 123456,
  "created_at": 1780747200,
  "purpose": "assistants",
  "content_type": "application/pdf",
  "sha256": "hex"
}
```

`GET /v1/files/{id}/content` returns the pseudonymized text derivative only.
It never returns the cleartext derivative.

`DELETE /v1/files/{id}` deletes metadata, pseudonymized text, optional cleartext
text, and optional raw bytes for that gateway-managed file id.

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
