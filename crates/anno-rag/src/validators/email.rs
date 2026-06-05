use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

#[derive(Debug, Clone, Copy)]
pub struct EmailRfcValidator;

impl EntityValidator for EmailRfcValidator {
    fn label(&self) -> &'static str {
        "email_address"
    }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let s = e.original.trim();
        if s.len() < 3 || s.len() > 256 {
            return ValidationResult::Reject {
                reason: "email_length_out_of_range",
            };
        }
        let Some(at) = s.rfind('@') else {
            return ValidationResult::Reject {
                reason: "email_no_at",
            };
        };
        let (local, domain_with_at) = s.split_at(at);
        let domain = &domain_with_at[1..];
        if local.is_empty() || local.len() > 64 {
            return ValidationResult::Reject {
                reason: "email_local_length",
            };
        }
        if domain.is_empty() || domain.len() > 255 || !domain.contains('.') {
            return ValidationResult::Reject {
                reason: "email_domain_invalid",
            };
        }
        if domain.starts_with('.')
            || domain.starts_with('-')
            || domain.ends_with('.')
            || domain.ends_with('-')
        {
            return ValidationResult::Reject {
                reason: "email_domain_invalid",
            };
        }
        ValidationResult::Accept
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
            category: EntityCategory::Custom("email_address".into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_normal() {
        assert_eq!(
            EmailRfcValidator.validate(&ent("alice@example.com"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_with_tag() {
        assert_eq!(
            EmailRfcValidator.validate(&ent("alice+tag@sub.example.fr"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn rejects_no_at() {
        assert!(matches!(
            EmailRfcValidator.validate(&ent("plainstring"), ""),
            ValidationResult::Reject {
                reason: "email_no_at"
            }
        ));
    }

    #[test]
    fn rejects_no_dot_in_domain() {
        assert!(matches!(
            EmailRfcValidator.validate(&ent("a@localhost"), ""),
            ValidationResult::Reject {
                reason: "email_domain_invalid"
            }
        ));
    }

    #[test]
    fn rejects_domain_starts_with_dot() {
        assert!(matches!(
            EmailRfcValidator.validate(&ent("a@.example.com"), ""),
            ValidationResult::Reject { .. }
        ));
    }
}
