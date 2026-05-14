//! Provider and privacy policy types.

/// Deployment provider profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProfile {
    /// Local provider. Cleartext may be allowed by later policy, but v0.3 still
    /// pseudonymizes by default.
    Local,
    /// Sovereign provider. Pseudonymized payloads only by default.
    Sovereign,
    /// Global provider through anonymized/pseudonymized payloads.
    GlobalAnonymized,
}

impl ProviderProfile {
    /// Parse a provider profile label.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "local" => Self::Local,
            "sovereign" => Self::Sovereign,
            _ => Self::GlobalAnonymized,
        }
    }
}

/// v0.3 unsupported feature policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedPolicy {
    /// Reject unsupported features fail-closed.
    Reject,
}
