//! Runtime configuration.

use std::net::SocketAddr;

/// Runtime configuration for the v0.3 gateway.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Address the gateway listens on.
    pub listen: SocketAddr,
    /// Anthropic-compatible upstream base URL.
    pub upstream_anthropic_base: String,
    /// Rehydrate known pseudo-tokens before returning to Cowork.
    pub auto_rehydrate: bool,
    /// Reject image blocks in regulated strict mode.
    pub reject_images: bool,
    /// Provider profile label for audit/routing.
    pub provider_profile: String,
    /// Optional local encrypted vault path. If absent, the gateway uses an
    /// ephemeral vault.
    pub vault_path: Option<String>,
    /// Optional 32-byte vault key encoded as 64 lowercase/uppercase hex chars.
    pub vault_key_hex: Option<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:3000".parse().expect("static listen addr parses"),
            upstream_anthropic_base: "http://127.0.0.1:3100".to_string(),
            auto_rehydrate: true,
            reject_images: true,
            provider_profile: "global_anonymized".to_string(),
            vault_path: None,
            vault_key_hex: None,
        }
    }
}

impl GatewayConfig {
    /// Build configuration from environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(listen) = std::env::var("ANNO_GATEWAY_LISTEN") {
            if let Ok(addr) = listen.parse() {
                cfg.listen = addr;
            }
        }
        if let Ok(base) = std::env::var("ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE") {
            cfg.upstream_anthropic_base = base;
        }
        if let Ok(profile) = std::env::var("ANNO_GATEWAY_PROVIDER_PROFILE") {
            cfg.provider_profile = profile;
        }
        if let Ok(path) = std::env::var("ANNO_GATEWAY_VAULT_PATH") {
            cfg.vault_path = Some(path);
        }
        if let Ok(key) = std::env::var("ANNO_GATEWAY_VAULT_KEY_HEX") {
            cfg.vault_key_hex = Some(key);
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_no_persistent_vault() {
        let cfg = GatewayConfig::default();
        assert!(cfg.vault_path.is_none());
        assert!(cfg.vault_key_hex.is_none());
    }
}
