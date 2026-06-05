use super::{EntityValidator, ValidationResult};
use cloakpipe_core::DetectedEntity;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct DateRangeValidator;

impl EntityValidator for DateRangeValidator {
    fn label(&self) -> &'static str { "date_of_birth" }
    fn validate(&self, e: &DetectedEntity, _ctx: &str) -> ValidationResult {
        let Some(year) = extract_year(&e.original) else {
            return ValidationResult::Reject { reason: "year_not_found" };
        };
        let now_year: u32 = 2026; // conservative fixed value — avoids chrono dep
        if (1900..=now_year).contains(&year) {
            ValidationResult::Accept
        } else {
            ValidationResult::Reject { reason: "year_out_of_range" }
        }
    }
}

fn extract_year(s: &str) -> Option<u32> {
    static YEAR_RE: OnceLock<Regex> = OnceLock::new();
    let re = YEAR_RE.get_or_init(|| Regex::new(r"\b(19\d{2}|20\d{2})\b").expect("year regex"));
    re.find(s).and_then(|m| m.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::{DetectionSource, EntityCategory};
    fn entity_with(original: &str) -> DetectedEntity {
        DetectedEntity { original: original.to_string(), start: 0, end: original.len(), category: EntityCategory::Custom("date_of_birth".into()), confidence: 0.8, source: DetectionSource::Ner }
    }
    #[test] fn accepts_valid_year() { assert_eq!(DateRangeValidator.validate(&entity_with("12/05/1984"), ""), ValidationResult::Accept); }
    #[test] fn accepts_year_2020() { assert_eq!(DateRangeValidator.validate(&entity_with("né le 3 juin 2010"), ""), ValidationResult::Accept); }
    #[test] fn rejects_year_too_new() { assert!(matches!(DateRangeValidator.validate(&entity_with("né en 2099"), ""), ValidationResult::Reject { reason: "year_out_of_range" })); }
    #[test] fn rejects_no_year() { assert!(matches!(DateRangeValidator.validate(&entity_with("hier matin"), ""), ValidationResult::Reject { reason: "year_not_found" })); }
}
