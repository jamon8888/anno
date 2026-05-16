//! Anthropic API [`LlmClient`](super::LlmClient) implementation.
//!
//! Two provider-specific tricks live here:
//!
//! 1. **Prompt caching.** The `system` block is tagged with
//!    `cache_control: { type: "ephemeral" }` so repeated extractions
//!    sharing the same extractor playbook only pay for the system
//!    prompt once per 5-min window. Per-cell `user` messages stay
//!    uncached — they're document-specific.
//!
//! 2. **Forced `tool_use` for constrained JSON.** Anthropic's
//!    `tool_choice: { type: "tool", name: ... }` is the canonical way
//!    to coerce structured output: we declare one tool whose
//!    `input_schema` is the caller's JSON schema and the model is
//!    obliged to call it. The tool input *is* the JSON we want — no
//!    free-form text parsing.
//!
//! Errors at this layer are wrapped in [`Error::Extract`] with
//! `doc: "?"` / `col: "?"` placeholders — the caller (extraction
//! engine) knows the real document and column ids and re-wraps if it
//! wants richer attribution. Keeping the placeholders here avoids
//! threading those ids through `LlmClient::generate_structured`.

use super::{LlmClient, StructuredOutput, Usage};
use crate::error::{Error, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};
use std::fmt;

/// Anthropic Messages API endpoint.
const API: &str = "https://api.anthropic.com/v1/messages";
/// Anthropic API version pinned in the `anthropic-version` header.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Default model. Per the v1.1 ADR for tabular-review; do not
/// substitute the current "latest" alias here without an ADR update.
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Real Anthropic-backed [`LlmClient`].
///
/// Cheap to clone (Arc'd reqwest client + a String key + a String
/// model). Construct via [`AnthropicLlm::new`] and optionally override
/// the model with [`AnthropicLlm::with_model`].
pub struct AnthropicLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl AnthropicLlm {
    /// Build a client using [`DEFAULT_MODEL`].
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: DEFAULT_MODEL.into(),
        }
    }

    /// Override the model id (e.g. for a benchmarking run against a
    /// different Anthropic model).
    #[must_use]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.into();
        self
    }
}

/// Tiny private error type so we can box a string-shaped failure into
/// [`Error::Extract::source`] without depending on `std::io::Error` or
/// inventing an ad-hoc enum variant.
#[derive(Debug)]
struct ApiError(String);

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ApiError {}

#[async_trait]
impl LlmClient for AnthropicLlm {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> Result<StructuredOutput> {
        // One tool, forced. The model's only legal move is to call
        // `emit_cells` with input that conforms to `json_schema`.
        let body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": [
                {
                    "type": "text",
                    "text": system,
                    "cache_control": { "type": "ephemeral" }
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": [{ "type": "text", "text": user }]
                }
            ],
            "tools": [{
                "name": "emit_cells",
                "description": "Emit extracted cell values for the requested columns.",
                "input_schema": json_schema
            }],
            "tool_choice": { "type": "tool", "name": "emit_cells" }
        });

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).map_err(|e| Error::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: Box::new(e),
            })?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let resp = self
            .client
            .post(API)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: Box::new(e),
            })?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Extract {
                doc: "?".into(),
                col: "?".into(),
                source: Box::new(ApiError(format!("anthropic {body}"))),
            });
        }

        // Parse as untyped Value. A typed struct can't easily model the
        // content-block sum-type (text vs tool_use) without manual
        // tagged-enum work; for one read site, direct navigation is
        // simpler and lets us survive future block-type additions.
        let body: Value = resp.json().await.map_err(|e| Error::Extract {
            doc: "?".into(),
            col: "?".into(),
            source: Box::new(e),
        })?;

        // Find the first content block whose type == "tool_use" and
        // pluck its `input` (the JSON the model produced for our tool).
        let tool_input = body
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            })
            .and_then(|b| b.get("input"))
            .cloned()
            .ok_or_else(|| Error::SchemaMismatch {
                expected: "content[].type=tool_use with input".into(),
                got: body.to_string(),
            })?;

        // Usage: input/output are always present; cache_* may be
        // absent on cache-miss responses, so default to 0.
        let usage_v = body.get("usage").cloned().unwrap_or(Value::Null);
        let u32_at = |key: &str| -> u32 {
            usage_v
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        };

        Ok(StructuredOutput {
            value: tool_input,
            usage: Usage {
                input_tokens: u32_at("input_tokens"),
                output_tokens: u32_at("output_tokens"),
                cache_read_tokens: u32_at("cache_read_input_tokens"),
                cache_create_tokens: u32_at("cache_creation_input_tokens"),
            },
        })
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

// Live integration test lives in tests/anthropic_live.rs (ignored;
// run with `--ignored` and `ANTHROPIC_API_KEY` set).
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builder_sets_model() {
        let c = AnthropicLlm::new("test".into()).with_model("claude-opus-4-7");
        assert_eq!(c.model_id(), "claude-opus-4-7");
    }

    #[test]
    fn body_includes_cache_control_on_system() {
        // The body shape we send must put cache_control: ephemeral on
        // the system block; this is what unlocks prompt caching.
        let system = "You are an extractor.";
        let body = json!({
            "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }]
        });
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    }
}
