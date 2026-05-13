//! Error types for the privacy gateway.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

/// Gateway result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Gateway error.
#[derive(Debug, thiserror::Error)]
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
    /// Upstream request failed.
    #[error("upstream request failed: {0}")]
    Upstream(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            Self::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::UnsupportedFeature(_) => StatusCode::BAD_REQUEST,
            Self::Privacy(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
        };
        let body = Json(json!({
            "type": "error",
            "error": {
                "type": status.canonical_reason().unwrap_or("gateway_error"),
                "message": self.to_string(),
            }
        }));
        (status, body).into_response()
    }
}
