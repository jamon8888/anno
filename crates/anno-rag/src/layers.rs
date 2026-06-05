//! Runtime selection of the GDPR detection layer set via ANNO_GDPR_LAYERS env var.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    pub fn includes_heuristics(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }

    pub fn includes_validators(self) -> bool {
        matches!(self, Self::Defense | Self::Shadow | Self::Full)
    }
}

impl FromStr for GdprLayerSet {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "basic" => Ok(Self::Basic),
            "defense" => Ok(Self::Defense),
            "shadow" => Ok(Self::Shadow),
            "full" => Ok(Self::Full),
            _ => Err(()),
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
