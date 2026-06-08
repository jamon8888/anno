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
