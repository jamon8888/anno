//! PII (personally identifiable information) detection and redaction.
//!
//! Two detection paths:
//! - [`classify_entity()`](crate::pii::classify_entity): classifies NER entities as PII (uses character offsets from `Entity`)
//! - [`scan_patterns()`](crate::pii::scan_patterns): regex-based pre-NER scan for structured PII (SSN, credit card, IBAN, email, phone, address)
//!
//! After detection, use [`redact()`](crate::pii::redact) or [`pseudonymize()`](crate::pii::pseudonymize) to produce sanitized text.
//!
//! # Example
//!
//! ```
//! use anno::{Model, StackedNER};
//! use anno::pii;
//!
//! let m = StackedNER::default();
//! let text = "John Smith's SSN is 123-45-6789.";
//! let ents = m.extract_entities(text, None)?;
//!
//! // Classify NER entities as PII
//! let mut pii_entities: Vec<pii::PiiEntity> = ents.iter().filter_map(pii::classify_entity).collect();
//! // Also scan for structured PII patterns
//! pii_entities.extend(pii::scan_patterns(text));
//!
//! let report = pii::report(&pii_entities);
//! let redacted = pii::redact(text, &pii_entities);
//! # Ok::<(), anno::Error>(())
//! ```

use crate::Entity;
use regex::Regex;
use std::collections::HashMap;

/// A detected PII entity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PiiEntity {
    /// The PII text.
    pub text: String,
    /// Type of PII: `PERSON`, `DOB`, `ADDRESS`, `CONTACT`, `ID_NUMBER`.
    pub pii_type: String,
    /// Start character offset.
    pub start: usize,
    /// End character offset (exclusive).
    pub end: usize,
    /// Risk level: `LOW`, `MEDIUM`, `HIGH`, `CRITICAL`.
    pub risk_level: String,
}

/// Summary of PII found in text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PiiReport {
    /// Count of person name entities.
    pub person_count: usize,
    /// Count of date/time entities (potential DOBs).
    pub date_count: usize,
    /// Count of address entities.
    pub location_count: usize,
    /// Count of contact info (email, phone).
    pub contact_count: usize,
    /// Count of ID numbers (SSN, credit card, IBAN).
    pub id_number_count: usize,
    /// All detected PII entities.
    pub entities: Vec<PiiEntity>,
    /// k-anonymity risk assessment.
    pub k_anonymity_risk: String,
}

/// Classify an NER entity as PII.
///
/// Returns `None` if the entity is not PII (e.g., regular dates, general locations).
pub fn classify_entity(entity: &Entity) -> Option<PiiEntity> {
    let label = entity.entity_type.as_label();
    let text = &entity.text;

    let (pii_type, risk_level) = match label {
        "PER" | "PERSON" => ("PERSON", assess_person_risk(text)),
        "DATE" => {
            if looks_like_dob(text) {
                ("DOB", "HIGH")
            } else {
                return None;
            }
        }
        "LOC" | "GPE" | "LOCATION" => {
            if looks_like_address(text) {
                ("ADDRESS", "HIGH")
            } else {
                return None;
            }
        }
        "EMAIL" => ("CONTACT", "HIGH"),
        "PHONE" => ("CONTACT", "HIGH"),
        "URL" | "MONEY" => return None,
        _ => {
            if looks_like_id_number(text) {
                ("ID_NUMBER", "CRITICAL")
            } else {
                return None;
            }
        }
    };

    Some(PiiEntity {
        text: text.clone(),
        pii_type: pii_type.to_string(),
        start: entity.start(),
        end: entity.end(),
        risk_level: risk_level.to_string(),
    })
}

/// Validate PESEL checksum (Polish national ID).
/// Uses mod-10 algorithm with official weights from Polish GUS.
/// See: https://en.wikipedia.org/wiki/PESEL
#[cfg(feature = "pii-eu")]
fn is_valid_pesel(pesel: &str) -> bool {
    if pesel.len() != 11 || !pesel.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Official weights for first 10 digits (per GUS specification)
    let weights = [1, 3, 7, 9, 1, 3, 7, 9, 1, 3];
    let mut sum = 0;
    for (i, w) in weights.iter().enumerate() {
        if let Some(d) = pesel.chars().nth(i).and_then(|c| c.to_digit(10)) {
            sum += (d as usize) * w;
        }
    }
    let check_digit = pesel
        .chars()
        .nth(10)
        .and_then(|c| c.to_digit(10))
        .unwrap_or(0) as usize;
    let expected = (10 - (sum % 10)) % 10;
    expected == check_digit
}

/// Validate BSN checksum (Dutch national ID).
/// Uses mod-11 algorithm per official Dutch RvIG specification.
/// Formula: 9*d1 + 8*d2 + 7*d3 + 6*d4 + 5*d5 + 4*d6 + 3*d7 + 2*d8 + (-1)*d9 ≡ 0 (mod 11)
/// See: https://en.wikipedia.org/wiki/Burgerservicenummer
#[cfg(feature = "pii-eu")]
fn is_valid_bsn(bsn: &str) -> bool {
    if bsn.len() != 9 || !bsn.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Official weights per RvIG: last digit gets weight -1
    let weights: [i32; 9] = [9, 8, 7, 6, 5, 4, 3, 2, -1];
    let mut sum: i32 = 0;
    for (i, w) in weights.iter().enumerate() {
        if let Some(d) = bsn.chars().nth(i).and_then(|c| c.to_digit(10)) {
            sum += (d as i32) * w;
        }
    }
    sum % 11 == 0
}

/// Validate Belgian Registre National checksum.
/// Uses the 97-modulo algorithm per official Belgian specification.
/// Format: YYMMDDXXXXX (6 birth date + 3 sequence + 2 check digits)
///
/// Two formulas depending on birth century:
/// - Pre-2000: `97 - (first_9 % 97)`
/// - Post-2000: `97 - ((2_000_000_000 + first_9) % 97)`
///
/// Both are tried when the century is ambiguous (YY digits alone do not
/// encode the century).
#[cfg(feature = "pii-eu")]
fn is_valid_belgian_registre(num: &str) -> bool {
    if num.len() != 11 || !num.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(n) = num[..9].parse::<u64>() else {
        return false;
    };
    let Ok(check_actual) = num[9..11].parse::<u64>() else {
        return false;
    };
    // Pre-2000 births
    if 97 - (n % 97) == check_actual {
        return true;
    }
    // Post-2000 births: prepend the century marker 2_000_000_000
    97 - ((2_000_000_000_u64 + n) % 97) == check_actual
}

/// Lazy-compiled regexes for EU PII patterns (feature-gated)
#[cfg(feature = "pii-eu")]
mod eu_patterns {
    use regex::Regex;
    use std::sync::OnceLock;

