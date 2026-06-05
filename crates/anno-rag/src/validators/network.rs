use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;
use std::net::IpAddr;

#[derive(Debug, Clone, Copy)]
pub struct IpAddressValidator;

impl EntityValidator for IpAddressValidator {
    fn label(&self) -> &'static str {
        "ip_address"
    }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let trimmed = e
            .original
            .trim()
            .trim_matches(|c: char| c == '[' || c == ']');
        if trimmed.parse::<IpAddr>().is_ok() {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject {
                reason: "ip_parse_failed",
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
            category: EntityCategory::Custom("ip_address".into()),
            confidence: 0.8,
            source: DetectionSource::Ner,
        }
    }

    #[test]
    fn accepts_ipv4() {
        assert_eq!(
            IpAddressValidator.validate(&ent("192.168.1.1"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_ipv6() {
        assert_eq!(
            IpAddressValidator.validate(&ent("::1"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_bracketed_ipv6() {
        assert_eq!(
            IpAddressValidator.validate(&ent("[2001:db8::1]"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn rejects_invalid() {
        assert!(matches!(
            IpAddressValidator.validate(&ent("999.999.999.999"), ""),
            ValidationResult::Reject {
                reason: "ip_parse_failed"
            }
        ));
    }

    #[test]
    fn rejects_plain_string() {
        assert!(matches!(
            IpAddressValidator.validate(&ent("hello"), ""),
            ValidationResult::Reject { .. }
        ));
    }
}
