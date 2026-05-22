//! French PII detection: regex pack + anno NER backend.
//!
//! Combines:
//! - Built-in FR regex (NIR, SIRET with Luhn check, IBAN-FR, FR phone)
//! - [`anno::backends::gliner2_fastino::GLiNER2Fastino`] (multi-v1 ONNX,
//!   FR-aware) for names, organizations, locations.
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
use anno::backends::gliner2_fastino::GLiNER2Fastino;
use anno::backends::inference::ZeroShotNER;
use anno::EntityType;
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
use regex::Regex;
use std::path::PathBuf;
use std::sync::OnceLock;

struct FrPatterns {
    nir: Regex,
    siret: Regex,
    iban_fr: Regex,
    phone_fr: Regex,
    email: Regex,
    person_fr_honorific: Regex,
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
            // The `\b` before `0` keeps the domestic branch anchored to a
            // word boundary; the `+33` branch can't use a leading `\b`
            // (the Rust `regex` crate has no lookbehind, and `\b\+` only
            // matches when preceded by a word char, missing every
            // start-of-line `+33`). The trailing `\b` still blocks
            // arbitrary 10-digit-run extension, and `+33` itself acts as
            // a structural guard against being grabbed mid-number.
            phone_fr: Regex::new(
                r"(?:\+33[\s\.\-]?[1-9]|\b0[1-9])(?:[\s\.\-]?\d{2}){4}\b",
            )
            .expect("phone regex is a literal"),
            // Email — pragmatic RFC-5321-ish: local-part @ domain . TLD.
            // Contract-style addresses only; quoted local parts are out of scope.
            email: Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")
                .expect("email regex is a literal"),
            // FR honorific-prefixed person — `Monsieur Julien Marchand`,
            // `Madame Anne-Sophie Perrin`, `Maître Hugo Faure`, `Mme Inès
            // Coulibaly`. Capture group 1 is the name (2+ capitalised words,
            // accents/hyphens/apostrophes allowed). GLiNER2-Fastino under-
            // detects names sitting right after these French honorifics, so
            // we complement the NER tier here. Lowercase function words
            // ("le", "la") after the honorific block role titles like
            // `Monsieur le Président` from being mistaken for a person.
            person_fr_honorific: Regex::new(
                r"(?:Monsieur|Madame|Mademoiselle|Mme\.?|Mlle\.?|M\.|Maître|Me\.?)\s+(\p{Lu}[\p{L}'\-]+(?:\s+\p{Lu}[\p{L}'\-]+)+)",
            )
            .expect("person honorific regex is a literal"),
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
    total.is_multiple_of(10)
}

/// Model-free PII detection: the French regex pattern layer only
/// (NIR, SIRET with Luhn check, IBAN-FR, French phone, email).
///
/// This is the layer `Detector::detect` runs before the NER model. It is
/// exposed so callers that must not pay the GLiNER2 model load — the
/// model-free eval tier — can score the regex categories on their own.
#[must_use]
pub fn detect_patterns(text: &str) -> Vec<DetectedEntity> {
    let p = FrPatterns::get();
    let mut all = Vec::new();
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
    for m in p.email.find_iter(text) {
        all.push(DetectedEntity {
            original: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
            category: EntityCategory::Email,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        });
    }
    for caps in p.person_fr_honorific.captures_iter(text) {
        if let Some(name) = caps.get(1) {
            all.push(DetectedEntity {
                original: name.as_str().to_string(),
                start: name.start(),
                end: name.end(),
                category: EntityCategory::Person,
                confidence: 0.95,
                source: DetectionSource::Pattern,
            });
        }
    }
    all
}

/// HuggingFace id of the NER model loaded by [`Detector::new`]. Surfaced
/// in the detector audit event so operators can reconcile observed
/// behaviour against a specific model version.
pub const NER_MODEL_ID: &str = "SemplificaAI/gliner2-multi-v1-onnx";

/// Aggregate PII detector: FR regex pack + anno NER.
pub struct Detector {
    ner: GLiNER2Fastino,
}

