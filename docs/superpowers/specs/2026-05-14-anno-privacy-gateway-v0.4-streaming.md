# anno privacy gateway v0.4 — streaming-first privacy boundary

**Date:** 2026-05-14
**Status:** approved design, awaiting implementation plan
**Depends on:** `anno-privacy-gateway v0.3`
**Supersedes:** `2026-05-13-anno-privacy-gateway-v0.4.md` for the v0.4 implementation scope

## Goal

Add safe Anthropic-compatible streaming support to `anno-privacy-gateway` without regressing the finalized v0.3 non-streaming privacy boundary.

v0.4 keeps the v0.3 process composition:

```text
Cowork
  -> anno-privacy-gateway
  -> anthropic-proxy-rs
  -> TensorZero or another OpenAI-compatible provider
```

The internal provider adapter is explicitly deferred. v0.4 is successful when Cowork can send `stream=true`, the gateway can pseudonymize before upstream, and the Cowork-facing SSE stream never leaks partial pseudonym tokens or fresh provider-side PII in regulated mode.

## Non-goals

- No internal Anthropic-to-OpenAI provider adapter in v0.4.
- No removal of `anthropic-proxy-rs` from the default streaming path.
- No native `/v1/files` or `document` support.
- No image/OCR redaction pipeline.
- No multi-tenant vault isolation.
- No TensorZero embedded gateway.

## Required v0.3 Non-Regression

The following v0.3 behavior must remain unchanged:

- non-streaming `POST /v1/messages` behavior,
- request pseudonymization before any upstream call,
- response rehydration before returning to Cowork,
- fresh response PII redaction,
- `X-Anno-PII-Leak-Redacted` for non-streaming redactions,
- `stream=true` disabled unless explicitly enabled by v0.4 config,
- `/v1/files` fail-closed,
- native `document` blocks fail-closed,
- image blocks fail-closed in strict regulated mode.

## Configuration

Streaming is disabled by default in the first v0.4 release.

```text
ANNO_GATEWAY_STREAMING=disabled | enabled
ANNO_GATEWAY_STREAM_PRIVACY=buffered_scan | token_rehydrate_only
ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS=4096
ANNO_GATEWAY_STREAM_MAX_BUFFER_MS=750
```

Defaults:

- `ANNO_GATEWAY_STREAMING=disabled`
- `ANNO_GATEWAY_STREAM_PRIVACY=buffered_scan`
- `ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS=4096`
- `ANNO_GATEWAY_STREAM_MAX_BUFFER_MS=750`

Later v0.4 stabilization may switch to profile-based activation:

- `local` / `dev`: streaming allowed,
- `sovereign` / `global_anonymized`: streaming allowed only with `buffered_scan`,
- `token_rehydrate_only`: local/dev or explicit override only.

## Streaming Modes

### `buffered_scan`

Default regulated mode.

The gateway buffers text deltas until it has a safe emission unit, scans for fresh PII, redacts new PII, rehydrates known pseudonym tokens, then emits an Anthropic-compatible SSE delta.

This mode trades latency for privacy. It must be the default whenever provider-side output may contain fresh cleartext PII.

### `token_rehydrate_only`

Low-latency local/dev mode.

The gateway rehydrates known pseudonym tokens across chunk boundaries but does not scan for fresh model-side PII before emission.

This mode is not the regulated default because a model could emit new cleartext PII that was not present in the request vault.

## Hybrid Buffering

`buffered_scan` uses a hybrid strategy:

1. Flush complete short sentences when punctuation/newline indicates a boundary.
2. If no sentence boundary appears, flush when `max_buffer_chars` is reached.
3. If neither boundary nor size limit is reached, flush when `max_buffer_ms` is reached.
4. Always keep enough trailing bytes to detect pseudonym tokens split across chunks.
5. Never emit an incomplete pseudonym token fragment.

Pseudonym token detection must match the actual token syntax produced by the local vault/replacer. It must not use an overly broad regex that rewrites ordinary text.

## SSE Handling

The gateway must parse and re-emit Anthropic-compatible SSE events.

Minimum event handling:

- pass through non-text structural events unchanged when safe,
- transform assistant text deltas,
- preserve event ordering,
- preserve final stop events,
- reject or error on malformed event sequences.

The gateway must not treat raw chunks as arbitrary text. It must parse `event:` and `data:` frames, transform only text-bearing JSON fields, and preserve unknown safe fields.

## Error Policy

Before the first SSE event is emitted:

- return a normal HTTP error if request pseudonymization or upstream connection fails.

After the stream has started:

- emit an Anthropic-compatible SSE `error` event,
- include only a safe technical message,
- close the stream,
- log only pseudonymized metadata.

Forbidden logs:

- original prompt text,
- rehydrated stream output,
- vault contents,
- provider API keys.

Allowed logs:

- request id,
- provider profile,
- stream privacy mode,
- entity counts,
- redaction counts,
- upstream status/error class,
- latency and buffer flush reason.

## Sidecar Path

The required v0.4 runtime chain is:

```text
Cowork
  -> anno-privacy-gateway :3000
  -> anthropic-proxy-rs   :3100
  -> mock OpenAI-compatible SSE or TensorZero
```

`anthropic-proxy-rs` remains the Anthropic-to-OpenAI sidecar for v0.4. The gateway owns privacy transforms and stream safety; the sidecar owns provider protocol translation.

## Testing Requirements

Unit tests:

- `stream=true` returns `400` when `ANNO_GATEWAY_STREAMING` is disabled.
- `stream=true` is accepted when streaming is enabled.
- pseudonym token split across chunks is rehydrated once.
- incomplete pseudonym token fragments are not emitted.
- fresh PII split across chunks is redacted in `buffered_scan`.
- `token_rehydrate_only` does not perform fresh PII scan.
- malformed SSE emits an error event after stream start.
- non-streaming v0.3 tests still pass unchanged.

Integration tests:

- mock Anthropic SSE upstream emits split pseudo-token chunks.
- gateway emits valid Anthropic SSE output.
- mock upstream captures request body and proves raw PII did not leave the gateway.
- `/v1/files`, `document`, and image fail-closed behavior remain unchanged.

Finalization smoke:

- blocking: `anno-privacy-gateway -> anthropic-proxy-rs -> mock OpenAI-compatible SSE`.
- documented operational smoke: `anno-privacy-gateway -> anthropic-proxy-rs -> TensorZero -> provider/mock provider`, with TensorZero observability checked for pseudonymized-only payloads.

## Acceptance Criteria

- `cargo test -p anno-privacy-gateway` passes.
- v0.3 smoke script still passes.
- v0.4 streaming smoke script passes with local sidecar and mock OpenAI-compatible SSE.
- No cleartext PII reaches the streaming upstream in tests.
- Known pseudonym tokens are rehydrated correctly across SSE chunk boundaries.
- Fresh provider-side PII is redacted before Cowork receives it in `buffered_scan`.
- `stream=true` remains disabled by default.
- GitNexus is reindexed after commit.

## Deferred

- Internal provider adapter.
- Direct TensorZero/OpenAI-compatible routing from `anno-privacy-gateway`.
- Native file/document ingress.
- Profile-based automatic streaming enablement.
- Multi-tenant vault and policy admin UI.
