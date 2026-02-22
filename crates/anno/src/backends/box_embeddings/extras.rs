//! BoxCorefConfig, TemporalBox, BoxVelocity, UncertainBox, and related types.

use super::*;

pub struct BoxCorefConfig {
    /// Minimum coreference score to link entities
    pub coreference_threshold: f32,
    /// Whether to enforce syntactic constraints (Principle B/C)
    pub enforce_syntactic_constraints: bool,
    /// Maximum token distance for local domain (Principle B)
    pub max_local_distance: usize,
    /// Radius for converting vector embeddings to boxes (if using vectors)
    pub vector_to_box_radius: Option<f32>,
}

impl Default for BoxCorefConfig {
    fn default() -> Self {
        Self {
            coreference_threshold: 0.7,
            enforce_syntactic_constraints: true,
            max_local_distance: 5,
            vector_to_box_radius: Some(0.1),
        }
    }
}

// =============================================================================
// Temporal Boxes (BoxTE-style)
// =============================================================================

/// A temporal box embedding that evolves over time.
///
/// Based on BoxTE (Messner et al., 2022), this models entities that change
/// over time. For example, "The President" refers to Obama in 2012 but
/// Trump in 2017 - they should not corefer despite the same surface form.
///
/// # Example
///
/// ```rust,ignore
/// use anno::backends::box_embeddings::{BoxEmbedding, TemporalBox, BoxVelocity};
///
/// // "The President" in 2012 (Obama)
/// let base = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
/// let velocity = BoxVelocity::new(vec![0.0, 0.0], vec![0.0, 0.0]); // Static
/// let obama_presidency = TemporalBox::new(base, velocity, (2012.0, 2016.0));
///
/// // "The President" in 2017 (Trump)
/// let trump_base = BoxEmbedding::new(vec![5.0, 5.0], vec![6.0, 6.0]);
/// let trump_presidency = TemporalBox::new(trump_base, velocity, (2017.0, 2021.0));
///
/// // Should not corefer (different time ranges)
/// assert_eq!(obama_presidency.coreference_at_time(&trump_presidency, 2015.0), 0.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalBox {
    /// Base box at time t=0 (or reference time)
    pub base: BoxEmbedding,
    /// Velocity: how box moves/resizes per time unit
    pub velocity: BoxVelocity,
    /// Time range where this box is valid [start, end)
    pub time_range: (f64, f64),
}

/// Velocity of a temporal box (change per time unit).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxVelocity {
    /// Change in min bounds per time unit (d-dimensional vector).
    pub min_delta: Vec<f32>,
    /// Change in max bounds per time unit (d-dimensional vector).
    pub max_delta: Vec<f32>,
}

impl BoxVelocity {
    /// Create a new box velocity (static by default).
    #[must_use]
    pub fn new(min_delta: Vec<f32>, max_delta: Vec<f32>) -> Self {
        Self {
            min_delta,
            max_delta,
        }
    }

    /// Create a static velocity (no change over time).
    #[must_use]
    pub fn static_velocity(dim: usize) -> Self {
        Self {
            min_delta: vec![0.0; dim],
            max_delta: vec![0.0; dim],
        }
    }
}

impl TemporalBox {
    /// Create a new temporal box.
    ///
    /// # Arguments
    ///
    /// * `base` - Base box at reference time
    /// * `velocity` - How box evolves per time unit
    /// * `time_range` - (start, end) time range where box is valid
    #[must_use]
    pub fn new(base: BoxEmbedding, velocity: BoxVelocity, time_range: (f64, f64)) -> Self {
        assert_eq!(
            base.dim(),
            velocity.min_delta.len(),
            "base and velocity must have same dimension"
        );
        assert_eq!(
            velocity.min_delta.len(),
            velocity.max_delta.len(),
            "velocity min and max deltas must have same dimension"
        );
        Self {
            base,
            velocity,
            time_range,
        }
    }

