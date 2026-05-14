# anno privacy gateway v0.3 runbook

This runbook is the v0.3 operating profile for Cowork 3P.

## Invariant

`anno-privacy-gateway` is the privacy boundary. Cowork may send cleartext to the gateway on localhost; every component behind it is treated as untrusted for cleartext PII.

```text
Cowork
  -> anno-privacy-gateway :3000
  -> anthropic-proxy-rs   :3100
  -> TensorZero           :4000
  -> local, sovereign, or global provider
```

TensorZero must only observe pseudonymized prompts and pseudonymized model responses.

## Local boundary smoke

Run this before any sidecar/provider smoke:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke-privacy-gateway-v0.3.ps1
```

The script builds `anno-privacy-gateway`, starts a mock Anthropic-compatible upstream, and proves:

- `Marie Dupont` and `marie.dupont@example.com` do not leave the gateway.
- the upstream receives `PERSON_*` and `EMAIL_*` pseudonyms.
- known pseudonyms are rehydrated before the Cowork-facing response.
- fresh PII invented by the upstream is redacted.
- `X-Anno-PII-Leak-Redacted` reports the redaction count.
- `/v1/files` fails closed.

## Start TensorZero

Create a TensorZero config where the provider is OpenAI-compatible. For NVIDIA NIM:

```toml
[gateway]
observability.enabled = true

[models.nim_nemotron]
routing = ["nim"]

[models.nim_nemotron.providers.nim]
type = "openai"
model_name = "nvidia/llama-3.1-nemotron-70b-instruct"
api_base = "https://integrate.api.nvidia.com/v1"
api_key_location = "env::NVIDIA_API_KEY"

[functions.chat]
type = "chat"

[functions.chat.variants.default]
type = "chat_completion"
model = "nim_nemotron"
```

Start TensorZero on `127.0.0.1:4000`, with ClickHouse enabled when observability is required.

## Start anthropic-proxy-rs

From the cloned proxy:

```powershell
cd C:\tmp\anno-research\anthropic-proxy-rs
$env:UPSTREAM_BASE_URL = "http://127.0.0.1:4000/openai"
$env:UPSTREAM_API_KEY = "dummy"
$env:COMPLETION_MODEL = "tensorzero::function_name::chat"
$env:PORT = "3100"
cargo run --release
```

Do not enable verbose body logging in regulated mode.

## Start anno-privacy-gateway

From this repository:

```powershell
$env:ANNO_GATEWAY_LISTEN = "127.0.0.1:3000"
$env:ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE = "http://127.0.0.1:3100"
$env:ANNO_GATEWAY_PROVIDER_PROFILE = "global_anonymized"
cargo run -p anno-privacy-gateway
```

For a persistent local vault, set both variables together:

```powershell
$env:ANNO_GATEWAY_VAULT_PATH = "$env:LOCALAPPDATA\Anno\privacy-gateway-v03.vault"
$env:ANNO_GATEWAY_VAULT_KEY_HEX = "<64 hex chars from the local secret manager>"
```

Never store `ANNO_GATEWAY_VAULT_KEY_HEX` in repository files.

## Configure Cowork 3P

```text
Gateway base URL: http://127.0.0.1:3000
Gateway API key:  dummy
Auth scheme:      Bearer
```

## Live privacy smoke

Send a Cowork-shaped request containing known fake PII:

```powershell
$body = @{
  model = "claude-smoke"
  max_tokens = 128
  messages = @(@{
    role = "user"
    content = "Bonjour Marie Dupont, contactez marie.dupont@example.com"
  })
} | ConvertTo-Json -Depth 8

Invoke-WebRequest `
  -Uri "http://127.0.0.1:3000/v1/messages" `
  -Method POST `
  -ContentType "application/json" `
  -Body $body
```

Acceptance checks:

- Cowork receives a valid Anthropic-shaped response.
- the final Cowork-facing response may contain `Marie Dupont` only because it was rehydrated locally.
- TensorZero observability contains no `Marie Dupont` and no `marie.dupont@example.com`.
- TensorZero observability does contain pseudonym tokens such as `PERSON_*` and `EMAIL_*`.
- `/v1/files` returns `400`.
- any `document` block in `/v1/messages` returns `400`.

## Failure rules

- If pseudonymization fails, stop: the gateway must fail closed before upstream.
- If TensorZero contains cleartext PII, treat the deployment as failed and purge that observability store.
- If Cowork needs native documents or Files API, do not bypass the gateway; use the v0.5 document ingress plan.
