//! French PII detection: regex pack + anno NER backend.
//!
//! Combines:
//! - Built-in FR regex (NIR, SIRET with Luhn check, IBAN-FR, FR phone)
//! - [`anno::StackedNER`] with French language hint for names, organizations, locations.
//!
//! Results are merged, sorted by `start`, and overlapping spans are
//! deduplicated (longer span wins; on equal length, Pattern source beats Ner).
//!
//! # Offset caveat
//!
//! The regex pack reports **byte** offsets; `anno::Entity` reports **character**
//! offsets. For ASCII-only inputs (covered by v0.1 unit tests) the two are
//! identical. For real French legal text containing accents/ligatures the
//! overlap-dedup step may keep an extra entity. We accept this in v0.1 and
//! plan a normalization pass via `anno::offset::bytes_to_chars` in v0.2.

use crate::error::{Error, Result};
use anno::{EntityType, Language, Model, StackedNER};
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
use regex::Regex;
use std::sync::OnceLock;

struct FrPatterns {
    nir: Regex,
    siret: Regex,
    iban_fr: Regex,
    phone_fr: Regex,
}

impl FrPatterns {
    fn get() -> &'static Self {
        static P: OnceLock<FrPatterns> = OnceLock::new();
        P.get_or_init(|| FrPatterns {
            // NIR (numéro de sécurité sociale) — 15-digit format with embedded sex/year/month/dept.
            nir: Regex::new(r"\b[12]\d{2}(0[1-9]|1[0-2])(2[AB]|\d{2})\d{3}\d{3}\d{2}\b")
                .expect("nir regex is a literal"),
            // SIRET — 14 digits (Luhn checked in code, not regex).
            siret: Regex::new(r"\b\d{14}\b").expect("siret regex is a literal"),
            // IBAN-FR — FR + 2 check digits + 23-char BBAN (5×4 + 3).
            // ISO 13616 allows uppercase letters in BBAN positions, e.g.
            // `FR14 2004 1010 0505 0001 3M02 606` (note `3M02`), so we
            // accept `[A-Z0-9]` not just `\d` on the 23 BBAN chars.
            iban_fr: Regex::new(r"\bFR\d{2}\s?(?:[A-Z0-9]{4}\s?){5}[A-Z0-9]{3}\b")
                .expect("iban regex is a literal"),
            // FR phone — +33 / 0-prefix, 10-digit, optional separators.
            phone_fr: Regex::new(r"\b(?:\+33[\s\.\-]?|0)[1-9](?:[\s\.\-]?\d{2}){4}\b")
                .expect("phone regex is a literal"),
        })
    }
}

/// Luhn checksum validator.
///
/// SIRET-shaped 14-digit numbers that pass Luhn are very likely SIRETs;
/// random 14-digit runs that pass are rare but possible — acceptable in v0.1.
fn luhn(s: &str) -> bool {
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

/// Aggregate PII detector: FR regex pack + anno NER.
pub struct Detector {
    ner: StackedNER,
}

impl Detector {
    /// Build a new detector. [`StackedNER::default`] picks the best available
    /// anno backend at runtime (gliner_pii / nuner / bert) and falls back to
    /// pattern+heuristic when no model is cached.
    pub fn new() -> Result<Self> {
        Ok(Self {
            ner: StackedNER::default(),
        })
    }

    /// Detect entities in `text`. Returns spans sorted by start, deduplicated
    /// to non-overlapping (longer span wins on overlap; Pattern beats Ner on tie).
    pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        let mut all = Vec::new();

        // 1. FR regex set.
        let p = FrPatterns::get();
        for m in p.nir.find_iter(text) {
            all.push(DetectedEntity {
                original: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                category: EntityCategory::Custom("NIR".into()),
                confidence: 1.0,
                source: DetectionSource::Pattern,
            });
        }
        for m in p.siret.find_iter(text) {
            if luhn(m.as_str()) {
                all.push(DetectedEntity {
                    original: m.as_str().to_string(),
                    start: m.start(),
                    end: m.end(),
                    category: EntityCategory::Custom("SIRET".into()),
                    confidence: 1.0,
                    source: DetectionSource::Pattern,
                });
            }
        }
        for m in p.iban_fr.find_iter(text) {
            all.push(DetectedEntity {
                original: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                category: EntityCategory::Custom("IBAN_FR".into()),
                confidence: 1.0,
                source: DetectionSource::Pattern,
            });
        }
        for m in p.phone_fr.find_iter(text) {
            all.push(DetectedEntity {
                original: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                category: EntityCategory::PhoneNumber,
                confidence: 0.95,
                source: DetectionSource::Pattern,
            });
        }

        // 2. anno NER. anno reports *character* offsets; cloakpipe (and Rust
        // string slicing) want *byte* offsets. Build a once-per-doc char→byte
        // lookup, then translate every anno entity through it. Without this,
        // any non-ASCII text (€, accents) triggers a "not a char boundary"
        // panic inside Replacer::pseudonymize.
        let anno_entities = self
            .ner
            .extract_entities(text, Some(Language::French))
            .map_err(|e| Error::Detect(e.to_string()))?;

        // char_idx → byte_idx table. The last sentinel is text.len() so a
        // span ending past the last char still resolves to a valid byte.
        let mut char_to_byte: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        char_to_byte.push(text.len());

        for e in anno_entities {
            let s_char = e.start();
            let n_char = e.end();
            if s_char >= char_to_byte.len() || n_char > char_to_byte.len() || s_char >= n_char {
                continue; // out of range — skip silently rather than panic
            }
            all.push(DetectedEntity {
                original: e.text.clone(),
                start: char_to_byte[s_char],
                end: char_to_byte[n_char],
                category: map_anno_category(&e.entity_type),
                confidence: f64::from(e.confidence),
                source: DetectionSource::Ner,
            });
        }

        // 3. Sort + dedup overlaps.
        // Sort by (start asc, span-length desc, Pattern-before-Ner).
        all.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        let mut out: Vec<DetectedEntity> = Vec::new();
        for e in all {
            if let Some(last) = out.last() {
                if e.start < last.end {
                    continue; // overlaps with previous — drop
                }
            }
            out.push(e);
        }
        Ok(out)
    }
}

