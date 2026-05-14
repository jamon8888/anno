# Anno Privacy Gateway v0.4 Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe Anthropic-compatible streaming support to `anno-privacy-gateway` while preserving the finalized v0.3 non-streaming behavior.

**Architecture:** Keep the v0.3 sidecar chain: Cowork -> `anno-privacy-gateway` -> `anthropic-proxy-rs` -> TensorZero or another OpenAI-compatible provider. Add streaming config, an SSE parser/serializer, a stream privacy transformer, and a streaming branch in `/v1/messages`. Do not add the internal provider adapter in this release.

**Tech Stack:** Rust 2021, Axum 0.8, Reqwest, Tokio, Serde JSON, `cloakpipe-core`, `futures-util`, `async-stream`.

---

## Scope Guard

This plan implements only `docs/superpowers/specs/2026-05-14-anno-privacy-gateway-v0.4-streaming.md`.

Do not implement:

- internal Anthropic-to-OpenAI provider adapter,
- direct TensorZero routing,
- `/v1/files` upload support,
- native `document` transform,
- image/OCR redaction,
- multi-tenant vaults.

Before editing Rust symbols, follow the repo GitNexus rule: run `npx gitnexus impact --repo anno <symbol>` for each modified symbol and record any HIGH/CRITICAL warning before proceeding.

## File Structure

- Modify `crates/anno-privacy-gateway/Cargo.toml`: add streaming helper dependencies.
- Modify root `Cargo.toml`: add workspace dependencies if the repo prefers workspace dependency declaration.
- Modify `crates/anno-privacy-gateway/src/lib.rs`: expose the new stream module.
- Modify `crates/anno-privacy-gateway/src/config.rs`: add streaming config fields and env parsing.
- Modify `crates/anno-privacy-gateway/src/privacy.rs`: allow stream requests when config permits and expose a small text transform for streaming output.
- Create `crates/anno-privacy-gateway/src/stream.rs`: SSE frame parsing, text delta extraction/replacement, hybrid buffer, and stream transform tests.
- Modify `crates/anno-privacy-gateway/src/upstream.rs`: add streaming upstream request support.
- Modify `crates/anno-privacy-gateway/src/server.rs`: branch `/v1/messages` into non-streaming and streaming responses.
- Create `scripts/smoke-privacy-gateway-v0.4-streaming.ps1`: local streaming smoke with mock Anthropic SSE upstream.
- Modify `docs/runbooks/anno-privacy-gateway-v0.3.md` or create `docs/runbooks/anno-privacy-gateway-v0.4-streaming.md`: document v0.4 streaming env, local sidecar smoke, and TensorZero operational smoke.

---

### Task 1: Streaming Config and Request Gate

**Files:**
- Modify: `crates/anno-privacy-gateway/src/config.rs`
- Modify: `crates/anno-privacy-gateway/src/privacy.rs`

- [ ] **Step 1: Run GitNexus impact checks**

Run:

```powershell
npx gitnexus impact --repo anno GatewayConfig
npx gitnexus impact --repo anno PrivacyEngine
```

Expected: LOW risk or no HIGH/CRITICAL warning. If HIGH/CRITICAL appears, stop and report the blast radius.

- [ ] **Step 2: Write failing config tests**

Add these tests to `crates/anno-privacy-gateway/src/config.rs`:

```rust
#[test]
fn streaming_defaults_to_disabled_buffered_scan() {
    let cfg = GatewayConfig::default();
    assert_eq!(cfg.streaming, StreamingMode::Disabled);
    assert_eq!(cfg.stream_privacy, StreamPrivacyMode::BufferedScan);
    assert_eq!(cfg.stream_max_buffer_chars, 4096);
    assert_eq!(cfg.stream_max_buffer_ms, 750);
}
```

Add this import inside the test module if needed:

```rust
use super::{GatewayConfig, StreamPrivacyMode, StreamingMode};
```

- [ ] **Step 3: Run the config test and verify it fails**

Run:

```powershell
cargo test -p anno-privacy-gateway streaming_defaults_to_disabled_buffered_scan
```

Expected: FAIL because `StreamingMode`, `StreamPrivacyMode`, and fields do not exist.

- [ ] **Step 4: Add config enums and fields**

In `crates/anno-privacy-gateway/src/config.rs`, add these enums above `GatewayConfig`:

```rust
/// Streaming availability for `/v1/messages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    /// Reject `stream=true`.
    Disabled,
    /// Accept `stream=true`.
    Enabled,
}

impl StreamingMode {
    /// Parse an environment label.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "enabled" | "true" | "1" => Self::Enabled,
            _ => Self::Disabled,
        }
    }

    /// Return true when streaming requests are accepted.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Privacy policy applied to streamed response text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPrivacyMode {
    /// Buffer, scan fresh PII, redact, then rehydrate known pseudonyms.
    BufferedScan,
    /// Rehydrate known pseudonyms only; no fresh PII scan.
    TokenRehydrateOnly,
}

