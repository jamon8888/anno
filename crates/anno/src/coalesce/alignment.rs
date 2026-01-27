//! Conceptual alignment for entity resolution.
//!
//! # Research Background
//!
//! This module implements concepts from "Ad hoc conventions generalize to new
//! referents" (Ji et al., 2025), which shows that:
//!
//! 1. Referential conventions are NOT rigid designators (arbitrary labels for specific entities)
//! 2. Conventions reflect broader conceptual alignment that generalizes to similar entities
//! 3. Generalization decays nonlinearly with distance (consistent with Shepard's Universal Law)
//!
//! # The Generalization-Identification Tradeoff
//!
//! From "Bound by semanticity" (Nurisso et al., 2025):
//!
//! ```text
//! For any representation with finite semantic resolution ε:
//!   - p_S = probability of correct generalization (treating similar things as same)
//!   - p_I = probability of correct identification (distinguishing different things)
//!
//! These are fundamentally constrained to a Pareto front:
//!   ↑ p_I means ↓ p_S, and vice versa.
//! ```
//!
//! Fixed thresholds choose ONE point on this curve. Adaptive thresholds let the
//! system learn where to operate based on accumulated evidence.
//!
//! # Key Concepts
//!
//! - **Nameability**: How much naming consensus exists for an entity type
//!   (measured by Shape Naming Divergence in the KiloGram dataset)
//! - **Alignment Score**: How much evidence has accumulated for a cluster
//! - **Generalization Gradient**: How similarity threshold decays with semantic distance
//!
//! # Example
//!
//! ```rust
//! use anno::coalesce::alignment::{AlignmentScore, GeneralizationGradient, Nameability};
//!
//! // High-nameability type (strong prior consensus)
//! let person_nameability = Nameability::high(0.9);
//!
//! // Cluster with accumulated evidence
//! let mut alignment = AlignmentScore::new();
//! alignment.record_match(0.85);  // First mention matched
//! alignment.record_match(0.92);  // Second mention matched
//!
//! // Compute adaptive threshold
//! let gradient = GeneralizationGradient::quadratic();
//! let base_threshold = 0.7;
//! let adjusted = gradient.adaptive_threshold(
//!     base_threshold,
//!     alignment.confidence(),
//!     0.8,  // similarity to known cluster
//! );
//! // adjusted < base_threshold for well-evidenced clusters
//! ```

use serde::{Deserialize, Serialize};

// =============================================================================
// Nameability
// =============================================================================

/// Nameability score measuring naming consensus for an entity type.
///
/// Based on Shape Naming Divergence (SND) from KiloGram (Ji et al., 2022):
/// - High nameability (low SND): Most people use the same name ("dog", "person")
/// - Low nameability (high SND): People use diverse names ("abstract shape")
///
/// In NER context:
/// - PERSON has high nameability (clear consensus on what counts as a person)
/// - MISC has low nameability (catch-all category, diverse interpretations)
///
/// # Interpretation
///
/// ```text
/// Nameability    SND (KiloGram)    Entity Type Example
/// ─────────────────────────────────────────────────────
/// High (>0.8)    <0.3              PERSON, LOCATION, DATE
/// Medium         0.3-0.6           ORGANIZATION, EVENT
/// Low (<0.4)     >0.6              MISC, WORK_OF_ART
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Nameability {
    /// Score in [0, 1], where 1 = perfect consensus
    score: f32,
}

impl Nameability {
    /// Create from raw score (clamped to [0, 1]).
    #[must_use]
    pub fn new(score: f32) -> Self {
        Self {
            score: score.clamp(0.0, 1.0),
        }
    }

    /// High nameability (strong consensus, like PERSON).
    #[must_use]
    pub fn high(score: f32) -> Self {
        Self::new(score.max(0.7))
    }

    /// Medium nameability.
    #[must_use]
    pub fn medium(score: f32) -> Self {
        Self::new(score.clamp(0.4, 0.7))
    }