    /// Get the box at a specific time.
    ///
    /// Returns None if time is outside the valid range.
    #[must_use]
    pub fn at_time(&self, time: f64) -> Option<BoxEmbedding> {
        if time < self.time_range.0 || time >= self.time_range.1 {
            return None;
        }

        // Compute time offset from reference (using start of range as reference)
        let time_offset = time - self.time_range.0;

        // Apply velocity to base box
        let new_min: Vec<f32> = self
            .base
            .min
            .iter()
            .zip(self.velocity.min_delta.iter())
            .map(|(&m, &delta)| m + delta * time_offset as f32)
            .collect();

        let new_max: Vec<f32> = self
            .base
            .max
            .iter()
            .zip(self.velocity.max_delta.iter())
            .map(|(&max_val, &delta)| max_val + delta * time_offset as f32)
            .collect();

        Some(BoxEmbedding::new(new_min, new_max))
    }

    /// Compute coreference score at a specific time.
    ///
    /// Returns 0.0 if either box is invalid at the given time.
    #[must_use]
    pub fn coreference_at_time(&self, other: &Self, time: f64) -> f32 {
        let box_a = match self.at_time(time) {
            Some(b) => b,
            None => return 0.0,
        };
        let box_b = match other.at_time(time) {
            Some(b) => b,
            None => return 0.0,
        };
        box_a.coreference_score(&box_b)
    }

    /// Check if this temporal box is valid at the given time.
    #[must_use]
    pub fn is_valid_at(&self, time: f64) -> bool {
        time >= self.time_range.0 && time < self.time_range.1
    }
}

// =============================================================================
// Uncertainty-Aware Boxes (UKGE-style)
// =============================================================================

/// An uncertainty-aware box embedding (UKGE-style).
///
/// Based on UKGE (Chen et al., 2021), box volume represents confidence:
/// - Small box = high confidence (precise, trusted fact)
/// - Large box = low confidence (vague, uncertain, or dubious claim)
///
/// This enables conflict detection: if two high-confidence boxes are disjoint,
/// they represent contradictory claims.
///
/// # Example
///
/// ```rust,ignore
/// use anno::backends::box_embeddings::{BoxEmbedding, UncertainBox};
///
/// // High-confidence claim: "Trump is in NY" (small, precise box)
/// let claim_a = UncertainBox::new(
///     BoxEmbedding::new(vec![0.0, 0.0], vec![0.1, 0.1]), // Small = high confidence
///     0.95, // Source trust
/// );
///
/// // Contradictory claim: "Trump is in FL" (also high confidence, but disjoint)
/// let claim_b = UncertainBox::new(
///     BoxEmbedding::new(vec![5.0, 5.0], vec![5.1, 5.1]), // Disjoint from claim_a
///     0.90,
/// );
///
/// // Should detect conflict
/// assert!(claim_a.detect_conflict(&claim_b).is_some());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct UncertainBox {
    /// The underlying box embedding
    pub box_embedding: BoxEmbedding,
    /// Source trustworthiness (0.0-1.0)
    pub source_trust: f32,
}

impl UncertainBox {
    /// Create a new uncertainty-aware box.
    ///
    /// Confidence is derived from box volume (smaller = higher confidence).
    #[must_use]
    pub fn new(box_embedding: BoxEmbedding, source_trust: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&source_trust),
            "source_trust must be in [0.0, 1.0]"
        );
        Self {
            box_embedding,
            source_trust,
        }
    }

    /// Get confidence derived from box volume.
    ///
    /// Smaller boxes = higher confidence. This is a heuristic:
    /// confidence ≈ 1.0 / (1.0 + volume)
    #[must_use]
    pub fn confidence(&self) -> f32 {
        let vol = self.box_embedding.volume();
        // Normalize: confidence decreases as volume increases
        // Using sigmoid-like function: 1 / (1 + volume)
        1.0 / (1.0 + vol)
    }

    /// Detect conflict with another uncertain box.
    ///
    /// Returns Some(Conflict) if both boxes are high-confidence but disjoint,
    /// indicating contradictory claims.
    #[must_use]
    pub fn detect_conflict(&self, other: &Self) -> Option<Conflict> {
        let overlap = self.box_embedding.intersection_volume(&other.box_embedding);
        let min_vol = self
            .box_embedding
            .volume()
            .min(other.box_embedding.volume());

        // If both are high-confidence (small volume) but disjoint, conflict
        let conf_a = self.confidence();
        let conf_b = other.confidence();
        let threshold = 0.8;

        if overlap < min_vol * 0.1 && conf_a > threshold && conf_b > threshold {
            Some(Conflict {
                claim_a_trust: self.source_trust,
                claim_b_trust: other.source_trust,
                severity: (1.0 - overlap / min_vol.max(1e-6)) * (conf_a + conf_b) / 2.0,
            })
        } else {
            None
        }
    }
}

