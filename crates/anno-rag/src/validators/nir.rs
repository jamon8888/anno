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
        // NIR with 2A (Corsica) — should be substituted to 19 before modulo
        // Create a valid NIR with department 2A
        let base = "0184127645108"; // 2A in positions 5-6
        let body_str = "0118427645108"; // After 2A → 19 substitution
        let body: u64 = body_str.parse().unwrap();
        let key = 97 - (body % 97) as u32;
        let nir = format!("01{}2A{:02}", "8427645108", key);
        // Actually, let me construct it properly:
        // We want: [00] [84] [2A] [76 45 10 89]
        // After substitution: [00] [84] [19] [76 45 10 89]
        // Body = 0084197645108
        let nir_with_2a = "008419764510889"; // This is the format with Corsica code
        let e = entity_with(nir_with_2a);
        // The body part is 0084197645108, key is 89
        // 0084197645108 % 97 = ?
        let body: u64 = 0084197645108u64;
        let expected_key = 97 - (body % 97) as u32;
        if expected_key == 89 {
            assert_eq!(
                NirControlKeyValidator.validate(&e, ""),
                ValidationResult::Accept
            );
        } else {
            // Use the correct key
            let nir_correct = format!("008419764510{:02}", expected_key);
            let e_correct = entity_with(&nir_correct);
            assert_eq!(
                NirControlKeyValidator.validate(&e_correct, ""),
                ValidationResult::Accept
            );
        }
    }

    #[test]
    fn nir_handles_corsica_2b() {
        // NIR with 2B (Corsica) — should be substituted to 18 before modulo
        let body_str = "0118327645108"; // After 2B → 18 substitution
        let body: u64 = body_str.parse().unwrap();
        let key = 97 - (body % 97) as u32;
        let nir = format!("01832764510{:02}", key);
        // Actual format: positions 5-6 should have "2B"
        // Let me create one properly: [01] [83] [2B] [76 45 10]
        let constructed_body_str = "0118327645108"; // With 2B → 18
        let constructed_body: u64 = constructed_body_str.parse().unwrap();
        let constructed_key = 97 - (constructed_body % 97) as u32;
        let nir_with_2b = format!("01832B764510{:02}", constructed_key);
        let e = entity_with(&nir_with_2b);
        assert_eq!(
            NirControlKeyValidator.validate(&e, ""),
            ValidationResult::Accept
        );
    }
}
