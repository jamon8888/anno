//! Gender type for coreference resolution and demographic analysis.
//!
//! This is the canonical `Gender` enum used throughout anno for:
//! - Pronoun resolution in coreference
//! - Literary character analysis
//! - Demographic bias evaluation
//!
//! # Design Notes
//!
//! The enum is intentionally simple with four variants to cover most NLP use cases.
//! For more nuanced gender representation (e.g., social vs grammatical gender),
//! consider domain-specific extensions.

use serde::{Deserialize, Serialize};

/// Gender classification for NLP tasks.
///
/// Used for pronoun resolution, coreference clustering, and bias analysis.
///
/// # Examples
///
/// ```rust
/// use anno_core::types::Gender;
///
/// let gender = Gender::from_pronoun("she");
/// assert_eq!(gender, Some(Gender::Feminine));
///
/// let gender = Gender::from_pronoun("they");
/// assert_eq!(gender, Some(Gender::Neutral));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    /// Masculine (he/him/his)
    Masculine,
    /// Feminine (she/her/hers)
    Feminine,
    /// Neutral or non-binary (they/them/theirs, it/its)
    #[default]
    Neutral,
    /// Unknown or unspecified
    Unknown,
}

impl Gender {
    /// Infer gender from a pronoun.
    ///
    /// Returns `None` for non-pronoun inputs.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use anno_core::types::Gender;
    ///
    /// assert_eq!(Gender::from_pronoun("he"), Some(Gender::Masculine));
    /// assert_eq!(Gender::from_pronoun("She"), Some(Gender::Feminine));
    /// assert_eq!(Gender::from_pronoun("they"), Some(Gender::Neutral));
    /// assert_eq!(Gender::from_pronoun("it"), Some(Gender::Neutral));
    /// assert_eq!(Gender::from_pronoun("John"), None);
    /// ```
    #[must_use]
    pub fn from_pronoun(pronoun: &str) -> Option<Self> {
        match pronoun.to_lowercase().as_str() {
            "he" | "him" | "his" | "himself" => Some(Gender::Masculine),
            "she" | "her" | "hers" | "herself" => Some(Gender::Feminine),
            "they" | "them" | "their" | "theirs" | "themself" | "themselves" => {
                Some(Gender::Neutral)
            }
            "it" | "its" | "itself" => Some(Gender::Neutral),
            _ => None,
        }
    }

    /// Get typical subject pronoun for this gender.
    #[must_use]
    pub fn subject_pronoun(&self) -> &'static str {
        match self {
            Gender::Masculine => "he",
            Gender::Feminine => "she",
            Gender::Neutral | Gender::Unknown => "they",
        }
    }

    /// Get typical object pronoun for this gender.
    #[must_use]
    pub fn object_pronoun(&self) -> &'static str {
        match self {
            Gender::Masculine => "him",
            Gender::Feminine => "her",
            Gender::Neutral | Gender::Unknown => "them",
        }
    }

    /// Get typical possessive pronoun for this gender.
    #[must_use]
    pub fn possessive_pronoun(&self) -> &'static str {
        match self {
            Gender::Masculine => "his",
            Gender::Feminine => "her",
            Gender::Neutral | Gender::Unknown => "their",
        }
    }

    /// Check if this gender is compatible with another for coreference.
    ///
    /// Unknown is compatible with everything. Neutral is compatible with
    /// everything (singular they can refer to any gender).
    #[must_use]
    pub fn is_compatible(&self, other: &Gender) -> bool {
        match (self, other) {
            // Unknown matches anything
            (Gender::Unknown, _) | (_, Gender::Unknown) => true,
            // Neutral (they) can refer to any gender
            (Gender::Neutral, _) | (_, Gender::Neutral) => true,
            // Same gender matches
            (a, b) => a == b,
        }
    }
}

impl std::fmt::Display for Gender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Gender::Masculine => write!(f, "masculine"),
            Gender::Feminine => write!(f, "feminine"),
            Gender::Neutral => write!(f, "neutral"),
            Gender::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for Gender {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "masculine" | "male" | "m" => Ok(Gender::Masculine),
            "feminine" | "female" | "f" => Ok(Gender::Feminine),
            "neutral" | "nonbinary" | "non-binary" | "nb" | "n" => Ok(Gender::Neutral),
            "unknown" | "?" | "" => Ok(Gender::Unknown),
            _ => Err(format!("Unknown gender: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_pronoun() {
        assert_eq!(Gender::from_pronoun("he"), Some(Gender::Masculine));
        assert_eq!(Gender::from_pronoun("She"), Some(Gender::Feminine));
        assert_eq!(Gender::from_pronoun("THEY"), Some(Gender::Neutral));
        assert_eq!(Gender::from_pronoun("it"), Some(Gender::Neutral));
        assert_eq!(Gender::from_pronoun("John"), None);
    }

    #[test]
    fn test_compatibility() {
        assert!(Gender::Masculine.is_compatible(&Gender::Masculine));
        assert!(!Gender::Masculine.is_compatible(&Gender::Feminine));
        assert!(Gender::Unknown.is_compatible(&Gender::Masculine));
        assert!(Gender::Neutral.is_compatible(&Gender::Feminine));
    }

    #[test]
    fn test_serde_roundtrip() {
        let gender = Gender::Feminine;
        let json = serde_json::to_string(&gender).unwrap();
        assert_eq!(json, "\"feminine\"");
        let recovered: Gender = serde_json::from_str(&json).unwrap();
        assert_eq!(gender, recovered);
    }

    #[test]
    fn test_from_str() {
        assert_eq!("masculine".parse::<Gender>().unwrap(), Gender::Masculine);
        assert_eq!("female".parse::<Gender>().unwrap(), Gender::Feminine);
        assert_eq!("nb".parse::<Gender>().unwrap(), Gender::Neutral);
        assert_eq!("".parse::<Gender>().unwrap(), Gender::Unknown);
    }

    #[test]
    fn test_pronouns() {
        assert_eq!(Gender::Masculine.subject_pronoun(), "he");
        assert_eq!(Gender::Masculine.object_pronoun(), "him");
        assert_eq!(Gender::Masculine.possessive_pronoun(), "his");

        assert_eq!(Gender::Feminine.subject_pronoun(), "she");
        assert_eq!(Gender::Feminine.object_pronoun(), "her");
        assert_eq!(Gender::Feminine.possessive_pronoun(), "her");

        assert_eq!(Gender::Neutral.subject_pronoun(), "they");
        assert_eq!(Gender::Neutral.object_pronoun(), "them");
        assert_eq!(Gender::Neutral.possessive_pronoun(), "their");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Gender::Masculine), "masculine");
        assert_eq!(format!("{}", Gender::Feminine), "feminine");
        assert_eq!(format!("{}", Gender::Neutral), "neutral");
        assert_eq!(format!("{}", Gender::Unknown), "unknown");
    }

    #[test]
    fn test_all_pronoun_forms() {
        // Masculine
        for pronoun in &["he", "him", "his", "himself"] {
            assert_eq!(Gender::from_pronoun(pronoun), Some(Gender::Masculine));
        }
        // Feminine
        for pronoun in &["she", "her", "hers", "herself"] {
            assert_eq!(Gender::from_pronoun(pronoun), Some(Gender::Feminine));
        }
        // Neutral
        for pronoun in &[
            "they",
            "them",
            "their",
            "theirs",
            "themself",
            "themselves",
            "it",
            "its",
            "itself",
        ] {
            assert_eq!(Gender::from_pronoun(pronoun), Some(Gender::Neutral));
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(Gender::default(), Gender::Neutral);
    }
}