fn pattern_priority(s: &DetectionSource) -> u8 {
    match s {
        DetectionSource::Pattern => 0,
        DetectionSource::Financial => 1,
        DetectionSource::Custom => 2,
        DetectionSource::Ner => 3,
    }
}

fn map_anno_category(t: &EntityType) -> EntityCategory {
    match t {
        EntityType::Person => EntityCategory::Person,
        EntityType::Organization => EntityCategory::Organization,
        EntityType::Location => EntityCategory::Location,
        _ => EntityCategory::Custom(format!("{t:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_validates_known_siret() {
        // INSEE-style real-ish SIRET that passes Luhn.
        assert!(luhn("73282932000074"));
        // Same digits but last position altered — must fail.
        assert!(!luhn("73282932000075"));
    }

    // The following four tests instantiate Detector::new() which calls
    // anno::StackedNER::default(). On a host without cached anno models,
    // anno attempts a HuggingFace download that hangs/times out in our
    // CI/WSL environment (the runner SIGKILLs after ~60s/test).
    //
    // They are #[ignore]'d for the default cargo-test pass and can be run
    // with `cargo test -- --ignored` once anno models are warmed, OR with
    // `ANNO_NO_DOWNLOADS=1 cargo test -- --ignored` to force the pattern
    // fallback (still requires a cached model attempt — anno's actual
    // download bypass is the v0.2 fix).
    //
    // Followup: refactor these to test the regex/dedup logic via
    // `FrPatterns::get()` and `Detector::detect` with a stubbed `ner`
    // field, so they don't pay for anno startup at all.

    #[test]
    #[ignore = "anno NER startup hangs without model cache; run with --ignored"]
    fn detects_iban_fr() {
        let d = Detector::new().expect("detector builds");
        let text = "Virement vers FR76 3000 6000 0112 3456 7890 189 demain.";
        let ents = d.detect(text).expect("detect ok");
        assert!(
            ents.iter()
                .any(|e| matches!(e.category, EntityCategory::Custom(ref s) if s == "IBAN_FR")),
            "expected IBAN_FR among {:?}",
            ents.iter().map(|e| &e.category).collect::<Vec<_>>()
        );
    }

    #[test]
    #[ignore = "anno NER startup hangs without model cache; run with --ignored"]
    fn detects_fr_phone() {
        let d = Detector::new().expect("detector builds");
        let text = "Appelez le 06 12 34 56 78 demain.";
        let ents = d.detect(text).expect("detect ok");
        assert!(
            ents.iter().any(|e| matches!(e.category, EntityCategory::PhoneNumber)),
            "expected phone among {:?}",
            ents.iter().map(|e| &e.category).collect::<Vec<_>>()
        );
    }

    #[test]
    #[ignore = "anno NER startup hangs without model cache; run with --ignored"]
    fn detects_siret_only_when_luhn_passes() {
        let d = Detector::new().expect("detector builds");
        // 73282932000074 is a Luhn-valid 14-digit. 73282932000075 is not.
        let text_valid = "SIRET 73282932000074 ici.";
        let text_invalid = "SIRET 73282932000075 ici.";
        let valid = d.detect(text_valid).expect("ok");
        let invalid = d.detect(text_invalid).expect("ok");

        assert!(
            valid
                .iter()
                .any(|e| matches!(e.category, EntityCategory::Custom(ref s) if s == "SIRET"))
        );
        assert!(
            !invalid
                .iter()
                .any(|e| matches!(e.category, EntityCategory::Custom(ref s) if s == "SIRET"))
        );
    }

    #[test]
    #[ignore = "anno NER startup hangs without model cache; run with --ignored"]
    fn no_overlapping_spans_in_output() {
        let d = Detector::new().expect("detector builds");
        let text = "Marie Dupont, IBAN FR76 1234 5678 9012 3456 7890 123";
        let ents = d.detect(text).expect("ok");
        for w in ents.windows(2) {
            assert!(
                w[0].end <= w[1].start,
                "overlap: {:?}({}..{}) then {:?}({}..{})",
                w[0].category,
                w[0].start,
                w[0].end,
                w[1].category,
                w[1].start,
                w[1].end
            );
        }
    }
}