impl StreamPrivacyMode {
    /// Parse an environment label.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "token_rehydrate_only" => Self::TokenRehydrateOnly,
            _ => Self::BufferedScan,
        }
    }
}
```

Add these fields to `GatewayConfig`:

```rust
/// Whether `stream=true` is accepted.
pub streaming: StreamingMode,
/// Privacy transform used for streamed response text.
pub stream_privacy: StreamPrivacyMode,
/// Maximum buffered text before a forced streaming flush.
pub stream_max_buffer_chars: usize,
/// Maximum buffered age before a forced streaming flush.
pub stream_max_buffer_ms: u64,
```

Add these defaults:

```rust
streaming: StreamingMode::Disabled,
stream_privacy: StreamPrivacyMode::BufferedScan,
stream_max_buffer_chars: 4096,
stream_max_buffer_ms: 750,
```

Add env parsing in `from_env()`:

```rust
if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAMING") {
    cfg.streaming = StreamingMode::parse(&value);
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_PRIVACY") {
    cfg.stream_privacy = StreamPrivacyMode::parse(&value);
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS") {
    if let Ok(parsed) = value.parse() {
        cfg.stream_max_buffer_chars = parsed;
    }
}
if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_MAX_BUFFER_MS") {
    if let Ok(parsed) = value.parse() {
        cfg.stream_max_buffer_ms = parsed;
    }
}
```

- [ ] **Step 5: Add stream-aware request pseudonymization**

In `crates/anno-privacy-gateway/src/privacy.rs`, keep the existing method as the strict v0.3 default:

```rust
pub fn pseudonymize_request(&mut self, request: &mut Value) -> Result<PrivacyReport> {
    self.pseudonymize_request_with_streaming(request, false)
}
```

Add this new method next to it:

```rust
/// Pseudonymize request text, optionally allowing `stream=true`.
pub fn pseudonymize_request_with_streaming(
    &mut self,
    request: &mut Value,
    allow_streaming: bool,
) -> Result<PrivacyReport> {
    if request
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !allow_streaming
    {
        return Err(Error::UnsupportedFeature(
            "stream=true is disabled; set ANNO_GATEWAY_STREAMING=enabled".to_string(),
        ));
    }

    reject_blocks(request)?;

    let mut report = PrivacyReport::default();
    if let Some(system) = request.get_mut("system") {
        self.transform_content_value(system, &mut report)?;
    }

    if let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.get_mut("content") {
                self.transform_content_value(content, &mut report)?;
            }
        }
    }

    Ok(report)
}
```

Remove the old duplicated body from `pseudonymize_request`.

- [ ] **Step 6: Add request gate tests**

Add to `crates/anno-privacy-gateway/src/privacy.rs` tests:

```rust
#[test]
fn allows_streaming_when_policy_enabled() {
    let mut engine = PrivacyEngine::default();
    let mut request = json!({
        "model": "claude",
        "stream": true,
        "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
    });

    let report = engine
        .pseudonymize_request_with_streaming(&mut request, true)
        .unwrap();
    let body = serde_json::to_string(&request).unwrap();

    assert_eq!(request["stream"], true);
    assert!(report.entities >= 1);
    assert!(!body.contains("Marie Dupont"));
    assert!(body.contains("PERSON_"));
}
```

- [ ] **Step 7: Run tests and commit**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway streaming_defaults_to_disabled_buffered_scan
cargo test -p anno-privacy-gateway allows_streaming_when_policy_enabled
cargo test -p anno-privacy-gateway rejects_streaming
```

Expected: all PASS.

Commit:

```powershell
git add crates/anno-privacy-gateway/src/config.rs crates/anno-privacy-gateway/src/privacy.rs
git commit -m "feat(anno): add privacy gateway streaming config"
```

---

### Task 2: SSE Frame Model and Parser

**Files:**
- Create: `crates/anno-privacy-gateway/src/stream.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write failing parser tests**

Create `crates/anno-privacy-gateway/src/stream.rs` with tests first:

```rust
//! Anthropic-compatible SSE stream transforms.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_serializes_sse_event() {
        let raw = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Bon\"}}\n\n";

        let event = SseFrame::parse(raw).unwrap();
        assert_eq!(event.event.as_deref(), Some("content_block_delta"));
        assert_eq!(event.data["delta"]["text"], "Bon");
        assert_eq!(event.to_sse(), raw);
    }

    #[test]
    fn rewrites_text_delta_only() {
        let mut event = SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "delta": {"type": "text_delta", "text": "PERSON_1"}
            }),
        };

        assert_eq!(event.text_delta(), Some("PERSON_1"));
        event.set_text_delta("Marie Dupont");
        assert_eq!(event.data["delta"]["text"], "Marie Dupont");
    }
}
```

- [ ] **Step 2: Run parser tests and verify they fail**

Run:

```powershell
cargo test -p anno-privacy-gateway parses_and_serializes_sse_event
cargo test -p anno-privacy-gateway rewrites_text_delta_only
```

Expected: FAIL because `SseFrame` is not defined and `stream` is not exposed.

- [ ] **Step 3: Expose the module**

Add to `crates/anno-privacy-gateway/src/lib.rs`:

```rust
pub mod stream;
```

- [ ] **Step 4: Implement `SseFrame`**

Replace the top of `crates/anno-privacy-gateway/src/stream.rs` with:

```rust
//! Anthropic-compatible SSE stream transforms.

