# anno privacy gateway v0.4 streaming runbook

## Local smoke

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke-privacy-gateway-v0.4-streaming.ps1
```

## Required environment

```powershell
$env:ANNO_GATEWAY_STREAMING = "enabled"
$env:ANNO_GATEWAY_STREAM_PRIVACY = "buffered_scan"
$env:ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS = "4096"
$env:ANNO_GATEWAY_STREAM_MAX_BUFFER_MS = "750"
```

## Sidecar smoke

1. Start mock OpenAI-compatible SSE provider on `127.0.0.1:3900`.
2. Start `anthropic-proxy-rs` on `127.0.0.1:3100` with `UPSTREAM_BASE_URL=http://127.0.0.1:3900`.
3. Start `anno-privacy-gateway` on `127.0.0.1:3000`.
4. Send a Cowork-shaped `stream=true` request to `http://127.0.0.1:3000/v1/messages`.
5. Verify the mock provider captured only pseudonyms.
6. Verify Cowork-facing SSE rehydrated known tokens and redacted fresh PII.

## TensorZero operational smoke

TensorZero remains behind the privacy boundary. After running the live sidecar path, inspect TensorZero observability and verify it contains pseudonym tokens but no original names, emails, phone numbers, IBANs, SIRETs, NIRs, or document text.