    pub static FR_INSEE: OnceLock<Regex> = OnceLock::new();
    pub static ES_DNI: OnceLock<Regex> = OnceLock::new();
    pub static IT_CODICE: OnceLock<Regex> = OnceLock::new();
    pub static PL_PESEL: OnceLock<Regex> = OnceLock::new();
    pub static NL_BSN: OnceLock<Regex> = OnceLock::new();
    pub static BE_REGISTRE: OnceLock<Regex> = OnceLock::new();
    pub static FR_SIRET: OnceLock<Regex> = OnceLock::new();
    pub static FR_SIREN: OnceLock<Regex> = OnceLock::new();
    pub static VAT: OnceLock<Regex> = OnceLock::new();
    pub static LICENSE_PLATE: OnceLock<Regex> = OnceLock::new();
    pub static HEALTH_KW: OnceLock<Regex> = OnceLock::new();
    pub static BIOMETRIC_KW: OnceLock<Regex> = OnceLock::new();
    pub static GENETIC_KW: OnceLock<Regex> = OnceLock::new();
    pub static POLITICAL_KW: OnceLock<Regex> = OnceLock::new();
    pub static RELIGION_KW: OnceLock<Regex> = OnceLock::new();
    pub static UNION_KW: OnceLock<Regex> = OnceLock::new();
    pub static CRIMINAL_KW: OnceLock<Regex> = OnceLock::new();
    pub static SEXUAL_ORIENT_KW: OnceLock<Regex> = OnceLock::new();
    pub static ETHNIC_KW: OnceLock<Regex> = OnceLock::new();
}

/// Scan text for structured PII patterns (SSN, credit card, IBAN, email, phone, address).
///
/// This is independent of NER -- it catches structured PII via regex.
/// Offsets are character offsets (Unicode scalar values), consistent with [`classify_entity`].
pub fn scan_patterns(text: &str) -> Vec<PiiEntity> {
    let mut results = Vec::new();

    // EU patterns run first to claim spans before the generic phone/digit patterns
    #[cfg(feature = "pii-eu")]
    scan_eu_patterns(text, &mut results);

    scan_generic_patterns(text, &mut results);

    results
}

/// Scan for EU structured PII and GDPR Art. 9 special categories using GLiNER2 zero-shot NER.
///
/// Unlike [`scan_patterns`] which uses keyword heuristics for Art. 9 detection (high false-positive
/// rate), this function passes [`EU_ART9_TYPES`] to the model's bi-encoder for context-aware
/// detection. Structured patterns (national IDs, tax IDs, license plates, SSN, IBAN, etc.) run
/// unchanged.
///
/// # Arguments
///
/// * `text` — input text to scan
/// * `model` — any [`crate::ZeroShotNER`] backend, e.g. `GLiNER2Fastino`
/// * `threshold` — confidence threshold (0.0–1.0). Recommended: 0.4 recall, 0.5–0.6 precision.
///
/// # Errors
///
/// Returns `Err` if the NER backend fails (model load error, ONNX runtime error, etc.).
///
/// # Note
///
/// This path does **not** run the keyword-based Art. 9 fallback ([`scan_eu_art9_keywords`]).
/// If the NER model misses an entity (threshold too high, short text, undertrained label),
/// no keyword fallback fires. For keyword-gated Art. 9 coverage, use [`scan_patterns`] instead.
#[cfg(all(feature = "pii-eu", feature = "gliner2-fastino"))]
pub fn scan_patterns_with_ner<M>(
    text: &str,
    model: &M,
    threshold: f32,
) -> crate::Result<Vec<PiiEntity>>
where
    M: crate::ZeroShotNER,
{
    let mut results = Vec::new();
    // Structured patterns run first to claim spans; NER overlap check respects them.
    scan_eu_structured(text, &mut results);
    scan_generic_patterns(text, &mut results);
    // Context-aware Art. 9 detection — replaces keyword heuristics on this path.
    let ner_entities = model.extract_with_types(text, EU_ART9_TYPES, threshold)?;
    for entity in ner_entities {
        let label = entity.entity_type.as_label().to_lowercase();
        let pii_type = pii_type_from_art9_label(&label);
        let risk = art9_risk_level(&label);
        let start = entity.start();
        let end = entity.end();
        if !results
            .iter()
            .any(|e: &PiiEntity| !(end <= e.start || start >= e.end))
        {
            results.push(PiiEntity {
                text: entity.text.clone(),
                pii_type: pii_type.to_string(),
                start,
                end,
                risk_level: risk.to_string(),
            });
        }
    }
    dedup_overlapping(&mut results);
    Ok(results)
}

/// Scan for generic structured PII patterns: SSN, credit card, IBAN, email, phone, address.
///
/// These patterns are not EU-specific. Called by [`scan_patterns`] after EU-specific patterns
/// so that EU national IDs and tax IDs claim their spans first.
fn scan_generic_patterns(text: &str, results: &mut Vec<PiiEntity>) {
    let patterns: &[(&str, &str, &str)] = &[
        (r"\b\d{3}-\d{2}-\d{4}\b", "ID_NUMBER", "CRITICAL"),
        (
            r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b",
            "ID_NUMBER",
            "CRITICAL",
        ),
        (
            r"\b[A-Z]{2}\d{2}[A-Z0-9]{4}\d{7}([A-Z0-9]{0,16})?\b",
            "ID_NUMBER",
            "CRITICAL",
        ),
        (
            r"\b[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\b",
            "CONTACT",
            "HIGH",
        ),
        (
            r"(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b",
            "CONTACT",
            "HIGH",
        ),
        (
            r"\b\d{1,5}\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*\s+(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Way|Court|Ct|Place|Pl|Circle|Cir|Terrace|Ter)\.?(?:,\s*[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*,\s*[A-Z]{2}\s+\d{5}(?:-\d{4})?)?\b",
            "ADDRESS",
            "HIGH",
        ),
    ];

    for &(pat, pii_type, risk) in patterns {
        if let Ok(re) = Regex::new(pat) {
            for m in re.find_iter(text) {
                // Convert byte offsets from regex to character offsets
                let start = text[..m.start()].chars().count();
                let end = text[..m.end()].chars().count();
                let overlaps = results
                    .iter()
                    .any(|e: &PiiEntity| !(end <= e.start || start >= e.end));
                if !overlaps {
                    results.push(PiiEntity {
                        text: m.as_str().to_string(),
                        pii_type: pii_type.to_string(),
                        start,
                        end,
                        risk_level: risk.to_string(),
                    });
                }
            }
        }
    }
}

/// Push a single match into results if it doesn't overlap an existing span.
#[cfg(feature = "pii-eu")]
fn push_eu_entity(
    results: &mut Vec<PiiEntity>,
    text: &str,
    m: regex::Match<'_>,
    pii_type: &str,
    risk_level: &str,
) {
    let start = text[..m.start()].chars().count();
    let end = text[..m.end()].chars().count();
    if !results
        .iter()
        .any(|e: &PiiEntity| !(end <= e.start || start >= e.end))
    {
        results.push(PiiEntity {
            text: m.as_str().to_string(),
            pii_type: pii_type.to_string(),
            start,
            end,
            risk_level: risk_level.to_string(),
        });
    }
}