use crate::{Error, Result};
use serde_json::Value;

/// Parsed SSE event with JSON `data`.
#[derive(Debug, Clone, PartialEq)]
pub struct SseFrame {
    /// Optional SSE event name.
    pub event: Option<String>,
    /// JSON payload from `data:`.
    pub data: Value,
}

impl SseFrame {
    /// Parse one complete SSE frame.
    pub fn parse(raw: &str) -> Result<Self> {
        let mut event = None;
        let mut data_lines = Vec::new();

        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.trim_start().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }

        if data_lines.is_empty() {
            return Err(Error::Privacy("SSE frame is missing data".to_string()));
        }

        let data = data_lines.join("\n");
        let data = serde_json::from_str(&data)
            .map_err(|e| Error::Privacy(format!("invalid SSE JSON data: {e}")))?;
        Ok(Self { event, data })
    }

    /// Serialize to one SSE frame.
    #[must_use]
    pub fn to_sse(&self) -> String {
        let mut out = String::new();
        if let Some(event) = &self.event {
            out.push_str("event: ");
            out.push_str(event);
            out.push('\n');
        }
        out.push_str("data: ");
        out.push_str(&self.data.to_string());
        out.push_str("\n\n");
        out
    }

    /// Return assistant text delta when this is a text delta frame.
    #[must_use]
    pub fn text_delta(&self) -> Option<&str> {
        self.data
            .get("delta")
            .and_then(|delta| delta.get("text"))
            .and_then(Value::as_str)
    }

    /// Replace assistant text delta.
    pub fn set_text_delta(&mut self, text: &str) {
        if let Some(delta) = self.data.get_mut("delta").and_then(Value::as_object_mut) {
            delta.insert("text".to_string(), Value::String(text.to_string()));
        }
    }
}
```

Keep the tests below the implementation.

- [ ] **Step 5: Run parser tests and commit**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway parses_and_serializes_sse_event
cargo test -p anno-privacy-gateway rewrites_text_delta_only
```

Expected: both PASS.

Commit:

```powershell
git add crates/anno-privacy-gateway/src/lib.rs crates/anno-privacy-gateway/src/stream.rs
git commit -m "feat(anno): add Anthropic SSE frame model"
```

---

### Task 3: Streaming Privacy Text Transform

**Files:**
- Modify: `crates/anno-privacy-gateway/src/privacy.rs`
- Modify: `crates/anno-privacy-gateway/src/stream.rs`

- [ ] **Step 1: Run GitNexus impact checks**

Run:

```powershell
npx gitnexus impact --repo anno PrivacyEngine
npx gitnexus impact --repo anno SseFrame
```

Expected: LOW risk or no HIGH/CRITICAL warning.

- [ ] **Step 2: Add failing stream text tests**

Add to `crates/anno-privacy-gateway/src/privacy.rs` tests:

```rust
#[test]
fn stream_transform_rehydrates_known_tokens() {
    let mut engine = PrivacyEngine::default();
    let mut request = json!({
        "messages": [{"role": "user", "content": "Marie Dupont"}]
    });
    engine.pseudonymize_request(&mut request).unwrap();
    let token = request["messages"][0]["content"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .find(|part| part.starts_with("PERSON_"))
        .unwrap()
        .to_string();

    let report = engine
        .transform_stream_text(&mut format!("Bonjour {token}"), true)
        .unwrap();

    assert_eq!(report.output, "Bonjour Marie Dupont");
    assert_eq!(report.privacy.rehydrated_tokens, 1);
    assert_eq!(report.privacy.fresh_pii_redacted, 0);
}

#[test]
fn stream_transform_redacts_fresh_pii_when_scanning() {
    let engine = PrivacyEngine::default();
    let report = engine
        .transform_stream_text(
            &mut "Le fournisseur invente Jean Martin et jean.martin@example.com".to_string(),
            true,
        )
        .unwrap();

    assert!(!report.output.contains("Jean Martin"));
    assert!(!report.output.contains("jean.martin@example.com"));
    assert_eq!(report.privacy.fresh_pii_redacted, 2);
}

#[test]
fn stream_transform_can_skip_fresh_pii_scan() {
    let engine = PrivacyEngine::default();
    let report = engine
        .transform_stream_text(
            &mut "Le fournisseur invente Jean Martin".to_string(),
            false,
        )
        .unwrap();

    assert!(report.output.contains("Jean Martin"));
    assert_eq!(report.privacy.fresh_pii_redacted, 0);
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-privacy-gateway stream_transform_
```

