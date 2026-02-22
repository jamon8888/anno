//! Configuration and data types for mention-ranking coreference.

#[allow(unused_imports)]
use super::*;


#[allow(unused_imports)]
use crate::{Model, Result};
use anno_core::{Gender, MentionType};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

/// A scored mention pair for easy-first clustering.
#[derive(Debug, Clone)]
pub(super) struct ScoredPair {
    pub(super) mention_idx: usize,
    pub(super) antecedent_idx: usize,
    pub(super) score: f64,
}

/// Clustering strategy for mention linking.
///
/// # Research Context (Bourgois & Poibeau 2025)
///
/// The paper compares two clustering strategies:
/// - **Left-to-right**: Traditional approach, processes mentions in document order
/// - **Easy-first**: Process high-confidence decisions first, constrains later decisions
///
/// Easy-first combined with global proper noun coreference can improve outcomes on long documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClusteringStrategy {
    /// Process mentions left-to-right in document order (traditional).
    #[default]
    LeftToRight,
    /// Process mentions by confidence score (high confidence first).
    /// High-confidence decisions constrain later decisions.
    /// Non-coreference predictions can prevent incorrect merges.
    EasyFirst,
}

/// Configuration for mention-ranking coref.
///
/// # Research-Informed Defaults
///
/// The defaults are informed by findings from Bourgois & Poibeau (2025):
/// - Pronouns tend to have shorter antecedent distances than proper nouns
/// - Proper/common nouns can span thousands of mentions
/// - Type-specific limits outperform uniform limits
///
/// # Example
///
/// ```rust
/// use anno::backends::mention_ranking::{MentionRankingConfig, ClusteringStrategy};
///
/// // Book-scale configuration
/// let config = MentionRankingConfig {
///     pronoun_max_antecedents: 30,     // 95% of pronouns within 7 mentions
///     proper_max_antecedents: 300,     // Proper nouns span further
///     nominal_max_antecedents: 300,    // Common nouns similar to proper
///     enable_global_proper_coref: true, // Bridge long-distance proper nouns
///     clustering_strategy: ClusteringStrategy::EasyFirst,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct MentionRankingConfig {
    /// Minimum score to link mentions.
    pub link_threshold: f64,

    // =========================================================================
    // Type-specific antecedent limits (Bourgois & Poibeau 2025)
    // =========================================================================
    /// Maximum number of antecedent candidates for pronouns.
    /// Research shows 95% of pronouns are within 7 mentions of antecedent.
    /// Default: 30 (conservative buffer above 95th percentile).
    pub pronoun_max_antecedents: usize,

    /// Maximum number of antecedent candidates for proper nouns.
    /// Proper nouns can span 1700+ mentions in long documents.
    /// Default: 300 (covers 99th percentile while remaining tractable).
    pub proper_max_antecedents: usize,

    /// Maximum number of antecedent candidates for nominal mentions.
    /// Similar distribution to proper nouns.
    /// Default: 300.
    pub nominal_max_antecedents: usize,

    /// Legacy uniform max distance (in characters). Used as fallback.
    /// Prefer type-specific limits for better accuracy.
    pub max_distance: usize,

    // =========================================================================
    // Global proper noun coreference (Bourgois & Poibeau 2025)
    // =========================================================================
    /// Enable global proper noun coreference propagation.
    /// When enabled, high-confidence proper noun coreference decisions are
    /// propagated document-wide, bridging mentions that exceed local windows.
    /// Gains 5-10 B³ points on documents >20k tokens.
    pub enable_global_proper_coref: bool,

    /// Minimum confidence to propagate proper noun coreference globally.
    /// Only pairs with scores above this threshold are propagated.
    pub global_proper_threshold: f64,

    // =========================================================================
    // Easy-first clustering (Clark & Manning 2016, Bourgois & Poibeau 2025)
    // =========================================================================
    /// Clustering strategy to use.
    pub clustering_strategy: ClusteringStrategy,

    /// Enable non-coreference constraints in easy-first clustering.
    /// High-confidence non-coreference predictions prevent incorrect merges.
    pub use_non_coref_constraints: bool,

    /// Threshold for non-coreference constraints.
    /// Pairs with scores below this are treated as definitely non-coreferent.
    pub non_coref_threshold: f64,

    // =========================================================================
    // Feature weights
    // =========================================================================
    /// Weight for string match features.
    pub string_match_weight: f64,
    /// Weight for type compatibility features.
    pub type_compat_weight: f64,
    /// Weight for distance feature.
    pub distance_weight: f64,

    // =========================================================================
    // Salience integration
    // =========================================================================
    /// Weight for salience boost when scoring antecedent candidates.
    ///
    /// When > 0, antecedents with higher salience scores receive a boost.
    /// This helps prefer linking to important/central entities in the document.
    ///
    /// Typical values: 0.0 (disabled) to 0.3 (moderate boost).
    pub salience_weight: f64,

    // =========================================================================
    // i2b2-inspired rule-based features (Chen et al. 2011)
    // =========================================================================
    /// Enable "be phrase" detection for identity linking.
    /// Patterns like "X is Y" or "resolution of X is Y" strongly indicate coreference.
    /// From i2b2 clinical coref: achieved high precision on medical texts.
    pub enable_be_phrase_detection: bool,

    /// Weight for be-phrase identity signal.
    pub be_phrase_weight: f64,

    /// Enable acronym matching (e.g., "MRSA" ↔ "Methicillin-resistant Staphylococcus aureus").
    pub enable_acronym_matching: bool,

    /// Weight for acronym match signal.
    pub acronym_weight: f64,

    /// Enable context-based link filtering.
    /// Uses surrounding context (dates, locations, modifiers) to filter false links.
    pub enable_context_filtering: bool,

    /// Enable synonym matching for related terms.
    ///
    /// When enabled, uses string similarity (from `anno::coalesce`) as a proxy
    /// for synonym relationships. High similarity (>0.8) indicates likely synonyms.
    ///
    /// For domain-specific synonyms (medical, legal, etc.), implement a custom
    /// `anno::coalesce::SynonymSource` and integrate it with the resolver.
    pub enable_synonym_matching: bool,

    /// Weight for synonym match signal.
    pub synonym_weight: f64,

    // =========================================================================
    // Nominal adjective detection (J2N: arXiv:2409.14374)
    // =========================================================================
    /// Enable detection of nominal adjectives as mentions.
    ///
    /// Nominal adjectives are phrases like "the poor", "the elderly", "the accused"
    /// where an adjective functions as a noun phrase referring to a group of people.
    ///
    /// # Linguistic Background
    ///
    /// In English, certain adjectives can be "nominalized" when preceded by a
    /// definite article: "The rich get richer while the poor get poorer."
    /// Here, "the poor" refers to poor people as a collective group.
    ///
    /// # Coreference Impact (J2N Paper)
    ///
    /// Qi, Han & Xie (arXiv:2409.14374) showed that correctly detecting these
    /// as mentions can improve coreference metrics slightly. Without detection, pronouns
    /// like "they" that refer back to "the poor" become orphaned.
    ///
    /// # Grammatical Number
    ///
    /// Nominal adjectives are grammatically plural in English:
    /// - "The poor ARE struggling" (not "is")
    /// - "The elderly NEED support" (not "needs")
    ///
    /// Default: false (for backward compatibility)
    pub enable_nominal_adjective_detection: bool,

    /// Language for language-specific features (ISO 639-1 code).
    ///
    /// When set, enables language-specific patterns for:
    /// - Nominal adjective detection (German "die Armen", French "les pauvres", etc.)
    /// - Pronoun resolution rules
    /// - Gender/number agreement
    ///
    /// Supported languages:
    /// - "en" (default): English
    /// - "de": German
    /// - "fr": French
    /// - "es": Spanish
    ///
    /// Default: "en"
    pub language: String,
}