/// Scan for EU-specific structured PII: national IDs, tax identifiers, and EU vehicle license plates.
///
/// Does NOT include GDPR Art. 9 keyword patterns — those are in [`scan_eu_art9_keywords`].
#[cfg(feature = "pii-eu")]
fn scan_eu_structured(text: &str, results: &mut Vec<PiiEntity>) {
    use eu_patterns::*;
    use regex::Regex;

    // --- National IDs ---

    // France: INSEE (13-digit social security number)
    let re = FR_INSEE.get_or_init(|| {
        Regex::new(r"\b[12]\d{2}(?:0[1-9]|1[0-2])\d{2}\d{3}\d{3}\d{2}\b").expect("FR INSEE regex")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "NATIONAL_ID_FR", "CRITICAL");
    }

    // Spain: DNI (8 digits + letter) or NIE (X/Y/Z + 7 digits + letter)
    let re =
        ES_DNI.get_or_init(|| Regex::new(r"\b(?:[XYZ]\d{7}|\d{8})[A-Z]\b").expect("ES DNI regex"));
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "NATIONAL_ID_ES", "CRITICAL");
    }

    // Italy: Codice Fiscale (6 letters + 2 digits + letter + 2 digits + letter + 3 digits + letter)
    let re = IT_CODICE.get_or_init(|| {
        Regex::new(r"\b[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z]\b").expect("IT Codice regex")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "NATIONAL_ID_IT", "CRITICAL");
    }

    // Poland: PESEL (11-digit with mod-10 checksum)
    let re = PL_PESEL.get_or_init(|| Regex::new(r"\b\d{11}\b").expect("PL PESEL regex"));
    for m in re.find_iter(text) {
        if is_valid_pesel(m.as_str()) {
            push_eu_entity(results, text, m, "NATIONAL_ID_PL", "CRITICAL");
        }
    }

    // Netherlands: BSN (9-digit with mod-11 checksum, weight -1 on last digit)
    let re = NL_BSN.get_or_init(|| Regex::new(r"\b\d{9}\b").expect("NL BSN regex"));
    for m in re.find_iter(text) {
        if is_valid_bsn(m.as_str()) {
            push_eu_entity(results, text, m, "NATIONAL_ID_NL", "CRITICAL");
        }
    }

    // Belgium: Registre National (11-digit with 97-modulo checksum)
    let re = BE_REGISTRE
        .get_or_init(|| Regex::new(r"\b\d{2}[0-1]\d[0-3]\d\d{5}\b").expect("BE Registre regex"));
    for m in re.find_iter(text) {
        if is_valid_belgian_registre(m.as_str()) {
            push_eu_entity(results, text, m, "NATIONAL_ID_BE", "CRITICAL");
        }
    }

    // --- Tax Identifiers ---

    // France: SIRET (14-digit business ID)
    let re = FR_SIRET
        .get_or_init(|| Regex::new(r"\b\d{3}\s?\d{3}\s?\d{3}\s?\d{5}\b").expect("FR SIRET regex"));
    for m in re.find_iter(text) {
        // Only flag 14+ digit matches as SIRET (not SIREN which is 9)
        let digits: String = m.as_str().chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 14 {
            push_eu_entity(results, text, m, "TAX_ID_SIRET", "HIGH");
        }
    }

    // France: SIREN (9-digit company ID) — only when not already captured as SIRET
    let re =
        FR_SIREN.get_or_init(|| Regex::new(r"\b\d{3}\s?\d{3}\s?\d{3}\b").expect("FR SIREN regex"));
    for m in re.find_iter(text) {
        let digits: String = m.as_str().chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() == 9 {
            push_eu_entity(results, text, m, "TAX_ID_SIREN", "HIGH");
        }
    }

    // EU VAT numbers (country prefix + digits).
    // GB is included post-Brexit: UK VAT numbers remain widely exchanged with
    // EU counterparties and are structurally identical, so we retain detection.
    let re = VAT.get_or_init(|| {
        Regex::new(r"\b(?:AT|BE|BG|CY|CZ|DE|DK|EE|EL|ES|FI|FR|GB|HR|HU|IE|IT|LT|LU|LV|MT|NL|PL|PT|RO|SE|SI|SK)\d{8,12}\b")
            .expect("EU VAT regex")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "TAX_ID_VAT", "HIGH");
    }

    // EU License plates (country code + digits + letters, requires at least 1 digit)
    let re = LICENSE_PLATE.get_or_init(|| {
        Regex::new(r"\b(?:DE|FR|IT|ES|PL|NL|BE|PT|CZ|HU|SE|AT|CH|RO|BG|DK|FI|GR|IE|SK|SI|HR|LT|LV|EE|LU|MT|CY)\s?-?\d[\w-]{2,6}\b")
            .expect("EU license plate regex")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "LICENSE_PLATE_EU", "MEDIUM");
    }
}

/// Scan for GDPR Art. 9 special category keywords (health, biometric, genetic, political,
/// religion, union, criminal, sexual orientation, ethnic origin).
///
/// Preserved for `scan_patterns()` backward compatibility. For context-aware detection,
/// use `scan_patterns_with_ner` instead.
///
/// Note: High false positive rate by design — Phase 2 will replace with GLiNER2 NER.
#[cfg(feature = "pii-eu")]
fn scan_eu_art9_keywords(text: &str, results: &mut Vec<PiiEntity>) {
    use eu_patterns::*;
    use regex::Regex;

    let re = HEALTH_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(diagnosed\s+with|suffers?\s+from|allergic\s+to|medical\s+condition|hospital|surgery|treatment|disease|illness|cancer|diabetes|hypertension|asthma|depression|anxiety)\b")
            .expect("health keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_HEALTH", "CRITICAL");
    }

    let re = BIOMETRIC_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(fingerprint|iris\s+scan|facial\s+recognition|biometric|face\s+scan|voice\s+recognition)\b")
            .expect("biometric keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_BIOMETRIC", "CRITICAL");
    }

    let re = GENETIC_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(genetic\s+data|DNA\s+test|genome|inherited\s+condition|hereditary)\b")
            .expect("genetic keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_GENETIC", "CRITICAL");
    }

    let re = POLITICAL_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(member\s+of\s+(?:the\s+)?(?:socialist|communist|conservative|liberal|democrat|republican)\s+party|party\s+affiliation|political\s+opinion)\b")
            .expect("political keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_POLITICAL", "HIGH");
    }

    let re = RELIGION_KW.get_or_init(|| {
        Regex::new(
            r"(?i)\b(catholic|protestant|muslim|jewish|buddhist|hindu|sikh|atheist|agnostic)\b",
        )
        .expect("religion keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_RELIGION", "HIGH");
    }

    let re = UNION_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(trade\s+union\s+member|union\s+membership|collective\s+bargaining)\b")
            .expect("union keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_UNION", "MEDIUM");
    }

    let re = CRIMINAL_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(convicted\s+of|arrested\s+for|charged\s+with|criminal\s+record|incarcerated|felony\s+conviction)\b")
            .expect("criminal keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_CRIMINAL", "CRITICAL");
    }

    let re = SEXUAL_ORIENT_KW.get_or_init(|| {
        Regex::new(r"(?i)\b(gay|lesbian|bisexual|transgender|lgbtq\+?|homosexual|queer)\b")
            .expect("sexual orientation keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(
            results,
            text,
            m,
            "SPECIAL_CATEGORY_SEXUAL_ORIENTATION",
            "HIGH",
        );
    }

    let re = ETHNIC_KW.get_or_init(|| {
        Regex::new(
            r"(?i)\b(ethnic\s+origin|racial\s+origin|Roma\s+community|indigenous\s+people)\b",
        )
        .expect("ethnic keywords")
    });
    for m in re.find_iter(text) {
        push_eu_entity(results, text, m, "SPECIAL_CATEGORY_ETHNIC", "HIGH");
    }
}

/// GDPR Art. 9 label strings for zero-shot NER with `extract_with_types`.
///
/// These labels are passed directly to the GLiNER2 bi-encoder — they are
/// semantic queries, not fixed vocabulary. Use with `scan_patterns_with_ner`
/// (requires both `pii-eu` and `gliner2-fastino` features).
#[cfg(feature = "pii-eu")]
pub const EU_ART9_TYPES: &[&str] = &[
    "health condition",
    "biometric data",
    "genetic data",
    "political opinion",
    "religious belief",
    "trade union membership",
    "criminal record",
    "sexual orientation",
    "ethnic origin",
];