Expected: FAIL because `transform_stream_text` and `StreamTextReport` do not exist.

- [ ] **Step 4: Implement `StreamTextReport` and transform method**

In `crates/anno-privacy-gateway/src/privacy.rs`, add after `PrivacyReport`:

```rust
/// Text output and counts emitted by a stream text transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTextReport {
    /// Transformed text safe to emit to Cowork.
    pub output: String,
    /// Privacy counters from this fragment.
    pub privacy: PrivacyReport,
}
```

Add this method to `impl PrivacyEngine`:

```rust
/// Transform a stream text fragment before it is emitted to Cowork.
pub fn transform_stream_text(
    &self,
    text: &mut String,
    scan_fresh_pii: bool,
) -> Result<StreamTextReport> {
    let mut report = PrivacyReport::default();
    if scan_fresh_pii {
        report.fresh_pii_redacted += self.redact_fresh_pii(text);
    }
    let rehydrated = Rehydrator::rehydrate(text, &self.vault)
        .map_err(|e| Error::Privacy(e.to_string()))?;
    report.rehydrated_tokens += rehydrated.rehydrated_count;
    Ok(StreamTextReport {
        output: rehydrated.text,
        privacy: report,
    })
}
```

- [ ] **Step 5: Run tests and commit**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway stream_transform_
```

Expected: all three stream transform tests PASS.

Commit:

```powershell
git add crates/anno-privacy-gateway/src/privacy.rs
git commit -m "feat(anno): add stream text privacy transform"
```

---

### Task 4: Hybrid Stream Buffer

**Files:**
- Modify: `crates/anno-privacy-gateway/src/stream.rs`

- [ ] **Step 1: Add failing buffer tests**

Add to `crates/anno-privacy-gateway/src/stream.rs` tests:

```rust
#[test]
fn buffer_holds_incomplete_pseudonym_token() {
    let mut buffer = StreamBuffer::new(4096);

    assert_eq!(buffer.push("Bonjour PER"), None);
    assert_eq!(buffer.push("SON_1."), Some("Bonjour PERSON_1.".to_string()));
}

#[test]
fn buffer_flushes_complete_sentence() {
    let mut buffer = StreamBuffer::new(4096);

    assert_eq!(buffer.push("Bonjour Marie"), None);
    assert_eq!(buffer.push(" Dupont."), Some("Bonjour Marie Dupont.".to_string()));
}

#[test]
fn buffer_flushes_on_size_limit() {
    let mut buffer = StreamBuffer::new(10);

    assert_eq!(buffer.push("abcdefghijk"), Some("abcdefghijk".to_string()));
}

