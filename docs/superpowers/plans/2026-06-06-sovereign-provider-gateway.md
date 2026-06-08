# Sovereign Provider Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `anno-privacy-gateway` from a single Anthropic-compatible upstream proxy into a provider-routed gateway with explicit `pseudonymized`, `cleartext_dpa`, and `cleartext_local` model IDs for Mistral, Scaleway, OVHcloud, and local OpenAI-compatible providers.

**Architecture:** Keep the current legacy upstream path as the default when no provider catalog is configured. Add a data-driven provider catalog, a privacy-mode resolver, OpenAI-compatible request/response adapters, DPA-gated cleartext mode, and safe audit metadata. Split new responsibilities into focused modules instead of growing `server.rs`.

**Tech Stack:** Rust 2021, Axum 0.8, Reqwest, Tokio, Serde JSON, TOML, `cloakpipe-core`, existing SSE parser/buffer, existing audit sink, GitNexus CLI, `scripts/dev-fast.ps1`.

**Spec:** [`docs/superpowers/specs/2026-06-06-cowork-3p-sovereign-gateway-design.md`](../specs/2026-06-06-cowork-3p-sovereign-gateway-design.md)

---

## Code Review Findings

- `crates/anno-privacy-gateway/src/config.rs` has a single `upstream_anthropic_base` and a string `provider_profile`; no provider catalog exists.
- `crates/anno-privacy-gateway/src/server.rs` owns routing, privacy transform, upstream calls, model proxying, and streaming in one file.
- `crates/anno-privacy-gateway/src/upstream.rs` only forwards Anthropic-compatible `/v1/messages`, `/v1/models`, and stream bytes.
- `crates/anno-privacy-gateway/src/privacy.rs` always pseudonymizes supported request text and rejects `document`/`image` blocks. It can already transform `tool_use.input` string leaves and rehydrate response/tool-use string leaves.
- `crates/anno-privacy-gateway/src/stream.rs` parses Anthropic SSE frames and buffers response text, but `server.rs` currently rejects streaming `input_json_delta`.
- `crates/anno-privacy-gateway/src/audit.rs` has a content-free audit event with only request id, provider profile, entity count, and fresh PII redaction count. It needs provider/model/privacy mode fields for DPA mode.
- Existing tests in `server.rs` use in-process Axum mock upstreams and should be reused for provider routing tests.

## Scope Check

This plan implements Phase 2 only. It does not add `/v1/files`, base64 document extraction, URL document fetching, OCR/image redaction, or provider-native sanitized upload. Keep all `document` and `/v1/files` behavior fail-closed until the separate file-ingress plan lands.

## File Map

Create:

- `crates/anno-privacy-gateway/src/privacy_mode.rs` - `PrivacyMode`, model suffix parsing, and privacy-mode display names.
- `crates/anno-privacy-gateway/src/provider.rs` - provider catalog structs, TOML loading, model resolution, DPA gating.
- `crates/anno-privacy-gateway/src/chat.rs` - provider-neutral chat request/response structs and Anthropic/OpenAI conversion helpers.
- `crates/anno-privacy-gateway/src/openai_compat.rs` - OpenAI-compatible HTTP adapter for Mistral, Scaleway, OVHcloud, and local endpoints.
- `crates/anno-privacy-gateway/src/model_catalog.rs` - `/v1/models` response builder for provider-mode model IDs.

Modify:

- `crates/anno-privacy-gateway/Cargo.toml` - add `toml = { workspace = true }`; add `uuid = { workspace = true }` only if request id generation uses UUID.
- `crates/anno-privacy-gateway/src/lib.rs` - expose new modules.
- `crates/anno-privacy-gateway/src/config.rs` - add provider catalog path/env and DPA cleartext global flag.
- `crates/anno-privacy-gateway/src/privacy.rs` - add mode-aware request/response transform wrappers.
- `crates/anno-privacy-gateway/src/audit.rs` - extend `AuditEvent` safely.
- `crates/anno-privacy-gateway/src/server.rs` - choose legacy path or provider-router path, render model catalog, and route messages through adapter.
- `docs/developers/gateway-api.md` - document provider catalog and model IDs.
- `docs/user-guide/privacy-gateway.md` - document privacy modes and DPA cleartext guardrails.

Do not modify:

- `crates/anno-rag-mcp/*`
- `/v1/files` routes beyond preserving fail-closed behavior
- `document` block behavior beyond preserving fail-closed behavior

## Build And Test Commands

Run before edits:

```powershell
Get-Process cargo,rustc -ErrorAction SilentlyContinue
npx gitnexus status
```

Targeted checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

Targeted tests:

```powershell
cargo test -p anno-privacy-gateway privacy_mode -- --nocapture
cargo test -p anno-privacy-gateway provider_catalog -- --nocapture
cargo test -p anno-privacy-gateway model_catalog -- --nocapture
cargo test -p anno-privacy-gateway openai_compat -- --nocapture
cargo test -p anno-privacy-gateway provider_router -- --nocapture
```

Final package test:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

---

### Task 0: Pre-Flight And Impact Checks

**Files:** none.

- [ ] **Step 1: Confirm worktree and index**

Run:

```powershell
git status --short --branch
npx gitnexus status
```

Expected: known worktree state and up-to-date GitNexus index. If stale, run:

```powershell
npx gitnexus analyze
```

- [ ] **Step 2: Run impact checks**

Run:

```powershell
npx gitnexus impact --repo anno GatewayConfig --direction upstream
npx gitnexus impact --repo anno AppState --direction upstream
npx gitnexus impact --repo anno messages --direction upstream
npx gitnexus impact --repo anno stream_messages --direction upstream
npx gitnexus impact --repo anno PrivacyEngine --direction upstream
npx gitnexus impact --repo anno AuditEvent --direction upstream
```

Expected: record blast radius. Stop and report if HIGH or CRITICAL appears.

---

### Task 1: Privacy Mode Types

