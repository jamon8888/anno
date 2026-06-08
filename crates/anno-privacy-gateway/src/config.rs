//! Runtime configuration.

use std::net::SocketAddr;

/// Streaming availability for `/v1/messages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    /// Reject `stream=true`.
    Disabled,
    /// Accept `stream=true`.
    Enabled,
}

impl StreamingMode {
    /// Parse an environment label.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "enabled" | "true" | "1" => Self::Enabled,
            _ => Self::Disabled,
        }
    }

    /// Return true when streaming requests are accepted.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Privacy policy applied to streamed response text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPrivacyMode {
    /// Buffer, scan fresh PII, redact, then rehydrate known pseudonyms.
    BufferedScan,
    /// Rehydrate known pseudonyms only; no fresh PII scan.
    TokenRehydrateOnly,
}

impl StreamPrivacyMode {
    /// Parse an environment label.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "token_rehydrate_only" => Self::TokenRehydrateOnly,
            _ => Self::BufferedScan,
        }
    }
}

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
    /// Optional provider catalog TOML path. If absent, legacy upstream proxy mode is used.
    pub provider_catalog_path: Option<std::path::PathBuf>,
    /// Local directory used for uploaded file metadata and text derivatives.
    pub file_store_dir: std::path::PathBuf,
    /// Maximum accepted upload size in bytes.
    pub file_max_bytes: usize,
    /// Whether raw uploaded bytes are retained after local text extraction.
    pub file_retain_raw: bool,
    /// Whether cleartext extracted text is retained for DPA/local cleartext modes.
    pub file_retain_cleartext: bool,
    /// Optional local encrypted vault path. If absent, the gateway uses an
    /// ephemeral vault.
    pub vault_path: Option<String>,
    /// Optional 32-byte vault key encoded as 64 lowercase/uppercase hex chars.
    pub vault_key_hex: Option<String>,
    /// Whether `stream=true` is accepted.
    pub streaming: StreamingMode,
    /// Privacy transform used for streamed response text.
    pub stream_privacy: StreamPrivacyMode,
    /// Maximum buffered text before a forced streaming flush.
    pub stream_max_buffer_chars: usize,
    /// Maximum buffered age before a forced streaming flush.
    pub stream_max_buffer_ms: u64,
    /// Directory where the persistent audit register writes
    /// `YYYY-MM-DD.jsonl` + `.sig` files. If `None`, a `NoopAuditSink`
    /// is used (suitable for tests only).
    pub audit_dir: Option<std::path::PathBuf>,
    /// 32-byte HMAC key (hex-encoded) for the daily audit signature file.
    /// If `audit_dir` is set and this is `None`, gateway startup fails.
    pub audit_hmac_key_hex: Option<String>,
    /// Bearer token required on protected routes. `/health` stays public.
    pub bearer_token: Option<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:3000".parse().expect("static listen addr parses"),
            upstream_anthropic_base: "http://127.0.0.1:3100".to_string(),
            auto_rehydrate: true,
            reject_images: true,
            provider_profile: "global_anonymized".to_string(),
            provider_catalog_path: None,
            file_store_dir: std::path::PathBuf::from(".anno/privacy-gateway/files"),
            file_max_bytes: 25 * 1024 * 1024,
            file_retain_raw: false,
            file_retain_cleartext: true,
            vault_path: None,
            vault_key_hex: None,
            streaming: StreamingMode::Disabled,
            stream_privacy: StreamPrivacyMode::BufferedScan,
            stream_max_buffer_chars: 4096,
            stream_max_buffer_ms: 750,
            audit_dir: None,
            audit_hmac_key_hex: None,
            bearer_token: None,
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
        if let Ok(path) = std::env::var("ANNO_GATEWAY_PROVIDER_CATALOG") {
            let path = path.trim();
            if !path.is_empty() {
                cfg.provider_catalog_path = Some(std::path::PathBuf::from(path));
            }
        }
        if let Ok(path) = std::env::var("ANNO_GATEWAY_FILE_STORE_DIR") {
            cfg.file_store_dir = std::path::PathBuf::from(path);
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_MAX_BYTES") {
            if let Ok(bytes) = value.parse::<usize>() {
                cfg.file_max_bytes = bytes;
            }
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_RETAIN_RAW") {
            cfg.file_retain_raw = matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES");
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT") {
            cfg.file_retain_cleartext =
                !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "NO");
        }
        if let Ok(path) = std::env::var("ANNO_GATEWAY_VAULT_PATH") {
            cfg.vault_path = Some(path);
        }
        if let Ok(key) = std::env::var("ANNO_GATEWAY_VAULT_KEY_HEX") {
            cfg.vault_key_hex = Some(key);
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAMING") {
            cfg.streaming = StreamingMode::parse(&value);
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_PRIVACY") {
            cfg.stream_privacy = StreamPrivacyMode::parse(&value);
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_MAX_BUFFER_CHARS") {
            if let Ok(parsed) = value.parse() {
                cfg.stream_max_buffer_chars = parsed;
            }
        }
        if let Ok(value) = std::env::var("ANNO_GATEWAY_STREAM_MAX_BUFFER_MS") {
            if let Ok(parsed) = value.parse() {
                cfg.stream_max_buffer_ms = parsed;
            }
        }
        if let Ok(d) = std::env::var("ANNO_GATEWAY_AUDIT_DIR") {
            cfg.audit_dir = Some(std::path::PathBuf::from(d));
        }
        if let Ok(k) = std::env::var("ANNO_GATEWAY_AUDIT_HMAC_KEY_HEX") {
            cfg.audit_hmac_key_hex = Some(k);
        }
        if let Ok(t) = std::env::var("ANNO_GATEWAY_BEARER_TOKEN") {
            let token = t.trim();
            if !token.is_empty() {
                cfg.bearer_token = Some(token.to_string());
            }
        }
        cfg
    }

    /// Validate cross-field security constraints.
    ///
    /// Loopback deployments may rely on the local network boundary. Any
    /// non-loopback listener must have an explicit bearer token.
    pub fn validate_security(&self) -> crate::Result<()> {
        if !self.listen.ip().is_loopback()
            && self
                .bearer_token
                .as_deref()
                .is_none_or(|token| token.trim().is_empty())
        {
            return Err(crate::Error::Config(format!(
                "ANNO_GATEWAY_BEARER_TOKEN is required when ANNO_GATEWAY_LISTEN binds non-loopback address {}",
                self.listen
            )));
        }

        Ok(())
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

    #[test]
    fn streaming_defaults_to_disabled_buffered_scan() {
        let cfg = GatewayConfig::default();
        assert_eq!(cfg.streaming, StreamingMode::Disabled);
        assert_eq!(cfg.stream_privacy, StreamPrivacyMode::BufferedScan);
        assert_eq!(cfg.stream_max_buffer_chars, 4096);
        assert_eq!(cfg.stream_max_buffer_ms, 750);
    }

    #[test]
    fn provider_catalog_path_loads_from_env() {
        unsafe {
            std::env::set_var(
                "ANNO_GATEWAY_PROVIDER_CATALOG",
                "C:\\ProgramData\\Hacienda\\providers.toml",
            );
        }
        let cfg = GatewayConfig::from_env();
        unsafe {
            std::env::remove_var("ANNO_GATEWAY_PROVIDER_CATALOG");
        }

        assert_eq!(
            cfg.provider_catalog_path.as_deref(),
            Some(std::path::Path::new(
                "C:\\ProgramData\\Hacienda\\providers.toml"
            ))
        );
    }

    #[test]
    fn file_config_default_is_local_and_bounded() {
        let cfg = GatewayConfig::default();

        assert_eq!(cfg.file_max_bytes, 25 * 1024 * 1024);
        assert!(!cfg.file_retain_raw);
        assert!(cfg.file_retain_cleartext);
        assert!(cfg.file_store_dir.ends_with("files"));
    }

    #[test]
    fn file_config_env_parses_values() {
        unsafe {
            std::env::set_var("ANNO_GATEWAY_FILE_STORE_DIR", "target/test-file-store");
            std::env::set_var("ANNO_GATEWAY_FILE_MAX_BYTES", "4096");
            std::env::set_var("ANNO_GATEWAY_FILE_RETAIN_RAW", "true");
            std::env::set_var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT", "false");
        }

        let cfg = GatewayConfig::from_env();

        unsafe {
            std::env::remove_var("ANNO_GATEWAY_FILE_STORE_DIR");
            std::env::remove_var("ANNO_GATEWAY_FILE_MAX_BYTES");
            std::env::remove_var("ANNO_GATEWAY_FILE_RETAIN_RAW");
            std::env::remove_var("ANNO_GATEWAY_FILE_RETAIN_CLEARTEXT");
        }

        assert_eq!(
            cfg.file_store_dir,
            std::path::PathBuf::from("target/test-file-store")
        );
        assert_eq!(cfg.file_max_bytes, 4096);
        assert!(cfg.file_retain_raw);
        assert!(!cfg.file_retain_cleartext);
    }
}
