//! Canonical mention selection for entity clusters.
//!
//! When multiple mentions refer to the same entity, we need to select
//! a **canonical** (representative) mention. This module provides
//! strategies for making that selection based on salience features.
//!
//! # Background
//!
//! In coreference resolution, choosing the canonical mention matters:
//! - "Barack Obama" is better than "he" or "the president"
//! - "Apple Inc." is better than "the company" or "it"
//!
//! # Strategies
//!
//! | Strategy | Description |
//! |----------|-------------|
//! | `FirstMention` | Use the first mention (document order) |
//! | `LongestMention` | Use the longest surface form |
//! | `NamedFirst` | Prefer named entities over nominals/pronouns |
//! | `SalienceBased` | Use multiple salience features |
//!
//! # Example
//!
//! ```rust
//! use anno::coalesce::canonical::{CanonicalSelector, MentionFeatures, SalienceBasedSelector};
//!
//! let mentions = vec![
//!     MentionFeatures::new("he").with_position(0).pronominal(),
//!     MentionFeatures::new("Barack Obama").with_position(10).named(),
//!     MentionFeatures::new("the president").with_position(50).nominal(),
//! ];
//!
//! let selector = SalienceBasedSelector::default();
//! let canonical = selector.select(&mentions);
//! assert_eq!(canonical.surface, "Barack Obama");
//! ```
//!
//! # References
//!
//! - Haghighi & Klein (2010): Activity scores for mention salience
//! - Wick et al. (2009): Entity canonicalization as centroid selection

use std::cmp::Ordering;

// Re-export the canonical MentionType from anno_core.
// This unifies the type system across the anno ecosystem.
pub use anno_core::types::MentionType;

/// Features of a mention used for canonical selection.
#[derive(Debug, Clone)]
pub struct MentionFeatures {
    /// Surface form of the mention
    pub surface: String,
    /// Position in document (character offset or sentence index)
    pub position: usize,
    /// Mention type: Proper, Nominal, or Pronominal
    ///
    /// Uses [`anno_core::types::MentionType`]. For compatibility with code
    /// using "Named" terminology, use [`MentionType::NAMED`] or the
    /// [`MentionFeatures::named()`] builder method.
    pub mention_type: MentionType,
    /// Is this mention in subject position?
    pub is_subject: bool,
    /// Is this mention in a heading or title?
    pub is_in_heading: bool,
    /// Frequency of this exact surface form in the cluster
    pub frequency: usize,
    /// Optional salience score from external model
    pub salience_score: Option<f64>,
}

impl MentionFeatures {
    /// Create new mention features with just the surface form.
    pub fn new(surface: impl Into<String>) -> Self {
        Self {
            surface: surface.into(),
            position: 0,
            mention_type: MentionType::Nominal,
            is_subject: false,
            is_in_heading: false,
            frequency: 1,
            salience_score: None,
        }
    }

    /// Set position in document.
    pub fn with_position(mut self, position: usize) -> Self {
        self.position = position;
        self
    }

    /// Mark as named/proper mention.
    ///
    /// Uses [`MentionType::Proper`] (also known as "Named" in NER terminology).
    pub fn named(mut self) -> Self {
        self.mention_type = MentionType::Proper;
        self
    }

    /// Mark as nominal mention.
    pub fn nominal(mut self) -> Self {
        self.mention_type = MentionType::Nominal;
        self
    }

    /// Mark as pronominal mention.
    pub fn pronominal(mut self) -> Self {
        self.mention_type = MentionType::Pronominal;
        self
    }

    /// Mark as subject position.
    pub fn in_subject_position(mut self) -> Self {
        self.is_subject = true;
        self
    }

    /// Mark as in heading.
    pub fn in_heading(mut self) -> Self {
        self.is_in_heading = true;
        self
    }

    /// Set frequency.
    pub fn with_frequency(mut self, freq: usize) -> Self {
        self.frequency = freq;
        self
    }

    /// Set external salience score.
    pub fn with_salience(mut self, score: f64) -> Self {
        self.salience_score = Some(score);
        self
    }