#[test]
fn timed_flush_does_not_emit_open_pseudonym_prefix() {
    let mut buffer = StreamBuffer::new(4096);

    assert_eq!(buffer.push("Bonjour PER"), None);
    assert_eq!(buffer.flush_if_safe(), None);
    assert_eq!(buffer.push("SON_1"), None);
    assert_eq!(buffer.flush_if_safe(), Some("Bonjour PERSON_1".to_string()));
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test -p anno-privacy-gateway buffer_
```

Expected: FAIL because `StreamBuffer` does not exist.

- [ ] **Step 3: Implement `StreamBuffer`**

Add to `crates/anno-privacy-gateway/src/stream.rs`:

```rust
/// Hybrid buffer for streamed assistant text.
#[derive(Debug, Clone)]
pub struct StreamBuffer {
    pending: String,
    max_chars: usize,
}

impl StreamBuffer {
    /// Build a buffer with a forced flush size.
    #[must_use]
    pub fn new(max_chars: usize) -> Self {
        Self {
            pending: String::new(),
            max_chars,
        }
    }

    /// Push a fragment and return a safe emission unit when available.
    pub fn push(&mut self, fragment: &str) -> Option<String> {
        self.pending.push_str(fragment);
        if ends_with_open_pseudonym_prefix(&self.pending) {
            return None;
        }
        if has_sentence_boundary(&self.pending) || self.pending.len() >= self.max_chars {
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }

    /// Return true when the buffer holds text.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Flush buffered text only when it cannot be an open pseudonym prefix.
    pub fn flush_if_safe(&mut self) -> Option<String> {
        if self.pending.is_empty() || ends_with_open_pseudonym_prefix(&self.pending) {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }

    /// Flush remaining buffered text.
    pub fn finish(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

fn has_sentence_boundary(text: &str) -> bool {
    text.ends_with('.')
        || text.ends_with('!')
        || text.ends_with('?')
        || text.ends_with('\n')
}

fn ends_with_open_pseudonym_prefix(text: &str) -> bool {
    const PREFIXES: &[&str] = &["PERSON_", "EMAIL_", "PHONE_", "IBAN_", "SIRET_"];
    text.char_indices().rev().take(12).any(|(index, _)| {
        let suffix = &text[index..];
        PREFIXES.iter().any(|prefix| prefix.starts_with(suffix))
    })
}
```

- [ ] **Step 4: Run tests and commit**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway buffer_
```

Expected: all buffer tests PASS.

Commit:

```powershell
git add crates/anno-privacy-gateway/src/stream.rs
git commit -m "feat(anno): add streaming privacy buffer"
```

---

### Task 5: Upstream Streaming Client

**Files:**
- Modify: root `Cargo.toml`
- Modify: `crates/anno-privacy-gateway/Cargo.toml`
- Modify: `crates/anno-privacy-gateway/src/upstream.rs`

- [ ] **Step 1: Add dependencies**

Add to root `[workspace.dependencies]`:

```toml
bytes = "1"
futures-util = "0.3"
async-stream = "0.3"
```

Add to `crates/anno-privacy-gateway/Cargo.toml` dependencies:

```toml
async-stream = { workspace = true }
bytes = { workspace = true }
futures-util = { workspace = true }
```

- [ ] **Step 2: Add streaming upstream function**

Add to `crates/anno-privacy-gateway/src/upstream.rs`:

```rust
use futures_util::Stream;

/// Forward a streaming `/v1/messages` request to the configured
/// Anthropic-compatible upstream.
pub async fn forward_messages_stream(
    client: &Client,
    base_url: &str,
    body: &Value,
) -> Result<impl Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>>> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let value = response
            .json::<Value>()
            .await
            .map_err(|e| Error::Upstream(e.to_string()))?;
        return Err(Error::Upstream(value.to_string()));
    }
    Ok(response.bytes_stream())
}
```

- [ ] **Step 3: Compile-check**

Run:

```powershell
cargo check -p anno-privacy-gateway
```

Expected: PASS.

- [ ] **Step 4: Commit**

Commit:

```powershell
git add Cargo.toml Cargo.lock crates/anno-privacy-gateway/Cargo.toml crates/anno-privacy-gateway/src/upstream.rs
git commit -m "feat(anno): add streaming upstream client"
```

---

### Task 6: Streaming Route in `/v1/messages`

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs`
- Modify: `crates/anno-privacy-gateway/src/stream.rs`

- [ ] **Step 1: Run GitNexus impact checks**

Run:

```powershell
npx gitnexus impact --repo anno messages
npx gitnexus impact --repo anno router
npx gitnexus impact --repo anno AppState
```

Expected: LOW risk or no HIGH/CRITICAL warning.

- [ ] **Step 2: Add failing disabled-stream route test**

Add to `crates/anno-privacy-gateway/src/server.rs` tests:

```rust
#[tokio::test]
async fn stream_true_fails_closed_when_disabled() {
    let config = GatewayConfig::default();
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let status = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        }))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
}
```

Run:

```powershell
cargo test -p anno-privacy-gateway stream_true_fails_closed_when_disabled
```

Expected: PASS before route changes because v0.3 rejects stream. Keep it as a regression guard.

- [ ] **Step 3: Add failing enabled-stream test with mock Anthropic SSE**

Add mock handler:

```rust
async fn mock_stream_messages(
    State(state): State<MockState>,
    Json(body): Json<Value>,
) -> axum::response::Sse<impl futures_util::Stream<Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>>> {
    *state.captured.lock().await = Some(body.clone());
    let serialized = serde_json::to_string(&body).expect("request serializes");
    let token = serialized
        .split('"')
        .find(|part| part.starts_with("PERSON_"))
        .unwrap_or("PERSON_1")
        .to_string();

    let stream = futures_util::stream::iter(vec![
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Bonjour "}}).to_string())),
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":token[0..3].to_string()}}).to_string())),
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":token[3..].to_string() + "."}}).to_string())),
        Ok(axum::response::sse::Event::default()
            .event("message_stop")
            .data(json!({"type":"message_stop"}).to_string())),
    ]);
    axum::response::Sse::new(stream)
}
```

Add test:

```rust
#[tokio::test]
async fn stream_route_never_sends_cleartext_and_rehydrates_split_token() {
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/v1/messages", post(mock_stream_messages))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;

    let config = GatewayConfig {
        upstream_anthropic_base: format!("http://{upstream_addr}"),
        streaming: crate::config::StreamingMode::Enabled,
        stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
        stream_max_buffer_chars: 4096,
        stream_max_buffer_ms: 750,
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let response = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(body.contains("Marie Dupont"));
    assert!(!body.contains("PERSON_"));

    let upstream_body = captured.lock().await.clone().expect("upstream called");
    let upstream_text = serde_json::to_string(&upstream_body).unwrap();
    assert!(!upstream_text.contains("Marie Dupont"));
    assert!(upstream_text.contains("PERSON_"));
}
```

Run:

```powershell
cargo test -p anno-privacy-gateway stream_route_never_sends_cleartext_and_rehydrates_split_token
```

Expected: FAIL because streaming route is not implemented.

- [ ] **Step 4: Add response enum for JSON or SSE**

In `server.rs`, add imports:

```rust
use axum::response::{
    sse::{Event, Sse},
    IntoResponse, Response,
};
use futures_util::{Stream, StreamExt};
use std::{convert::Infallible, pin::Pin};
use tokio::time::{timeout, Duration};
```

Add enum near `AppState`:

```rust
type SseResultStream = Pin<Box<dyn Stream<Item = std::result::Result<Event, Infallible>> + Send>>;

enum MessagesResponse {
    Json(HeaderMap, Json<Value>),
    Stream(Sse<SseResultStream>),
}

impl IntoResponse for MessagesResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Json(headers, body) => (headers, body).into_response(),
            Self::Stream(stream) => stream.into_response(),
        }
    }
}
```

Change `messages` return type to:

```rust
) -> Result<MessagesResponse> {
```

Return non-streaming as:

```rust
Ok(MessagesResponse::Json(headers, Json(response)))
```

- [ ] **Step 5: Branch on `stream=true`**

At the top of `messages`, add:

```rust
let wants_stream = body
    .get("stream")
    .and_then(Value::as_bool)
    .unwrap_or(false);

if wants_stream {
    return stream_messages(state, body).await;
}
```

Add `stream_messages`:

```rust
async fn transform_stream_ready_text(
    privacy: &Arc<Mutex<PrivacyEngine>>,
    mut ready: String,
    scan_fresh: bool,
) -> Result<String> {
    let privacy = privacy.lock().await;
    let report = privacy.transform_stream_text(&mut ready, scan_fresh)?;
    Ok(report.output)
}

fn stream_error_event(error_type: &str, message: &str) -> Event {
    Event::default().event("error").data(json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message
        }
    }).to_string())
}

