//! Provider catalog, model-id resolution, and DPA gates.

use crate::privacy_mode::PrivacyMode;

/// Provider protocol kind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub enum ProviderKind {
    /// OpenAI-compatible chat completions API.
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

/// One model exposed through one provider.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ProviderModel {
    /// Anno-visible model segment.
    pub id: String,
    /// Upstream provider model id.
    pub upstream: String,
}

/// One configured provider.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ProviderConfig {
    /// Stable provider id used in `anno/<provider>/<model>:<mode>`.
    pub id: String,
    /// Provider protocol.
    pub kind: ProviderKind,
    /// OpenAI-compatible base URL.
    pub base_url: String,
    /// Environment variable that contains the provider API key. Empty for local providers.
    #[serde(default)]
    pub api_key_env: String,
    /// Administrator-verified DPA flag.
    #[serde(default)]
    pub dpa_verified: bool,
    /// Allowed privacy modes using TOML labels: `pseudonymized`, `cleartext_dpa`, `cleartext_local`.
    pub allowed_privacy_modes: Vec<PrivacyMode>,
    /// Exposed models.
    pub models: Vec<ProviderModel>,
}

/// Provider catalog loaded at startup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ProviderCatalog {
    /// Deployment-wide gate for `cleartext_dpa`.
    #[serde(default)]
    pub allow_cleartext_dpa: bool,
    /// Configured providers.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// Resolved provider/model/privacy mode for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    /// Provider config.
    pub provider: ProviderConfig,
    /// Upstream model id.
    pub upstream_model: String,
    /// Selected privacy mode.
    pub privacy_mode: PrivacyMode,
    /// Anno-visible request model id.
    pub requested_model: String,
}

impl ProviderCatalog {
    /// Parse a provider catalog TOML string.
    pub fn from_toml_str(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("parse provider catalog: {e}"))
    }

    /// Load a provider catalog from a TOML file.
    pub fn from_toml_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read provider catalog {}: {e}", path.display()))?;
        Self::from_toml_str(&text)
    }

    /// Load a provider catalog from a TOML file.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        Self::from_toml_path(path)
    }

    /// Anno-visible model ids exposed by this catalog.
    #[must_use]
    pub fn model_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for provider in &self.providers {
            for model in &provider.models {
                for mode in &provider.allowed_privacy_modes {
                    if self.validate_mode(provider, *mode).is_ok() {
                        ids.push(model_id(&provider.id, &model.id, *mode));
                    }
                }
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    /// Resolve an Anno-visible model id into provider/model/mode routing metadata.
    pub fn resolve_model(&self, model_id: &str) -> Result<ResolvedModel, String> {
        let (provider_id, model_segment, privacy_mode) = parse_model_id(model_id)?;
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| format!("unknown provider in model id: {provider_id}"))?;
        if !provider.allowed_privacy_modes.contains(&privacy_mode) {
            return Err(format!(
                "privacy mode {} is not allowed for provider {}",
                privacy_mode.suffix(),
                provider.id
            ));
        }
        self.validate_mode(provider, privacy_mode)?;
        let model = provider
            .models
            .iter()
            .find(|model| model.id == model_segment)
            .ok_or_else(|| {
                format!(
                    "unknown model for provider {}: {model_segment}",
                    provider.id
                )
            })?;

        Ok(ResolvedModel {
            provider: provider.clone(),
            upstream_model: model.upstream.clone(),
            privacy_mode,
            requested_model: model_id.to_string(),
        })
    }

    fn validate_mode(
        &self,
        provider: &ProviderConfig,
        privacy_mode: PrivacyMode,
    ) -> Result<(), String> {
        match privacy_mode {
            PrivacyMode::Pseudonymized => Ok(()),
            PrivacyMode::CleartextDpa => {
                if !self.allow_cleartext_dpa {
                    return Err("cleartext-dpa requires allow_cleartext_dpa=true".to_string());
                }
                if !provider.dpa_verified {
                    return Err(format!(
                        "cleartext-dpa requires provider {} dpa_verified=true",
                        provider.id
                    ));
                }
                Ok(())
            }
            PrivacyMode::CleartextLocal => {
                if !provider.is_local() {
                    return Err(format!(
                        "cleartext-local requires a local provider: {}",
                        provider.id
                    ));
                }
                Ok(())
            }
        }
    }
}

