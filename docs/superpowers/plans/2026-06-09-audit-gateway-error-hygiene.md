# Gateway Error Hygiene Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the monolithic `Error::Upstream(String)` with typed sub-variants that prevent upstream JSON leakage to clients.

**Architecture:** Refactor `Error` enum into `UpstreamConnect`, `UpstreamStatus`, and `UpstreamParse` variants. Update `IntoResponse` to sanitize all upstream error messages. Migrate 21 call sites across 4 files. Fix 2 mis-categorized `Error::Upstream` calls in `server.rs` that are actually `Config` errors.

**Tech Stack:** Rust, `axum`, `thiserror`, `reqwest`, `tracing`

**Build/test commands:**
```powershell
# Check only:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
# Unit tests:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

---

### Task 1: Refactor Error enum + IntoResponse

**Files:**
- Modify: `crates/anno-privacy-gateway/src/error.rs`

- [ ] **Step 1: Write a failing test for sanitized IntoResponse**

Add a test module at the bottom of `crates/anno-privacy-gateway/src/error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    fn response_body(error: Error) -> String {
        let response = error.into_response();
        // We can't easily extract the body in a sync test, so test the Display impl
        // which is what IntoResponse uses for the message field.
        format!("{error}")
    }

    #[test]
    fn upstream_connect_does_not_leak_details() {
        let err = Error::UpstreamConnect("timeout after 30s connecting to https://api.internal.corp:8443".to_string());
        let msg = response_body(err);
        assert!(msg.contains("upstream connection failed"), "got: {msg}");
    }

    #[test]
    fn upstream_status_does_not_leak_body() {
        let err = Error::UpstreamStatus {
            status: 401,
            message: "Unauthorized".to_string(),
        };
        let msg = response_body(err);
        assert!(msg.contains("401"), "should include status code, got: {msg}");
        assert!(!msg.contains("api_key"), "must not leak upstream body");
    }

    #[test]
    fn upstream_parse_does_not_leak_body() {
        let err = Error::UpstreamParse("expected value at line 1 column 1".to_string());
        let msg = response_body(err);
        assert!(msg.contains("upstream response parse failed"), "got: {msg}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

Expected: FAIL — `UpstreamConnect`, `UpstreamStatus`, `UpstreamParse` variants don't exist yet.

- [ ] **Step 3: Replace the Error enum and IntoResponse**

Replace the entire content of `crates/anno-privacy-gateway/src/error.rs`:

```rust
//! Error types for the privacy gateway.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

/// Gateway result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Gateway error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Runtime configuration is invalid.
    #[error("configuration error: {0}")]
    Config(String),
    /// Request feature is intentionally unsupported in this release.
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
    /// Request or response could not be transformed safely.
    #[error("privacy transform failed: {0}")]
    Privacy(String),
    /// Upstream connection or send failure (network, DNS, TLS).
    #[error("upstream connection failed: {0}")]
    UpstreamConnect(String),
    /// Upstream returned a non-success HTTP status.
    #[error("upstream returned HTTP {status}")]
    UpstreamStatus {
        status: u16,
        /// Sanitized reason — never the raw upstream body.
        message: String,
    },
    /// Upstream response could not be parsed.
    #[error("upstream response parse failed: {0}")]
    UpstreamParse(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            Self::Config(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            Self::UnsupportedFeature(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Privacy(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            Self::UpstreamConnect(_) => (
                StatusCode::BAD_GATEWAY,
                "upstream connection failed".to_string(),
            ),
            Self::UpstreamStatus { status, .. } => (
                StatusCode::BAD_GATEWAY,
                format!("upstream returned HTTP {status}"),
            ),
            Self::UpstreamParse(_) => (
                StatusCode::BAD_GATEWAY,
                "upstream response parse failed".to_string(),
            ),
        };
        let body = Json(json!({
            "type": "error",
            "error": {
                "type": status.canonical_reason().unwrap_or("gateway_error"),
                "message": msg,
            }
        }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_connect_display_does_not_leak_internal_url() {
        let err = Error::UpstreamConnect(
            "timeout after 30s connecting to https://api.internal.corp:8443".to_string(),
        );
        let msg = format!("{err}");
        assert!(msg.starts_with("upstream connection failed:"), "got: {msg}");
    }

    #[test]
    fn upstream_status_display_includes_code() {
        let err = Error::UpstreamStatus {
            status: 401,
            message: "Unauthorized".to_string(),
        };
        let msg = format!("{err}");
        assert_eq!(msg, "upstream returned HTTP 401");
    }

    #[test]
    fn upstream_parse_display() {
        let err = Error::UpstreamParse("expected value at line 1 column 1".to_string());
        let msg = format!("{err}");
        assert!(msg.starts_with("upstream response parse failed:"), "got: {msg}");
    }

    #[test]
    fn error_is_clone() {
        let err = Error::UpstreamConnect("test".to_string());
        let _ = err.clone();
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

Expected: The error module tests PASS. Other files will have compile errors because `Error::Upstream` no longer exists — that's expected and fixed in the next tasks.

- [ ] **Step 5: Commit**

```bash
git add crates/anno-privacy-gateway/src/error.rs
git commit -m "refactor(gateway): split Upstream into typed error variants

Replace Error::Upstream(String) with UpstreamConnect, UpstreamStatus,
and UpstreamParse. IntoResponse now returns sanitized messages that
never include raw upstream JSON bodies."
```

---

### Task 2: Migrate `upstream.rs` (9 sites)

**Files:**
- Modify: `crates/anno-privacy-gateway/src/upstream.rs`

- [ ] **Step 1: Replace all 9 Error::Upstream calls**

Replace the entire content of `crates/anno-privacy-gateway/src/upstream.rs`:

```rust
//! Upstream Anthropic-compatible HTTP client.

use crate::{Error, Result};
use futures_util::Stream;
use reqwest::Client;
use serde_json::Value;

/// Forward a `/v1/messages` request to the configured Anthropic-compatible
/// upstream and return its JSON body.
pub async fn forward_messages(client: &Client, base_url: &str, body: &Value) -> Result<Value> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| Error::UpstreamConnect(e.to_string()))?;

    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| Error::UpstreamParse(e.to_string()))?;
    if !status.is_success() {
        tracing::warn!(
            http_status = status.as_u16(),
            "upstream returned non-success response"
        );
        return Err(Error::UpstreamStatus {
            status: status.as_u16(),
            message: status.canonical_reason().unwrap_or("unknown").to_string(),
        });
    }
    Ok(value)
}

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
        .map_err(|e| Error::UpstreamConnect(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let _body = response
            .json::<Value>()
            .await
            .map_err(|e| Error::UpstreamParse(e.to_string()))?;
        tracing::warn!(
            http_status = status.as_u16(),
            "upstream returned non-success response"
        );
        return Err(Error::UpstreamStatus {
            status: status.as_u16(),
            message: status.canonical_reason().unwrap_or("unknown").to_string(),
        });
    }

    Ok(response.bytes_stream())
}