**Files:**
- Create: `crates/anno-privacy-gateway/src/privacy_mode.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/anno-privacy-gateway/src/privacy_mode.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_suffix_privacy_modes() {
        assert_eq!(
            PrivacyMode::from_suffix("pseudonymized"),
            Some(PrivacyMode::Pseudonymized)
        );
        assert_eq!(
            PrivacyMode::from_suffix("cleartext-dpa"),
            Some(PrivacyMode::CleartextDpa)
        );
        assert_eq!(
            PrivacyMode::from_suffix("cleartext-local"),
            Some(PrivacyMode::CleartextLocal)
        );
        assert_eq!(PrivacyMode::from_suffix("cleartext"), None);
    }

    #[test]
    fn display_uses_gateway_model_suffix() {
        assert_eq!(PrivacyMode::Pseudonymized.suffix(), "pseudonymized");
        assert_eq!(PrivacyMode::CleartextDpa.suffix(), "cleartext-dpa");
        assert_eq!(PrivacyMode::CleartextLocal.suffix(), "cleartext-local");
    }
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway privacy_mode -- --nocapture
```

Expected: FAIL because `PrivacyMode` does not exist.

- [ ] **Step 3: Implement the type**

Replace the file with:

```rust
//! Privacy mode selected by an Anno gateway model id.

/// Privacy transform policy for one provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// Default regulated mode. Pseudonymize request content before upstream.
    Pseudonymized,
    /// Cleartext to a DPA-verified remote provider.
    CleartextDpa,
    /// Cleartext to a local provider only.
    CleartextLocal,
}

impl PrivacyMode {
    /// Parse the model-id suffix used by `/v1/models`.
    #[must_use]
    pub fn from_suffix(value: &str) -> Option<Self> {
        match value {
            "pseudonymized" => Some(Self::Pseudonymized),
            "cleartext-dpa" => Some(Self::CleartextDpa),
            "cleartext-local" => Some(Self::CleartextLocal),
            _ => None,
        }
    }

    /// Stable suffix used in Anno-visible model ids.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Pseudonymized => "pseudonymized",
            Self::CleartextDpa => "cleartext-dpa",
            Self::CleartextLocal => "cleartext-local",
        }
    }

    /// Stable audit label.
    #[must_use]
    pub fn audit_label(self) -> &'static str {
        self.suffix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_suffix_privacy_modes() {
        assert_eq!(
            PrivacyMode::from_suffix("pseudonymized"),
            Some(PrivacyMode::Pseudonymized)
        );
        assert_eq!(
            PrivacyMode::from_suffix("cleartext-dpa"),
            Some(PrivacyMode::CleartextDpa)
        );
        assert_eq!(
            PrivacyMode::from_suffix("cleartext-local"),
            Some(PrivacyMode::CleartextLocal)
        );
        assert_eq!(PrivacyMode::from_suffix("cleartext"), None);
    }

    #[test]
    fn display_uses_gateway_model_suffix() {
        assert_eq!(PrivacyMode::Pseudonymized.suffix(), "pseudonymized");
        assert_eq!(PrivacyMode::CleartextDpa.suffix(), "cleartext-dpa");
        assert_eq!(PrivacyMode::CleartextLocal.suffix(), "cleartext-local");
    }
}
```

- [ ] **Step 4: Expose the module**

In `crates/anno-privacy-gateway/src/lib.rs`, add:

```rust
pub mod privacy_mode;
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-privacy-gateway privacy_mode -- --nocapture
```

Expected: PASS.

---

### Task 2: Provider Catalog And DPA Gating

