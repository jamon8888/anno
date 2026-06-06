//! Deterministic post-aggregator validators for PII entities.
//!
//! Each validator targets one label and either accepts, rejects, or
//! adjusts the confidence of a `DetectedEntity`. Rejections are
//! aggregated into counters that are emitted via the detect audit
//! event so operators can monitor false-positive suppression rates
//! without seeing the underlying text.

use cloakpipe_core::{DetectedEntity, EntityCategory};
use std::collections::BTreeMap;

/// Date-range validator (rejects implausible years).
pub mod dates;
/// RFC-light email-address validator.
pub mod email;
/// IBAN mod-97 (ISO 13616) checksum validator.
pub mod iban;
/// Luhn checksum validator (SIRET, card numbers).
pub mod luhn;
/// IP-address validator (IPv4/IPv6).
pub mod network;
/// French NIR control-key validator (with Corsica 2A/2B handling).
pub mod nir;
/// French postal-code validator (mainland + DOM).
pub mod postal;

/// Outcome of a single validator on a single entity.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Pass-through, no change.
    Accept,
    /// Drop the entity. `reason` is a static identifier (e.g. "luhn_failed").
    Reject {
        /// Static identifier for the rejection cause (used as a counter key).
        reason: &'static str,
    },
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

/// Apply a chain of validators to a list of entities. Only validators whose
/// `label()` matches the entity's category string are run. First `Reject`
/// short-circuits the chain for that entity. Last `AdjustConfidence` wins.
///
/// Returns `(kept, rejection_counts)`.
pub fn apply_validators(
    entities: Vec<DetectedEntity>,
    text: &str,
    validators: &[Box<dyn EntityValidator>],
) -> (Vec<DetectedEntity>, RejectionCounts) {
    let mut kept = Vec::with_capacity(entities.len());
    let mut counts: RejectionCounts = BTreeMap::new();
    'outer: for mut entity in entities {
        let label = entity_label_str(&entity).to_owned();
        for v in validators.iter().filter(|v| v.label() == label.as_str()) {
            match v.validate(&entity, text) {
                ValidationResult::Accept => {}
                ValidationResult::Reject { reason } => {
                    *counts.entry(reason).or_insert(0) += 1;
                    continue 'outer;
                }
                ValidationResult::AdjustConfidence(c) => {
                    entity.confidence = f64::from(c);
                }
            }
        }
        kept.push(entity);
    }
    (kept, counts)
}

fn entity_label_str(e: &DetectedEntity) -> &str {
    match &e.category {
        EntityCategory::Custom(name) => name.as_str(),
        EntityCategory::Person => "person",
        EntityCategory::Organization => "organization",
        EntityCategory::Location => "location",
        EntityCategory::Email => "email_address",
        EntityCategory::PhoneNumber => "phone_number",
        EntityCategory::IpAddress => "ip_address",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    #[derive(Debug)]
    struct AlwaysAccept;
    impl EntityValidator for AlwaysAccept {
        fn label(&self) -> &'static str {
            "test"
        }
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

    #[test]
    fn orchestrator_rejects_failing_luhn_keeps_others() {
        use cloakpipe_core::{DetectionSource, EntityCategory};
        let validators: Vec<Box<dyn EntityValidator>> =
            vec![Box::new(luhn::LuhnValidator::new("SIRET"))];
        let make = |cat: EntityCategory, val: &str| DetectedEntity {
            original: val.to_string(),
            start: 0,
            end: val.len(),
            category: cat,
            confidence: 0.9,
            source: DetectionSource::Ner,
        };
        let entities = vec![
            make(EntityCategory::Custom("SIRET".into()), "73282932000074"), // valid
            make(EntityCategory::Custom("SIRET".into()), "12345678901234"), // invalid
            make(EntityCategory::Person, "Jean Dupont"),                    // unrelated, passes
        ];
        let (kept, counts) = apply_validators(entities, "", &validators);
        assert_eq!(kept.len(), 2);
        assert_eq!(counts.get("luhn_failed"), Some(&1));
    }

    #[test]
    fn orchestrator_empty_validators_keeps_all() {
        use cloakpipe_core::{DetectionSource, EntityCategory};
        let e = DetectedEntity {
            original: "anything".into(),
            start: 0,
            end: 8,
            category: EntityCategory::Custom("x".into()),
            confidence: 0.5,
            source: DetectionSource::Ner,
        };
        let (kept, counts) = apply_validators(vec![e], "", &[]);
        assert_eq!(kept.len(), 1);
        assert!(counts.is_empty());
    }
}