/// Represents a conflict between two uncertain claims.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// Trust in first claim's source
    pub claim_a_trust: f32,
    /// Trust in second claim's source
    pub claim_b_trust: f32,
    /// Severity of conflict (0.0-1.0, higher = more severe)
    pub severity: f32,
}

// =============================================================================
// Interaction Modeling (Triple Intersection)
// =============================================================================

/// Compute interaction strength between actor, action, and target.
///
/// Models asymmetric relations (e.g., "Company A acquired Company B")
/// via triple intersection volume. The interaction is the volume where
/// all three boxes overlap.
///
/// # Arguments
///
/// * `actor_box` - Box for the actor (e.g., buyer)
/// * `action_box` - Box for the action/relation (e.g., "acquired")
/// * `target_box` - Box for the target (e.g., company being acquired)
///
/// # Returns
///
/// Conditional probability P(action, target | actor), representing
/// how much of the actor's space contains the interaction.
#[must_use]
pub fn interaction_strength(
    actor_box: &BoxEmbedding,
    action_box: &BoxEmbedding,
    target_box: &BoxEmbedding,
) -> f32 {
    // Triple intersection: where all three boxes overlap
    // For simplicity, we compute pairwise intersections and take minimum
    // In full implementation, would compute true 3-way intersection
    let actor_action = actor_box.intersection_volume(action_box);
    let action_target = action_box.intersection_volume(target_box);
    let actor_target = actor_box.intersection_volume(target_box);

    // Interaction volume ≈ minimum of pairwise intersections
    let interaction_vol = actor_action.min(action_target).min(actor_target);

    // P(interaction | actor) = interaction_vol / vol(actor)
    let vol_actor = actor_box.volume();
    if vol_actor == 0.0 {
        return 0.0;
    }
    interaction_vol / vol_actor
}

/// Compute asymmetric roles in a relation.
///
/// For a relation like "acquired", determines which entity is the
/// buyer vs. seller based on conditional probabilities.
///
/// # Returns
///
/// (buyer_role, seller_role) where each is the interaction strength
/// for that role.
#[must_use]
pub fn acquisition_roles(
    entity_a: &BoxEmbedding,
    entity_b: &BoxEmbedding,
    acquisition_box: &BoxEmbedding,
) -> (f32, f32) {
    let buyer_role = interaction_strength(entity_a, acquisition_box, entity_b);
    let seller_role = interaction_strength(entity_b, acquisition_box, entity_a);
    (buyer_role, seller_role)
}

// =============================================================================
// Gumbel Boxes (Noise Robustness)
// =============================================================================

/// A Gumbel box with soft, probabilistic boundaries.
///
/// Instead of hard walls, boundaries are modeled as Gumbel distributions,
/// creating "fuzzy" boxes that tolerate slight misalignments. This prevents
/// brittle logic failures when data is noisy.
///
/// # Example
///
/// ```rust,ignore
/// use anno::backends::box_embeddings::{BoxEmbedding, GumbelBox};
///
/// let mean_box = BoxEmbedding::new(vec![0.0, 0.0], vec![1.0, 1.0]);
/// let gumbel_box = GumbelBox::new(mean_box, 0.1); // Low temperature = sharp
///
/// // Membership is probabilistic, not binary
/// let point = vec![0.5, 0.5];
/// let prob = gumbel_box.membership_probability(&point);
/// assert!(prob > 0.5); // High probability inside box
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GumbelBox {
    /// Mean box boundaries (lower bounds)
    pub mean_min: Vec<f32>,
    /// Mean box boundaries (upper bounds)
    pub mean_max: Vec<f32>,
    /// Temperature: controls fuzziness (higher = more fuzzy)
    /// Typical values: 0.01-0.1 for sharp, 0.5-1.0 for fuzzy
    pub temperature: f32,
}