**Files:**
- Create: `crates/anno-privacy-gateway/src/provider.rs`
- Modify: `crates/anno-privacy-gateway/Cargo.toml`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`
- Modify: `crates/anno-privacy-gateway/src/config.rs`

- [ ] **Step 1: Add dependency**

In `crates/anno-privacy-gateway/Cargo.toml`, add:

```toml
toml = { workspace = true }
```

- [ ] **Step 2: Write failing provider tests**

Create `crates/anno-privacy-gateway/src/provider.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;

    #[test]
    fn loads_catalog_and_expands_model_ids() {
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
    fn rejects_cleartext_dpa_without_verified_provider() {
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
    fn allows_cleartext_local_only_for_local_provider() {
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
```

- [ ] **Step 3: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_catalog -- --nocapture
```

Expected: FAIL because `ProviderCatalog` does not exist.

- [ ] **Step 4: Implement catalog structs**

Replace `provider.rs` with:

```rust
//! Provider catalog, model-id resolution, and DPA gates.

use crate::privacy_mode::PrivacyMode;

/// Provider protocol kind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// OpenAI-compatible chat completions API.
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
    /// Original gateway model id.
    pub gateway_model_id: String,
}

impl ProviderCatalog {
    /// Parse a TOML catalog string.
    pub fn from_toml_str(text: &str) -> Result<Self, String> {
        let catalog: Self = toml::from_str(text).map_err(|e| format!("provider catalog: {e}"))?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Load catalog from file.
    pub fn from_path(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read provider catalog {}: {e}", path.display()))?;
        Self::from_toml_str(&text)
    }

    /// Return true when provider routing is configured.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.providers.is_empty()
    }

    /// All model ids exposed to Cowork.
    #[must_use]
    pub fn model_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for provider in &self.providers {
            for model in &provider.models {
                for mode in &provider.allowed_privacy_modes {
                    ids.push(format!(
                        "anno/{}/{}:{}",
                        provider.id,
                        model.id,
                        mode.suffix()
                    ));
                }
            }
        }
        ids.sort();
        ids
    }

    /// Resolve an Anno gateway model id.
    pub fn resolve_model(&self, id: &str) -> Result<ResolvedModel, String> {
        let Some(rest) = id.strip_prefix("anno/") else {
            return Err(format!("unknown gateway model id: {id}"));
        };
        let Some((provider_and_model, suffix)) = rest.rsplit_once(':') else {
            return Err(format!("model id must include privacy suffix: {id}"));
        };
        let privacy_mode = PrivacyMode::from_suffix(suffix)
            .ok_or_else(|| format!("unsupported privacy mode suffix: {suffix}"))?;
        let Some((provider_id, model_id)) = provider_and_model.split_once('/') else {
            return Err(format!("model id must be anno/<provider>/<model>:<mode>: {id}"));
        };

        let provider = self
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| format!("unknown provider id: {provider_id}"))?;
        if !provider.allowed_privacy_modes.contains(&privacy_mode) {
            return Err(format!(
                "privacy mode {} is not allowed for provider {}",
                privacy_mode.suffix(),
                provider.id
            ));
        }
        if privacy_mode == PrivacyMode::CleartextDpa {
            if !self.allow_cleartext_dpa {
                return Err("cleartext_dpa is disabled by gateway policy".to_string());
            }
            if !provider.dpa_verified {
                return Err(format!(
                    "provider {} must set dpa_verified=true for cleartext_dpa",
                    provider.id
                ));
            }
        }
        if privacy_mode == PrivacyMode::CleartextLocal && provider.id != "local" {
            return Err("cleartext_local is allowed only for provider id 'local'".to_string());
        }

        let model = provider
            .models
            .iter()
            .find(|model| model.id == model_id)
            .ok_or_else(|| format!("unknown model {model_id} for provider {provider_id}"))?;

        Ok(ResolvedModel {
            provider: provider.clone(),
            upstream_model: model.upstream.clone(),
            privacy_mode,
            gateway_model_id: id.to_string(),
        })
    }

    fn validate(&self) -> Result<(), String> {
        let mut provider_ids = std::collections::HashSet::new();
        for provider in &self.providers {
            if provider.id.trim().is_empty() {
                return Err("provider id must not be empty".to_string());
            }
            if !provider_ids.insert(provider.id.as_str()) {
                return Err(format!("duplicate provider id: {}", provider.id));
            }
            if provider.base_url.trim().is_empty() {
                return Err(format!("provider {} base_url must not be empty", provider.id));
            }
            if provider.allowed_privacy_modes.is_empty() {
                return Err(format!(
                    "provider {} must allow at least one privacy mode",
                    provider.id
                ));
            }
            if provider.models.is_empty() {
                return Err(format!("provider {} must expose at least one model", provider.id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy_mode::PrivacyMode;

    #[test]
    fn loads_catalog_and_expands_model_ids() {
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
    fn rejects_cleartext_dpa_without_verified_provider() {
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
    fn allows_cleartext_local_only_for_local_provider() {
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
```

- [ ] **Step 5: Expose module**

In `lib.rs`, add:

```rust
pub mod provider;
```

- [ ] **Step 6: Add catalog path to runtime config**

In `GatewayConfig`, add:

```rust
/// Optional provider catalog TOML path. If absent, legacy upstream proxy mode is used.
pub provider_catalog_path: Option<std::path::PathBuf>,
```

In `Default`, add:

```rust
provider_catalog_path: None,
```

In `from_env`, add:

```rust
if let Ok(path) = std::env::var("ANNO_GATEWAY_PROVIDER_CATALOG") {
    let path = path.trim();
    if !path.is_empty() {
        cfg.provider_catalog_path = Some(std::path::PathBuf::from(path));
    }
}
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_catalog -- --nocapture
cargo test -p anno-privacy-gateway streaming_defaults_to_disabled_buffered_scan -- --nocapture
```

Expected: PASS.

---

### Task 3: Model Catalog Route

**Files:**
- Create: `crates/anno-privacy-gateway/src/model_catalog.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`
- Modify: `crates/anno-privacy-gateway/src/server.rs`

- [ ] **Step 1: Write failing model catalog test**

Create `crates/anno-privacy-gateway/src/model_catalog.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderCatalog;

    #[test]
    fn renders_anthropic_models_payload() {
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
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway model_catalog -- --nocapture
```

Expected: FAIL because `models_response` does not exist.

- [ ] **Step 3: Implement renderer**

Replace `model_catalog.rs` with:

```rust
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
    fn renders_anthropic_models_payload() {
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
```

- [ ] **Step 4: Expose module**

In `lib.rs`, add:

```rust
pub mod model_catalog;
```

- [ ] **Step 5: Add catalog to app state**

In `AppState`, add:

```rust
provider_catalog: Option<crate::provider::ProviderCatalog>,
```

In `try_new`, before `Ok(Self { ... })`, add:

```rust
let provider_catalog = match &config.provider_catalog_path {
    Some(path) => Some(
        crate::provider::ProviderCatalog::from_path(path)
            .map_err(crate::Error::Config)?,
    ),
    None => None,
};
```

Then include `provider_catalog` in `Self`.

- [ ] **Step 6: Route models through catalog when configured**

Change `models` in `server.rs`:

```rust
async fn models(State(state): State<AppState>) -> Result<Json<Value>> {
    if let Some(catalog) = &state.provider_catalog {
        return Ok(Json(crate::model_catalog::models_response(catalog)));
    }
    upstream::forward_models(&state.client, &state.config.upstream_anthropic_base)
        .await
        .map(Json)
}
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test -p anno-privacy-gateway model_catalog -- --nocapture
```

Expected: PASS.

---

### Task 4: Chat Normalization And OpenAI-Compatible Adapter

**Files:**
- Create: `crates/anno-privacy-gateway/src/chat.rs`
- Create: `crates/anno-privacy-gateway/src/openai_compat.rs`
- Modify: `crates/anno-privacy-gateway/src/lib.rs`

- [ ] **Step 1: Write chat conversion tests**

Create `crates/anno-privacy-gateway/src/chat.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_request_converts_to_openai_messages() {
        let input = json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "system": "Tu es juriste.",
            "messages": [{"role": "user", "content": "Bonjour PERSON_1"}],
            "max_tokens": 128,
            "temperature": 0.2
        });

        let request = ChatRequest::from_anthropic(&input, "mistral-large-latest")
            .expect("request");
        let openai = request.to_openai_json();

        assert_eq!(openai["model"], "mistral-large-latest");
        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["messages"][1]["role"], "user");
        assert_eq!(openai["max_tokens"], 128);
    }

    #[test]
    fn openai_text_response_renders_anthropic_content() {
        let upstream = json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4}
        });

        let response = anthropic_response_from_openai(&upstream).expect("response");

        assert_eq!(response["content"][0]["type"], "text");
        assert_eq!(response["content"][0]["text"], "Bonjour PERSON_1");
    }
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway chat -- --nocapture
```

Expected: FAIL because conversion types do not exist.

- [ ] **Step 3: Implement minimal chat conversion**

Replace `chat.rs` with:

```rust
//! Provider-neutral chat request and Anthropic/OpenAI JSON conversion.

use crate::{Error, Result};
use serde_json::{json, Value};

/// Provider-neutral chat request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// Upstream model id.
    pub model: String,
    /// OpenAI-shaped messages.
    pub messages: Vec<Value>,
    /// Original request with non-message settings.
    pub original: Value,
}

impl ChatRequest {
    /// Convert supported Anthropic Messages fields to OpenAI-compatible chat completions.
    pub fn from_anthropic(input: &Value, upstream_model: &str) -> Result<Self> {
        let mut messages = Vec::new();
        if let Some(system) = input.get("system").and_then(Value::as_str) {
            messages.push(json!({"role": "system", "content": system}));
        }

        let Some(input_messages) = input.get("messages").and_then(Value::as_array) else {
            return Err(Error::Privacy("messages must be an array".to_string()));
        };
        for message in input_messages {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Privacy("message role missing".to_string()))?;
            let content = anthropic_content_to_text(message.get("content").unwrap_or(&Value::Null));
            messages.push(json!({"role": role, "content": content}));
        }

        Ok(Self {
            model: upstream_model.to_string(),
            messages,
            original: input.clone(),
        })
    }

    /// Render OpenAI chat completions JSON.
    #[must_use]
    pub fn to_openai_json(&self) -> Value {
        let mut out = json!({
            "model": self.model,
            "messages": self.messages,
        });
        for key in ["max_tokens", "temperature", "top_p", "stream"] {
            if let Some(value) = self.original.get(key) {
                out[key] = value.clone();
            }
        }
        if let Some(tools) = self.original.get("tools") {
            out["tools"] = anthropic_tools_to_openai(tools);
        }
        out
    }
}

fn anthropic_content_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") | Some("thinking") => block.get("text").and_then(Value::as_str),
                Some("tool_result") => block.get("content").and_then(Value::as_str),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn anthropic_tools_to_openai(tools: &Value) -> Value {
    let Some(items) = tools.as_array() else {
        return Value::Null;
    };
    Value::Array(
        items
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").cloned().unwrap_or(Value::Null),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": tool.get("input_schema").cloned().unwrap_or_else(|| json!({"type":"object"}))
                    }
                })
            })
            .collect(),
    )
}

/// Convert an OpenAI-compatible non-streaming response to Anthropic Messages JSON.
pub fn anthropic_response_from_openai(value: &Value) -> Result<Value> {
    let message = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| Error::Upstream("OpenAI response missing choices[0].message".to_string()))?;

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        let content = tool_calls
            .iter()
            .map(|call| {
                let function = call.get("function").unwrap_or(&Value::Null);
                json!({
                    "type": "tool_use",
                    "id": call.get("id").and_then(Value::as_str).unwrap_or("toolu_anno"),
                    "name": function.get("name").and_then(Value::as_str).unwrap_or("tool"),
                    "input": function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or_else(|| json!({}))
                })
            })
            .collect::<Vec<_>>();
        return Ok(json!({
            "type": "message",
            "role": "assistant",
            "content": content,
            "stop_reason": "tool_use"
        }));
    }

    let text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_request_converts_to_openai_messages() {
        let input = json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "system": "Tu es juriste.",
            "messages": [{"role": "user", "content": "Bonjour PERSON_1"}],
            "max_tokens": 128,
            "temperature": 0.2
        });

        let request = ChatRequest::from_anthropic(&input, "mistral-large-latest")
            .expect("request");
        let openai = request.to_openai_json();

        assert_eq!(openai["model"], "mistral-large-latest");
        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["messages"][1]["role"], "user");
        assert_eq!(openai["max_tokens"], 128);
    }

    #[test]
    fn openai_text_response_renders_anthropic_content() {
        let upstream = json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4}
        });

        let response = anthropic_response_from_openai(&upstream).expect("response");

        assert_eq!(response["content"][0]["type"], "text");
        assert_eq!(response["content"][0]["text"], "Bonjour PERSON_1");
    }
}
```

- [ ] **Step 4: Create OpenAI-compatible adapter**

Create `crates/anno-privacy-gateway/src/openai_compat.rs`:

```rust
//! OpenAI-compatible provider adapter.

use crate::{chat::ChatRequest, provider::ResolvedModel, Error, Result};
use futures_util::Stream;
use reqwest::Client;
use serde_json::Value;

/// Send non-streaming chat completion to an OpenAI-compatible provider.
pub async fn complete(client: &Client, resolved: &ResolvedModel, request: &ChatRequest) -> Result<Value> {
    let url = format!("{}/chat/completions", resolved.provider.base_url.trim_end_matches('/'));
    let mut builder = client.post(url).json(&request.to_openai_json());
    if !resolved.provider.api_key_env.trim().is_empty() {
        let key = std::env::var(&resolved.provider.api_key_env).map_err(|_| {
            Error::Config(format!(
                "provider {} requires env {}",
                resolved.provider.id, resolved.provider.api_key_env
            ))
        })?;
        builder = builder.bearer_auth(key);
    }
    let response = builder.send().await.map_err(|e| Error::Upstream(e.to_string()))?;
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

/// Send streaming chat completion to an OpenAI-compatible provider.
pub async fn stream(
    client: &Client,
    resolved: &ResolvedModel,
    request: &ChatRequest,
) -> Result<impl Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>>> {
    let url = format!("{}/chat/completions", resolved.provider.base_url.trim_end_matches('/'));
    let mut body = request.to_openai_json();
    body["stream"] = Value::Bool(true);
    let mut builder = client.post(url).json(&body);
    if !resolved.provider.api_key_env.trim().is_empty() {
        let key = std::env::var(&resolved.provider.api_key_env).map_err(|_| {
            Error::Config(format!(
                "provider {} requires env {}",
                resolved.provider.id, resolved.provider.api_key_env
            ))
        })?;
        builder = builder.bearer_auth(key);
    }
    let response = builder.send().await.map_err(|e| Error::Upstream(e.to_string()))?;
    if !response.status().is_success() {
        return Err(Error::Upstream(format!("provider stream status {}", response.status())));
    }
    Ok(response.bytes_stream())
}
```

- [ ] **Step 5: Expose modules**

In `lib.rs`, add:

```rust
pub mod chat;
pub mod openai_compat;
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test -p anno-privacy-gateway chat -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
```

Expected: PASS.

---

### Task 5: Mode-Aware Privacy And Audit

**Files:**
- Modify: `crates/anno-privacy-gateway/src/privacy.rs`
- Modify: `crates/anno-privacy-gateway/src/audit.rs`
- Modify: `crates/anno-privacy-gateway/src/server.rs`

- [ ] **Step 1: Add privacy wrapper tests**

In `privacy.rs` test module, add:

```rust
#[test]
fn cleartext_dpa_validates_blocks_without_pseudonymizing() {
    let mut engine = PrivacyEngine::default();
    let mut request = json!({
        "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
    });
    let report = engine
        .transform_request_for_mode(&mut request, crate::privacy_mode::PrivacyMode::CleartextDpa, false)
        .expect("cleartext allowed");
    assert_eq!(report.pseudonymized_values, 0);
    assert!(serde_json::to_string(&request).unwrap().contains("Marie Dupont"));
}

#[test]
fn pseudonymized_mode_pseudonymizes() {
    let mut engine = PrivacyEngine::default();
    let mut request = json!({
        "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
    });
    let report = engine
        .transform_request_for_mode(&mut request, crate::privacy_mode::PrivacyMode::Pseudonymized, false)
        .expect("pseudonymized");
    assert!(report.pseudonymized_values > 0);
    assert!(!serde_json::to_string(&request).unwrap().contains("Marie Dupont"));
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway cleartext_dpa_validates_blocks_without_pseudonymizing -- --nocapture
cargo test -p anno-privacy-gateway pseudonymized_mode_pseudonymizes -- --nocapture
```

Expected: both commands FAIL until wrapper exists.

- [ ] **Step 3: Implement wrapper**

In `impl PrivacyEngine`, add:

```rust
/// Transform a request according to selected privacy mode.
pub fn transform_request_for_mode(
    &mut self,
    request: &mut Value,
    mode: crate::privacy_mode::PrivacyMode,
    allow_streaming: bool,
) -> Result<PrivacyReport> {
    match mode {
        crate::privacy_mode::PrivacyMode::Pseudonymized => {
            self.pseudonymize_request_with_streaming(request, allow_streaming)
        }
        crate::privacy_mode::PrivacyMode::CleartextDpa
        | crate::privacy_mode::PrivacyMode::CleartextLocal => {
            if request
                .get("stream")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && !allow_streaming
            {
                return Err(Error::UnsupportedFeature(
                    "stream=true is disabled; set ANNO_GATEWAY_STREAMING=enabled".to_string(),
                ));
            }
            reject_blocks(request)?;
            Ok(PrivacyReport::default())
        }
    }
}
```

- [ ] **Step 4: Extend audit event**

In `audit.rs`, replace `AuditEvent` with:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AuditEvent {
    /// Request id generated by the gateway or caller.
    pub request_id: String,
    /// Provider profile used for routing.
    pub provider_profile: String,
    /// Provider id, for provider-router mode.
    #[serde(default)]
    pub provider_id: String,
    /// Gateway-facing model id.
    #[serde(default)]
    pub model_id: String,
    /// Upstream provider model id.
    #[serde(default)]
    pub upstream_model: String,
    /// Privacy mode label.
    #[serde(default)]
    pub privacy_mode: String,
    /// Count of replaced entities.
    pub entity_count: usize,
    /// Count of fresh PII redactions on response.
    pub fresh_pii_redacted: usize,
}
```

Update the `ev` helper in audit tests:

```rust
fn ev(id: &str) -> AuditEvent {
    AuditEvent {
        request_id: id.to_string(),
        provider_profile: "test".to_string(),
        provider_id: "test".to_string(),
        model_id: "anno/test/model:pseudonymized".to_string(),
        upstream_model: "model".to_string(),
        privacy_mode: "pseudonymized".to_string(),
        entity_count: 0,
        fresh_pii_redacted: 0,
    }
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p anno-privacy-gateway cleartext_dpa_validates_blocks_without_pseudonymizing -- --nocapture
cargo test -p anno-privacy-gateway pseudonymized_mode_pseudonymizes -- --nocapture
cargo test -p anno-privacy-gateway jsonl_sink_appends_line_and_chains_hashes -- --nocapture
```

Expected: PASS.

---

### Task 6: Provider-Routed Non-Streaming Messages

**Files:**
- Modify: `crates/anno-privacy-gateway/src/server.rs`

- [ ] **Step 1: Add failing integration-style tests in `server.rs`**

In the `server.rs` test module, add:

```rust
async fn mock_openai_chat(State(state): State<MockState>, Json(body): Json<Value>) -> Json<Value> {
    *state.captured.lock().await = Some(body);
    Json(json!({
        "choices": [{
            "message": {"role": "assistant", "content": "Bonjour PERSON_1"}
        }]
    }))
}

fn provider_catalog_file(tmp: &tempfile::TempDir, base_url: &str, dpa: bool) -> std::path::PathBuf {
    let path = tmp.path().join("providers.toml");
    std::fs::write(
        &path,
        format!(
            r#"
allow_cleartext_dpa = true
[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "{base_url}"
api_key_env = ""
dpa_verified = {dpa}
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{{ id = "mistral-large-latest", upstream = "mistral-large-latest" }}]
"#
        ),
    )
    .expect("write provider catalog");
    path
}

#[tokio::test]
async fn provider_router_pseudonymizes_before_openai_upstream() {
    let tmp = tempfile::TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/chat/completions", post(mock_openai_chat))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;
    let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);

    let config = GatewayConfig {
        provider_catalog_path: Some(catalog_path),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let response: Value = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let upstream_body = captured.lock().await.clone().expect("upstream called");
    let upstream_text = serde_json::to_string(&upstream_body).unwrap();
    assert!(!upstream_text.contains("Marie Dupont"));
    assert!(upstream_text.contains("PERSON_"));
    assert_eq!(response["content"][0]["text"], "Bonjour Marie Dupont");
}

#[tokio::test]
async fn provider_router_cleartext_dpa_sends_cleartext_to_verified_provider() {
    let tmp = tempfile::TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/chat/completions", post(mock_openai_chat))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;
    let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);

    let config = GatewayConfig {
        provider_catalog_path: Some(catalog_path),
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let status = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "anno/mistral/mistral-large-latest:cleartext-dpa",
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        }))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status, reqwest::StatusCode::OK);
    let upstream_body = captured.lock().await.clone().expect("upstream called");
    let upstream_text = serde_json::to_string(&upstream_body).unwrap();
    assert!(upstream_text.contains("Marie Dupont"));
}
```

- [ ] **Step 2: Run and verify failure**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_router_pseudonymizes_before_openai_upstream -- --nocapture
cargo test -p anno-privacy-gateway provider_router_cleartext_dpa_sends_cleartext_to_verified_provider -- --nocapture
```

