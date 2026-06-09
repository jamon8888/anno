# Gateway Error Hygiene — Audit Remediation (P5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the monolithic `Error::Upstream(String)` variant with typed sub-variants that preserve diagnostic context without leaking upstream JSON to clients.

**Priority:** P2 — quick win, self-contained in `anno-privacy-gateway`.

**Crate touched:** `anno-privacy-gateway` (4 files)

---

## Problem

The `Error` enum in `crates/anno-privacy-gateway/src/error.rs` has a single `Upstream(String)` variant used for 3 fundamentally different failure modes:

1. **Connection failure** — `reqwest::Error` from `.send().await` (network timeout, DNS, TLS)
2. **Non-2xx response** — upstream returned an error status; the raw JSON body is stuffed into the String via `value.to_string()`
3. **Parse failure** — response body could not be parsed as JSON

All 21 call sites flatten the cause to `e.to_string()`, losing:
- The distinction between retryable (connection) vs permanent (auth) failures
- The HTTP status code from the upstream
- The original error chain

Worse: `IntoResponse` at line 27-42 relays the `self.to_string()` directly to the client. When the cause is `value.to_string()` (the raw upstream JSON), the client receives the **full upstream response body**, which may contain API keys, internal URLs, or debug info.

### Call sites (21 total)

| File | Count | Types |
|------|-------|-------|
| `upstream.rs` | 9 | connect (4), status (3), parse (2) |
| `chat.rs` | 4 | status (3), parse (1) |
| `openai_compat.rs` | 6 | connect (2), status (2), parse (2) |
| `server.rs` | 2 | connect (1), parse (1) |

---

## Design

### New error variants

Replace `Upstream(String)` with three variants in `error.rs`:

```rust
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
        /// Sanitized message — never the raw upstream body.
        message: String,
    },

    /// Upstream response could not be parsed.
    #[error("upstream response parse failed: {0}")]
    UpstreamParse(String),
}
```

### Why `String` instead of `#[source] reqwest::Error`

The current `Error` enum is `Clone`. `reqwest::Error` is not `Clone`. Changing to `#[source]` would break the `Clone` derive. Since the gateway clones errors in streaming paths, keep `String` for the cause but use the variant to carry the semantic type.

### Sanitized `IntoResponse`

Update the `IntoResponse` impl so all three upstream variants map to `502 Bad Gateway` with a **generic message** that does NOT include the upstream body:

```rust
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
```

The **full error detail** (including upstream body) is only available via `tracing::error!` at the call site, never sent to the client.

### Call site migration

Each of the 21 call sites gets the appropriate variant:

**Pattern: `.send().await.map_err(...)` → `UpstreamConnect`**
```rust
// Before:
.map_err(|e| Error::Upstream(e.to_string()))?;
// After:
.map_err(|e| Error::UpstreamConnect(e.to_string()))?;
```

**Pattern: `if !status.is_success() { Err(Error::Upstream(value.to_string())) }` → `UpstreamStatus`**
```rust
// Before:
return Err(Error::Upstream(value.to_string()));
// After:
return Err(Error::UpstreamStatus {
    status: status.as_u16(),
    message: status.canonical_reason().unwrap_or("unknown").to_string(),
});
```

Note: the upstream JSON body (`value`) is NOT stored in the error. It is logged via `tracing::warn!` at the call site before returning the error.

**Pattern: `.json::<Value>().await.map_err(...)` → `UpstreamParse`**
```rust
// Before:
.map_err(|e| Error::Upstream(e.to_string()))?;
// After:
.map_err(|e| Error::UpstreamParse(e.to_string()))?;
```

### Tests

1. **Unit test per variant** — verify `IntoResponse` returns 502 and does NOT contain upstream JSON.
2. **Integration: connection failure** — mock upstream that refuses connection → `UpstreamConnect`.
3. **Integration: 401 response** — mock upstream returns 401 with JSON body → `UpstreamStatus { status: 401 }`, body NOT leaked to client.

---

## Files

| File | Change |
|------|--------|
| `crates/anno-privacy-gateway/src/error.rs` | Replace `Upstream(String)` with 3 variants, update `IntoResponse` |
| `crates/anno-privacy-gateway/src/upstream.rs` | Migrate 9 call sites |
| `crates/anno-privacy-gateway/src/chat.rs` | Migrate 4 call sites |
| `crates/anno-privacy-gateway/src/openai_compat.rs` | Migrate 6 call sites (includes format! variants) |
| `crates/anno-privacy-gateway/src/server.rs` | Migrate 2 call sites |

## Non-goals

- **Retry logic based on variant type:** Future work. This spec only structures the errors.
- **Wrapping `reqwest::Error` as source:** Blocked by `Clone` requirement. Not in scope.
- **Rate limiting or circuit breaker:** E4 strategic evolution, separate spec.

## Risk assessment

| Change | Blast radius | Risk |
|--------|-------------|------|
| Error enum variants | All `match` on `Error` in the crate | LOW — mechanical, compiler-guided |
| IntoResponse sanitization | Client-visible error messages change | LOW — messages become MORE generic, not less |
| Call site migration | 21 sites across 4 files | LOW — one-to-one replacement |
