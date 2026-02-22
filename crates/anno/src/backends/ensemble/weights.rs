//! Backend weighting, type weights, and candidate/span-key helpers.

use super::*;

// =============================================================================
// Backend Weights
// =============================================================================

/// Reliability weight for a backend (0.0 to 1.0).
///
/// Higher weight = more trusted when resolving conflicts.
#[derive(Debug, Clone, Copy)]
pub struct BackendWeight {
    /// Overall reliability of this backend
    pub overall: f64,
    /// Type-specific weights (optional overrides)
    pub per_type: Option<TypeWeights>,
}

impl Default for BackendWeight {
    fn default() -> Self {
        Self {
            overall: 0.5,
            per_type: None,
        }
    }
}

/// Type-specific reliability weights.
///
/// Different backends may have different accuracy profiles for different entity types.
/// These weights adjust confidence scores based on the entity type being extracted.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeWeights {
    /// Weight multiplier for Person entities
    pub person: f64,
    /// Weight multiplier for Organization entities
    pub organization: f64,
    /// Weight multiplier for Location entities
    pub location: f64,
    /// Weight multiplier for Date entities
    pub date: f64,
    /// Weight multiplier for Money entities
    pub money: f64,
    /// Weight multiplier for other/misc entity types
    pub other: f64,
}

impl TypeWeights {
    pub(super) fn get(&self, entity_type: &EntityType) -> f64 {
        match entity_type {
            EntityType::Person => self.person,
            EntityType::Organization => self.organization,
            EntityType::Location => self.location,
            EntityType::Date => self.date,
            EntityType::Money => self.money,
            _ => self.other,
        }
    }
}

/// Default weights based on empirical observations.
pub(super) fn default_backend_weights() -> HashMap<&'static str, BackendWeight> {
    let mut weights = HashMap::new();

    // Pattern backends: very high precision when they fire
    weights.insert(
        "regex",
        BackendWeight {
            overall: 0.98,
            per_type: Some(TypeWeights {
                date: 0.99,
                money: 0.99,
                person: 0.50, // Pattern doesn't do NER
                organization: 0.50,
                location: 0.50,
                other: 0.95, // URLs, emails, etc.
            }),
        },
    );

    // GLiNER: good ML-based NER
    weights.insert(
        "gliner",
        BackendWeight {
            overall: 0.85,
            per_type: Some(TypeWeights {
                person: 0.90,
                organization: 0.85,
                location: 0.80,
                date: 0.75,
                money: 0.70,
                other: 0.75,
            }),
        },
    );
    weights.insert(
        "GLiNER-ONNX",
        BackendWeight {
            overall: 0.85,
            per_type: Some(TypeWeights {
                person: 0.90,
                organization: 0.85,
                location: 0.80,
                date: 0.75,
                money: 0.70,
                other: 0.75,
            }),
        },
    );

    // GLiNER Candle
    weights.insert(
        "gliner-candle",
        BackendWeight {
            overall: 0.85,
            per_type: None,
        },
    );

    // BERT NER
    weights.insert(
        "bert-ner-onnx",
        BackendWeight {
            overall: 0.80,
            per_type: None,
        },
    );

    // Heuristic: reasonable but noisy
    weights.insert(
        "heuristic",
        BackendWeight {
            overall: 0.60,
            per_type: Some(TypeWeights {
                person: 0.65,       // Title + Name pattern works well
                organization: 0.70, // "Inc", "Corp" patterns
                location: 0.55,     // Context-dependent
                date: 0.40,         // Better to use pattern
                money: 0.40,
                other: 0.50,
            }),
        },
    );

    weights
}

// =============================================================================
// Candidate Entity (with source tracking)
// =============================================================================

/// An entity candidate from a specific backend.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) entity: Entity,
    pub(super) source: String,
    pub(super) backend_weight: f64,
}

// =============================================================================
// Span Key (for grouping overlapping entities)
// =============================================================================

/// Key for grouping entities by span.
///
/// Two entities are considered "same span" if they significantly overlap.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SpanKey {
    pub(super) start: usize,
    pub(super) end: usize,
}

impl SpanKey {
    pub(super) fn from_entity(e: &Entity) -> Self {
        Self {
            start: e.start,
            end: e.end,
        }
    }

    /// Check if two spans overlap significantly (>50% of smaller span).
    pub(super) fn overlaps(&self, other: &SpanKey) -> bool {
        let overlap_start = self.start.max(other.start);
        let overlap_end = self.end.min(other.end);

        if overlap_start >= overlap_end {
            return false;
        }

        let overlap = overlap_end - overlap_start;
        let smaller_span = (self.end - self.start).min(other.end - other.start);

        // Overlap if >50% of smaller span is covered
        (overlap as f64 / smaller_span as f64) > 0.5
    }
}

// =============================================================================
// EnsembleNER
// =============================================================================