Expected: FAIL because messages still use legacy upstream.

- [ ] **Step 3: Implement provider branch**

In `messages`, after `wants_stream` handling but before the legacy privacy block, add:

```rust
if state.provider_catalog.is_some() {
    return provider_messages(state, body).await;
}
```

Add helper:

```rust
async fn provider_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    let catalog = state
        .provider_catalog
        .as_ref()
        .ok_or_else(|| Error::Config("provider catalog missing".to_string()))?;
    let model_id = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Privacy("model is required".to_string()))?;
    let resolved = catalog
        .resolve_model(model_id)
        .map_err(Error::Config)?;

    let privacy_report = {
        let mut privacy = state.privacy.lock().await;
        privacy.transform_request_for_mode(&mut body, resolved.privacy_mode, false)?
    };
    let request = crate::chat::ChatRequest::from_anthropic(&body, &resolved.upstream_model)?;
    let upstream = crate::openai_compat::complete(&state.client, &resolved, &request).await?;
    let mut response = crate::chat::anthropic_response_from_openai(&upstream)?;

    let mut headers = HeaderMap::new();
    let mut fresh_pii_redacted = 0usize;
    if state.config.auto_rehydrate
        && resolved.privacy_mode == crate::privacy_mode::PrivacyMode::Pseudonymized
    {
        let privacy = state.privacy.lock().await;
        let report = privacy.rehydrate_response(&mut response)?;
        fresh_pii_redacted = report.fresh_pii_redacted;
        if fresh_pii_redacted > 0 {
            let count = HeaderValue::from_str(&fresh_pii_redacted.to_string())
                .map_err(|e| Error::Privacy(e.to_string()))?;
            headers.insert("x-anno-pii-leak-redacted", count);
        }
    }

    state.audit.record(crate::audit::AuditEvent {
        request_id: "provider-router".to_string(),
        provider_profile: state.config.provider_profile.clone(),
        provider_id: resolved.provider.id.clone(),
        model_id: resolved.gateway_model_id.clone(),
        upstream_model: resolved.upstream_model.clone(),
        privacy_mode: resolved.privacy_mode.audit_label().to_string(),
        entity_count: privacy_report.entities,
        fresh_pii_redacted,
    });

    Ok(MessagesResponse::Json(headers, Json(response)))
}
```