#[cfg(feature = "pii-eu")]
fn pii_type_from_art9_label(label: &str) -> &'static str {
    match label {
        "health condition" => "SPECIAL_CATEGORY_HEALTH",
        "biometric data" => "SPECIAL_CATEGORY_BIOMETRIC",
        "genetic data" => "SPECIAL_CATEGORY_GENETIC",
        "political opinion" => "SPECIAL_CATEGORY_POLITICAL",
        "religious belief" => "SPECIAL_CATEGORY_RELIGION",
        "trade union membership" => "SPECIAL_CATEGORY_UNION",
        "criminal record" => "SPECIAL_CATEGORY_CRIMINAL",
        "sexual orientation" => "SPECIAL_CATEGORY_SEXUAL_ORIENTATION",
        "ethnic origin" => "SPECIAL_CATEGORY_ETHNIC",
        _ => "SPECIAL_CATEGORY",
    }
}

#[cfg(feature = "pii-eu")]
fn art9_risk_level(label: &str) -> &'static str {
    match label {
        "health condition" | "biometric data" | "genetic data" | "criminal record" => "CRITICAL",
        "political opinion" | "religious belief" | "sexual orientation" | "ethnic origin" => "HIGH",
        "trade union membership" => "MEDIUM",
        _ => "HIGH",
    }
}

/// Combined structured + keyword scan (backward-compatible wrapper).
///
/// Calls [`scan_eu_structured`] for national IDs, tax IDs, and license plates,
/// then [`scan_eu_art9_keywords`] for GDPR Art. 9 special category keywords.
#[cfg(feature = "pii-eu")]
fn scan_eu_patterns(text: &str, results: &mut Vec<PiiEntity>) {
    scan_eu_structured(text, results);
    scan_eu_art9_keywords(text, results);
}

/// Generate a PII report from detected entities.
pub fn report(entities: &[PiiEntity]) -> PiiReport {
    let mut person_count = 0;
    let mut date_count = 0;
    let mut location_count = 0;
    let mut contact_count = 0;
    let mut id_number_count = 0;

    for e in entities {
        match e.pii_type.as_str() {
            "PERSON" => person_count += 1,
            "DOB" => date_count += 1,
            "ADDRESS" => location_count += 1,
            "CONTACT" => contact_count += 1,
            "ID_NUMBER" | "NATIONAL_ID_FR" | "NATIONAL_ID_ES" | "NATIONAL_ID_IT"
            | "NATIONAL_ID_PL" | "NATIONAL_ID_NL" | "NATIONAL_ID_BE" | "TAX_ID_SIRET"
            | "TAX_ID_SIREN" | "TAX_ID_VAT" | "LICENSE_PLATE_EU" => id_number_count += 1,
            _ => {}
        }
    }

    let unique_names: std::collections::HashSet<_> = entities
        .iter()
        .filter(|e| e.pii_type == "PERSON")
        .map(|e| e.text.to_lowercase())
        .collect();

    let k_anonymity_risk = if id_number_count > 0 {
        "CRITICAL (direct identifiers present)"
    } else if unique_names.len() > 5 && date_count > 0 && location_count > 0 {
        "HIGH (quasi-identifier combination)"
    } else if unique_names.len() > 3 {
        "MEDIUM (multiple names)"
    } else {
        "LOW"
    };

    PiiReport {
        person_count,
        date_count,
        location_count,
        contact_count,
        id_number_count,
        entities: entities.to_vec(),
        k_anonymity_risk: k_anonymity_risk.to_string(),
    }
}

/// Redact PII by replacing with type tokens (`[PERSON_1]`, `[ID_NUMBER_2]`, etc.).
///
/// Entity offsets are character offsets (Unicode scalar values). Entities must
/// not overlap -- overlapping spans produce garbled output because each
/// replacement shifts byte offsets for subsequent replacements.
pub fn redact(text: &str, entities: &[PiiEntity]) -> String {
    let mut result = text.to_string();
    let mut type_counts: HashMap<&str, usize> = HashMap::new();

    // Deduplicate and remove overlapping spans before redacting.
    // Sort by start ascending, longest span first for ties.
    let mut sorted: Vec<_> = entities.iter().collect();
    sorted.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    sorted.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    // Keep only non-overlapping spans (greedy, longest first at each position).
    let mut max_end = 0;
    sorted.retain(|e| {
        if e.start < max_end {
            false
        } else {
            max_end = e.end;
            true
        }
    });
    // Reverse for back-to-front replacement (so char offsets stay valid).
    sorted.reverse();

    for entity in sorted {
        let count = type_counts.entry(&entity.pii_type).or_insert(0);
        *count += 1;
        let replacement = format!("[{}_{}]", entity.pii_type, count);
        // Convert char offsets to byte offsets for replace_range
        let byte_start: usize = result
            .chars()
            .take(entity.start)
            .map(|c| c.len_utf8())
            .sum();
        let byte_end: usize = result.chars().take(entity.end).map(|c| c.len_utf8()).sum();
        result.replace_range(byte_start..byte_end, &replacement);
    }

    result
}

/// Redact structured PII (SSN, credit card, IBAN, email, phone, address) from
/// a string without loading an NER model.
///
/// Runs [`scan_patterns`] followed by [`redact`]. Suitable for log pipelines
/// and other hot paths where model load time is unacceptable; catches all
/// pattern-detectable PII but misses names (those require NER). For the full
/// pipeline including name detection, use [`scan_and_redact`].
///
/// # Example
///
/// ```
/// use anno::pii;
///
/// let scrubbed = pii::redact_patterns("SSN 123-45-6789 and email a@b.com");
/// assert!(scrubbed.contains("[ID_NUMBER_1]"));
/// assert!(scrubbed.contains("[CONTACT_1]"));
/// ```
pub fn redact_patterns(text: &str) -> String {
    let entities = scan_patterns(text);
    redact(text, &entities)
}

/// Replace each PII span with a fixed character (e.g. `'*'`), preserving length.
///
/// Useful for log display where position matters but content must be hidden.
/// Counts are character-level, not byte-level — `mask("héllo", ..., '*')` on the
/// entire span returns `"*****"` (5 chars), not `"******"`.
///
/// Entity offsets are character offsets. Overlapping spans are deduped the same
/// way [`redact`] deduplicates them.
pub fn mask(text: &str, entities: &[PiiEntity], fill: char) -> String {
    apply_per_entity(text, entities, |entity| {
        let width = entity.end.saturating_sub(entity.start);
        std::iter::repeat_n(fill, width).collect::<String>()
    })
}

/// Replace each PII span with a short fingerprint derived from the entity text.
///
/// The fingerprint is a 64-bit FxHash, hex-encoded. Same input always yields the
/// same fingerprint in the same process, which lets downstream systems correlate
/// occurrences of the same PII value without knowing its content. This is not
/// cryptographically secure — use it for log-scrub and analytics, not secrets.
///
/// Format: `[<TYPE>_<8-hex>]`, e.g. `"[PERSON_a1b2c3d4]"`.
pub fn fingerprint(text: &str, entities: &[PiiEntity]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    apply_per_entity(text, entities, |entity| {
        let mut h = DefaultHasher::new();
        entity.text.hash(&mut h);
        format!(
            "[{}_{:08x}]",
            entity.pii_type,
            (h.finish() & 0xFFFF_FFFF) as u32
        )
    })
}