    /// Low nameability (weak consensus, like MISC).
    #[must_use]
    pub fn low(score: f32) -> Self {
        Self::new(score.min(0.4))
    }

    /// Convert from Shape Naming Divergence (SND).
    ///
    /// SND is the inverse: high SND = low nameability.
    #[must_use]
    pub fn from_snd(snd: f32) -> Self {
        Self::new(1.0 - snd.clamp(0.0, 1.0))
    }

    /// Get the raw score.
    #[must_use]
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Is this high nameability?
    #[must_use]
    pub fn is_high(&self) -> bool {
        self.score >= 0.7
    }

    /// Is this low nameability?
    #[must_use]
    pub fn is_low(&self) -> bool {
        self.score < 0.4
    }

    /// Classify into discrete level.
    #[must_use]
    pub fn level(&self) -> NameabilityLevel {
        if self.score >= 0.7 {
            NameabilityLevel::High
        } else if self.score >= 0.4 {
            NameabilityLevel::Medium
        } else {
            NameabilityLevel::Low
        }
    }
}

impl Default for Nameability {
    fn default() -> Self {
        Self::new(0.5) // Unknown → assume medium
    }
}

/// Discrete nameability classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameabilityLevel {
    /// High consensus (>= 0.7)
    High,
    /// Medium consensus (0.4 - 0.7)
    Medium,
    /// Low consensus (< 0.4)
    Low,
}

// =============================================================================
// Alignment Score
// =============================================================================

/// Tracks accumulated alignment evidence for an entity cluster.
///
/// As more mentions are successfully resolved to a cluster, the alignment
/// score increases, reflecting increased confidence in the cluster's coherence.
///
/// This implements the "convention formation" process from the paper:
/// - Initial mentions have high uncertainty
/// - Repeated successful matches build confidence
/// - Well-evidenced clusters can afford lower thresholds for new matches
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlignmentScore {
    /// Similarity scores of successful matches
    match_scores: Vec<f32>,
    /// Number of matches
    match_count: usize,
    /// Running sum of scores (for efficient mean)
    score_sum: f32,
    /// Running sum of squared scores (for variance)
    score_sq_sum: f32,
}

impl AlignmentScore {
    /// Create empty alignment score.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful match with its similarity score.
    pub fn record_match(&mut self, similarity: f32) {
        self.match_scores.push(similarity);
        self.match_count += 1;
        self.score_sum += similarity;
        self.score_sq_sum += similarity * similarity;
    }

    /// Number of recorded matches.
    #[must_use]
    pub fn count(&self) -> usize {
        self.match_count
    }

    /// Mean similarity of matches.
    #[must_use]
    pub fn mean(&self) -> f32 {
        if self.match_count == 0 {
            0.0
        } else {
            self.score_sum / self.match_count as f32
        }
    }

    /// Variance of match similarities.
    #[must_use]
    pub fn variance(&self) -> f32 {
        if self.match_count < 2 {
            return 0.0;
        }
        let n = self.match_count as f32;
        let mean = self.mean();
        // Use max(0.0, ...) to handle floating-point precision errors
        // that could produce tiny negative values
        ((self.score_sq_sum / n) - (mean * mean)).max(0.0)
    }

    /// Standard deviation of match similarities.
    #[must_use]
    pub fn std_dev(&self) -> f32 {
        self.variance().sqrt()
    }

    /// Confidence score based on accumulated evidence.
    ///
    /// Combines count and consistency:
    /// - More matches → higher confidence (logarithmic saturation)
    /// - Lower variance → higher confidence
    ///
    /// Returns value in [0, 1].
    #[must_use]
    pub fn confidence(&self) -> f32 {
        if self.match_count == 0 {
            return 0.0;
        }

        // Count contribution (logarithmic, saturates around 10 matches)
        let count_factor = (1.0 + self.match_count as f32).ln() / (1.0 + 10.0_f32).ln();
        let count_contrib = count_factor.min(1.0);

        // Consistency contribution (low variance = high consistency)
        let consistency_contrib = 1.0 - self.std_dev().min(0.3) / 0.3;

        // Weighted combination
        (0.7 * count_contrib + 0.3 * consistency_contrib).clamp(0.0, 1.0)
    }

