//! Upstream Anthropic-compatible HTTP client.

use crate::{Error, Result};
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
        .map_err(|e| Error::Upstream(e.to_string()))?;

    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;
    if !status.is_success() {
        return Err(Error::Upstream(value.to_string()));
    }
    Ok(value)
}

/// Forward `/v1/models` to the upstream without privacy transforms.
pub async fn forward_models(client: &Client, base_url: &str) -> Result<Value> {
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;

    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| Error::Upstream(e.to_string()))?;
    if !status.is_success() {
        return Err(Error::Upstream(value.to_string()));
    }
    Ok(value)
}