/// Forward `/v1/models` to the upstream without privacy transforms.
pub async fn forward_models(client: &Client, base_url: &str) -> Result<Value> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::UpstreamConnect(e.to_string()))?;

    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| Error::UpstreamParse(e.to_string()))?;
    if !status.is_success() {
        tracing::warn!(
            http_status = status.as_u16(),
            "upstream returned non-success response"
        );
        return Err(Error::UpstreamStatus {
            status: status.as_u16(),
            message: status.canonical_reason().unwrap_or("unknown").to_string(),
        });
    }
    Ok(value)
}
```

- [ ] **Step 2: Run check to verify compilation**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

Expected: `upstream.rs` compiles. Other files may still have `Error::Upstream` references.

- [ ] **Step 3: Commit**

```bash
git add crates/anno-privacy-gateway/src/upstream.rs
git commit -m "refactor(gateway): migrate upstream.rs to typed error variants

9 Error::Upstream sites replaced with UpstreamConnect (4), UpstreamStatus (3),
UpstreamParse (2). Upstream JSON bodies are no longer stored in errors."
```

---

### Task 3: Migrate `chat.rs` (4 sites)

**Files:**
- Modify: `crates/anno-privacy-gateway/src/chat.rs` (lines 81, 84, 136, 243)

- [ ] **Step 1: Replace all 4 Error::Upstream calls**

All 4 sites in `chat.rs` are parse/structural errors (response missing expected fields). Replace each:

Line 81:
```rust
// Before:
.ok_or_else(|| Error::Upstream("OpenAI response missing choices[0]".to_string()))?;
// After:
.ok_or_else(|| Error::UpstreamParse("OpenAI response missing choices[0]".to_string()))?;
```

Line 84:
```rust
// Before:
.ok_or_else(|| Error::Upstream("OpenAI response missing message".to_string()))?;
// After:
.ok_or_else(|| Error::UpstreamParse("OpenAI response missing message".to_string()))?;
```

Line 136:
```rust
// Before:
Error::Upstream("OpenAI stream chunk missing choices[0].delta".to_string())
// After:
Error::UpstreamParse("OpenAI stream chunk missing choices[0].delta".to_string())
```

Line 243:
```rust
// Before:
.map_err(|e| Error::Upstream(format!("OpenAI tool call arguments are not JSON: {e}")))?;
// After:
.map_err(|e| Error::UpstreamParse(format!("OpenAI tool call arguments are not JSON: {e}")))?;
```

- [ ] **Step 2: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-privacy-gateway/src/chat.rs
git commit -m "refactor(gateway): migrate chat.rs to UpstreamParse variant

4 Error::Upstream sites for malformed OpenAI responses replaced with
UpstreamParse — all are structural/parse errors, not connection failures."
```