impl GumbelBox {
    /// Create a new Gumbel box.
    #[must_use]
    pub fn new(mean_box: BoxEmbedding, temperature: f32) -> Self {
        assert!(
            temperature > 0.0,
            "temperature must be positive, got {}",
            temperature
        );
        Self {
            mean_min: mean_box.min,
            mean_max: mean_box.max,
            temperature,
        }
    }

    /// Compute membership probability for a point.
    ///
    /// Returns probability that point belongs to this box (0.0-1.0).
    /// Uses Gumbel CDF approximation for soft boundaries.
    #[must_use]
    pub fn membership_probability(&self, point: &[f32]) -> f32 {
        assert_eq!(
            point.len(),
            self.mean_min.len(),
            "point dimension must match box dimension"
        );

        let mut prob = 1.0;
        for (i, &coord) in point.iter().enumerate() {
            // Gumbel CDF approximation: P(x < max) ≈ 1 / (1 + exp(-(max - x) / temp))
            // For min boundary: P(x > min) ≈ 1 / (1 + exp(-(x - min) / temp))
            let min_prob = 1.0 / (1.0 + (-(coord - self.mean_min[i]) / self.temperature).exp());
            let max_prob = 1.0 / (1.0 + (-(self.mean_max[i] - coord) / self.temperature).exp());
            prob *= min_prob * max_prob;
        }
        prob
    }

    /// Compute robust coreference score with another Gumbel box.
    ///
    /// Samples points from self and checks membership in other, averaging
    /// probabilities. This tolerates slight misalignments.
    ///
    /// # Arguments
    ///
    /// * `other` - The other Gumbel box to compare against
    /// * `samples` - Number of sample points to use (more = more accurate but slower)
    /// * `rng` - Optional RNG for sampling. If None, uses deterministic grid sampling.
    #[must_use]
    pub fn robust_coreference(&self, other: &Self, samples: usize) -> f32 {
        assert_eq!(
            self.mean_min.len(),
            other.mean_min.len(),
            "boxes must have same dimension"
        );

        // Deterministic grid sampling (no RNG dependency)
        // For each dimension, sample at regular intervals
        let samples_per_dim = (samples as f32)
            .powf(1.0 / self.mean_min.len() as f32)
            .ceil() as usize;
        let mut total_prob = 0.0;
        let mut count = 0;

        // Generate grid points
        let mut indices = vec![0; self.mean_min.len()];
        loop {
            // Compute point from grid indices
            let point: Vec<f32> = self
                .mean_min
                .iter()
                .zip(self.mean_max.iter())
                .zip(indices.iter())
                .map(|((&min_val, &max_val), &idx)| {
                    let t = idx as f32 / (samples_per_dim - 1).max(1) as f32;
                    min_val + t * (max_val - min_val)
                })
                .collect();

            total_prob += other.membership_probability(&point);
            count += 1;

            // Increment grid indices
            let mut carry = true;
            for idx in &mut indices {
                if carry {
                    *idx += 1;
                    if *idx >= samples_per_dim {
                        *idx = 0;
                        carry = true;
                    } else {
                        carry = false;
                    }
                }
            }

            if carry || count >= samples {
                break;
            }
        }

        total_prob / count as f32
    }
}

// =============================================================================
// Subsume Trait Implementations for GumbelBox
// =============================================================================

#[cfg(feature = "subsume")]
impl subsume_core::Box for GumbelBox {
    type Scalar = f32;
    type Vector = Vec<f32>;

    fn min(&self) -> &Self::Vector {
        &self.mean_min
    }

    fn max(&self) -> &Self::Vector {
        &self.mean_max
    }

    fn dim(&self) -> usize {
        self.mean_min.len()
    }

    fn volume(&self, temperature: Self::Scalar) -> Result<Self::Scalar, subsume_core::BoxError> {
        // Use log-space volume approximation for Gumbel boxes
        let mut log_vol = 0.0;
        for i in 0..self.dim() {
            let diff = self.mean_max[i] - self.mean_min[i];
            // Softplus approximation: temp * log(1 + exp(x/temp))
            log_vol += (diff / temperature).exp().ln_1p() * temperature;
        }
        Ok(log_vol.exp())
    }

