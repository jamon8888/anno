//! Deterministic post-aggregator validators for PII entities.
//!
//! Each validator targets one label and either accepts, rejects, or
//! adjusts the confidence of a `DetectedEntity`. Rejections are
//! aggregated into counters that are emitted via the detect audit
//! event so operators can monitor false-positive suppression rates
//! without seeing the underlying text.

use cloakpipe_core::DetectedEntity;
use std::collections::BTreeMap;

/// Outcome of a single validator on a single entity.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Pass-through, no change.
    Accept,
    /// Drop the entity. `reason` is a static identifier (e.g. "luhn_failed").
    Reject { reason: &'static str },
    /// Keep the entity but override its confidence to the given value.
    AdjustConfidence(f32),
}

/// A label-targeted deterministic check. Implementations must be `Send + Sync`
/// because the detector is shared between async tasks via `Arc`.
pub trait EntityValidator: Send + Sync + std::fmt::Debug {
    /// The label this validator applies to. Validators only run on
    /// `DetectedEntity` whose category string matches this label.
    fn label(&self) -> &'static str;

    /// Run the check. `ctx` is the original document text.
    fn validate(&self, entity: &DetectedEntity, ctx: &str) -> ValidationResult;
}

/// Counter for rejections, keyed by validator reason string.
pub type RejectionCounts = BTreeMap<&'static str, usize>;

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    #[derive(Debug)]
    struct AlwaysAccept;
    impl EntityValidator for AlwaysAccept {
        fn label(&self) -> &'static str { "test" }
        fn validate(&self, _e: &DetectedEntity, _c: &str) -> ValidationResult {
            ValidationResult::Accept
        }
    }

    #[test]
    fn trait_is_object_safe() {
        let _: Box<dyn EntityValidator> = Box::new(AlwaysAccept);
    }

    #[test]
    fn validation_result_equality() {
        assert_eq!(ValidationResult::Accept, ValidationResult::Accept);
        assert_eq!(
            ValidationResult::Reject { reason: "x" },
            ValidationResult::Reject { reason: "x" },
        );
        assert_ne!(
            ValidationResult::Reject { reason: "x" },
            ValidationResult::Reject { reason: "y" },
        );
    }
}
