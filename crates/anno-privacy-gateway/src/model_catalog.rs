//! Anthropic-compatible `/v1/models` rendering for Anno provider catalog.

use crate::provider::ProviderCatalog;
use serde_json::{json, Value};

/// Build a model list response accepted by Anthropic-compatible clients.
#[must_use]
pub fn models_response(catalog: &ProviderCatalog) -> Value {
    let data = catalog
        .model_ids()
        .into_iter()
        .map(|id| {
            json!({
                "type": "model",
                "id": id,
                "display_name": id,
                "created_at": "2026-06-06T00:00:00Z",
            })
        })
        .collect::<Vec<_>>();

    json!({
        "type": "list",
        "data": data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderCatalog;

    #[test]
    fn model_catalog_renders_anthropic_models_payload() {
        let catalog = ProviderCatalog::from_toml_str(
            r#"
allow_cleartext_dpa = true
[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "mistral-large-latest", upstream = "mistral-large-latest" }]
"#,
        )
        .expect("catalog");

        let value = models_response(&catalog);
        assert_eq!(value["type"], "list");
        assert_eq!(value["data"].as_array().expect("data").len(), 2);
        assert_eq!(value["data"][0]["type"], "model");
    }
}