/// Apply a caller-supplied replacement function to each PII span.
///
/// Generic version of [`redact`] / [`mask`] / [`fingerprint`] — use this when
/// the built-in operators don't fit. `replacement_fn` is called once per
/// entity after dedup+sort; the return value replaces that span.
///
/// Entity offsets are character offsets; internal byte-offset conversion is
/// handled here.
pub fn replace<F>(text: &str, entities: &[PiiEntity], mut replacement_fn: F) -> String
where
    F: FnMut(&PiiEntity) -> String,
{
    apply_per_entity(text, entities, |e| replacement_fn(e))
}

/// Shared core for `redact` / `mask` / `fingerprint` / `replace`.
///
/// Deduplicates overlapping spans greedily (keeps longest-at-start), then
/// walks them back-to-front so earlier char offsets remain valid during
/// `replace_range` calls.
fn apply_per_entity<F>(text: &str, entities: &[PiiEntity], mut replacement_fn: F) -> String
where
    F: FnMut(&PiiEntity) -> String,
{
    let mut result = text.to_string();

    let mut sorted: Vec<_> = entities.iter().collect();
    sorted.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    sorted.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    let mut max_end = 0;
    sorted.retain(|e| {
        if e.start < max_end {
            false
        } else {
            max_end = e.end;
            true
        }
    });
    sorted.reverse();

    for entity in sorted {
        let byte_start: usize = result
            .chars()
            .take(entity.start)
            .map(|c| c.len_utf8())
            .sum();
        let byte_end: usize = result.chars().take(entity.end).map(|c| c.len_utf8()).sum();
        let replacement = replacement_fn(entity);
        result.replace_range(byte_start..byte_end, &replacement);
    }

    result
}

/// Pseudonymize PII with consistent fake values.
///
/// Returns `(pseudonymized_text, mapping)` where mapping maps original -> fake
/// for audit/re-identification purposes.
pub fn pseudonymize(text: &str, entities: &[PiiEntity]) -> (String, HashMap<String, String>) {
    let mut result = text.to_string();
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut name_counter = 0;
    let mut date_counter = 0;
    let mut addr_counter = 0;

    let fake_names = [
        "John Smith",
        "Jane Doe",
        "Alex Johnson",
        "Sam Williams",
        "Chris Brown",
        "Pat Davis",
        "Jordan Miller",
        "Taylor Wilson",
        "Morgan Lee",
        "Casey Martinez",
    ];

    let mut sorted: Vec<_> = entities.iter().collect();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.start));

    for entity in sorted {
        let fake = if let Some(existing) = mapping.get(&entity.text) {
            existing.clone()
        } else {
            let fake = match entity.pii_type.as_str() {
                "PERSON" => {
                    let name = fake_names[name_counter % fake_names.len()];
                    name_counter += 1;
                    name.to_string()
                }
                "DOB" => {
                    date_counter += 1;
                    format!("1990-01-{:02}", (date_counter % 28) + 1)
                }
                "ADDRESS" => {
                    addr_counter += 1;
                    format!("{} Main St", 100 + addr_counter)
                }
                "CONTACT" => {
                    if entity.text.contains('@') {
                        "contact@example.com".to_string()
                    } else {
                        format!("555-000-{:04}", (entity.start % 9000) + 1000)
                    }
                }
                "ID_NUMBER" => "XXX-XX-XXXX".to_string(),
                _ => "[REDACTED]".to_string(),
            };
            mapping.insert(entity.text.clone(), fake.clone());
            fake
        };

        // Convert char offsets to byte offsets for replace_range
        let byte_start: usize = result
            .chars()
            .take(entity.start)
            .map(|c| c.len_utf8())
            .sum();
        let byte_end: usize = result.chars().take(entity.end).map(|c| c.len_utf8()).sum();
        result.replace_range(byte_start..byte_end, &fake);
    }

    (result, mapping)
}

/// Scan for PII and redact in one call.
///
/// Combines [`classify_entity`] (NER-based) with [`scan_patterns`] (regex-based)
/// and applies [`redact`].
///
/// ```
/// use anno::{pii, Model, StackedNER};
///
/// let text = "John's SSN is 123-45-6789.";
/// let m = StackedNER::default();
/// let redacted = pii::scan_and_redact(text, &m)?;
/// assert!(!redacted.contains("123-45-6789"));
/// # Ok::<(), anno::Error>(())
/// ```
pub fn scan_and_redact(text: &str, model: &dyn crate::Model) -> crate::Result<String> {
    let entities = model.extract_entities(text, None)?;
    let mut pii_entities: Vec<PiiEntity> = entities.iter().filter_map(classify_entity).collect();
    pii_entities.extend(scan_patterns(text));
    dedup_overlapping(&mut pii_entities);
    Ok(redact(text, &pii_entities))
}

/// Remove duplicate and overlapping PII entities, keeping the longest span.
///
/// After merging NER-based and regex-based detections, duplicates and overlaps
/// are common (e.g., NER finds "John Smith" and regex finds "123-45-6789" within
/// a span the NER also matched). This function sorts by start offset, then
/// greedily keeps the longest non-overlapping spans.
fn dedup_overlapping(entities: &mut Vec<PiiEntity>) {
    // Sort by start, then longest span first for ties
    entities.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    // Dedup exact duplicates
    entities.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    // Remove overlaps: keep the first (longest at each start position)
    let mut max_end = 0;
    entities.retain(|e| {
        if e.start < max_end {
            false // overlaps with a prior span
        } else {
            max_end = e.end;
            true
        }
    });
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn assess_person_risk(text: &str) -> &'static str {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() >= 3 {
        "HIGH"
    } else if words.len() == 2 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

fn looks_like_dob(text: &str) -> bool {
    Regex::new(r"19[0-9]{2}|20[0-1][0-9]")
        .map(|re| re.is_match(text))
        .unwrap_or(false)
}

/// Check if text looks like a physical address.
pub fn looks_like_address(text: &str) -> bool {
    let has_number = text.chars().any(|c| c.is_numeric());
    let street_indicators = [
        "St", "Street", "Ave", "Avenue", "Rd", "Road", "Blvd", "Dr", "Lane", "Ln", "Way", "Drive",
        "Court", "Ct", "Place", "Pl", "Circle", "Cir",
    ];
    let has_street = street_indicators.iter().any(|ind| text.contains(ind));

    let has_zip = Regex::new(r"\b\d{5}(?:-\d{4})?\b")
        .map(|re| re.is_match(text))
        .unwrap_or(false);
    let us_states = [
        "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA",
        "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ",
        "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT",
        "VA", "WA", "WV", "WI", "WY", "DC",
    ];
    let has_state = us_states.iter().any(|s| text.contains(s));

    (has_number && has_street) || (has_zip && has_state)
}

