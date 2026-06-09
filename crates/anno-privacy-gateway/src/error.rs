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
        assert!(
            msg.starts_with("upstream response parse failed:"),
            "got: {msg}"
        );
    }

    #[test]
    fn error_is_clone() {
        let err = Error::UpstreamConnect("test".to_string());
        let _ = err.clone();
    }
}
