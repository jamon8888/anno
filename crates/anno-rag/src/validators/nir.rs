use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;

/// Validator for French NIR (Numéro d'inscription au répertoire) control keys.
///
/// NIR format is 15 digits. The last two digits form a control key computed as:
/// `97 - (first_13_digits mod 97)`
///
/// For Corsica codes (department 2A or 2B), they are substituted before modulo:
/// - "2A" → "19"
/// - "2B" → "18"
#[derive(Debug, Clone, Copy)]
pub struct NirControlKeyValidator;

impl EntityValidator for NirControlKeyValidator {
    fn label(&self) -> &'static str {
        "NIR"
    }

    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        if nir_control_key_valid(&e.original) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject {
                reason: "nir_control_key_failed",
            }
        }
    }
}

fn nir_control_key_valid(raw: &str) -> bool {
    // Strip whitespace
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();

    // NIR must be exactly 15 digits (or 13 + 2-digit key)
    if cleaned.len() != 15 {
        return false;
    }

    // Substitute Corsica codes
    let mut substituted = cleaned.clone();
    if cleaned.get(5..7) == Some("2A") {
        substituted.replace_range(5..7, "19");
    } else if cleaned.get(5..7) == Some("2B") {
        substituted.replace_range(5..7, "18");
    }

    // All characters must be digits after substitution
    if !substituted.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // Parse the body (first 13 digits) and key (last 2 digits)
    let body: u64 = match substituted[..13].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };

    let key: u32 = match substituted[13..15].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };

    // Compute the expected key
    let expected = 97 - (body % 97) as u32;

    key == expected
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
            category: EntityCategory::Custom("NIR".into()),
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }
    }

    #[test]
    fn nir_rejects_wrong_key() {
        // Use a body that gives key != 99
        let e = entity_with("184127645108999");
        assert!(matches!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Reject { .. }
        ));
    }

    #[test]
    fn nir_rejects_wrong_length() {
        assert!(matches!(
            NirControlKeyValidator.validate(&entity_with("12345"), ""),
            ValidationResult::Reject { .. }
        ));
    }

    #[test]
    fn nir_rejects_non_digit() {
        assert!(matches!(
            NirControlKeyValidator.validate(&entity_with("1ABCDEFG6451089"), ""),
            ValidationResult::Reject { .. }
        ));
    }

    #[test]
    fn nir_accepts_computed_valid() {
        // Compute and embed a valid NIR programmatically
        let body: u64 = 1841276451089;
        let key = 97 - (body % 97) as u32;
        let nir = format!("{:013}{:02}", body, key);
        assert_eq!(nir.len(), 15);
        let e = entity_with(&nir);
        assert_eq!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn nir_accepts_with_whitespace() {
        // Compute a valid NIR and add whitespace
        let body: u64 = 1841276451089;
        let key = 97 - (body % 97) as u32;
        let nir_no_ws = format!("{:013}{:02}", body, key);
        let nir_with_ws = format!("1 84 12 76 451 089 {}", format!("{:02}", key));
        let e = entity_with(&nir_with_ws);
        assert_eq!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn nir_handles_corsica_2a() {
        // NIR format: [sex(1)][yy(2)][mm(2)][dept(2)][commune(3)][order(3)][key(2)] = 15 chars
        // dept = "2A" at positions 5-6; substituted to "19" before modulo
        // body_str = sex+yy+mm+19+commune+order = "184122A750123" → "1841219750123"
        let body_str = "1841219750123"; // after 2A→19 substitution, 13 digits
        let body: u64 = body_str.parse().unwrap();
        let key = 97 - (body % 97) as u32;
        // Reconstruct with "2A" at positions 5-6
        let nir = format!("18412{}{}{:02}", "2A", "750123", key);
        assert_eq!(nir.len(), 15, "NIR must be 15 chars");
        let e = entity_with(&nir);
        assert_eq!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Accept
        );
    }

    #[test]
    fn nir_handles_corsica_2b() {
        // dept = "2B" at positions 5-6; substituted to "18" before modulo
        // body_str after 2B→18: "1841218750123"
        let body_str = "1841218750123"; // after 2B→18 substitution, 13 digits
        let body: u64 = body_str.parse().unwrap();
        let key = 97 - (body % 97) as u32;
        // Reconstruct with "2B" at positions 5-6
        let nir = format!("18412{}{}{:02}", "2B", "750123", key);
        assert_eq!(nir.len(), 15, "NIR must be 15 chars");
        let e = entity_with(&nir);
        assert_eq!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Accept
        );
    }
}
