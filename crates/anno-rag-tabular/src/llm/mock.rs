//! Deterministic in-memory [`LlmClient`] used by tests. Responses are
//! keyed by a **prefix of the user prompt** (`user.starts_with(key)`),
//! so extraction-engine tests can stage responses by their stable
//! opening text without knowing the full prompt.

use super::{LlmClient, StructuredOutput, Usage};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock LLM that returns canned responses keyed by a prefix of the
/// `user` argument. A response is returned when `user.starts_with(key)`
/// holds for some registered key. If multiple keys match, the **longest**
/// wins (longer = more specific). Anything that doesn't match falls
/// back to [`Self::default`].
pub struct MockLlm {
    /// Prefix → canned response map. `Mutex` because `add_response`
    /// mutates after construction (e.g. from inside a test fn).
    pub responses: Mutex<HashMap<String, Value>>,
    /// Returned when no registered prefix matches.
    pub default: Value,
}

impl MockLlm {
    /// Construct with a fallback response.
    #[must_use]
    pub fn new(default: Value) -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
            default,
        }
    }

    /// Register a canned response for any `user` argument starting with
    /// `key`. Later registrations overwrite earlier ones for the same key.
    pub fn add_response(&self, key: &str, value: Value) {
        self.responses
            .lock()
            .expect("MockLlm responses mutex poisoned")
            .insert(key.to_string(), value);
    }
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn generate_structured(
        &self,
        _system: &str,
        user: &str,
        _json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        let guard = self
            .responses
            .lock()
            .expect("MockLlm responses mutex poisoned");
        // Longest-prefix-wins: more-specific stagings override broader ones.
        let val = guard
            .iter()
            .filter(|(k, _)| user.starts_with(k.as_str()))
            .max_by_key(|(k, _)| k.len())
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| self.default.clone());
        Ok(StructuredOutput {
            value: val,
            usage: Usage::default(),
        })
    }

    fn model_id(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn returns_default_when_no_match() {
        let m = MockLlm::new(json!({"k": "v"}));
        let out = m
            .generate_structured("sys", "user prompt", &json!({}))
            .await
            .expect("mock never errors");
        assert_eq!(out.value, json!({"k": "v"}));
    }

    #[tokio::test]
    async fn returns_specific_response_when_user_prefix_matches() {
        let m = MockLlm::new(json!({"default": true}));
        m.add_response("Extract NDA fields from this", json!({"matched": true}));
        let out = m
            .generate_structured(
                "sys",
                "Extract NDA fields from this NDA doc XYZ",
                &json!({}),
            )
            .await
            .expect("mock never errors");
        assert_eq!(out.value, json!({"matched": true}));
    }

    #[tokio::test]
    async fn usage_is_zeroed_by_default() {
        let m = MockLlm::new(json!(null));
        let out = m
            .generate_structured("sys", "anything", &json!({}))
            .await
            .expect("mock never errors");
        assert_eq!(out.usage.input_tokens, 0);
        assert_eq!(out.usage.cache_read_tokens, 0);
    }

    #[test]
    fn model_id_is_stable() {
        let m = MockLlm::new(json!(null));
        assert_eq!(m.model_id(), "mock");
    }
}