impl Default for MentionRankingConfig {
    fn default() -> Self {
        Self {
            link_threshold: 0.3,

            // Type-specific limits (Bourgois & Poibeau 2025)
            pronoun_max_antecedents: 30,  // 95% within 7 mentions
            proper_max_antecedents: 300,  // Can span 1700+ mentions
            nominal_max_antecedents: 300, // Similar to proper nouns

            // Legacy uniform limit (fallback)
            max_distance: 100,

            // Global proper noun coreference
            enable_global_proper_coref: false, // Off by default for compatibility
            global_proper_threshold: 0.7,

            // Clustering strategy
            clustering_strategy: ClusteringStrategy::LeftToRight,
            use_non_coref_constraints: false,
            non_coref_threshold: 0.2,

            // Feature weights
            string_match_weight: 1.0,
            type_compat_weight: 0.5,
            distance_weight: 0.1,

            // Salience (disabled by default for backward compatibility)
            salience_weight: 0.0,

            // i2b2-inspired features (off by default for backward compatibility)
            enable_be_phrase_detection: false,
            be_phrase_weight: 0.8,
            enable_acronym_matching: false,
            acronym_weight: 0.7,
            enable_context_filtering: false,
            enable_synonym_matching: false,
            synonym_weight: 0.5,

            // Nominal adjective detection (J2N: arXiv:2409.14374)
            enable_nominal_adjective_detection: false,

            // Language (English by default)
            language: "en".to_string(),
        }
    }
}

