//! Runtime selection of the GDPR detection layer set via ANNO_GDPR_LAYERS env var.

use std::str::FromStr;

/// Tiered GDPR detection layer set, selected at runtime via `ANNO_GDPR_LAYERS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GdprLayerSet {
    /// Regex + GLiNER2 only — original behaviour.
    Basic,
    /// Adds FR heuristics backend + validators. Default in prod.
    #[default]
    Defense,
    /// Phase C: multi-task composition + entropy filter (not yet implemented).
    Shadow,
    /// Phase D: calibration runtime + review queue (not yet implemented).
    Full,
}

impl GdprLayerSet {
    /// Read from ANNO_GDPR_LAYERS. Falls back to Defense if unset or unrecognised.
    pub fn from_env() -> Self {
        std::env::var("ANNO_GDPR_LAYERS")
            .ok()
            .and_then(|v| Self::from_str(&v).ok())
            .unwrap_or_default()
    }

    /// Whether this layer set runs the deterministic FR heuristics backend.
    pub fn includes_heuristics(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }

    /// Whether this layer set runs the post-aggregator entity validators.
    pub fn includes_validators(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }
}

impl std::fmt::Display for GdprLayerSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Basic => write!(f, "basic"),
            Self::Defense => write!(f, "defense"),
            Self::Shadow => write!(f, "shadow"),
            Self::Full => write!(f, "full"),
        }
    }
}

impl FromStr for GdprLayerSet {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "basic" => Ok(Self::Basic),
            "defense" => Ok(Self::Defense),
            "shadow" => Ok(Self::Shadow),
            "full" => Ok(Self::Full),
            other => Err(format!(
                "unknown gdpr-layers '{other}'; valid: basic, defense, shadow, full"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_strings() {
        assert_eq!(GdprLayerSet::from_str("basic"), Ok(GdprLayerSet::Basic));
        assert_eq!(GdprLayerSet::from_str("Defense"), Ok(GdprLayerSet::Defense));
        assert_eq!(GdprLayerSet::from_str("  full  "), Ok(GdprLayerSet::Full));
    }

    #[test]
    fn rejects_unknown_strings() {
        assert!(GdprLayerSet::from_str("xyz").is_err());
        assert!(GdprLayerSet::from_str("").is_err());
    }

    #[test]
    fn default_is_defense() {
        assert_eq!(GdprLayerSet::default(), GdprLayerSet::Defense);
    }

    #[test]
    fn layer_predicates() {
        assert!(!GdprLayerSet::Basic.includes_heuristics());
        assert!(GdprLayerSet::Defense.includes_heuristics());
        assert!(GdprLayerSet::Defense.includes_validators());
        assert!(!GdprLayerSet::Basic.includes_validators());
    }
}