    /// Compute salience score based on features.
    /// Higher score = more salient = better canonical candidate.
    pub fn compute_salience(&self) -> f64 {
        let mut score = 0.0;

        // Mention type: Named >> Nominal >> Pronominal
        score += match self.mention_type {
            MentionType::Proper => 100.0,
            MentionType::Nominal => 50.0,
            MentionType::Pronominal => 0.0,
            MentionType::Zero => -10.0, // Zero pronouns are poor canonical choices
            MentionType::Unknown => 25.0, // Between pronominal and nominal
        };

        // Length bonus: longer mentions are often more informative
        // Cap at 50 chars to avoid runaway scores
        score += (self.surface.chars().count().min(50) as f64) * 0.5;

        // Position penalty: earlier mentions are slightly preferred
        // But not as important as type
        score -= (self.position as f64).log10().max(0.0) * 2.0;

        // Subject position bonus
        if self.is_subject {
            score += 10.0;
        }

        // Heading bonus
        if self.is_in_heading {
            score += 15.0;
        }

        // Frequency bonus (log scale)
        score += (self.frequency as f64).log2().max(0.0) * 5.0;

        // External salience (if available, weighted heavily)
        if let Some(ext) = self.salience_score {
            score += ext * 50.0;
        }

        score
    }
}

/// Trait for selecting the canonical mention from a cluster.
pub trait CanonicalSelector: Send + Sync {
    /// Select the canonical mention from a list of candidates.
    /// Returns the index of the selected mention.
    fn select_index(&self, mentions: &[MentionFeatures]) -> usize;

    /// Select the canonical mention, returning a reference.
    fn select<'a>(&self, mentions: &'a [MentionFeatures]) -> &'a MentionFeatures {
        let idx = self.select_index(mentions);
        &mentions[idx]
    }
}

/// Select the first mention (simple baseline).
#[derive(Debug, Clone, Default)]
pub struct FirstMentionSelector;

