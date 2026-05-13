# anno privacy gateway v0.3 — non-streaming Cowork privacy boundary

**Status:** Draft spec  
**Date:** 2026-05-13  
**Depends on:** `anno-rag v0.2` MCP minimum  
**Research inputs:**
- TensorZero clone: `C:\tmp\anno-research\tensorzero` at `6ea911b8b697e50521b26e075b347f6b46d96539`
- anthropic-proxy-rs clone: `C:\tmp\anno-research\anthropic-proxy-rs` at `596dfad5306ec6c88cfcc1f748e01ec9670347bf`

## Goal

Add a local Anthropic-compatible privacy gateway in front of Cowork so every prompt sent by Cowork is pseudonymized before it reaches any model provider, including TensorZero, NVIDIA NIM, OpenAI, Anthropic, sovereign providers, or local OpenAI-compatible servers.

The user-facing answer is rehydrated automatically before it is returned to Cowork.

## Non-goals

- No streaming/SSE support in v0.3.
- No embedded TensorZero gateway.
- No fork of `anthropic-proxy-rs`.
- No replacement for `anno-rag mcp`; the RAG MCP tool rail remains separate.
- No multi-tenant auth model.
- No cloud-side vault.

## Architecture

```text
Cowork
  -> Anthropic /v1/messages
anno-privacy-gateway
  -> pseudonymize request text
  -> persist mappings in local vault
  -> forward Anthropic request with pseudonyms
anthropic-proxy-rs
  -> translate Anthropic to OpenAI-compatible
TensorZero or direct OpenAI-compatible provider
  -> route to local / sovereign / global provider
LLM
  -> response with pseudonyms
anthropic-proxy-rs
  -> translate OpenAI-compatible response back to Anthropic
anno-privacy-gateway
  -> scan for new leaked PII
  -> rehydrate known pseudonyms
  -> return clear response to Cowork
```

`anno-privacy-gateway` is the trust boundary. Every component behind it must be treated as untrusted for cleartext PII.

## Why process composition for v0.3

`anthropic-proxy-rs` is currently a binary-only crate with no public `src/lib.rs`. Reusing it as a library would require a fork or upstream refactor before we have validated the product flow.

TensorZero is a large Rust workspace with its own gateway binary, Rust 1.93 baseline, edition 2024, and an HTTP/OpenAI-compatible gateway designed for process-level deployment. Embedding it into `anno` would add avoidable dependency and build-system friction.

So v0.3 composes processes:

```text
anno-privacy-gateway :3000
anthropic-proxy-rs   :3100
TensorZero           :4000
provider             remote or local
```

This keeps `anno` focused on privacy and makes TensorZero optional.

## Crate shape

Create a new workspace crate:

```text
crates/anno-privacy-gateway/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── server.rs
    ├── anthropic.rs
    ├── privacy.rs
    ├── upstream.rs
    ├── policy.rs
    └── audit.rs
```

Responsibilities:

- `main.rs`: CLI/env config, tracing init, server start.
- `server.rs`: Axum routes `/v1/messages`, `/v1/models`, `/health`.
- `anthropic.rs`: minimal Anthropic request/response structs, preserving unknown fields.
- `privacy.rs`: pure request/response transforms.
- `upstream.rs`: reqwest forwarding to `anthropic-proxy-rs`.
- `policy.rs`: provider policy and streaming refusal.
- `audit.rs`: pseudonymized audit events only.

## API surface

### `POST /v1/messages`

Accepts Anthropic-compatible requests with:

- `system`: string or array of text blocks.
- `messages[].content`: string or array of blocks.
- text blocks.
- tool result text blocks.
- tool use JSON arguments, if string fields are present.

Rejects or strips unsupported features explicitly:

- `stream=true` returns `400` with a clear message: streaming is deferred to v0.4.
- file APIs and batch APIs are out of scope.

### `GET /v1/models`

Pass-through to the configured upstream, with no privacy transform.

### `GET /health`

Returns gateway health and optionally upstream health state.

## Privacy transform rules

Request pseudonymization must inspect every outbound text-bearing field:

- `system` string.
- `system[].text`.
- `messages[].content` string.
- `messages[].content[].text`.
- `messages[].content[].content` for `tool_result`.
- JSON string leaves inside `tool_use.input`, because tools may carry user text or file snippets.

Images are not transformed in v0.3. If an image block is present, the gateway should either pass it unchanged only when policy permits images, or reject it in strict mode. Default for regulated-profession mode: reject image blocks until OCR/image redaction is designed.

Response rehydration must inspect:

- assistant text blocks.
- thinking blocks, if present and returned by the proxy.
- tool use JSON string leaves.

Before rehydration, the response must be scanned for fresh cleartext PII. Any detected PII that is not already represented in the local vault is treated as provider leakage or model hallucinated PII and redacted by default.

## Vault and sessions

v0.3 uses a local single-user vault. The session identity can be:

- header-based, if Cowork forwards a stable header.
- connection-generated, if no stable identifier exists.
- default global session for the first implementation.

The vault must never be sent to TensorZero or any provider. TensorZero observability must only contain pseudonymized prompts and responses.

## Provider policy

The gateway should not hardcode one provider path. It should support an upstream chain:

```toml
[gateway]
listen = "127.0.0.1:3000"
upstream_anthropic_base = "http://127.0.0.1:3100"
auto_rehydrate = true
streaming = "reject"

[privacy]
mode = "strict"
reject_images = true
log_cleartext = false

[providers]
profile = "global_anonymized" # local | sovereign | global_anonymized
```

The first v0.3 runnable profile is:

```text
Cowork -> anno-privacy-gateway -> anthropic-proxy-rs -> TensorZero -> NIM/OpenAI-compatible
```

Direct OpenAI-compatible routing can be added later, but should not block v0.3.

## Logging and audit

Allowed logs:

- request id.
- entity counts.
- entity categories.
- provider profile.
- upstream latency.
- pseudonymized outbound body in debug-only test mode.

Forbidden logs:

- original cleartext prompt.
- rehydrated final answer.
- vault contents.
- provider API keys.

## Error handling

- If pseudonymization fails: fail closed with `500`, do not call upstream.
- If unsupported streaming is requested: return `400`.
- If upstream fails: return Anthropic-shaped error where possible.
- If rehydration fails: return pseudonymized response plus a warning header only if configured; default should fail closed for regulated mode.
- If fresh PII appears in the model response: redact it and add `X-Anno-PII-Leak-Redacted: <count>`.

## Test plan

Unit tests:

- pseudonymizes `system` string.
- pseudonymizes `system[]`.
- pseudonymizes message string content.
- pseudonymizes text blocks.
- pseudonymizes `tool_result.content`.
- pseudonymizes JSON string leaves inside `tool_use.input`.
- rejects `stream=true`.
- rehydrates known pseudonyms in response text.
- redacts fresh PII in model response.

Integration tests with upstream mock:

- send a Cowork-shaped `/v1/messages` request containing French legal PII.
- assert upstream mock receives no raw name, NIR, SIRET, IBAN, phone, email.
- upstream mock returns text containing `PERSON_1`.
- final Cowork response contains the original name when `auto_rehydrate=true`.
- audit log contains only pseudonymized payload and counts.

Manual smoke:

```text
Cowork base URL: http://127.0.0.1:3000
anno-privacy-gateway upstream: http://127.0.0.1:3100
anthropic-proxy-rs upstream: http://127.0.0.1:4000/openai
TensorZero provider: NIM / local / sovereign
```

## Acceptance criteria

- `cargo check -p anno-privacy-gateway` passes.
- Unit tests cover all request/response text-bearing Anthropic fields in scope.
- Mock upstream proves no cleartext PII leaves `anno-privacy-gateway`.
- Final response is automatically rehydrated.
- `stream=true` is rejected with an explicit error.
- TensorZero receives only pseudonymized content in the smoke path.

## Deferred to v0.4

- Streaming/SSE rehydration.
- Internal Anthropic-to-OpenAI translation.
- Provider routing without `anthropic-proxy-rs`.
- Policy UI.
- Multi-tenant vault isolation.