- [ ] **Step 4: Preserve legacy tests**

Run:

```powershell
cargo test -p anno-privacy-gateway messages_route_never_sends_cleartext_to_upstream_and_rehydrates -- --nocapture
cargo test -p anno-privacy-gateway files_api_fails_closed -- --nocapture
```

Expected: PASS. Legacy mode still uses `upstream_anthropic_base`.

- [ ] **Step 5: Run provider tests**

Run:

```powershell
cargo test -p anno-privacy-gateway provider_router_pseudonymizes_before_openai_upstream -- --nocapture
cargo test -p anno-privacy-gateway provider_router_cleartext_dpa_sends_cleartext_to_verified_provider -- --nocapture
```

Expected: PASS.

---

### Task 7: Provider-Routed Streaming And Tool Calls

**Files:**
- Modify: `crates/anno-privacy-gateway/src/chat.rs`
- Modify: `crates/anno-privacy-gateway/src/server.rs`
- Modify: `crates/anno-privacy-gateway/src/stream.rs`

- [ ] **Step 1: Add streaming adapter tests**

In `chat.rs`, add:

```rust
#[test]
fn openai_stream_text_chunk_to_anthropic_delta() {
    let chunk = json!({
        "choices": [{"delta": {"content": "Bonjour PERSON_1"}}]
    });
    let frame = anthropic_stream_frame_from_openai(&chunk, 0).expect("frame");
    assert_eq!(frame.data["type"], "content_block_delta");
    assert_eq!(frame.data["delta"]["type"], "text_delta");
    assert_eq!(frame.data["delta"]["text"], "Bonjour PERSON_1");
}

#[test]
fn openai_tool_call_arguments_render_as_single_safe_input_json_delta() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "search", "arguments": "{\"query\":\"PERSON_1\"}"}
                }]
            }
        }]
    });
    let frame = anthropic_stream_frame_from_openai(&chunk, 0).expect("frame");
    assert_eq!(frame.data["delta"]["type"], "input_json_delta");
    assert_eq!(frame.data["delta"]["partial_json"], "{\"query\":\"PERSON_1\"}");
}
```

