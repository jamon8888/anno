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
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:3000".parse().expect("static listen addr parses"),
            upstream_anthropic_base: "http://127.0.0.1:3100".to_string(),
            auto_rehydrate: true,
            reject_images: true,
            provider_profile: "global_anonymized".to_string(),
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
        cfg
    }
}