impl MentionRankingConfig {
    /// Create a configuration optimized for book-scale documents.
    ///
    /// Based on findings from Bourgois & Poibeau (2025):
    /// - Type-specific antecedent limits
    /// - Global proper noun coreference enabled
    /// - Easy-first clustering
    #[must_use]
    pub fn book_scale() -> Self {
        Self {
            link_threshold: 0.3,

            // Type-specific limits
            pronoun_max_antecedents: 30,
            proper_max_antecedents: 300,
            nominal_max_antecedents: 300,

            max_distance: 500, // Larger for book-scale

            // Enable book-scale optimizations
            enable_global_proper_coref: true,
            global_proper_threshold: 0.7,

            clustering_strategy: ClusteringStrategy::EasyFirst,
            use_non_coref_constraints: true,
            non_coref_threshold: 0.2,

            // Feature weights
            string_match_weight: 1.0,
            type_compat_weight: 0.5,
            distance_weight: 0.05, // Lower weight for distance in long docs

            // Salience helps in long documents where context is limited
            salience_weight: 0.2,

            // i2b2-inspired features (useful for long documents)
            enable_be_phrase_detection: true,
            be_phrase_weight: 0.8,
            enable_acronym_matching: true,
            acronym_weight: 0.7,
            enable_context_filtering: true,
            enable_synonym_matching: false, // Off by default, requires domain synonyms
            synonym_weight: 0.5,
            enable_nominal_adjective_detection: false,
            language: "en".to_string(),
        }
    }

    /// Create a configuration optimized for clinical/biomedical text.
    ///
    /// Based on Chen et al. (2011) "A Rule Based Solution to Co-reference
    /// Resolution in Clinical Text" from i2b2 NLP Challenge:
    /// - "Be phrase" detection for identity linking
    /// - Acronym matching (e.g., MRSA ↔ Methicillin-resistant...)
    /// - Context-based link filtering
    /// - Synonym matching for medical terms
    ///
    /// # Example
    ///
    /// ```rust
    /// use anno::backends::mention_ranking::MentionRankingConfig;
    ///
    /// let config = MentionRankingConfig::clinical();
    /// assert!(config.enable_be_phrase_detection);
    /// assert!(config.enable_acronym_matching);
    /// ```
    #[must_use]
    pub fn clinical() -> Self {
        Self {
            link_threshold: 0.3,

            // Clinical documents are typically shorter than books
            pronoun_max_antecedents: 30,
            proper_max_antecedents: 100,
            nominal_max_antecedents: 100,

            max_distance: 200,

            // Global proper coref helps with patient/doctor names
            enable_global_proper_coref: true,
            global_proper_threshold: 0.6,

            // Easy-first clustering works well for clinical
            clustering_strategy: ClusteringStrategy::EasyFirst,
            use_non_coref_constraints: true,
            non_coref_threshold: 0.2,

            // Feature weights (slightly higher for string matching in clinical)
            string_match_weight: 1.2,
            type_compat_weight: 0.5,
            distance_weight: 0.08,

            // Salience moderate
            salience_weight: 0.15,

            // Enable all i2b2-inspired features
            enable_be_phrase_detection: true,
            be_phrase_weight: 0.9, // High weight for clinical "X is Y" patterns
            enable_acronym_matching: true,
            acronym_weight: 0.8, // Medical acronyms are reliable
            enable_context_filtering: true,
            enable_synonym_matching: true, // Enable with medical synonyms
            synonym_weight: 0.6,
            enable_nominal_adjective_detection: false,
            language: "en".to_string(),
        }
    }