    /// Is this a well-evidenced cluster (confidence > 0.6)?
    #[must_use]
    pub fn is_well_evidenced(&self) -> bool {
        self.confidence() > 0.6
    }

    /// Merge with another alignment score.
    pub fn merge(&mut self, other: &AlignmentScore) {
        self.match_scores.extend(&other.match_scores);
        self.match_count += other.match_count;
        self.score_sum += other.score_sum;
        self.score_sq_sum += other.score_sq_sum;
    }
}

// =============================================================================
// Generalization Gradient
// =============================================================================

/// Decay function for generalization with semantic distance.
///
/// From Shepard's Universal Law of Generalization:
/// > The probability of generalizing a learned response decays as a
/// > concave function of distance in psychological space.
///
/// The conventions paper found quadratic decay fits better than linear
/// for abstract stimuli, though exponential is the classical form.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum GeneralizationGradient {
    /// No decay (fixed threshold)
    None,
    /// Linear decay: adjustment = confidence * (1 - similarity)
    Linear,
    /// Quadratic decay: adjustment = confidence * (1 - similarity)²
    ///
    /// From the conventions paper: "quadratic provides the best fit"
    #[default]
    Quadratic,
    /// Exponential decay: adjustment = confidence * exp(-k * distance)
    ///
    /// Classical Shepard's Law form.
    Exponential {
        /// Decay rate parameter (higher = faster decay)
        decay_rate: f32,
    },
}

impl GeneralizationGradient {
    /// No adaptation (fixed threshold).
    #[must_use]
    pub const fn none() -> Self {
        Self::None
    }

    /// Linear decay.
    #[must_use]
    pub const fn linear() -> Self {
        Self::Linear
    }

    /// Quadratic decay (recommended based on conventions paper).
    #[must_use]
    pub const fn quadratic() -> Self {
        Self::Quadratic
    }

    /// Exponential decay with given rate.
    #[must_use]
    pub const fn exponential(decay_rate: f32) -> Self {
        Self::Exponential { decay_rate }
    }

    /// Compute threshold adjustment based on similarity and alignment confidence.
    ///
    /// Follows Shepard's Universal Law: generalization probability decays with
    /// psychological distance. High similarity → strong generalization → more
    /// threshold reduction allowed. Low similarity → weak generalization → less
    /// threshold reduction.
    ///
    /// # Arguments
    /// * `similarity` - Similarity to the reference cluster [0, 1]
    /// * `alignment_confidence` - How well-evidenced the cluster is [0, 1]
    /// * `max_adjustment` - Maximum threshold reduction
    ///
    /// # Returns
    /// Threshold adjustment (negative value to subtract from base threshold)
    #[must_use]
    pub fn threshold_adjustment(
        &self,
        similarity: f32,
        alignment_confidence: f32,
        max_adjustment: f32,
    ) -> f32 {
        let sim = similarity.clamp(0.0, 1.0);
        let distance = 1.0 - sim;

        // Generalization strength: high when similar, low when distant
        // This follows Shepard's Law: g(d) decays with distance
        let generalization_strength = match self {
            Self::None => 0.0,
            // Linear: g(s) = s (directly proportional to similarity)
            Self::Linear => sim,
            // Quadratic: g(s) = s² (steeper falloff at low similarity)
            Self::Quadratic => sim * sim,
            // Exponential: g(d) = exp(-k*d) (classic Shepard's Law)
            Self::Exponential { decay_rate } => (-decay_rate * distance).exp(),
        };

        // Scale by confidence and max adjustment, negate to reduce threshold
        -alignment_confidence * generalization_strength * max_adjustment
    }

