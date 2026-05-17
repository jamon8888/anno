//! Live Anthropic test — gated by `--ignored` and a real
//! `ANTHROPIC_API_KEY`. Not run in CI; here so a human can sanity-check
//! the wire format and prompt-caching path against the real API.

use anno_rag_tabular::llm::{anthropic::AnthropicLlm, LlmClient};
use serde_json::json;

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn live_structured_extraction() {
    let key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");
    let llm = AnthropicLlm::new(key);

    let schema = json!({
        "type": "object",
        "required": ["country"],
        "properties": { "country": { "type": "string" } }
    });

    let out = llm
        .generate_structured(
            "Answer with exact JSON matching the schema.",
            "Paris is the capital of which country?",
            &schema,
        )
        .await
        .expect("live call succeeds");

    assert_eq!(out.value["country"], "France");
}