    /// Create a configuration with salience integration enabled.
    ///
    /// Salience-weighted scoring boosts antecedents that are more
    /// important/central in the document.
    #[must_use]
    pub fn with_salience(mut self, weight: f64) -> Self {
        self.salience_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Get maximum antecedents for a given mention type.
    #[must_use]
    pub fn max_antecedents_for_type(&self, mention_type: MentionType) -> usize {
        match mention_type {
            MentionType::Pronominal => self.pronoun_max_antecedents,
            MentionType::Proper => self.proper_max_antecedents,
            MentionType::Nominal => self.nominal_max_antecedents,
            // Zero anaphora and unknown types use nominal limits as default
            MentionType::Zero | MentionType::Unknown => self.nominal_max_antecedents,
        }
    }
}

// MentionType imported from anno_core

/// A detected mention with phi-features for coreference resolution.
///
/// This is the core data structure for mention-ranking coreference. Each mention
/// carries the linguistic features needed to determine coreference compatibility:
///
/// - **Span** (`start`, `end`): Character offsets in the source text
/// - **Type** (`mention_type`): Proper/Nominal/Pronominal/Zero (affects salience)
/// - **Phi-features** (`gender`, `number`): Agreement constraints
/// - **Head** (`head`): Syntactic head for matching
///
/// # Phi-Features and Agreement
///
/// The `gender` and `number` fields encode phi-features (φ-features) from
/// linguistic theory. These are the grammatical features that govern agreement:
///
/// | Feature | Purpose | Example constraint |
/// |---------|---------|-------------------|
/// | Gender | Pronoun resolution | "Mary... she" not "he" |
/// | Number | Singular/plural match | "The dogs... they" not "it" |
///
/// `None` values indicate unknown features, which are treated as compatible
/// with any value (permissive matching).
///
/// # Cross-Linguistic Notes
///
/// - **Person** is not stored here (would be 3rd for most mentions)
/// - **Dual number** is supported via `Number::Dual` (Arabic, Sanskrit, Hebrew)
/// - **Noun class** systems (Bantu, Dyirbal) would need extension beyond `Gender`
/// - **Zero mentions** (pro-drop) have spans but no surface text
#[derive(Debug, Clone)]
pub struct RankedMention {
    /// Character start offset (0-indexed, inclusive).
    ///
    /// Uses character offsets, not byte offsets, for Unicode safety.
    pub start: usize,

    /// Character end offset (exclusive).
    ///
    /// The span `[start, end)` extracts the mention text.
    pub end: usize,

    /// The mention text as it appears in the source.
    ///
    /// For zero pronouns (pro-drop), this may be empty or a placeholder.
    pub text: String,

    /// Mention type classification.
    ///
    /// Affects antecedent search: pronouns look locally, proper nouns globally.
    /// See [`MentionType`] for the accessibility hierarchy.
    pub mention_type: MentionType,

    /// Grammatical gender (if determinable).
    ///
    /// - `Some(Masculine/Feminine)`: Gendered pronoun or name
    /// - `Some(Neutral)`: "they"/"it" (compatible with any gender)
    /// - `Some(Unknown)`: Neopronouns or ungendered names
    /// - `None`: Feature not applicable or not detected
    pub gender: Option<Gender>,

    /// Grammatical number (if determinable).
    ///
    /// - `Some(Singular)`: "he", "she", "it", "the dog"
    /// - `Some(Dual)`: Arabic/Sanskrit dual forms
    /// - `Some(Plural)`: "they", "the dogs"
    /// - `Some(Unknown)`: "you" (ambiguous), singular "they"
    /// - `None`: Feature not detected
    pub number: Option<Number>,