impl Detector {
    /// Build a new detector. Loads the GLiNER2Fastino multi-v1 ONNX model
    /// (multilingual, FR-aware) from the HF Hub cache.
    pub fn new() -> Result<Self> {
        // ── ANNO_MODELS_DIR fast-path ─────────────────────────────────────────
        if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
            let model_path = PathBuf::from(models_dir).join("gliner2-multi-v1-onnx");
            if model_path.exists() {
                let ner =
                    anno::backends::gliner2_fastino::GLiNER2Fastino::from_local(&model_path)
                        .map_err(|e| Error::Detect(format!("gliner2_fastino load (local): {e}")))?;
                return Ok(Self { ner });
            }
        }
        // ─────────────────────────────────────────────────────────────────────
        let ner = GLiNER2Fastino::from_pretrained(NER_MODEL_ID)
            .map_err(|e| Error::Detect(format!("gliner2_fastino load: {e}")))?;
        Ok(Self { ner })
    }

    /// Detect entities in `text`. Returns spans sorted by start, deduplicated
    /// to non-overlapping (longer span wins on overlap; Pattern beats Ner on tie).
    ///
    /// Emits an audit event at `target = "anno_rag::detect::audit"` with
    /// AI Act Art. 12 / Art. 72 logging payload: input length (chars), wall-
    /// clock duration, per-category span counts, per-source span counts,
    /// detector version, NER model id. **Never logs raw text or detected
    /// values** — the event carries only counts and IDs that are safe to
    /// pipe to a SIEM.
    pub fn detect(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        let started = std::time::Instant::now();
        let input_chars = text.chars().count();
        let out = self.detect_inner(text)?;
        let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        emit_detect_audit(input_chars, elapsed_us, &out);
        Ok(out)
    }

    fn detect_inner(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        // 1. FR regex set (model-free layer).
        let mut all = detect_patterns(text);

        // 2. anno NER. anno reports *character* offsets; cloakpipe (and Rust
        // string slicing) want *byte* offsets. Build a once-per-doc char→byte
        // lookup, then translate every anno entity through it. Without this,
        // any non-ASCII text (€, accents) triggers a "not a char boundary"
        // panic inside Replacer::pseudonymize.
        let anno_entities = self
            .ner
            .extract_with_types(text, &["person", "organization", "location"], 0.5)
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

/// Emit the AI Act Art. 12 / Art. 72 detector audit event. Cleartext-free:
/// only counts, durations, and model ids. Deployers pipe the
/// `anno_rag::detect::audit` target to their SIEM / Art. 30 register.
fn emit_detect_audit(input_chars: usize, elapsed_us: u64, out: &[DetectedEntity]) {
    use std::collections::BTreeMap;
    let mut per_category: BTreeMap<String, usize> = BTreeMap::new();
    let mut from_pattern: usize = 0;
    let mut from_ner: usize = 0;
    let mut from_other: usize = 0;
    for e in out {
        let key = match &e.category {
            EntityCategory::Custom(s) => format!("Custom({s})"),
            other => format!("{other:?}"),
        };
        *per_category.entry(key).or_insert(0) += 1;
        match e.source {
            DetectionSource::Ner => from_ner += 1,
            DetectionSource::Pattern => from_pattern += 1,
            _ => from_other += 1,
        }
    }
    // Serialise per_category as a compact JSON map so a single field carries
    // the breakdown without exploding the tracing schema.
    let per_category_json = serde_json::to_string(&per_category).unwrap_or_default();
    tracing::info!(
        target: "anno_rag::detect::audit",
        event = "detect",
        detector_version = env!("CARGO_PKG_VERSION"),
        ner_model_id = NER_MODEL_ID,
        input_chars = input_chars,
        elapsed_us = elapsed_us,
        spans_total = out.len(),
        spans_from_pattern = from_pattern,
        spans_from_ner = from_ner,
        spans_from_other = from_other,
        per_category = %per_category_json,
        "detector pass complete"
    );
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
    fn email_regex_matches_contract_emails() {
        let ents = detect_patterns("Contact : claire.fontaine@atelier-numerique.fr pour le suivi.");
        assert!(
            ents.iter()
                .any(|e| matches!(e.category, EntityCategory::Email)
                    && e.original == "claire.fontaine@atelier-numerique.fr"),
            "expected Email entity, got {:?}",
            ents.iter()
                .map(|e| (&e.category, &e.original))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn email_regex_rejects_non_emails() {
        // A bare `@`, and `a@b` with no TLD, must not be detected as emails.
        let ents = detect_patterns("mention @julien et adresse a@b sans domaine.");
        assert!(
            !ents
                .iter()
                .any(|e| matches!(e.category, EntityCategory::Email)),
            "no Email entity expected, got {:?}",
            ents.iter()
                .map(|e| (&e.category, &e.original))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn detect_patterns_finds_structured_pii_without_model() {
        // detect_patterns is the model-free regex layer.
        let text = "IBAN FR76 3000 4000 0500 0612 3456 789, tel 06 12 34 56 78.";
        let ents = detect_patterns(text);
        assert!(ents
            .iter()
            .any(|e| matches!(&e.category, EntityCategory::Custom(s) if s == "IBAN_FR")));
        assert!(ents
            .iter()
            .any(|e| matches!(e.category, EntityCategory::PhoneNumber)));
    }

    #[test]
    fn luhn_validates_known_siret() {
        // INSEE-style real-ish SIRET that passes Luhn.
        assert!(luhn("73282932000074"));
        // Same digits but last position altered — must fail.
        assert!(!luhn("73282932000075"));
    }

    // The following four tests instantiate Detector::new() which loads
    // GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx").
    // On a host without the model cached, this triggers a HuggingFace
    // download that hangs/times out in our CI/WSL environment (the runner
    // SIGKILLs after ~60s/test).
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
            ents.iter()
                .any(|e| matches!(e.category, EntityCategory::PhoneNumber)),
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

        assert!(valid
            .iter()
            .any(|e| matches!(e.category, EntityCategory::Custom(ref s) if s == "SIRET")));
        assert!(!invalid
            .iter()
            .any(|e| matches!(e.category, EntityCategory::Custom(ref s) if s == "SIRET")));
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

    #[test]
    fn anno_models_dir_missing_ner_dir_fast_path_not_taken() {
        // ANNO_MODELS_DIR is set but gliner2-multi-v1-onnx/ does NOT exist.
        // The fast-path must not be taken; Detector::new falls through to
        // from_pretrained which will fail (ANNO_NO_DOWNLOADS=1 blocks network).
        // The error must NOT contain "(local)" — that would indicate the fast-path ran.
        let dir = tempfile::tempdir().expect("tempdir");
        // deliberately do NOT create dir.path()/gliner2-multi-v1-onnx

        std::env::set_var("ANNO_MODELS_DIR", dir.path());
        std::env::set_var("ANNO_NO_DOWNLOADS", "1");
        let result = Detector::new();
        std::env::remove_var("ANNO_MODELS_DIR");
        std::env::remove_var("ANNO_NO_DOWNLOADS");

        let err = result.expect_err("must fail — no model dir and downloads blocked");
        let msg = err.to_string();
        assert!(
            !msg.contains("(local)"),
            "fast-path must NOT be taken when gliner2 dir absent, got: {msg}"
        );
    }

    #[test]
    fn anno_models_dir_local_path_entered_when_ner_dir_exists() {
        // When ANNO_MODELS_DIR/gliner2-multi-v1-onnx/ exists (even empty),
        // the fast-path IS taken and from_local errors with a typed error.
        // This proves the branch is entered without requiring real model files.
        let dir = tempfile::tempdir().expect("tempdir");
        let ner_dir = dir.path().join("gliner2-multi-v1-onnx");
        std::fs::create_dir_all(&ner_dir).expect("mkdir");

        std::env::set_var("ANNO_MODELS_DIR", dir.path());
        let result = Detector::new();
        std::env::remove_var("ANNO_MODELS_DIR");

        // from_local on an empty dir returns an error (no ONNX files)
        let err = result.expect_err("must fail on empty model dir");
        let msg = err.to_string();
        assert!(
            msg.contains("(local)"),
            "error must come from local path, got: {msg}"
        );
    }
}