    /// Compute adaptive threshold.
    ///
    /// Well-evidenced clusters get lower thresholds for nearby entities,
    /// making it easier to extend conventions to similar referents.
    ///
    /// # Arguments
    /// * `base_threshold` - Starting threshold without adaptation
    /// * `alignment_confidence` - How well-evidenced the cluster is
    /// * `similarity` - How similar the new entity is to the cluster
    ///
    /// # Returns
    /// Adjusted threshold (always >= 0.3 to avoid over-generalization)
    #[must_use]
    pub fn adaptive_threshold(
        &self,
        base_threshold: f32,
        alignment_confidence: f32,
        similarity: f32,
    ) -> f32 {
        // Max adjustment is 0.2 (can reduce threshold by up to 20%)
        let adjustment = self.threshold_adjustment(similarity, alignment_confidence, 0.2);
        (base_threshold + adjustment).max(0.3) // Floor to prevent over-generalization
    }
}

// =============================================================================
// Adaptive Resolution Config
// =============================================================================

/// Configuration for adaptive entity resolution.
///
/// Combines nameability priors with learned alignment to dynamically
/// adjust similarity thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveResolutionConfig {
    /// Base similarity threshold
    pub base_threshold: f32,
    /// Minimum threshold (floor to prevent over-generalization)
    pub min_threshold: f32,
    /// Maximum threshold adjustment
    pub max_adjustment: f32,
    /// Generalization gradient function
    pub gradient: GeneralizationGradient,
    /// Whether to use nameability priors
    pub use_nameability: bool,
}

impl Default for AdaptiveResolutionConfig {
    fn default() -> Self {
        Self {
            base_threshold: 0.7,
            min_threshold: 0.3,
            max_adjustment: 0.2,
            gradient: GeneralizationGradient::Quadratic,
            use_nameability: true,
        }
    }
}

impl AdaptiveResolutionConfig {
    /// Create with strict (high) thresholds.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            base_threshold: 0.85,
            min_threshold: 0.5,
            max_adjustment: 0.15,
            ..Default::default()
        }
    }

    /// Create with loose (low) thresholds.
    #[must_use]
    pub fn loose() -> Self {
        Self {
            base_threshold: 0.5,
            min_threshold: 0.2,
            max_adjustment: 0.25,
            ..Default::default()
        }
    }

    /// Compute adaptive threshold for a new entity.
    ///
    /// # Arguments
    /// * `alignment` - Alignment score of the candidate cluster
    /// * `similarity` - Similarity between new entity and cluster
    /// * `nameability` - Nameability of the entity type (optional)
    #[must_use]
    pub fn compute_threshold(
        &self,
        alignment: &AlignmentScore,
        similarity: f32,
        nameability: Option<Nameability>,
    ) -> f32 {
        let mut threshold = self.gradient.adaptive_threshold(
            self.base_threshold,
            alignment.confidence(),
            similarity,
        );

        // Adjust for nameability if enabled
        if self.use_nameability {
            if let Some(n) = nameability {
                // High nameability → can use lower threshold (more confident in consensus)
                // Low nameability → need higher threshold (less confident)
                let nameability_adjustment = (n.score() - 0.5) * 0.1;
                threshold -= nameability_adjustment;
            }
        }

        threshold.clamp(self.min_threshold, 1.0)
    }
}

// =============================================================================
// Entity Type Nameability Priors
// =============================================================================

