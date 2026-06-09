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