impl ProviderConfig {
    /// Return true when the provider endpoint is local-only.
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.id == "local"
            || self.base_url.starts_with("http://127.0.0.1:")
            || self.base_url.starts_with("http://localhost:")
            || self.base_url.starts_with("http://[::1]:")
    }
}

fn model_id(provider: &str, model: &str, privacy_mode: PrivacyMode) -> String {
    format!("anno/{provider}/{model}:{}", privacy_mode.suffix())
}

fn parse_model_id(model_id: &str) -> Result<(&str, &str, PrivacyMode), String> {
    let rest = model_id
        .strip_prefix("anno/")
        .ok_or_else(|| "provider model id must start with anno/".to_string())?;
    let (provider_id, model_and_mode) = rest
        .split_once('/')
        .ok_or_else(|| "provider model id must be anno/<provider>/<model>:<mode>".to_string())?;
    let (model_segment, suffix) = model_and_mode
        .rsplit_once(':')
        .ok_or_else(|| "provider model id must include :<privacy-mode>".to_string())?;
    let privacy_mode = PrivacyMode::from_suffix(suffix)
        .ok_or_else(|| format!("unsupported privacy mode: {suffix}"))?;
    if provider_id.is_empty() || model_segment.is_empty() {
        return Err("provider model id has empty provider or model segment".to_string());
    }
    Ok((provider_id, model_segment, privacy_mode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;

    #[test]
    fn provider_catalog_loads_and_expands_model_ids() {
        let text = r#"
allow_cleartext_dpa = true

[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "mistral-large-latest", upstream = "mistral-large-latest" }]
"#;
        let catalog = ProviderCatalog::from_toml_str(text).expect("catalog");
        let models = catalog.model_ids();
        assert!(models.contains(&"anno/mistral/mistral-large-latest:pseudonymized".to_string()));
        assert!(models.contains(&"anno/mistral/mistral-large-latest:cleartext-dpa".to_string()));
    }

    #[test]
    fn provider_catalog_rejects_cleartext_dpa_without_verified_provider() {
        let text = r#"
allow_cleartext_dpa = true

[[providers]]
id = "ovh"
kind = "openai_compatible"
base_url = "https://oai.endpoints.kepler.ai.cloud.ovh.net/v1"
api_key_env = "OVH_AI_ENDPOINTS_ACCESS_TOKEN"
dpa_verified = false
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "ovh-chat", upstream = "ovh-chat" }]
"#;
        let catalog = ProviderCatalog::from_toml_str(text).expect("catalog");
        let err = catalog
            .resolve_model("anno/ovh/ovh-chat:cleartext-dpa")
            .expect_err("dpa gate");
        assert!(err.contains("dpa_verified"));
    }

    #[test]
    fn provider_catalog_allows_cleartext_local_only_for_local_provider() {
        let text = r#"
allow_cleartext_dpa = false

[[providers]]
id = "local"
kind = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
api_key_env = ""
dpa_verified = false
allowed_privacy_modes = ["pseudonymized", "cleartext_local"]
models = [{ id = "llama-local", upstream = "llama-local" }]
"#;
        let catalog = ProviderCatalog::from_toml_str(text).expect("catalog");
        let resolved = catalog
            .resolve_model("anno/local/llama-local:cleartext-local")
            .expect("local cleartext");
        assert_eq!(resolved.privacy_mode, PrivacyMode::CleartextLocal);
        assert_eq!(resolved.provider.id, "local");
    }
}