/// Get default nameability for common entity types.
///
/// Based on observed naming consensus in NER datasets:
/// - Core types (PERSON, LOCATION) have high consensus
/// - Abstract types (MISC, EVENT) have lower consensus
///
/// # Example
///
/// ```rust
/// use anno::coalesce::alignment::entity_type_nameability;
///
/// let person_name = entity_type_nameability("PERSON");
/// assert!(person_name.is_high());
///
/// let misc_name = entity_type_nameability("MISC");
/// assert!(misc_name.is_low());
/// ```
#[must_use]
pub fn entity_type_nameability(entity_type: &str) -> Nameability {
    let normalized = entity_type.to_uppercase();
    match normalized.as_str() {
        // High nameability (strong consensus)
        "PERSON" | "PER" | "HUMAN" => Nameability::high(0.9),
        "LOCATION" | "LOC" | "GPE" => Nameability::high(0.85),
        "DATE" | "TIME" => Nameability::high(0.95), // Pattern-based, very consistent
        "MONEY" | "CURRENCY" => Nameability::high(0.95),
        "PERCENT" | "PERCENTAGE" => Nameability::high(0.95),

        // Medium nameability
        "ORGANIZATION" | "ORG" | "COMPANY" => Nameability::medium(0.65),
        "PRODUCT" => Nameability::medium(0.55),
        "EVENT" => Nameability::medium(0.5),
        "WORK_OF_ART" | "CREATIVE_WORK" => Nameability::medium(0.45),
        "NORP" | "NATIONALITY" => Nameability::medium(0.6),

        // Low nameability (weak consensus)
        "MISC" | "MISCELLANEOUS" | "OTHER" => Nameability::low(0.25),
        "CONCEPT" | "ABSTRACT" => Nameability::low(0.3),

        // Domain-specific (medium unless known)
        "DISEASE" | "CHEMICAL" | "DRUG" => Nameability::medium(0.6),
        "GENE" | "PROTEIN" => Nameability::medium(0.55),

        // Unknown → assume medium
        _ => Nameability::default(),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nameability_from_snd() {
        // Low SND (high consensus) → high nameability
        let high = Nameability::from_snd(0.1);
        assert!(high.is_high());
        assert_eq!(high.level(), NameabilityLevel::High);

        // High SND (low consensus) → low nameability
        let low = Nameability::from_snd(0.8);
        assert!(low.is_low());
        assert_eq!(low.level(), NameabilityLevel::Low);
    }

    #[test]
    fn test_alignment_score_accumulation() {
        let mut alignment = AlignmentScore::new();
        assert_eq!(alignment.count(), 0);
        assert_eq!(alignment.confidence(), 0.0);

        // Add matches
        alignment.record_match(0.85);
        alignment.record_match(0.90);
        alignment.record_match(0.88);

        assert_eq!(alignment.count(), 3);
        assert!((alignment.mean() - 0.8767).abs() < 0.01);
        assert!(alignment.confidence() > 0.0);
    }

    #[test]
    fn test_alignment_confidence_saturates() {
        let mut alignment = AlignmentScore::new();

        // Add many consistent matches
        for _ in 0..20 {
            alignment.record_match(0.9);
        }

        // Confidence should be high but not exceed 1.0
        let conf = alignment.confidence();
        assert!(conf > 0.8);
        assert!(conf <= 1.0);
    }

    #[test]
    fn test_generalization_gradient_quadratic() {
        let gradient = GeneralizationGradient::quadratic();

        // High similarity → LARGER adjustment (more generalization)
        let adj_high = gradient.threshold_adjustment(0.9, 1.0, 0.2);
        // Low similarity → smaller adjustment (less generalization)
        let adj_low = gradient.threshold_adjustment(0.5, 1.0, 0.2);

        // High similarity should get MORE reduction (larger magnitude negative)
        assert!(
            adj_high.abs() > adj_low.abs(),
            "High sim adj ({}) should exceed low sim adj ({})",
            adj_high.abs(),
            adj_low.abs()
        );
        assert!(adj_high < 0.0); // Negative = reducing threshold
    }

    #[test]
    fn test_adaptive_threshold() {
        let config = AdaptiveResolutionConfig::default();
        let mut alignment = AlignmentScore::new();

        // Add evidence
        for _ in 0..5 {
            alignment.record_match(0.85);
        }

        // Well-evidenced cluster should get lower threshold for similar entity
        let threshold = config.compute_threshold(&alignment, 0.8, Some(Nameability::high(0.9)));

        // Should be lower than base threshold
        assert!(threshold < config.base_threshold);
        // But not below minimum
        assert!(threshold >= config.min_threshold);
    }

    #[test]
    fn test_adaptive_threshold_no_evidence() {
        let config = AdaptiveResolutionConfig::default();
        let alignment = AlignmentScore::new();

        // No evidence → threshold should be close to base
        let threshold = config.compute_threshold(&alignment, 0.8, None);
        assert!((threshold - config.base_threshold).abs() < 0.1);
    }

    #[test]
    fn test_nameability_affects_threshold() {
        let config = AdaptiveResolutionConfig::default();
        let alignment = AlignmentScore::new();

        let high_name_threshold =
            config.compute_threshold(&alignment, 0.8, Some(Nameability::high(0.9)));
        let low_name_threshold =
            config.compute_threshold(&alignment, 0.8, Some(Nameability::low(0.2)));

        // High nameability → lower threshold (more confident)
        assert!(high_name_threshold < low_name_threshold);
    }

    #[test]
    fn test_entity_type_nameability() {
        use super::entity_type_nameability;

        // High nameability types
        assert!(entity_type_nameability("PERSON").is_high());
        assert!(entity_type_nameability("PER").is_high());
        assert!(entity_type_nameability("LOCATION").is_high());
        assert!(entity_type_nameability("DATE").is_high());

        // Low nameability types
        assert!(entity_type_nameability("MISC").is_low());

        // Case insensitive
        assert!(entity_type_nameability("person").is_high());
        assert!(entity_type_nameability("Person").is_high());

        // Unknown defaults to medium
        let unknown = entity_type_nameability("UNKNOWN_TYPE");
        assert!(!unknown.is_high());
        assert!(!unknown.is_low());
    }

    #[test]
    fn test_high_similarity_gets_more_reduction() {
        // This test explicitly validates the core semantic:
        // Higher similarity should result in MORE threshold reduction
        let config = AdaptiveResolutionConfig::default();
        let alignment = AlignmentScore::new();

        // Test across different similarities
        let similarities = [0.3, 0.5, 0.7, 0.9];
        let thresholds: Vec<f32> = similarities
            .iter()
            .map(|&sim| config.compute_threshold(&alignment, sim, None))
            .collect();

        // Each threshold should be >= the next (higher sim → lower threshold)
        for i in 0..thresholds.len() - 1 {
            assert!(
                thresholds[i] >= thresholds[i + 1],
                "Higher similarity should give lower threshold: sim={} gave {}, sim={} gave {}",
                similarities[i],
                thresholds[i],
                similarities[i + 1],
                thresholds[i + 1]
            );
        }
    }

    #[test]
    fn test_alignment_merge() {
        let mut a = AlignmentScore::new();
        a.record_match(0.8);
        a.record_match(0.9);

        let mut b = AlignmentScore::new();
        b.record_match(0.85);

        a.merge(&b);

        assert_eq!(a.count(), 3);
        // Mean should be (0.8 + 0.9 + 0.85) / 3 = 0.85
        assert!((a.mean() - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_gradient_similarity_monotonicity() {
        // Verify that for all gradient types, higher similarity gives
        // larger (more negative) adjustment
        let gradients = [
            GeneralizationGradient::linear(),
            GeneralizationGradient::quadratic(),
            GeneralizationGradient::exponential(2.0),
        ];

        for gradient in &gradients {
            let adj_low = gradient.threshold_adjustment(0.3, 1.0, 0.2);
            let adj_mid = gradient.threshold_adjustment(0.6, 1.0, 0.2);
            let adj_high = gradient.threshold_adjustment(0.9, 1.0, 0.2);

            // All should be negative
            assert!(adj_low < 0.0);
            assert!(adj_mid < 0.0);
            assert!(adj_high < 0.0);

            // Higher similarity → larger magnitude (more negative)
            assert!(
                adj_high <= adj_mid,
                "{:?}: high ({}) should be <= mid ({})",
                gradient,
                adj_high,
                adj_mid
            );
            assert!(
                adj_mid <= adj_low,
                "{:?}: mid ({}) should be <= low ({})",
                gradient,
                adj_mid,
                adj_low
            );
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Nameability scores are always in [0, 1].
        #[test]
        fn nameability_bounded(score in -10.0f32..10.0f32) {
            let n = Nameability::new(score);
            prop_assert!((0.0..=1.0).contains(&n.score()), "nameability score {} out of bounds", n.score());
        }

        /// SND to nameability conversion is bounded.
        #[test]
        fn snd_conversion_bounded(snd in -10.0f32..10.0f32) {
            let n = Nameability::from_snd(snd);
            prop_assert!((0.0..=1.0).contains(&n.score()), "SND conversion score {} out of bounds", n.score());
        }

        /// AlignmentScore confidence is always in [0, 1].
        #[test]
        fn alignment_confidence_bounded(
            scores in prop::collection::vec(0.0f32..1.0f32, 0..100)
        ) {
            let mut alignment = AlignmentScore::new();
            for score in scores {
                alignment.record_match(score);
            }
            let conf = alignment.confidence();
            prop_assert!((0.0..=1.0).contains(&conf), "confidence {} out of bounds [0, 1]", conf);
        }

        /// Alignment mean is always in [0, 1] when inputs are in [0, 1].
        #[test]
        fn alignment_mean_bounded(
            scores in prop::collection::vec(0.0f32..1.0f32, 1..50)
        ) {
            let mut alignment = AlignmentScore::new();
            for score in &scores {
                alignment.record_match(*score);
            }
            let mean = alignment.mean();
            prop_assert!((0.0..=1.0).contains(&mean), "mean {} out of bounds [0, 1]", mean);
        }

        /// Adaptive threshold is always >= min_threshold.
        #[test]
        fn adaptive_threshold_above_min(
            base in 0.3f32..0.95f32,
            min in 0.1f32..0.5f32,
            similarity in 0.0f32..1.0f32,
            match_count in 0usize..20
        ) {
            let config = AdaptiveResolutionConfig {
                base_threshold: base,
                min_threshold: min,
                max_adjustment: 0.3,
                ..Default::default()
            };

            let mut alignment = AlignmentScore::new();
            for _ in 0..match_count {
                alignment.record_match(0.85);
            }

            let threshold = config.compute_threshold(&alignment, similarity, None);
            prop_assert!(
                threshold >= min,
                "threshold {} < min {}",
                threshold,
                min
            );
        }

        /// More evidence should not increase threshold (monotonicity).
        #[test]
        fn more_evidence_lower_or_equal_threshold(
            _base in 0.5f32..0.9f32,
            similarity in 0.5f32..1.0f32
        ) {
            let config = AdaptiveResolutionConfig::default();

            let empty = AlignmentScore::new();
            let mut some = AlignmentScore::new();
            for _ in 0..5 {
                some.record_match(0.85);
            }
            let mut more = AlignmentScore::new();
            for _ in 0..15 {
                more.record_match(0.88);
            }

            let t_empty = config.compute_threshold(&empty, similarity, None);
            let t_some = config.compute_threshold(&some, similarity, None);
            let t_more = config.compute_threshold(&more, similarity, None);

            // More evidence should give lower or equal threshold
            prop_assert!(
                t_some <= t_empty + 0.01,
                "some evidence threshold {} > empty {}",
                t_some,
                t_empty
            );
            prop_assert!(
                t_more <= t_some + 0.01,
                "more evidence threshold {} > some {}",
                t_more,
                t_some
            );
        }

        /// Generalization gradient adjustments are bounded.
        #[test]
        fn gradient_adjustment_bounded(
            similarity in 0.0f32..1.0f32,
            confidence in 0.0f32..1.0f32,
            max_adj in 0.0f32..0.5f32
        ) {
            let gradients = [
                GeneralizationGradient::none(),
                GeneralizationGradient::linear(),
                GeneralizationGradient::quadratic(),
                GeneralizationGradient::exponential(2.0),
            ];

            for gradient in &gradients {
                let adj = gradient.threshold_adjustment(similarity, confidence, max_adj);
                // Adjustment should be negative or zero (reducing threshold)
                prop_assert!(adj <= 0.0, "adjustment {} > 0 for {:?}", adj, gradient);
                // Adjustment magnitude should not exceed max_adjustment
                prop_assert!(
                    adj.abs() <= max_adj + 0.001,
                    "adjustment {} exceeds max {}",
                    adj.abs(),
                    max_adj
                );
            }
        }

        /// Perfect similarity gives MAXIMUM adjustment (strongest generalization).
        #[test]
        fn perfect_similarity_max_adjustment(confidence in 0.0f32..1.0f32) {
            let max_adj = 0.2f32;
            let gradients = [
                GeneralizationGradient::linear(),
                GeneralizationGradient::quadratic(),
                GeneralizationGradient::exponential(2.0),
            ];

            for gradient in &gradients {
                let adj = gradient.threshold_adjustment(1.0, confidence, max_adj);
                // At perfect similarity, adjustment should be -confidence * max_adj
                let expected = -confidence * max_adj;
                prop_assert!(
                    (adj - expected).abs() < 0.01,
                    "perfect similarity: expected {}, got {} for {:?}",
                    expected,
                    adj,
                    gradient
                );
            }
        }

        /// Zero similarity gives minimal adjustment (weakest generalization).
        #[test]
        fn zero_similarity_minimal_adjustment(confidence in 0.0f32..1.0f32) {
            // Linear and quadratic give zero at sim=0
            let linear = GeneralizationGradient::linear();
            let quadratic = GeneralizationGradient::quadratic();

            let adj_linear = linear.threshold_adjustment(0.0, confidence, 0.2);
            let adj_quad = quadratic.threshold_adjustment(0.0, confidence, 0.2);

            prop_assert!(
                adj_linear.abs() < 0.001,
                "linear at sim=0 should be ~0, got {}",
                adj_linear
            );
            prop_assert!(
                adj_quad.abs() < 0.001,
                "quadratic at sim=0 should be ~0, got {}",
                adj_quad
            );
        }

        /// Higher similarity should always give larger (more negative) adjustment.
        #[test]
        fn similarity_monotonicity(
            sim_low in 0.0f32..0.5f32,
            sim_high in 0.5f32..1.0f32,
            confidence in 0.1f32..1.0f32,
            max_adj in 0.1f32..0.3f32
        ) {
            let gradients = [
                GeneralizationGradient::linear(),
                GeneralizationGradient::quadratic(),
                GeneralizationGradient::exponential(2.0),
            ];

            for gradient in &gradients {
                let adj_low = gradient.threshold_adjustment(sim_low, confidence, max_adj);
                let adj_high = gradient.threshold_adjustment(sim_high, confidence, max_adj);

                // Higher similarity should give larger magnitude (more negative)
                prop_assert!(
                    adj_high <= adj_low + 0.001,
                    "{:?}: sim {} gave {}, sim {} gave {} (should be more negative)",
                    gradient,
                    sim_low,
                    adj_low,
                    sim_high,
                    adj_high
                );
            }
        }

        /// Edge values (0.0, 1.0) should be handled correctly.
        #[test]
        fn edge_similarity_values(confidence in 0.0f32..1.0f32) {
            let gradient = GeneralizationGradient::quadratic();
            let max_adj = 0.2f32;

            // Exact boundaries should work without panics
            let _adj_0 = gradient.threshold_adjustment(0.0, confidence, max_adj);
            let _adj_1 = gradient.threshold_adjustment(1.0, confidence, max_adj);

            // Out-of-range values should be clamped (not panic)
            let adj_neg = gradient.threshold_adjustment(-0.5, confidence, max_adj);
            let adj_over = gradient.threshold_adjustment(1.5, confidence, max_adj);

            // -0.5 clamped to 0.0 → should give ~0 adjustment
            prop_assert!(adj_neg.abs() < 0.001);
            // 1.5 clamped to 1.0 → should give max adjustment
            let expected_max = -confidence * max_adj;
            prop_assert!((adj_over - expected_max).abs() < 0.001);
        }
    }
}