    /// Syntactic head word of the mention.
    ///
    /// For "the former president", head = "president".
    /// Used for head matching in coreference scoring.
    pub head: String,
}

impl RankedMention {
    /// Get the character span as a tuple.
    #[must_use]
    pub fn span(&self) -> (usize, usize) {
        (self.start, self.end)
    }
}

/// Convert RankedMention to eval::coref::Mention for evaluation.
///
/// This enables using mention-ranking output directly in coreference evaluation.
impl From<&RankedMention> for anno_core::Mention {
    fn from(mention: &RankedMention) -> Self {
        Self {
            text: mention.text.clone(),
            start: mention.start,
            end: mention.end,
            head_start: None,
            head_end: None,
            entity_type: None,
            mention_type: Some(mention.mention_type),
        }
    }
}

impl From<RankedMention> for anno_core::Mention {
    fn from(mention: RankedMention) -> Self {
        Self::from(&mention)
    }
}

/// Convert Entity to RankedMention for coreference resolution.
///
/// This enables using NER output directly in mention-ranking coreference.
impl From<&crate::Entity> for RankedMention {
    fn from(entity: &crate::Entity) -> Self {
        Self {
            start: entity.start,
            end: entity.end,
            text: entity.text.clone(),
            mention_type: MentionType::classify(&entity.text),
            gender: None,
            number: None,
            head: extract_head(&entity.text),
        }
    }
}

impl From<crate::Entity> for RankedMention {
    fn from(entity: crate::Entity) -> Self {
        Self::from(&entity)
    }
}

/// Extract the head word from a mention (last word heuristic).
fn extract_head(text: &str) -> String {
    text.split_whitespace().last().unwrap_or(text).to_string()
}

// Gender and Number imported from anno_core
// Number includes Dual for Arabic, Hebrew, Sanskrit, etc.
pub use anno_core::Number;

/// Coreference cluster from mention ranking.
#[derive(Debug, Clone)]
pub struct MentionCluster {
    /// Cluster ID.
    pub id: usize,
    /// Mentions in this cluster.
    pub mentions: Vec<RankedMention>,
}

impl MentionCluster {
    /// Convert this cluster's mentions to Signals for use with GroundedDocument.
    ///
    /// Returns a vector of Signals with Location::Text locations.
    /// Signal IDs are assigned based on mention order within the cluster.
    ///
    /// # Arguments
    /// * `signal_id_base` - Starting signal ID (to avoid collisions with other clusters)
    #[must_use]
    pub fn to_signals(
        &self,
        signal_id_base: anno_core::SignalId,
    ) -> Vec<anno_core::Signal<anno_core::Location>> {
        self.mentions
            .iter()
            .enumerate()
            .map(|(idx, mention)| anno_core::Signal {
                id: signal_id_base + idx as u64,
                location: anno_core::Location::Text {
                    start: mention.start,
                    end: mention.end,
                },
                surface: mention.text.clone(),
                label: anno_core::TypeLabel::from(mention.mention_type.as_label()),
                confidence: 1.0,
                hierarchical: None,
                provenance: None,
                modality: anno_core::Modality::Symbolic,
                normalized: None,
                negated: false,
                quantifier: None,
            })
            .collect()
    }

    /// Convert this cluster to a Track for use with GroundedDocument.
    ///
    /// This bridges mention-ranking output to the canonical Signal→Track→Identity hierarchy.
    ///
    /// # Arguments
    /// * `signal_id_base` - Starting signal ID for the signals in this track
    ///
    /// # Returns
    /// A tuple of `(Track, Vec<Signal>)` containing the track and its signals.
    /// The signals should be added to the GroundedDocument separately.
    #[must_use]
    pub fn to_track(
        &self,
        signal_id_base: anno_core::SignalId,
    ) -> (
        anno_core::Track,
        Vec<anno_core::Signal<anno_core::Location>>,
    ) {
        let signals = self.to_signals(signal_id_base);

        // Find the canonical surface: prefer proper nouns, else first mention
        let canonical_surface = self
            .mentions
            .iter()
            .find(|m| m.mention_type == MentionType::Proper)
            .or_else(|| self.mentions.first())
            .map(|m| m.text.clone())
            .unwrap_or_default();

        // Build track with signal references
        let mut track =
            anno_core::Track::new(anno_core::TrackId::new(self.id as u64), canonical_surface);
        // Mention-ranking coref does not infer entity type; leave unset.
        track.entity_type = None;

        for (idx, _) in signals.iter().enumerate() {
            track.add_signal(signal_id_base + idx as u64, idx as u32);
        }

        (track, signals)
    }

    /// Get the canonical mention (first proper noun, or first mention if none).
    #[must_use]
    pub fn canonical_mention(&self) -> Option<&RankedMention> {
        self.mentions
            .iter()
            .find(|m| m.mention_type == MentionType::Proper)
            .or_else(|| self.mentions.first())
    }
}

impl RankedMention {
    /// Convert to a Signal with Location::Text.
    #[must_use]
    pub fn to_signal(
        &self,
        signal_id: anno_core::SignalId,
    ) -> anno_core::Signal<anno_core::Location> {
        anno_core::Signal {
            id: signal_id,
            location: anno_core::Location::Text {
                start: self.start,
                end: self.end,
            },
            surface: self.text.clone(),
            label: anno_core::TypeLabel::from(self.mention_type.as_label()),
            confidence: 1.0,
            hierarchical: None,
            provenance: None,
            modality: anno_core::Modality::Symbolic,
            normalized: None,
            negated: false,
            quantifier: None,
        }
    }
}

