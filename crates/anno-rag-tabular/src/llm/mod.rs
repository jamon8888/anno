//! LLM client trait — abstracts the structured-JSON generation call
//! the extraction engine relies on. Real backend lives in
//! [`anthropic`]; tests use [`mock::MockLlm`] for determinism.
//!
//! The trait stays small on purpose: one method (`generate_structured`)
//! plus an introspection accessor (`model_id`). Provider-specific
//! features (prompt caching, streaming, tool use) live behind the
//! trait — callers above just see "give me JSON conforming to this
//! schema, here's the user message, here's the system prompt."

pub mod anthropic;
pub mod local;
pub mod mock;
pub mod privacy;
pub mod routing;

use async_trait::async_trait;
use serde_json::Value;

/// Output of a single structured-generation call.
///
/// `value` is the parsed JSON the provider produced (already validated
/// against `json_schema` if the provider supports constrained decoding;
/// otherwise the caller is responsible for schema-checking).
///
/// `usage` carries token accounting — split out for prompt caching so
/// callers can attribute spend to cache hits vs cache writes.
#[derive(Debug, Clone)]
pub struct StructuredOutput {
    /// Parsed JSON returned by the provider.
    pub value: Value,
    /// Token usage breakdown for this call.
    pub usage: Usage,
}

/// Token accounting for one call. All counts are zero-default so mocks
/// and providers without cache reporting can leave the cache fields at
/// 0 without ceremony.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// Input tokens billed at the standard rate.
    pub input_tokens: u32,
    /// Output tokens billed at the standard rate.
    pub output_tokens: u32,
    /// Input tokens served from the provider's prompt cache.
    pub cache_read_tokens: u32,
    /// Input tokens that wrote into the prompt cache this call.
    pub cache_create_tokens: u32,
}

/// One LLM call abstraction. Implementors must be `Send + Sync` so the
/// extraction engine can fan out across tokio tasks holding an
/// `Arc<dyn LlmClient>`.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Ask the provider to produce JSON matching `json_schema`.
    ///
    /// The `system` prompt carries cacheable instructions (extractor
    /// playbook, schema description); the `user` message carries the
    /// document body + per-column prompts. Splitting them this way is
    /// what lets the Anthropic impl flag the `system` block for
    /// prompt caching.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`](crate::error::Error) on transport failure,
    /// auth error, malformed JSON, or schema-validation mismatch.
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput>;

    /// Stable identifier for the model behind this client — used in
    /// audit logs and `Author::System { extractor_version }`.
    fn model_id(&self) -> &str;
}

/// Resolve the default LLM client from environment + OS keyring.
///
/// Resolution order:
/// 1. `ANTHROPIC_API_KEY` env var (override for CI, scripted runs,
///    or local dev where you want to bypass the keyring).
/// 2. OS keyring entry under service `anno-rag`, user `anthropic`
///    (set via `anno-rag config set-llm-key`, mirroring the vault-key
///    pattern in `anno-rag::vault`).
/// 3. Error.
///
/// Returns a boxed [`LlmClient`] so callers don't have to name the
/// concrete provider — swapping Anthropic for another backend in v1.x
/// stays a one-line change here.
///
/// # Errors
///
/// Returns [`Error::Extract`] with `doc = "config"` when:
/// - The keyring entry cannot be opened (OS-level keyring failure).
/// - No keyring entry exists and `ANTHROPIC_API_KEY` is unset.
pub fn default_from_env() -> crate::error::Result<Box<dyn LlmClient>> {
    use crate::error::Error;

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(Box::new(anthropic::AnthropicLlm::new(key)));
    }
    let entry = keyring::Entry::new("anno-rag", "anthropic").map_err(|e| Error::Extract {
        doc: "config".into(),
        col: "?".into(),
        source: Box::new(e),
    })?;
    let key = entry.get_password().map_err(|e| Error::Extract {
        doc: "config".into(),
        col: "?".into(),
        source: Box::new(e),
    })?;
    Ok(Box::new(anthropic::AnthropicLlm::new(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_from_env_picks_up_env_var() {
        // SAFETY: we set and immediately remove the env var. Other
        // tests touching ANTHROPIC_API_KEY would race; none do today.
        // SAFETY block needed because `set_var`/`remove_var` are
        // `unsafe` since Rust 1.78 (mutating process-global state).
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        let c = default_from_env().expect("env path must resolve");
        assert_eq!(c.model_id(), "claude-sonnet-4-6");
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }
}