fn stream_text_event(text: String) -> Event {
    Event::default().event("content_block_delta").data(json!({
        "type": "content_block_delta",
        "delta": {
            "type": "text_delta",
            "text": text
        }
    }).to_string())
}

fn passthrough_event(frame: crate::stream::SseFrame) -> Event {
    Event::default()
        .event(frame.event.unwrap_or_else(|| "message".to_string()))
        .data(frame.data.to_string())
}

async fn stream_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    {
        let mut privacy = state.privacy.lock().await;
        privacy.pseudonymize_request_with_streaming(&mut body, state.config.streaming.is_enabled())?;
    }

    let upstream = upstream::forward_messages_stream(
        &state.client,
        &state.config.upstream_anthropic_base,
        &body,
    )
    .await?;

    let scan_fresh = matches!(
        state.config.stream_privacy,
        crate::config::StreamPrivacyMode::BufferedScan
    );
    let max_chars = state.config.stream_max_buffer_chars;
    let max_ms = state.config.stream_max_buffer_ms;
    let privacy = Arc::clone(&state.privacy);
    let stream = async_stream::stream! {
        let mut raw = String::new();
        let mut text_buffer = crate::stream::StreamBuffer::new(max_chars);
        futures_util::pin_mut!(upstream);

        loop {
            let next_chunk = timeout(Duration::from_millis(max_ms), upstream.next()).await;
            let chunk = match next_chunk {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    if let Some(ready) = text_buffer.finish() {
                        match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                            Ok(output) => yield Ok(stream_text_event(output)),
                            Err(_) => yield Ok(stream_error_event("privacy_error", "stream privacy transform failed")),
                        }
                    }
                    return;
                }
                Err(_) => {
                    if let Some(ready) = text_buffer.flush_if_safe() {
                        match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                            Ok(output) => yield Ok(stream_text_event(output)),
                            Err(_) => {
                                yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                return;
                            }
                        }
                    }
                    continue;
                }
            };

            let Ok(bytes) = chunk else {
                yield Ok(stream_error_event("upstream_error", "stream upstream error"));
                return;
            };
            raw.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(frame_end) = raw.find("\n\n") {
                let frame_raw = raw[..frame_end + 2].to_string();
                raw = raw[frame_end + 2..].to_string();

                let Ok(mut frame) = crate::stream::SseFrame::parse(&frame_raw) else {
                    yield Ok(stream_error_event("stream_parse_error", "malformed SSE frame"));
                    return;
                };

                if let Some(text) = frame.text_delta() {
                    if let Some(ready) = text_buffer.push(text) {
                        match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                            Ok(output) => {
                                frame.set_text_delta(&output);
                                yield Ok(Event::default()
                                    .event(frame.event.clone().unwrap_or_else(|| "content_block_delta".to_string()))
                                    .data(frame.data.to_string()));
                            }
                            Err(_) => {
                                yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                return;
                            }
                        }
                    }
                } else {
                    yield Ok(passthrough_event(frame));
                }
            }
        }
    };

    Ok(MessagesResponse::Stream(Sse::new(Box::pin(stream) as SseResultStream)))
}
```

- [ ] **Step 6: Run route tests and commit**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway stream_true_fails_closed_when_disabled
cargo test -p anno-privacy-gateway stream_route_never_sends_cleartext_and_rehydrates_split_token
cargo test -p anno-privacy-gateway messages_route_never_sends_cleartext_to_upstream_and_rehydrates
```

