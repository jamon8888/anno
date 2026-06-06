use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

/// Validates numbers via the Luhn checksum (SIRET, card numbers), scoped to one entity label.
#[derive(Debug, Clone, Copy)]
pub struct LuhnValidator {
    target_label: &'static str,
}

impl LuhnValidator {
    /// Build a validator bound to `label` (the entity category it applies to).
    pub const fn new(label: &'static str) -> Self {
        Self {
            target_label: label,
        }
    }
}

impl EntityValidator for LuhnValidator {
    fn label(&self) -> &'static str {
        self.target_label
    }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if luhn_check(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject {
                reason: "luhn_failed",
            }
        }
    }
}

fn luhn_check(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.is_empty() {
        return false;
    }
    let total: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                d
            }
        })
        .sum();
    total % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity {
            original: original.to_string(),
            start: 0,
            end: original.len(),
            category: EntityCategory::Custom("SIRET".into()),
            confidence: 0.9,
            source: DetectionSource::Pattern,
        }
    }

    #[test]
    fn luhn_accepts_known_valid_siret() {
        let e = entity_with("73282932000074");
        assert_eq!(
            LuhnValidator::new("SIRET").validate(&e, ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn luhn_rejects_random_14_digit_run() {
        let e = entity_with("12345678901234");
        assert_eq!(
            LuhnValidator::new("SIRET").validate(&e, ""),
            ValidationResult::Reject {
                reason: "luhn_failed"
            }
        );
    }

    #[test]
    fn luhn_rejects_empty_or_non_digit() {
        let v = LuhnValidator::new("SIRET");
        assert!(matches!(
            v.validate(&entity_with(""), ""),
            ValidationResult::Reject { .. }
        ));
        assert!(matches!(
            v.validate(&entity_with("abcdefg"), ""),
            ValidationResult::Reject { .. }
        ));
    }

    #[test]
    fn luhn_accepts_valid_card_number() {
        let e = entity_with("4111111111111111");
        assert_eq!(
            LuhnValidator::new("card_number").validate(&e, ""),
            ValidationResult::Accept
        );
    }
}