/// Check if text looks like an ID number (SSN, credit card, IBAN, MRN).
pub fn looks_like_id_number(text: &str) -> bool {
    if let Ok(re) = Regex::new(r"\d{3}-\d{2}-\d{4}") {
        if re.is_match(text) {
            return true;
        }
    }
    if let Ok(re) = Regex::new(r"\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}") {
        if re.is_match(text) {
            return true;
        }
    }
    if let Ok(re) = Regex::new(r"[A-Z]{2}\d{2}[A-Z0-9]{4}\d{7}([A-Z0-9]{0,16})?") {
        if re.is_match(text) {
            return true;
        }
    }
    // Alphanumeric catch-all for short ID-like tokens (e.g. MRNs, short codes).
    // Require that digits make up at least half the characters to avoid
    // false-positives on version strings like "Python3", "iPhone6", "Cent0S".
    let digit_count = text.chars().filter(|c| c.is_ascii_digit()).count();
    if text.len() >= 6
        && text.len() <= 10
        && text.chars().all(|c| c.is_alphanumeric())
        && digit_count * 2 >= text.len()
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "pii-eu")]
    fn pesel_80051501231_valid() {
        // Valid PESEL: sum(d*w for weights [1,3,7,9,1,3,7,9,1,3])=89, check=(10-9)%10=1
        assert!(is_valid_pesel("80051501231"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn pesel_80051501230_invalid() {
        // Check digit should be 1, not 0
        assert!(!is_valid_pesel("80051501230"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn pesel_wrong_length_rejected() {
        assert!(!is_valid_pesel("8005150123")); // 10 digits
        assert!(!is_valid_pesel("800515012345")); // 12 digits
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn bsn_123456782_valid() {
        // Example with valid mod-11 checksum
        assert!(is_valid_bsn("123456782"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn bsn_123456780_invalid() {
        assert!(!is_valid_bsn("123456780"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn belgian_registre_valid() {
        // 800515012 % 97 = 8, check = 97 - 8 = 89 → "80051501289"
        assert!(is_valid_belgian_registre("80051501289"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn belgian_registre_invalid() {
        // Wrong check digits (89 expected, 94 provided)
        assert!(!is_valid_belgian_registre("80051501294"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn belgian_registre_post2000_valid() {
        // Born 2001-05-15, sequence 012: n = 010515012 = 10_515_012
        // 2_000_000_000 % 97 = 68; 10_515_012 % 97 = 18
        // (68 + 18) % 97 = 86 → check = 97 - 86 = 11 → "01051501211"
        assert!(is_valid_belgian_registre("01051501211"));
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn belgian_registre_post2000_rejected_by_pre2000_formula() {
        // "01051501211": pre-2000 check = 97 - (10_515_012 % 97) = 97 - 18 = 79 ≠ 11
        // so only the post-2000 path accepts it
        let n: u64 = 10_515_012;
        assert_ne!(
            97 - (n % 97),
            11,
            "pre-2000 formula must not accept this number"
        );
        assert!(
            is_valid_belgian_registre("01051501211"),
            "but overall must accept via post-2000 path"
        );
    }

    // --- EU national IDs ---

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_fr_insee() {
        let result = scan_patterns("INSEE: 185057511602324");
        assert!(
            result.iter().any(|p| p.pii_type == "NATIONAL_ID_FR"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_pl_pesel_valid() {
        let result = scan_patterns("PESEL: 80051501231");
        assert!(
            result.iter().any(|p| p.pii_type == "NATIONAL_ID_PL"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_rejects_pl_pesel_invalid_checksum() {
        let result = scan_patterns("80051501230");
        assert!(
            !result.iter().any(|p| p.pii_type == "NATIONAL_ID_PL"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_nl_bsn_valid() {
        let result = scan_patterns("BSN: 123456782");
        assert!(
            result.iter().any(|p| p.pii_type == "NATIONAL_ID_NL"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_es_dni() {
        let result = scan_patterns("DNI: 12345678Z");
        assert!(
            result.iter().any(|p| p.pii_type == "NATIONAL_ID_ES"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_it_codice_fiscale() {
        let result = scan_patterns("Codice Fiscale: RSSMRA85T10A562S");
        assert!(
            result.iter().any(|p| p.pii_type == "NATIONAL_ID_IT"),
            "{result:?}"
        );
    }

    // --- GDPR Art. 9 special categories ---

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_health_keyword() {
        let result = scan_patterns("Patient diagnosed with diabetes");
        assert!(
            result
                .iter()
                .any(|p| p.pii_type == "SPECIAL_CATEGORY_HEALTH"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_religion_keyword() {
        let result = scan_patterns("He is Catholic");
        assert!(
            result
                .iter()
                .any(|p| p.pii_type == "SPECIAL_CATEGORY_RELIGION"),
            "{result:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_criminal_keyword() {
        let result = scan_patterns("He was convicted of fraud");
        assert!(
            result
                .iter()
                .any(|p| p.pii_type == "SPECIAL_CATEGORY_CRIMINAL"),
            "{result:?}"
        );
    }

    // --- Tax IDs ---

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_patterns_detects_fr_siret() {
        let result = scan_patterns("SIRET: 73282932000074");
        assert!(
            result.iter().any(|p| p.pii_type == "TAX_ID_SIRET"),
            "{result:?}"
        );
    }

    // --- Integration: UTF-8 offsets ---

    #[test]
    #[cfg(feature = "pii-eu")]
    fn eu_detection_uses_char_offsets() {
        let text = "Café PESEL: 80051501231 end";
        let result = scan_patterns(text);
        let pesel = result
            .iter()
            .find(|p| p.pii_type == "NATIONAL_ID_PL")
            .expect("PESEL found");
        let extracted: String = text
            .chars()
            .skip(pesel.start)
            .take(pesel.end - pesel.start)
            .collect();
        assert_eq!(extracted, "80051501231");
    }

    #[test]
    fn ssn_detected_by_scan() {
        let pii = scan_patterns("My SSN is 123-45-6789 and that's it.");
        assert!(pii.iter().any(|p| p.text == "123-45-6789"));
    }

    #[test]
    fn credit_card_detected() {
        let pii = scan_patterns("Card: 4111-1111-1111-1111 on file.");
        assert!(pii.iter().any(|p| p.text == "4111-1111-1111-1111"));
    }

    #[test]
    fn email_detected() {
        let pii = scan_patterns("Contact me at bob@example.com please.");
        assert!(pii.iter().any(|p| p.pii_type == "CONTACT"));
    }

    #[test]
    fn iban_detected() {
        assert!(looks_like_id_number("DE89370400440532013000"));
    }

    #[test]
    fn common_word_not_id() {
        assert!(!looks_like_id_number("Chemistry"));
    }

    #[test]
    fn version_strings_not_id() {
        assert!(!looks_like_id_number("Python3"));
        assert!(!looks_like_id_number("Win10"));
        assert!(!looks_like_id_number("iPhone6"));
        assert!(!looks_like_id_number("Cent0S"));
    }

    #[test]
    fn address_with_zip() {
        assert!(looks_like_address("1234 Elm Street, Springfield, IL 62704"));
    }

    #[test]
    fn redact_replaces_pii() {
        // "My SSN is " = 10 chars, "123-45-6789" = chars 10..21
        let entities = vec![PiiEntity {
            text: "123-45-6789".to_string(),
            pii_type: "ID_NUMBER".to_string(),
            start: 10,
            end: 21,
            risk_level: "CRITICAL".to_string(),
        }];
        let result = redact("My SSN is 123-45-6789.", &entities);
        assert_eq!(result, "My SSN is [ID_NUMBER_1].");
    }

    #[test]
    fn redact_non_ascii() {
        // "caf\u{e9}" is 4 chars (e with accent = 1 char, 2 bytes)
        let text = "caf\u{e9} SSN: 123-45-6789.";
        let entities = vec![PiiEntity {
            text: "123-45-6789".to_string(),
            pii_type: "ID_NUMBER".to_string(),
            start: 10, // "caf\u{e9} SSN: " = 10 chars
            end: 21,   // 10 + 11 chars
            risk_level: "CRITICAL".to_string(),
        }];
        let result = redact(text, &entities);
        assert_eq!(result, "caf\u{e9} SSN: [ID_NUMBER_1].");
        assert!(!result.contains("123-45-6789"));
    }

    #[test]
    fn scan_patterns_returns_char_offsets() {
        let text = "caf\u{e9} SSN: 123-45-6789 end";
        let pii = scan_patterns(text);
        let ssn = pii.iter().find(|p| p.text == "123-45-6789");
        assert!(ssn.is_some(), "should detect SSN");
        let ssn = ssn.unwrap();
        // Verify these are char offsets, not byte offsets
        let extracted: String = text
            .chars()
            .skip(ssn.start)
            .take(ssn.end - ssn.start)
            .collect();
        assert_eq!(extracted, "123-45-6789");
    }

    #[test]
    fn pseudonymize_consistent() {
        let entities = vec![
            PiiEntity {
                text: "bob@example.com".to_string(),
                pii_type: "CONTACT".to_string(),
                start: 0,
                end: 15,
                risk_level: "HIGH".to_string(),
            },
            PiiEntity {
                text: "555-867-5309".to_string(),
                pii_type: "CONTACT".to_string(),
                start: 20,
                end: 32,
                risk_level: "HIGH".to_string(),
            },
        ];
        let (result, mapping) = pseudonymize("bob@example.com --- 555-867-5309", &entities);
        assert!(mapping.get("bob@example.com").unwrap().contains('@'));
        assert!(mapping.get("555-867-5309").unwrap().starts_with("555-000-"));
        assert!(!result.contains("bob@example.com"));
    }

    #[test]
    fn pseudonymize_same_entity_gets_same_pseudonym() {
        // The same entity text appearing twice should produce the same pseudonym.
        let entities = vec![
            PiiEntity {
                text: "John Smith".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 10,
                risk_level: "MEDIUM".to_string(),
            },
            PiiEntity {
                text: "John Smith".to_string(),
                pii_type: "PERSON".to_string(),
                start: 15,
                end: 25,
                risk_level: "MEDIUM".to_string(),
            },
        ];
        let text = "John Smith met John Smith again.";
        let (result, mapping) = pseudonymize(text, &entities);
        let fake = mapping
            .get("John Smith")
            .expect("mapping should contain John Smith");
        // Both occurrences should be replaced with the same pseudonym
        assert_eq!(
            result.matches(fake.as_str()).count(),
            2,
            "Both occurrences of 'John Smith' should map to the same pseudonym '{}', got: {}",
            fake,
            result
        );
    }

    #[test]
    fn redact_overlapping_spans_no_panic() {
        // Overlapping spans should be resolved gracefully (no panic, no garbled output).
        // The implementation drops the inner span, keeping the outer one.
        let entities = vec![
            PiiEntity {
                text: "John Smith".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 10,
                risk_level: "MEDIUM".to_string(),
            },
            PiiEntity {
                // Overlaps with "John Smith"
                text: "John".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 4,
                risk_level: "LOW".to_string(),
            },
        ];
        let text = "John Smith called.";
        // Should not panic and should produce valid UTF-8 output
        let result = redact(text, &entities);
        assert!(
            !result.contains("John Smith"),
            "original text should be redacted"
        );
        assert!(
            result.contains("called"),
            "non-PII text should be preserved"
        );
    }

    #[test]
    fn report_counts() {
        let entities = vec![
            PiiEntity {
                text: "John".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 4,
                risk_level: "LOW".to_string(),
            },
            PiiEntity {
                text: "123-45-6789".to_string(),
                pii_type: "ID_NUMBER".to_string(),
                start: 10,
                end: 21,
                risk_level: "CRITICAL".to_string(),
            },
        ];
        let r = report(&entities);
        assert_eq!(r.person_count, 1);
        assert_eq!(r.id_number_count, 1);
        assert!(r.k_anonymity_risk.starts_with("CRITICAL"));
    }

    #[test]
    fn mask_preserves_length_and_position() {
        let text = "John met Alice.";
        let entities = vec![
            PiiEntity {
                text: "John".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 4,
                risk_level: "LOW".to_string(),
            },
            PiiEntity {
                text: "Alice".to_string(),
                pii_type: "PERSON".to_string(),
                start: 9,
                end: 14,
                risk_level: "LOW".to_string(),
            },
        ];
        let masked = mask(text, &entities, '*');
        assert_eq!(masked, "**** met *****.");
    }

    #[test]
    fn mask_handles_multibyte_unicode() {
        // "café" is 4 chars (4 code points) but 5 bytes in UTF-8.
        // mask works in character-space, so the result has 4 fill chars.
        let text = "café alice";
        let entities = vec![PiiEntity {
            text: "café".to_string(),
            pii_type: "PERSON".to_string(),
            start: 0,
            end: 4,
            risk_level: "LOW".to_string(),
        }];
        let masked = mask(text, &entities, '#');
        assert_eq!(masked, "#### alice");
    }

    #[test]
    fn fingerprint_is_deterministic_same_input() {
        let text = "John met John.";
        let entities = vec![
            PiiEntity {
                text: "John".to_string(),
                pii_type: "PERSON".to_string(),
                start: 0,
                end: 4,
                risk_level: "LOW".to_string(),
            },
            PiiEntity {
                text: "John".to_string(),
                pii_type: "PERSON".to_string(),
                start: 9,
                end: 13,
                risk_level: "LOW".to_string(),
            },
        ];
        let fp = fingerprint(text, &entities);
        // Both occurrences of "John" should receive the same fingerprint.
        let tokens: Vec<&str> = fp
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '[' && c != ']')
            .filter(|s| s.starts_with("[PERSON_") && s.ends_with(']'))
            .collect();
        assert_eq!(
            tokens.len(),
            2,
            "expected two fingerprint tokens, got {fp:?}"
        );
        assert_eq!(
            tokens[0], tokens[1],
            "identical entity text must produce identical fingerprint"
        );
    }

    #[test]
    fn replace_applies_caller_fn() {
        let text = "SSN 123-45-6789 recorded.";
        let entities = vec![PiiEntity {
            text: "123-45-6789".to_string(),
            pii_type: "ID_NUMBER".to_string(),
            start: 4,
            end: 15,
            risk_level: "CRITICAL".to_string(),
        }];
        let replaced = replace(text, &entities, |e| format!("<{}>", e.pii_type));
        assert_eq!(replaced, "SSN <ID_NUMBER> recorded.");
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_eu_structured_does_not_contain_keyword_match() {
        // scan_eu_structured must NOT match "diabetes" — keywords are in scan_eu_art9_keywords
        let mut results = Vec::new();
        scan_eu_structured("Patient has diabetes", &mut results);
        assert!(
            !results
                .iter()
                .any(|e| e.pii_type.starts_with("SPECIAL_CATEGORY")),
            "scan_eu_structured must not run keyword patterns: {results:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn scan_eu_art9_keywords_matches_health() {
        let mut results = Vec::new();
        scan_eu_art9_keywords("Patient has diabetes", &mut results);
        assert!(
            results
                .iter()
                .any(|e| e.pii_type == "SPECIAL_CATEGORY_HEALTH"),
            "scan_eu_art9_keywords must match health keywords: {results:?}"
        );
    }

    #[test]
    fn scan_generic_patterns_detects_ssn() {
        let mut results = Vec::new();
        scan_generic_patterns("SSN: 123-45-6789", &mut results);
        assert!(
            results.iter().any(|e| e.text == "123-45-6789"),
            "{results:?}"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_type_mapping_health() {
        assert_eq!(
            pii_type_from_art9_label("health condition"),
            "SPECIAL_CATEGORY_HEALTH"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_risk_health_is_critical() {
        assert_eq!(art9_risk_level("health condition"), "CRITICAL");
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_risk_union_is_medium() {
        assert_eq!(art9_risk_level("trade union membership"), "MEDIUM");
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_type_mapping_remaining_labels() {
        assert_eq!(
            pii_type_from_art9_label("biometric data"),
            "SPECIAL_CATEGORY_BIOMETRIC"
        );
        assert_eq!(
            pii_type_from_art9_label("genetic data"),
            "SPECIAL_CATEGORY_GENETIC"
        );
        assert_eq!(
            pii_type_from_art9_label("political opinion"),
            "SPECIAL_CATEGORY_POLITICAL"
        );
        assert_eq!(
            pii_type_from_art9_label("religious belief"),
            "SPECIAL_CATEGORY_RELIGION"
        );
        assert_eq!(
            pii_type_from_art9_label("trade union membership"),
            "SPECIAL_CATEGORY_UNION"
        );
        assert_eq!(
            pii_type_from_art9_label("criminal record"),
            "SPECIAL_CATEGORY_CRIMINAL"
        );
        assert_eq!(
            pii_type_from_art9_label("sexual orientation"),
            "SPECIAL_CATEGORY_SEXUAL_ORIENTATION"
        );
        assert_eq!(
            pii_type_from_art9_label("ethnic origin"),
            "SPECIAL_CATEGORY_ETHNIC"
        );
        assert_eq!(
            pii_type_from_art9_label("unknown label"),
            "SPECIAL_CATEGORY"
        );
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_risk_critical_labels() {
        assert_eq!(art9_risk_level("biometric data"), "CRITICAL");
        assert_eq!(art9_risk_level("genetic data"), "CRITICAL");
        assert_eq!(art9_risk_level("criminal record"), "CRITICAL");
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn art9_risk_high_labels() {
        assert_eq!(art9_risk_level("political opinion"), "HIGH");
        assert_eq!(art9_risk_level("religious belief"), "HIGH");
        assert_eq!(art9_risk_level("sexual orientation"), "HIGH");
        assert_eq!(art9_risk_level("ethnic origin"), "HIGH");
        assert_eq!(art9_risk_level("unknown label"), "HIGH");
    }

    #[test]
    #[cfg(feature = "pii-eu")]
    fn report_counts_eu_national_id_as_direct_identifier() {
        let entities = vec![PiiEntity {
            text: "80051501231".to_string(),
            pii_type: "NATIONAL_ID_PL".to_string(),
            start: 0,
            end: 11,
            risk_level: "CRITICAL".to_string(),
        }];
        let r = report(&entities);
        assert_eq!(
            r.id_number_count, 1,
            "NATIONAL_ID_PL must count as id_number"
        );
        assert!(
            r.k_anonymity_risk.starts_with("CRITICAL"),
            "k_anonymity_risk must be CRITICAL when EU national ID present: {}",
            r.k_anonymity_risk
        );
    }

    #[cfg(all(test, feature = "pii-eu", feature = "gliner2-fastino"))]
    mod ner_unit_tests {
        use super::*;

        /// Returns empty — tests that structured patterns still work without NER hits.
        struct NullNer;
        impl crate::ZeroShotNER for NullNer {
            fn default_types(&self) -> &[&'static str] {
                &[]
            }
            fn extract_with_types(
                &self,
                _text: &str,
                _types: &[&str],
                _threshold: f32,
            ) -> crate::Result<Vec<crate::Entity>> {
                Ok(vec![])
            }
            fn extract_with_descriptions(
                &self,
                _text: &str,
                _descriptions: &[&str],
                _threshold: f32,
            ) -> crate::Result<Vec<crate::Entity>> {
                Ok(vec![])
            }
        }

        /// Returns the whole input text as one entity with the given label.
        struct StubNer {
            label: &'static str,
        }
        impl crate::ZeroShotNER for StubNer {
            fn default_types(&self) -> &[&'static str] {
                &[]
            }
            fn extract_with_types(
                &self,
                text: &str,
                _types: &[&str],
                _threshold: f32,
            ) -> crate::Result<Vec<crate::Entity>> {
                let char_len = text.chars().count();
                Ok(vec![crate::Entity::builder(
                    text.to_string(),
                    crate::EntityType::custom(self.label, crate::EntityCategory::Misc),
                )
                .span(0, char_len)
                .build()])
            }
            fn extract_with_descriptions(
                &self,
                _text: &str,
                _descriptions: &[&str],
                _threshold: f32,
            ) -> crate::Result<Vec<crate::Entity>> {
                Ok(vec![])
            }
        }

        #[test]
        fn null_ner_still_returns_national_id() {
            let text = "PESEL: 80051501231";
            let found = scan_patterns_with_ner(text, &NullNer, 0.5).expect("scan ok");
            assert!(
                found.iter().any(|e| e.pii_type == "NATIONAL_ID_PL"),
                "structured patterns must run even with null NER: {found:?}"
            );
            assert!(
                !found
                    .iter()
                    .any(|e| e.pii_type.starts_with("SPECIAL_CATEGORY")),
                "no Art.9 entities expected from NullNer: {found:?}"
            );
        }

        #[test]
        fn stub_ner_health_maps_to_special_category_health() {
            let found = scan_patterns_with_ner(
                "diabetes",
                &StubNer {
                    label: "health condition",
                },
                0.0,
            )
            .expect("scan ok");
            let health = found
                .iter()
                .find(|e| e.pii_type == "SPECIAL_CATEGORY_HEALTH");
            assert!(
                health.is_some(),
                "expected SPECIAL_CATEGORY_HEALTH: {found:?}"
            );
            assert_eq!(health.unwrap().risk_level, "CRITICAL");
        }

        #[test]
        fn stub_ner_religion_maps_to_special_category_religion() {
            let found = scan_patterns_with_ner(
                "Muslim",
                &StubNer {
                    label: "religious belief",
                },
                0.0,
            )
            .expect("scan ok");
            let rel = found
                .iter()
                .find(|e| e.pii_type == "SPECIAL_CATEGORY_RELIGION");
            assert!(
                rel.is_some(),
                "expected SPECIAL_CATEGORY_RELIGION: {found:?}"
            );
            assert_eq!(rel.unwrap().risk_level, "HIGH");
        }

        #[test]
        fn stub_ner_unknown_label_maps_to_generic_special_category() {
            let found = scan_patterns_with_ner(
                "something",
                &StubNer {
                    label: "unknown category",
                },
                0.0,
            )
            .expect("scan ok");
            assert!(
                found.iter().any(|e| e.pii_type == "SPECIAL_CATEGORY"),
                "unknown label must fall through to SPECIAL_CATEGORY: {found:?}"
            );
        }

        #[test]
        fn ner_entity_overlapping_structured_is_dropped() {
            // Valid PESEL: StubNer would return an entity covering the same span.
            // The structured pattern (NATIONAL_ID_PL) runs first and claims the span.
            // The NER entity must be dropped by the overlap check.
            let text = "80051501231";
            let found = scan_patterns_with_ner(
                text,
                &StubNer {
                    label: "health condition",
                },
                0.0,
            )
            .expect("scan ok");
            // Only one entity covering [0, 11]
            let at_zero: Vec<_> = found
                .iter()
                .filter(|e| e.start == 0 && e.end == 11)
                .collect();
            assert_eq!(
                at_zero.len(),
                1,
                "overlapping span must be deduped to one entity: {found:?}"
            );
            assert_eq!(
                at_zero[0].pii_type, "NATIONAL_ID_PL",
                "structured pattern must win over NER on overlapping span: {found:?}"
            );
        }
    }
}