Expected: all PASS.

Commit:

```powershell
git add crates/anno-privacy-gateway/src/server.rs
git commit -m "feat(anno): stream privacy gateway messages"
```

---

### Task 7: Fresh PII Streaming Redaction and Error Event Tests

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs`
- Modify: `crates/anno-privacy-gateway/src/stream.rs`

- [ ] **Step 1: Add fresh PII split test**

Add a mock handler that emits fresh PII split across chunks:

```rust
async fn mock_stream_leaky_messages(
    State(state): State<MockState>,
    Json(body): Json<Value>,
) -> axum::response::Sse<impl futures_util::Stream<Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>>> {
    *state.captured.lock().await = Some(body);
    let stream = futures_util::stream::iter(vec![
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Le fournisseur invente Jean "}}).to_string())),
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data(json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"Martin et jean.martin@example.com."}}).to_string())),
    ]);
    axum::response::Sse::new(stream)
}
```

Add test:

```rust
#[tokio::test]
async fn stream_buffered_scan_redacts_fresh_pii_split_across_chunks() {
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/v1/messages", post(mock_stream_leaky_messages))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;

    let config = GatewayConfig {
        upstream_anthropic_base: format!("http://{upstream_addr}"),
        streaming: crate::config::StreamingMode::Enabled,
        stream_privacy: crate::config::StreamPrivacyMode::BufferedScan,
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let body = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour"}]
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(!body.contains("Jean Martin"));
    assert!(!body.contains("jean.martin@example.com"));
    assert!(body.contains("[REDACTED]"));
}
```

- [ ] **Step 2: Add token-only mode test**

Add test:

```rust
#[tokio::test]
async fn stream_token_rehydrate_only_does_not_scan_fresh_pii() {
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/v1/messages", post(mock_stream_leaky_messages))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;

    let config = GatewayConfig {
        upstream_anthropic_base: format!("http://{upstream_addr}"),
        streaming: crate::config::StreamingMode::Enabled,
        stream_privacy: crate::config::StreamPrivacyMode::TokenRehydrateOnly,
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let body = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour"}]
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("Jean Martin"));
    assert!(body.contains("jean.martin@example.com"));
}
```

- [ ] **Step 3: Add malformed SSE test**

Add a mock handler that emits invalid data:

```rust
async fn mock_stream_malformed_messages(
    State(state): State<MockState>,
    Json(body): Json<Value>,
) -> axum::response::Sse<impl futures_util::Stream<Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>>> {
    *state.captured.lock().await = Some(body);
    let stream = futures_util::stream::iter(vec![
        Ok(axum::response::sse::Event::default()
            .event("content_block_delta")
            .data("{not-json")),
    ]);
    axum::response::Sse::new(stream)
}
```

Add test:

```rust
#[tokio::test]
async fn malformed_stream_emits_error_event() {
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/v1/messages", post(mock_stream_malformed_messages))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;

    let config = GatewayConfig {
        upstream_anthropic_base: format!("http://{upstream_addr}"),
        streaming: crate::config::StreamingMode::Enabled,
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let body = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "claude",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour"}]
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("event: error"));
    assert!(body.contains("stream_parse_error"));
}
```

- [ ] **Step 4: Run tests and adjust implementation**

Run:

```powershell
cargo fmt -p anno-privacy-gateway
cargo test -p anno-privacy-gateway stream_buffered_scan_redacts_fresh_pii_split_across_chunks
cargo test -p anno-privacy-gateway stream_token_rehydrate_only_does_not_scan_fresh_pii
cargo test -p anno-privacy-gateway malformed_stream_emits_error_event
```

If `stream_buffered_scan_redacts_fresh_pii_split_across_chunks` fails because the buffer emits too early, modify `StreamBuffer::push` to emit only after punctuation/newline or size limit, not after whitespace.

- [ ] **Step 5: Commit**

Commit:

```powershell
git add crates/anno-privacy-gateway/src/server.rs crates/anno-privacy-gateway/src/stream.rs
git commit -m "test(anno): cover streaming redaction and errors"
```

---

### Task 8: v0.4 Streaming Smoke Script and Runbook

**Files:**
- Create: `scripts/smoke-privacy-gateway-v0.4-streaming.ps1`
- Create: `docs/runbooks/anno-privacy-gateway-v0.4-streaming.md`

- [ ] **Step 1: Create local mock streaming smoke script**

Create `scripts/smoke-privacy-gateway-v0.4-streaming.ps1` with the same process lifecycle as `scripts/smoke-privacy-gateway-v0.3.ps1`: build the gateway binary, start a hidden Node mock upstream, start the gateway, send a request, assert the captured upstream body and response body, then stop both processes in `finally`.

The v0.4 script-specific content is:

- request body includes `"stream": true`,
- env includes `$env:ANNO_GATEWAY_STREAMING = "enabled"`,
- env includes `$env:ANNO_GATEWAY_STREAM_PRIVACY = "buffered_scan"`,
- mock upstream responds with SSE frames:

```javascript
res.writeHead(200, {
  "content-type": "text/event-stream",
  "cache-control": "no-cache",
  "connection": "keep-alive"
});
res.write(`event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Bonjour "}}\n\n`);
res.write(`event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"${token.slice(0, 3)}"}}\n\n`);
res.write(`event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"${token.slice(3)}. Fuite: Jean "}}\n\n`);
res.write(`event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Martin jean.martin@example.com."}}\n\n`);
res.write(`event: message_stop\ndata: {"type":"message_stop"}\n\n`);
res.end();
```

The script must fail if:

- captured upstream body contains `Marie Dupont`,
- captured upstream body contains `marie.dupont@example.com`,
- response body contains `PERSON_`,
- response body does not contain `Marie Dupont`,
- response body contains `Jean Martin`,
- response body contains `jean.martin@example.com`,
- response body does not contain `[REDACTED]`.

- [ ] **Step 2: Run smoke script and fix failures**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\smoke-privacy-gateway-v0.4-streaming.ps1
```

Expected output includes:

```text
[privacy-gateway-v0.4-streaming] PASS
```

- [ ] **Step 3: Create v0.4 runbook**

Create `docs/runbooks/anno-privacy-gateway-v0.4-streaming.md` with:

````markdown
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
````

- [ ] **Step 4: Commit**

Commit:

```powershell
git add scripts/smoke-privacy-gateway-v0.4-streaming.ps1 docs/runbooks/anno-privacy-gateway-v0.4-streaming.md
git commit -m "docs(anno): add privacy gateway v0.4 streaming smoke"
```

---

### Task 9: Final Verification and GitNexus Refresh

**Files:**
- Modify only if previous tasks surfaced doc drift.

- [ ] **Step 1: Run full gateway verification**

Run:

```powershell
cargo fmt -p anno-privacy-gateway --check
cargo test -p anno-privacy-gateway
cargo check -p anno-privacy-gateway
cargo clippy -p anno-privacy-gateway -- -D warnings
powershell -ExecutionPolicy Bypass -File .\scripts\smoke-privacy-gateway-v0.3.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\smoke-privacy-gateway-v0.4-streaming.ps1
```

Expected:

- all 0.3 tests still pass,
- streaming tests pass,
- v0.3 smoke passes,
- v0.4 streaming smoke passes,
- clippy emits no warnings.

- [ ] **Step 2: Run a secret scan**

Run:

```powershell
rg -n "api_key\s*=|ANTHROPIC_API_KEY|OPENAI_API_KEY|nvapi-|sk-[A-Za-z0-9]|ANNO_GATEWAY_VAULT_KEY_HEX\s*=\s*\"[0-9a-fA-F]" crates\anno-privacy-gateway docs\runbooks scripts\smoke-privacy-gateway-v0.4-streaming.ps1
```

Expected: no matches. Exit code 1 from `rg` is acceptable when no matches are found.

- [ ] **Step 3: Commit final status docs if needed**

If the v0.4 spec or runbook needs final status wording, change only those docs and commit:

```powershell
git add docs\superpowers\specs\2026-05-14-anno-privacy-gateway-v0.4-streaming.md docs\runbooks\anno-privacy-gateway-v0.4-streaming.md
git commit -m "docs(anno): finalize privacy gateway v0.4 streaming"
```

If no docs changed, skip this commit.

- [ ] **Step 4: Refresh and verify GitNexus**

After the last commit, the hook should refresh GitNexus. Verify:

```powershell
npx gitnexus status
```

Expected:

```text
Status: ✅ up-to-date
```

If stale, run:

```powershell
npx gitnexus analyze --force
```

---

## Coverage Review

Spec requirement coverage:

- Streaming disabled by default: Task 1 and Task 6.
- `ANNO_GATEWAY_STREAMING` and stream privacy env config: Task 1.
- `buffered_scan` and `token_rehydrate_only`: Task 3 and Task 7.
- Hybrid buffering: Task 4.
- Time-based safe flush with `stream_max_buffer_ms`: Task 6.
- SSE parser and text delta transform: Task 2 and Task 6.
- Error event after malformed stream: Task 7.
- Sidecar path retained: Task 8 runbook and smoke design.
- v0.3 non-regression: Tasks 6 and 9.
- Final smoke: Task 8 and Task 9.
- GitNexus refresh: Task 9.
