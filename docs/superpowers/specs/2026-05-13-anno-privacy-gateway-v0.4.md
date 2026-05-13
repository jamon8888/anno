# anno privacy gateway v0.4 — streaming and internal provider adapter

**Status:** Draft spec  
**Date:** 2026-05-13  
**Depends on:** `anno-privacy-gateway v0.3`

## Goal

Turn the v0.3 privacy boundary into a production-grade provider gateway:

- support streaming/SSE without leaking pseudonym tokens or partial cleartext,
- remove the hard dependency on `anthropic-proxy-rs` as a runtime sidecar,
- route to local, sovereign, or global providers from `anno` policy,
- preserve TensorZero as an optional backend for observability, fallback, and A/B testing.

## Non-goals

- No replacement for TensorZero's LLMOps features.
- No TensorZero embedded gateway unless a later benchmark proves it is worth the build and dependency cost.
- No multi-tenant SaaS control plane.
- No image redaction pipeline.

## Architecture

v0.4 evolves the v0.3 process chain:

```text
Cowork
  -> anno-privacy-gateway
     -> privacy transform
     -> provider adapter
        -> TensorZero OpenAI-compatible
        -> direct OpenAI-compatible
        -> direct Anthropic-compatible
        -> local vLLM/Ollama/NIM
```

The key change is that `anno` owns the provider adapter boundary. `anthropic-proxy-rs` remains a compatibility fallback during migration, but it is no longer required in the default path.

## Why internalize translation in v0.4

The v0.3 sidecar architecture is clean for validation, but it has limits:

- two HTTP hops between privacy and model routing,
- duplicated operational config,
- no single place to enforce provider policy,
- harder streaming rehydration because the stream has already been translated twice,
- `anthropic-proxy-rs` is binary-only, so we cannot call its translation logic as a stable Rust API.

v0.4 should either:

1. upstream a library split to `anthropic-proxy-rs`, then depend on it, or
2. implement a narrow internal adapter for the Anthropic subset Cowork uses.

Preferred path: attempt upstreamable extraction first. If that is not accepted or too slow, implement the narrow adapter internally with tests copied from observed Cowork traffic fixtures.

## Provider adapter model

Define a local trait:

```rust
#[async_trait]
pub trait LlmProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderResponse>;
    async fn stream(&self, request: ProviderRequest) -> Result<ProviderStream>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
}
```

Adapters:

- `AnthropicDirectProvider`: sends Anthropic-compatible requests directly to Anthropic or Anthropic-compatible sovereign gateways.
- `OpenAiCompatibleProvider`: translates the normalized internal request to OpenAI-compatible chat completions.
- `TensorZeroProvider`: OpenAI-compatible adapter pointed at `/openai/v1/chat/completions`, usually using `tensorzero::function_name::<name>` or `tensorzero::model_name::<provider>::<model>`.
- `SidecarAnthropicProxyProvider`: compatibility fallback to the v0.3 `anthropic-proxy-rs` chain.

## Normalized request model

v0.4 should not let privacy logic depend on provider-specific JSON. Introduce:

```text
Anthropic HTTP JSON
  -> CoworkRequest
  -> PrivacyTransform
  -> NormalizedChatRequest
  -> Provider adapter
```

`NormalizedChatRequest` should preserve:

- role order,
- system messages,
- text blocks,
- tool definitions,
- tool uses,
- tool results,
- thinking/reasoning flags,
- model intent,
- max tokens, temperature, stop sequences,
- metadata needed for audit.

The privacy transform still runs before provider translation.

## Streaming/SSE design

Streaming is the risky feature. A provider can split text arbitrarily:

```text
"PER" + "SON" + "_1"
```

The gateway must not attempt naive per-chunk rehydration.

Use a buffered token-aware stream transform:

1. Maintain a rolling buffer of the last `max_token_len + delimiter_margin` bytes.
2. Emit only confirmed-safe prefixes.
3. Detect pseudo-token patterns across chunk boundaries.
4. Rehydrate complete tokens only.
5. Preserve provider event ordering and Anthropic SSE event names.
6. On malformed stream, fail closed or terminate with an Anthropic error event.

Default token pattern should match the actual cloakpipe/anno token syntax, not a loose regex that can rewrite ordinary user text.

Fresh PII detection in streaming responses is harder because entity spans can cross chunks. v0.4 should support two modes:

- `stream_privacy = "token_rehydrate_only"`: rehydrate known pseudonyms, do not scan fresh PII in stream.
- `stream_privacy = "buffered_scan"`: buffer sentences/paragraphs, scan before emission, higher latency.

Recommended v0.4 default for regulated mode: `buffered_scan`.

## Policy routing

Introduce provider classes:

```toml
[providers.local]
kind = "openai-compatible"
base_url = "http://127.0.0.1:11434/v1"
credential_source = "none"
allowed_data = "clear_or_pseudonymized"

[providers.sovereign]
kind = "openai-compatible"
base_url = "https://..."
credential_source = "env:SOVEREIGN_PROVIDER_TOKEN"
allowed_data = "pseudonymized_only"

[providers.tensorzero]
kind = "tensorzero"
base_url = "http://127.0.0.1:4000/openai/v1"
credential_source = "none"
allowed_data = "pseudonymized_only"

[routing]
default = "tensorzero"
fallbacks = ["sovereign", "local"]
```

Rules:

- Global providers may only receive pseudonymized payloads.
- Sovereign providers may only receive pseudonymized payloads unless explicitly configured otherwise.
- Local providers may receive cleartext only if policy permits it, but the default still sends pseudonymized text for consistency.
- TensorZero logs must remain pseudonymized.

## TensorZero role

TensorZero remains optional infrastructure, not the trust boundary.

Use TensorZero when a deployment needs:

- observability,
- fallback,
- A/B testing,
- cost and latency tracking,
- prompt/model experiments.

Do not require TensorZero for:

- local-only single model use,
- air-gapped deployments,
- minimal office installs.

## Operational modes

### v0.4 default

```text
Cowork -> anno-privacy-gateway -> TensorZero -> provider
```

### v0.4 minimal local

```text
Cowork -> anno-privacy-gateway -> Ollama/vLLM/NIM local
```

### v0.4 compatibility fallback

```text
Cowork -> anno-privacy-gateway -> anthropic-proxy-rs -> TensorZero -> provider
```

## Testing

Translation tests:

- Anthropic text request to OpenAI-compatible request.
- tool definitions to OpenAI tools.
- tool use to OpenAI tool calls.
- tool result to OpenAI tool messages.
- OpenAI response to Anthropic response.
- model override and TensorZero model names.

Streaming tests:

- pseudo-token split across chunks is rehydrated once.
- ordinary text similar to a token is not rewritten.
- stream ending with partial token does not leak or panic.
- tool call stream remains valid.
- provider stream error maps to Anthropic error event.

Privacy tests:

- no cleartext leaves gateway in all provider classes except explicitly permitted local cleartext mode.
- TensorZero request bodies are pseudonymized.
- streaming response rehydration does not emit partial pseudo-token fragments.

Integration tests:

- mock TensorZero endpoint records body and returns streaming chunks.
- direct local OpenAI-compatible mock records body and returns both streaming/non-streaming.
- optional sidecar compatibility test against `anthropic-proxy-rs`.

## Acceptance criteria

- v0.3 non-streaming behavior remains unchanged.
- `stream=true` works for text responses.
- known pseudonyms are rehydrated in streaming responses.
- provider adapter can route to TensorZero without `anthropic-proxy-rs`.
- sidecar mode still works for rollback.
- tests prove TensorZero never receives cleartext PII.

## Deferred beyond v0.4

- Multi-tenant deployments.
- image/OCR redaction.
- admin UI for policies.
- customer-managed key vault integrations.
- advanced legal privilege policy engine.
