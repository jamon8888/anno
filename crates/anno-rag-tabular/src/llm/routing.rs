//! Routing LLM client — runs local extraction first, then optionally
//! calls a fallback LLM for columns the local engine cannot handle.
//!
//! The merge policy is "local wins": if the local client already produced
//! a value for a column, the fallback result for that column is ignored.
//! Fallback calls are skipped entirely when `fallback_prompt_is_safe`
//! returns `false` (raw PII detected in the prompt).

use crate::llm::{privacy::fallback_prompt_is_safe, LlmClient, StructuredOutput};
use async_trait::async_trait;
use serde_json::Value;

/// LLM client that runs local extraction first and optionally falls back to a remote LLM.
pub struct RoutingLlmClient {
    local: Box<dyn LlmClient>,
    fallback: Option<Box<dyn LlmClient>>,
}

impl RoutingLlmClient {
    /// Create a routing client with a mandatory `local` client and an optional `fallback`.
    pub fn new(local: Box<dyn LlmClient>, fallback: Option<Box<dyn LlmClient>>) -> Self {
        Self { local, fallback }
    }
}

#[async_trait]
impl LlmClient for RoutingLlmClient {
    async fn generate_structured(
        &self,
        system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        let mut local_out =
            self.local.generate_structured(system, user, json_schema).await?;

        if let Some(fallback) = &self.fallback {
            // Safety gate: never send clear PII to a remote LLM.
            if !fallback_prompt_is_safe(user) {
                return Ok(local_out);
            }
            let llm_out = fallback.generate_structured(system, user, json_schema).await?;
            // Merge: local columns take priority; fallback fills the gaps.
            merge_objects(&mut local_out.value, llm_out.value);
            // Aggregate token usage.
            local_out.usage.input_tokens += llm_out.usage.input_tokens;
            local_out.usage.output_tokens += llm_out.usage.output_tokens;
            local_out.usage.cache_read_tokens += llm_out.usage.cache_read_tokens;
            local_out.usage.cache_create_tokens += llm_out.usage.cache_create_tokens;
        }

        Ok(local_out)
    }

    fn model_id(&self) -> &str {
        "routing-local-tabular"
    }
}

/// Merge `src` object into `dst` without overwriting existing keys.
fn merge_objects(dst: &mut Value, src: Value) {
    let Some(dst_obj) = dst.as_object_mut() else { return };
    let Some(src_obj) = src.into_object() else { return };
    for (key, value) in src_obj {
        dst_obj.entry(key).or_insert(value);
    }
}

trait IntoObject {
    fn into_object(self) -> Option<serde_json::Map<String, Value>>;
}
impl IntoObject for Value {
    fn into_object(self) -> Option<serde_json::Map<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmClient, StructuredOutput, Usage};
    use async_trait::async_trait;
    use serde_json::json;

    struct StaticClient {
        id: &'static str,
        value: Value,
    }

    #[async_trait]
    impl LlmClient for StaticClient {
        async fn generate_structured(
            &self,
            _system: &str,
            _user: &str,
            _json_schema: &Value,
        ) -> crate::error::Result<StructuredOutput> {
            Ok(StructuredOutput { value: self.value.clone(), usage: Usage::default() })
        }
        fn model_id(&self) -> &str {
            self.id
        }
    }

    #[tokio::test]
    async fn routing_merges_local_and_llm_outputs_without_raw_pii() {
        let local = Box::new(StaticClient {
            id: "local",
            value: json!({
                "landlord": { "value": "ACME SAS", "reasoning": "local", "citations": [] }
            }),
        });
        let llm = Box::new(StaticClient {
            id: "llm",
            value: json!({
                "repair_obligations": {
                    "value": "gross repairs", "reasoning": "llm", "citations": []
                }
            }),
        });
        let router = RoutingLlmClient::new(local, Some(llm));

        let user =
            "[CHUNK::018f0000-0000-7000-8000-000000000001]ORG_1 signe le contrat.[/CHUNK]\n";
        let out = router
            .generate_structured("", user, &json!({ "type": "object" }))
            .await
            .expect("route");

        assert_eq!(out.value["landlord"]["value"], "ACME SAS");
        assert_eq!(out.value["repair_obligations"]["value"], "gross repairs");
    }

    #[tokio::test]
    async fn routing_aborts_fallback_when_prompt_contains_clear_pii() {
        let local = Box::new(StaticClient { id: "local", value: json!({}) });
        let llm = Box::new(StaticClient {
            id: "llm",
            value: json!({
                "unsafe": {
                    "value": "should not appear", "reasoning": "llm", "citations": []
                }
            }),
        });
        let router = RoutingLlmClient::new(local, Some(llm));

        let user =
            "[CHUNK::018f0000-0000-7000-8000-000000000001]Contact: marie.dupont@example.com[/CHUNK]\n";
        let out = router
            .generate_structured("", user, &json!({ "type": "object" }))
            .await
            .expect("route");

        assert!(out.value.as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn local_value_not_overwritten_by_fallback() {
        let local = Box::new(StaticClient {
            id: "local",
            value: json!({
                "landlord": { "value": "local-wins", "reasoning": "local", "citations": [] }
            }),
        });
        let llm = Box::new(StaticClient {
            id: "llm",
            value: json!({
                "landlord": { "value": "fallback-loses", "reasoning": "llm", "citations": [] }
            }),
        });
        let router = RoutingLlmClient::new(local, Some(llm));

        let user = "[CHUNK::018f0000-0000-7000-8000-000000000001]ORG_1.[/CHUNK]\n";
        let out = router
            .generate_structured("", user, &json!({ "type": "object" }))
            .await
            .expect("route");

        assert_eq!(out.value["landlord"]["value"], "local-wins");
    }
}
