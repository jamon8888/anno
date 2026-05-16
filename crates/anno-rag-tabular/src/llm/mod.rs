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
pub mod mock;

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