impl CanonicalSelector for FirstMentionSelector {
    fn select_index(&self, mentions: &[MentionFeatures]) -> usize {
        // Find first by position
        mentions
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| m.position)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

/// Select the longest mention.
#[derive(Debug, Clone, Default)]
pub struct LongestMentionSelector;

impl CanonicalSelector for LongestMentionSelector {
    fn select_index(&self, mentions: &[MentionFeatures]) -> usize {
        mentions
            .iter()
            .enumerate()
            .max_by_key(|(_, m)| m.surface.chars().count())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

/// Prefer named entities, then longest.
#[derive(Debug, Clone, Default)]
pub struct NamedFirstSelector;

impl CanonicalSelector for NamedFirstSelector {
    fn select_index(&self, mentions: &[MentionFeatures]) -> usize {
        // First, find all named mentions
        let named: Vec<_> = mentions
            .iter()
            .enumerate()
            .filter(|(_, m)| m.mention_type == MentionType::Proper)
            .collect();

        if !named.is_empty() {
            // Among named, pick longest
            return named
                .into_iter()
                .max_by_key(|(_, m)| m.surface.chars().count())
                .map(|(i, _)| i)
                .unwrap_or(0);
        }

        // Fall back to nominals
        let nominals: Vec<_> = mentions
            .iter()
            .enumerate()
            .filter(|(_, m)| m.mention_type == MentionType::Nominal)
            .collect();

        if !nominals.is_empty() {
            return nominals
                .into_iter()
                .max_by_key(|(_, m)| m.surface.chars().count())
                .map(|(i, _)| i)
                .unwrap_or(0);
        }

        // Fall back to first
        0
    }
}

/// Salience-based selection using multiple features.
#[derive(Debug, Clone)]
pub struct SalienceBasedSelector {
    /// Weight for mention type
    pub type_weight: f64,
    /// Weight for length
    pub length_weight: f64,
    /// Weight for position (negative = prefer earlier)
    pub position_weight: f64,
    /// Weight for subject position
    pub subject_weight: f64,
    /// Weight for heading
    pub heading_weight: f64,
}

impl Default for SalienceBasedSelector {
    fn default() -> Self {
        Self {
            type_weight: 1.0,
            length_weight: 1.0,
            position_weight: 1.0,
            subject_weight: 1.0,
            heading_weight: 1.0,
        }
    }
}

impl CanonicalSelector for SalienceBasedSelector {
    fn select_index(&self, mentions: &[MentionFeatures]) -> usize {
        mentions
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                let score_a = a.compute_salience();
                let score_b = b.compute_salience();
                score_a.partial_cmp(&score_b).unwrap_or(Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

/// Detect if a string is likely a pronoun.
pub fn is_pronoun(s: &str) -> bool {
    let lower = s.to_lowercase();
    matches!(
        lower.as_str(),
        "he" | "she"
            | "it"
            | "they"
            | "him"
            | "her"
            | "them"
            | "his"
            | "hers"
            | "its"
            | "their"
            | "theirs"
            | "i"
            | "me"
            | "my"
            | "mine"
            | "we"
            | "us"
            | "our"
            | "ours"
            | "you"
            | "your"
            | "yours"
            | "this"
            | "that"
            | "these"
            | "those"
            | "who"
            | "whom"
            | "whose"
            | "which"
            | "what"
    )
}

/// Heuristically detect mention type from surface form.
pub fn detect_mention_type(surface: &str) -> MentionType {
    let trimmed = surface.trim();

    // Check for pronoun
    if is_pronoun(trimmed) {
        return MentionType::Pronominal;
    }

    // Check for named entity heuristics:
    // - Starts with capital letter (not at sentence start)
    // - Contains multiple capitalized words
    // - Doesn't start with "the", "a", "an"
    let words: Vec<&str> = trimmed.split_whitespace().collect();

    if words.is_empty() {
        return MentionType::Nominal;
    }

    // Starts with determiner -> likely nominal
    let first_lower = words[0].to_lowercase();
    if matches!(first_lower.as_str(), "the" | "a" | "an" | "this" | "that") {
        return MentionType::Nominal;
    }

    // Check if most words are capitalized (proper noun pattern)
    let capitalized_count = words
        .iter()
        .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
        .count();

    if capitalized_count > words.len() / 2 {
        MentionType::Proper
    } else {
        MentionType::Nominal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_type_detection() {
        assert_eq!(detect_mention_type("he"), MentionType::Pronominal);
        assert_eq!(detect_mention_type("Barack Obama"), MentionType::Proper);
        assert_eq!(detect_mention_type("the president"), MentionType::Nominal);
        assert_eq!(detect_mention_type("Apple Inc."), MentionType::Proper);
        assert_eq!(detect_mention_type("a company"), MentionType::Nominal);
    }

    #[test]
    fn test_salience_scoring() {
        let named = MentionFeatures::new("Barack Obama")
            .named()
            .with_position(10);
        let pronoun = MentionFeatures::new("he").pronominal().with_position(0);
        let nominal = MentionFeatures::new("the president")
            .nominal()
            .with_position(50);

        assert!(named.compute_salience() > nominal.compute_salience());
        assert!(nominal.compute_salience() > pronoun.compute_salience());
    }

    #[test]
    fn test_salience_selector() {
        let mentions = vec![
            MentionFeatures::new("he").pronominal().with_position(0),
            MentionFeatures::new("Barack Obama")
                .named()
                .with_position(10),
            MentionFeatures::new("the president")
                .nominal()
                .with_position(50),
        ];

        let selector = SalienceBasedSelector::default();
        let canonical = selector.select(&mentions);
        assert_eq!(canonical.surface, "Barack Obama");
    }

    #[test]
    fn test_named_first_selector() {
        let mentions = vec![
            MentionFeatures::new("the company").nominal(),
            MentionFeatures::new("Apple").named(),
            MentionFeatures::new("Apple Inc.").named(),
        ];

        let selector = NamedFirstSelector;
        let canonical = selector.select(&mentions);
        // Should pick longest named
        assert_eq!(canonical.surface, "Apple Inc.");
    }

    #[test]
    fn test_first_mention_selector() {
        let mentions = vec![
            MentionFeatures::new("second").with_position(10),
            MentionFeatures::new("first").with_position(0),
            MentionFeatures::new("third").with_position(20),
        ];

        let selector = FirstMentionSelector;
        let canonical = selector.select(&mentions);
        assert_eq!(canonical.surface, "first");
    }

    #[test]
    fn test_is_pronoun() {
        assert!(is_pronoun("he"));
        assert!(is_pronoun("She"));
        assert!(is_pronoun("THEY"));
        assert!(!is_pronoun("Obama"));
        assert!(!is_pronoun("the president"));
    }
}
