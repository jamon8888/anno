use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct PostalCodeValidator;

impl EntityValidator for PostalCodeValidator {
    fn label(&self) -> &'static str {
        "postal_code"
    }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let s = e.original.trim();
        if s.len() != 5 || !s.chars().all(|c| c.is_ascii_digit()) {
            return ValidationResult::Reject {
                reason: "postal_format",
            };
        }
        let n: u32 = s.parse().expect("digits-only above");
        if (1000..=95999).contains(&n) || (97000..=98999).contains(&n) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject {
                reason: "postal_range",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};

    fn ent(v: &str) -> DetectedEntity {
        DetectedEntity {
            original: v.to_string(),
            start: 0,
            end: v.len(),
            category: EntityCategory::Custom("postal_code".into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_mainland() {
        assert_eq!(
            PostalCodeValidator.validate(&ent("75001"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_dom() {
        assert_eq!(
            PostalCodeValidator.validate(&ent("97400"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn rejects_00xxx() {
        assert!(matches!(
            PostalCodeValidator.validate(&ent("00100"), ""),
            ValidationResult::Reject {
                reason: "postal_range"
            }
        ));
    }

    #[test]
    fn rejects_letters() {
        assert!(matches!(
            PostalCodeValidator.validate(&ent("ABCDE"), ""),
            ValidationResult::Reject {
                reason: "postal_format"
            }
        ));
    }

    #[test]
    fn rejects_too_short() {
        assert!(matches!(
            PostalCodeValidator.validate(&ent("1234"), ""),
            ValidationResult::Reject {
                reason: "postal_format"
            }
        ));
    }
}
