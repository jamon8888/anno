//! Clean, parseable labels for detected-entity source,
//! replacing Rust `Debug` formatting in `detect` output. Spec C §2 (U6).

use cloakpipe_core::DetectionSource;

/// Stable lowercase source label: `"pattern"`, `"ner"`, `"financial"`, `"custom"`.
pub(crate) fn source_label(source: &DetectionSource) -> &'static str {
    match source {
        DetectionSource::Pattern => "pattern",
        DetectionSource::Financial => "financial",
        DetectionSource::Ner => "ner",
        DetectionSource::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_label_is_lowercase() {
        assert_eq!(source_label(&DetectionSource::Pattern), "pattern");
        assert_eq!(source_label(&DetectionSource::Ner), "ner");
        assert_eq!(source_label(&DetectionSource::Financial), "financial");
        assert_eq!(source_label(&DetectionSource::Custom), "custom");
    }
}