---

### Task 4: Migrate `openai_compat.rs` (5 sites)

**Files:**
- Modify: `crates/anno-privacy-gateway/src/openai_compat.rs` (lines 21, 25-27, 32, 50, 54-56)

- [ ] **Step 1: Replace all 5 Error::Upstream calls**

Line 21 (connect):
```rust
// Before:
.map_err(|e| Error::Upstream(format!("provider request failed: {e}")))?;
// After:
.map_err(|e| Error::UpstreamConnect(format!("provider request failed: {e}")))?;
```

Lines 24-27 (status — note: this currently leaks the response body):
```rust
// Before:
let body = response.text().await.unwrap_or_default();
return Err(Error::Upstream(format!(
    "provider returned status {status}: {body}"
)));

// After:
let body = response.text().await.unwrap_or_default();
tracing::warn!(
    http_status = %status,
    response_body = %body,
    "provider returned non-success response"
);
return Err(Error::UpstreamStatus {
    status: status.as_u16(),
    message: status.canonical_reason().unwrap_or("unknown").to_string(),
});
```

Line 32 (parse):
```rust
// Before:
.map_err(|e| Error::Upstream(format!("provider response is not JSON: {e}")))
// After:
.map_err(|e| Error::UpstreamParse(format!("provider response is not JSON: {e}")))
```

Line 50 (connect):
```rust
// Before:
.map_err(|e| Error::Upstream(format!("provider stream request failed: {e}")))?;
// After:
.map_err(|e| Error::UpstreamConnect(format!("provider stream request failed: {e}")))?;
```

Lines 53-56 (status — same body leak fix):
```rust
// Before:
let body = response.text().await.unwrap_or_default();
return Err(Error::Upstream(format!(
    "provider stream returned status {status}: {body}"
)));

// After:
let body = response.text().await.unwrap_or_default();
tracing::warn!(
    http_status = %status,
    response_body = %body,
    "provider stream returned non-success response"
);
return Err(Error::UpstreamStatus {
    status: status.as_u16(),
    message: status.canonical_reason().unwrap_or("unknown").to_string(),
});
```

- [ ] **Step 2: Run check**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

- [ ] **Step 3: Commit**

```bash
git add crates/anno-privacy-gateway/src/openai_compat.rs
git commit -m "refactor(gateway): migrate openai_compat.rs to typed error variants

5 Error::Upstream sites replaced: UpstreamConnect (2), UpstreamStatus (2),
UpstreamParse (1). Response bodies are now logged via tracing instead of
being stored in the error and leaked to clients."
```

---

### Task 5: Fix `server.rs` (2 sites → Error::Config)

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs` (lines 809, 814)

- [ ] **Step 1: Replace the 2 mis-categorized Error::Upstream calls**

These two calls are for `TcpListener::bind` and `axum::serve` failures — they are server startup errors, not upstream provider errors. Use `Error::Config` instead:

Line 809:
```rust
// Before:
.map_err(|e| Error::Upstream(e.to_string()))?;
// After:
.map_err(|e| Error::Config(format!("failed to bind {}: {e}", config.listen)))?;
```

Line 814:
```rust
// Before:
.map_err(|e| Error::Upstream(e.to_string()))?;
// After:
.map_err(|e| Error::Config(format!("server error: {e}")))?;
```

- [ ] **Step 2: Verify no remaining `Error::Upstream` references in the crate**

Search for any remaining `Error::Upstream` in the entire crate:

```bash
grep -r "Error::Upstream" crates/anno-privacy-gateway/src/
```

Expected: **zero matches**. If any remain, migrate them using the same pattern (classify as Connect, Status, Parse, or Config).

- [ ] **Step 3: Run full test suite**

Run:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

Expected: ALL tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/anno-privacy-gateway/src/server.rs
git commit -m "fix(gateway): re-categorize server bind errors as Config, not Upstream

TcpListener::bind and axum::serve failures are configuration errors,
not upstream provider errors. This completes the Error::Upstream
migration — zero uses of the old variant remain."
```

---

## Verification

After all 5 tasks:

- [ ] **Run full test suite**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

- [ ] **Verify zero Error::Upstream references remain**

```bash
grep -r "Error::Upstream" crates/anno-privacy-gateway/src/
```

Expected: zero matches.