- [ ] **Step 2: Implement stream frame conversion**

Add to `chat.rs`:

```rust
/// Convert one OpenAI streaming chunk to an Anthropic SSE frame payload.
pub fn anthropic_stream_frame_from_openai(value: &Value, index: usize) -> Result<crate::stream::SseFrame> {
    let delta = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .ok_or_else(|| Error::Upstream("OpenAI stream chunk missing choices[0].delta".to_string()))?;

    if let Some(content) = delta.get("content").and_then(Value::as_str) {
        return Ok(crate::stream::SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "text_delta", "text": content}
            }),
        });
    }

    if let Some(tool_call) = delta
        .get("tool_calls")
        .and_then(Value::as_array)
        .and_then(|calls| calls.first())
    {
        let function = tool_call.get("function").unwrap_or(&Value::Null);
        let args = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("{}");
        return Ok(crate::stream::SseFrame {
            event: Some("content_block_delta".to_string()),
            data: json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {"type": "input_json_delta", "partial_json": args}
            }),
        });
    }

    Ok(crate::stream::SseFrame {
        event: Some("message_stop".to_string()),
        data: json!({"type": "message_stop"}),
    })
}
```

- [ ] **Step 3: Add provider streaming route test**

In `server.rs` tests, add a mock OpenAI SSE endpoint and test:

```rust
async fn mock_openai_stream_chat(
    State(state): State<MockState>,
    Json(body): Json<Value>,
) -> axum::response::Sse<
    impl futures_util::Stream<
        Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    *state.captured.lock().await = Some(body);
    let stream = futures_util::stream::iter(vec![
        Ok(axum::response::sse::Event::default().data(json!({
            "choices": [{"delta": {"content": "Bonjour PERSON_1."}}]
        }).to_string())),
        Ok(axum::response::sse::Event::default().data("[DONE]")),
    ]);
    axum::response::Sse::new(stream)
}

#[tokio::test]
async fn provider_router_stream_rehydrates_text() {
    let tmp = tempfile::TempDir::new().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let upstream = Router::new()
        .route("/chat/completions", post(mock_openai_stream_chat))
        .with_state(MockState {
            captured: Arc::clone(&captured),
        });
    let upstream_addr = spawn(upstream).await;
    let catalog_path = provider_catalog_file(&tmp, &format!("http://{upstream_addr}"), true);
    let config = GatewayConfig {
        provider_catalog_path: Some(catalog_path),
        streaming: crate::config::StreamingMode::Enabled,
        ..GatewayConfig::default()
    };
    let gateway_addr = spawn(router(AppState::new(config))).await;

    let body = reqwest::Client::new()
        .post(format!("http://{gateway_addr}/v1/messages"))
        .json(&json!({
            "model": "anno/mistral/mistral-large-latest:pseudonymized",
            "stream": true,
            "messages": [{"role": "user", "content": "Bonjour Marie Dupont"}]
        }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("Marie Dupont"));
    assert!(!body.contains("PERSON_"));
}
```

- [ ] **Step 4: Implement provider streaming branch**

In `stream_messages`, before legacy pseudonymization, add:

```rust
if state.provider_catalog.is_some() {
    return provider_stream_messages(state, body).await;
}
```

Implement `provider_stream_messages` by:

1. resolving the model from catalog;
2. calling `transform_request_for_mode(..., allow_streaming=true)`;
3. converting request to OpenAI chat JSON;
4. calling `openai_compat::stream`;
5. parsing OpenAI SSE `data:` payloads;
6. converting each payload with `chat::anthropic_stream_frame_from_openai`;
7. reusing `StreamBuffer` and `transform_stream_ready_text` for text deltas;
8. for `input_json_delta`, buffer until `serde_json::from_str::<Value>(partial_json)` succeeds, transform string leaves with `PrivacyEngine::transform_stream_text` on the serialized complete JSON when pseudonymized mode is active, then emit one safe `input_json_delta`.

Use this helper shape inside `server.rs`:

```rust
async fn provider_stream_messages(state: AppState, mut body: Value) -> Result<MessagesResponse> {
    let catalog = state
        .provider_catalog
        .as_ref()
        .ok_or_else(|| Error::Config("provider catalog missing".to_string()))?;
    let model_id = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Privacy("model is required".to_string()))?;
    let resolved = catalog.resolve_model(model_id).map_err(Error::Config)?;
    {
        let mut privacy = state.privacy.lock().await;
        privacy.transform_request_for_mode(&mut body, resolved.privacy_mode, true)?;
    }
    let request = crate::chat::ChatRequest::from_anthropic(&body, &resolved.upstream_model)?;
    let upstream = crate::openai_compat::stream(&state.client, &resolved, &request).await?;
    let scan_fresh = matches!(state.config.stream_privacy, crate::config::StreamPrivacyMode::BufferedScan)
        && resolved.privacy_mode == crate::privacy_mode::PrivacyMode::Pseudonymized;
    let privacy = Arc::clone(&state.privacy);
    let max_chars = state.config.stream_max_buffer_chars;
    let stream = async_stream::stream! {
        let mut raw = String::new();
        let mut text_buffer = crate::stream::StreamBuffer::new(max_chars);
        let mut last_text_frame = None;
        futures_util::pin_mut!(upstream);
        while let Some(chunk) = upstream.next().await {
            let Ok(bytes) = chunk else {
                yield Ok(stream_error_event("upstream_error", "provider stream upstream error"));
                return;
            };
            raw.push_str(&String::from_utf8_lossy(&bytes));
            while let Some((frame_end, delimiter_len)) = next_sse_frame_boundary(&raw) {
                let frame_raw = raw[..frame_end + delimiter_len].to_string();
                raw = raw[frame_end + delimiter_len..].to_string();
                for line in frame_raw.lines().filter_map(|line| line.strip_prefix("data:")) {
                    let data = line.trim();
                    if data == "[DONE]" {
                        if let Some(flush) = flush_stream_text(&privacy, &mut text_buffer, &last_text_frame, scan_fresh, true).await {
                            yield Ok(flush.event);
                        }
                        yield Ok(passthrough_event(crate::stream::SseFrame {
                            event: Some("message_stop".to_string()),
                            data: json!({"type": "message_stop"}),
                        }));
                        return;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(data) else {
                        yield Ok(stream_error_event("stream_parse_error", "malformed provider SSE JSON"));
                        return;
                    };
                    let Ok(mut frame) = crate::chat::anthropic_stream_frame_from_openai(&value, 0) else {
                        yield Ok(stream_error_event("stream_parse_error", "unsupported provider stream chunk"));
                        return;
                    };
                    if let Some(text) = frame.text_delta() {
                        last_text_frame = Some(frame.clone());
                        if let Some(ready) = text_buffer.push(text) {
                            match transform_stream_ready_text(&privacy, ready, scan_fresh).await {
                                Ok(output) => {
                                    frame.set_text_delta(&output);
                                    yield Ok(passthrough_event(frame));
                                }
                                Err(_) => {
                                    yield Ok(stream_error_event("privacy_error", "stream privacy transform failed"));
                                    return;
                                }
                            }
                        }
                    } else {
                        yield Ok(passthrough_event(frame));
                    }
                }
            }
        }
    };
    Ok(MessagesResponse::Stream(Sse::new(Box::pin(stream) as SseResultStream)))
}
```