    fn intersection(&self, other: &Self) -> Result<Self, subsume_core::BoxError> {
        if self.dim() != other.dim() {
            return Err(subsume_core::BoxError::DimensionMismatch {
                expected: self.dim(),
                actual: other.dim(),
            });
        }

        // Gumbel intersection uses LSE for max-stability
        let mut new_min = Vec::with_capacity(self.dim());
        let mut new_max = Vec::with_capacity(self.dim());

        for i in 0..self.dim() {
            let m1 = self.mean_min[i];
            let m2 = other.mean_min[i];
            let lse_min =
                m1.max(m2) + self.temperature * (-(m1 - m2).abs() / self.temperature).exp().ln_1p();
            new_min.push(lse_min);

            let x1 = self.mean_max[i];
            let x2 = other.mean_max[i];
            let lse_max =
                x1.min(x2) - self.temperature * (-(x1 - x2).abs() / self.temperature).exp().ln_1p();
            new_max.push(lse_max);
        }

        Ok(GumbelBox {
            mean_min: new_min,
            mean_max: new_max,
            temperature: self.temperature,
        })
    }

    fn containment_prob(
        &self,
        other: &Self,
        temperature: Self::Scalar,
    ) -> Result<Self::Scalar, subsume_core::BoxError> {
        let intersection = self.intersection(other)?;
        let vol_int = intersection.volume(temperature)?;
        let vol_other = other.volume(temperature)?;
        if vol_other == 0.0 {
            return Ok(0.0);
        }
        Ok(vol_int / vol_other)
    }

    fn overlap_prob(
        &self,
        other: &Self,
        temperature: Self::Scalar,
    ) -> Result<Self::Scalar, subsume_core::BoxError> {
        let intersection = self.intersection(other)?;
        let vol_int = intersection.volume(temperature)?;
        let vol_self = self.volume(temperature)?;
        let vol_other = other.volume(temperature)?;
        let vol_union = vol_self + vol_other - vol_int;
        if vol_union <= 0.0 {
            return Ok(0.0);
        }
        Ok(vol_int / vol_union)
    }

    fn union(&self, other: &Self) -> Result<Self, subsume_core::BoxError> {
        let mut new_min = Vec::with_capacity(self.dim());
        let mut new_max = Vec::with_capacity(self.dim());
        for i in 0..self.dim() {
            new_min.push(self.mean_min[i].min(other.mean_min[i]));
            new_max.push(self.mean_max[i].max(other.mean_max[i]));
        }
        Ok(GumbelBox {
            mean_min: new_min,
            mean_max: new_max,
            temperature: self.temperature,
        })
    }

    fn center(&self) -> Result<Self::Vector, subsume_core::BoxError> {
        let mut center = Vec::with_capacity(self.dim());
        for i in 0..self.dim() {
            center.push((self.mean_min[i] + self.mean_max[i]) / 2.0);
        }
        Ok(center)
    }

    fn distance(&self, other: &Self) -> Result<Self::Scalar, subsume_core::BoxError> {
        let mut dist_sq = 0.0;
        for i in 0..self.dim() {
            let gap = if self.mean_max[i] < other.mean_min[i] {
                other.mean_min[i] - self.mean_max[i]
            } else if other.mean_max[i] < self.mean_min[i] {
                self.mean_min[i] - other.mean_max[i]
            } else {
                0.0
            };
            dist_sq += gap * gap;
        }
        Ok(dist_sq.sqrt())
    }
}

#[cfg(feature = "subsume")]
impl subsume_core::GumbelBox for GumbelBox {
    fn temperature(&self) -> Self::Scalar {
        self.temperature
    }

    fn membership_probability(
        &self,
        point: &Self::Vector,
    ) -> Result<Self::Scalar, subsume_core::BoxError> {
        Ok(self.membership_probability(point))
    }

    fn sample(&self) -> Self::Vector {
        self.center().unwrap_or_default()
    }
}

// Note: BoxCorefResolver is implemented in src/eval/coref_resolver.rs
// to be alongside other coreference resolvers.

