use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

/// Validates IBANs via the ISO 13616 mod-97 checksum, scoped to one entity label.
#[derive(Debug, Clone, Copy)]
pub struct Iban97Validator {
    target_label: &'static str,
}

impl Iban97Validator {
    /// Build a validator bound to `label` (the entity category it applies to).
    pub const fn new(label: &'static str) -> Self {
        Self {
            target_label: label,
        }
    }
}

impl EntityValidator for Iban97Validator {
    fn label(&self) -> &'static str {
        self.target_label
    }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if iban_mod97(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject {
                reason: "iban_mod97_failed",
            }
        }
    }
}

fn iban_mod97(raw: &str) -> bool {
    let s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 || s.len() > 34 {
        return false;
    }
    let s = s.to_ascii_uppercase();
    if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let (head, tail) = s.split_at(4);
    let rearranged = format!("{tail}{head}");
    let mut numeric = String::with_capacity(rearranged.len() * 2);
    for c in rearranged.chars() {
        if c.is_ascii_digit() {
            numeric.push(c);
        } else if c.is_ascii_uppercase() {
            numeric.push_str(&((c as u32 - 'A' as u32 + 10).to_string()));
        } else {
            return false;
        }
    }
    let mut remainder: u32 = 0;
    for d in numeric.chars() {
        remainder = (remainder * 10 + d.to_digit(10).unwrap()) % 97;
    }
    remainder == 1
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
            category: EntityCategory::Custom("IBAN_FR".into()),
            confidence: 0.9,
            source: DetectionSource::Pattern,
        }
    }

    #[test]
    fn accepts_valid_fr() {
        assert_eq!(
            Iban97Validator::new("IBAN_FR").validate(&ent("FR1420041010050500013M02606"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_valid_with_spaces() {
        assert_eq!(
            Iban97Validator::new("IBAN_FR").validate(&ent("FR14 2004 1010 0505 0001 3M02 606"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn accepts_valid_de() {
        assert_eq!(
            Iban97Validator::new("iban").validate(&ent("DE89370400440532013000"), ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn rejects_wrong_checksum() {
        assert!(matches!(
            Iban97Validator::new("IBAN_FR").validate(&ent("FR9999999999999999999999999"), ""),
            ValidationResult::Reject {
                reason: "iban_mod97_failed"
            }
        ));
    }

    #[test]
    fn rejects_too_short() {
        assert!(matches!(
            Iban97Validator::new("iban").validate(&ent("FR12"), ""),
            ValidationResult::Reject { .. }
        ));
    }
}