- [ ] **Step 5: Run streaming tests**

Run:

```powershell
cargo test -p anno-privacy-gateway openai_stream_text_chunk_to_anthropic_delta -- --nocapture
cargo test -p anno-privacy-gateway openai_tool_call_arguments_render_as_single_safe_input_json_delta -- --nocapture
cargo test -p anno-privacy-gateway provider_router_stream_rehydrates_text -- --nocapture
cargo test -p anno-privacy-gateway stream_input_json_delta_fails_closed -- --nocapture
```

Expected: PASS. Legacy sidecar path still fails closed on unsafe upstream `input_json_delta`; provider-router path can emit safe buffered tool JSON.

---

### Task 8: Docs And Final Verification

**Files:**
- Modify: `docs/developers/gateway-api.md`
- Modify: `docs/user-guide/privacy-gateway.md`

- [ ] **Step 1: Document provider catalog**

In `docs/developers/gateway-api.md`, add:

````markdown
## Provider Catalog

Set `ANNO_GATEWAY_PROVIDER_CATALOG` to enable provider-router mode. Without this
variable, the gateway keeps the legacy `ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE`
proxy behavior.

Example:

```toml
allow_cleartext_dpa = true

[[providers]]
id = "mistral"
kind = "openai_compatible"
base_url = "https://api.mistral.ai/v1"
api_key_env = "MISTRAL_API_KEY"
dpa_verified = true
allowed_privacy_modes = ["pseudonymized", "cleartext_dpa"]
models = [{ id = "mistral-large-latest", upstream = "mistral-large-latest" }]
```

Visible model ids include the provider and privacy mode:

```text
anno/mistral/mistral-large-latest:pseudonymized
anno/mistral/mistral-large-latest:cleartext-dpa
```
````

- [ ] **Step 2: Document privacy modes**

In `docs/user-guide/privacy-gateway.md`, add:

````markdown
## Privacy Modes

Provider-router models expose the privacy mode in the model id:

- `:pseudonymized` is the default regulated mode.
- `:cleartext-dpa` sends cleartext only to a provider with `dpa_verified=true`
  and only when the catalog sets `allow_cleartext_dpa=true`.
- `:cleartext-local` is accepted only for provider id `local`.

Cleartext DPA mode is intentionally opt-in and writes content-free audit
metadata. It does not store prompt or file contents in logs.
````

- [ ] **Step 3: Run final tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -Package anno-privacy-gateway -Mode check -Profile dev-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-local.ps1 -Package anno-privacy-gateway
```

Expected: PASS.

- [ ] **Step 4: Verify fail-closed file/document behavior remains**

Run:

```powershell
cargo test -p anno-privacy-gateway files_api_fails_closed -- --nocapture
cargo test -p anno-privacy-gateway rejects_document_blocks -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Secret scan**

Run:

```powershell
rg -n "MISTRAL_API_KEY\\s*=|SCALEWAY_API_KEY\\s*=|OVH_AI_ENDPOINTS_ACCESS_TOKEN\\s*=|Bearer [A-Za-z0-9]" crates docs
```

Expected: no real secret values. Env variable names in docs are acceptable; assignments with real values are not.

- [ ] **Step 6: Commit**

Run:

```powershell
git add crates\anno-privacy-gateway docs\developers\gateway-api.md docs\user-guide\privacy-gateway.md
git commit -m "feat: add sovereign provider gateway routing"
npx gitnexus analyze
npx gitnexus status
```

Expected: commit succeeds and GitNexus is up to date. If `AGENTS.md` or `CLAUDE.md` counters change, include or amend them so the worktree is clean.

## Acceptance Criteria

- Legacy gateway mode still works when `ANNO_GATEWAY_PROVIDER_CATALOG` is unset.
- Provider catalog mode exposes `/v1/models` IDs with provider and privacy suffix.
- Mistral, Scaleway, OVHcloud, and local OpenAI-compatible providers can be configured without hardcoded secrets.
- `pseudonymized` is default and never sends detected raw PII upstream in provider-router tests.
- `cleartext_dpa` is blocked unless deployment and provider DPA gates are both true.
- `cleartext_local` is blocked unless provider id is `local`.
- Non-streaming OpenAI-compatible responses render as Anthropic-compatible responses.
- Provider-router streaming emits safe text deltas and does not regress legacy streaming privacy tests.
- `/v1/files` and native `document` blocks remain fail-closed for the separate file-ingress plan.
