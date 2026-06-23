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
use crate::layers::GdprLayerSet;
use crate::legal::{LegalEntity, LegalLabel};
use crate::validators::{
    apply_validators, dates::DateRangeValidator, email::EmailRfcValidator, iban::Iban97Validator,
    luhn::LuhnValidator, network::IpAddressValidator, nir::NirControlKeyValidator,
    postal::PostalCodeValidator, EntityValidator, RejectionCounts,
};
use anno::backends::gliner2_fastino::GLiNER2Fastino;
use anno::backends::inference::ZeroShotNER;
use anno::EntityType;
use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
use regex::Regex;
use std::collections::HashMap;
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
                r"(?:Monsieur|Madame|Mademoiselle|Mme\.?|Mlle\.?|M\.|Maître|Me\.?|Dr\.?|Pr\.?)\s+(\p{Lu}[\p{L}'\-]+(?:\s+\p{Lu}[\p{L}'\-]+)+)",
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
///
/// # Deprecated
/// Prefer [`crate::config::AnnoRagConfig::ner_model_id`].
#[deprecated(since = "0.12.0", note = "use AnnoRagConfig::ner_model_id instead")]
pub const NER_MODEL_ID: &str = "SemplificaAI/gliner2-multi-v1-onnx";

/// Candle/PyTorch GLiNER2 repo used for Metal and CPU Candle detector backends.
///
/// # Deprecated
/// Prefer [`crate::config::AnnoRagConfig::ner_candle_model_id`].
#[deprecated(
    since = "0.12.0",
    note = "use AnnoRagConfig::ner_candle_model_id instead"
)]
pub const CANDLE_NER_MODEL_ID: &str = "fastino/gliner2-multi-v1";

/// GDPR-coverage NER labels: (label, description sent to the model, per-label threshold).
///
/// Descriptions are in French to match the primary document language. The model
/// receives them as `[E] <label> [DESCRIPTION] <description>` in its schema
/// prompt, which improves span precision over bare labels.
///
/// Thresholds:
/// - Art. 4(1) basic personal data: 0.38–0.50 (balanced precision/recall)
/// - Art. 4(1) identifiers: 0.35–0.42 (slight recall bias)
/// - Art. 9 special categories: 0.30 (recall priority — missed sensitive spans
///   are more costly than false positives in a redaction workflow)
/// - Art. 10 criminal convictions: 0.32
///
/// The regex layer (NIR, SIRET, IBAN-FR, phone FR, email, honorific names)
/// is complementary and always runs first.
static GDPR_NER_LABELS: &[(&str, &str, f32)] = &[
    // ── Art. 4(1) — Basic personal data ──────────────────────────────────────
    ("person",       "nom complet, prénom, nom de famille ou alias d'une personne physique identifiable", 0.40),
    ("address",      "adresse postale complète incluant numéro de voie, rue, ville ou code postal", 0.40),
    ("date_of_birth","date de naissance d'une personne physique", 0.38),
    ("age",          "âge d'une personne physique exprimé en années", 0.45),
    ("nationality",  "nationalité, pays d'origine ou citoyenneté d'une personne physique", 0.42),
    ("profession",   "profession, emploi, titre ou fonction permettant d'identifier une personne", 0.45),
    ("organization", "organisation ou entreprise directement associée à une personne physique identifiable (ex : auto-entrepreneur, médecin libéral)", 0.50),
    ("location",     "lieu de résidence, domicile ou lieu de travail habituel d'une personne physique", 0.48),
    // ── Art. 4(1) — Identifiers ───────────────────────────────────────────────
    ("national_id",  "numéro de carte nationale d'identité, passeport, permis de conduire ou titre de séjour", 0.35),
    ("tax_id",       "numéro fiscal personnel, référence fiscale ou numéro de TVA lié à une personne physique", 0.35),
    ("bank_account", "numéro de carte bancaire, numéro de compte, date d'expiration ou cryptogramme visuel d'une carte", 0.35),
    ("ip_address",   "adresse IP version 4 ou 6, identifiant de session ou cookie permettant d'identifier un utilisateur", 0.38),
    ("username",     "nom d'utilisateur, identifiant de compte, pseudonyme en ligne ou handle sur réseau social", 0.42),
    ("device_id",    "adresse MAC, numéro IMEI ou identifiant unique d'un appareil personnel", 0.40),
    // ── Art. 9 — Special categories (lower threshold: recall priority) ────────
    ("racial_ethnic_origin",  "origine raciale ou ethnique, appartenance déclarée à un groupe ethnique ou communautaire", 0.30),
    ("political_opinion",     "opinion politique, affiliation partisane ou conviction politique d'une personne", 0.30),
    ("religious_belief",      "croyance religieuse, conviction philosophique, appartenance à une religion, culte ou secte", 0.30),
    ("trade_union_membership","adhésion syndicale, appartenance à un syndicat ou mandat de représentation syndicale", 0.30),
    ("health_data",           "état de santé, maladie, diagnostic, traitement, ordonnance, handicap ou antécédent médical d'une personne", 0.30),
    ("genetic_data",          "données génétiques, résultat de test ADN, séquence génomique ou information héréditaire", 0.30),
    ("biometric_data",        "empreinte digitale, reconnaissance faciale, scan d'iris, empreinte vocale ou toute donnée biométrique unique", 0.30),
    ("sexual_orientation",    "orientation sexuelle, vie sexuelle ou identité de genre d'une personne physique", 0.30),
    // ── Art. 10 — Criminal convictions ───────────────────────────────────────
    ("criminal_record", "condamnation pénale, infraction, casier judiciaire, mise en examen ou poursuite pénale", 0.32),
];

fn gdpr_described() -> Vec<(&'static str, &'static str)> {
    GDPR_NER_LABELS.iter().map(|(l, d, _)| (*l, *d)).collect()
}

fn gdpr_label_thresholds() -> HashMap<&'static str, f32> {
    GDPR_NER_LABELS.iter().map(|(l, _, t)| (*l, *t)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DetectorOnnxProviderConfig {
    use_cpu_provider: bool,
    prefer_coreml: bool,
    prefer_cuda: bool,
}

fn detector_onnx_config_for(
    preference: crate::accelerator::AcceleratorPreference,
) -> Result<DetectorOnnxProviderConfig> {
    let requested = crate::accelerator::AcceleratorPreference::from_env_or(preference)?;
    let mut onnx = DetectorOnnxProviderConfig {
        use_cpu_provider: true,
        prefer_coreml: false,
        prefer_cuda: false,
    };
    match requested {
        crate::accelerator::AcceleratorPreference::Cpu => {
            onnx.prefer_cuda = false;
            onnx.prefer_coreml = false;
        }
        crate::accelerator::AcceleratorPreference::Auto => {
            onnx.prefer_cuda = cfg!(feature = "gpu-cuda");
        }
        crate::accelerator::AcceleratorPreference::Cuda => {
            if !cfg!(feature = "gpu-cuda") {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=cuda requires a binary built with feature gpu-cuda".into(),
                ));
            }
            onnx.prefer_cuda = true;
        }
        crate::accelerator::AcceleratorPreference::Metal => {
            if !cfg!(feature = "gpu-metal") {
                return Err(Error::Config(
                    "ANNO_ACCELERATOR=metal requires a binary built with feature gpu-metal".into(),
                ));
            }
            onnx.prefer_coreml = false;
        }
    }
    Ok(onnx)
}

fn detector_model_config_for(
    preference: crate::accelerator::AcceleratorPreference,
) -> Result<anno::backends::gliner2_fastino::GLiNER2FastinoConfig> {
    let onnx = detector_onnx_config_for(preference)?;
    Ok(
        anno::backends::gliner2_fastino::GLiNER2FastinoConfig::default()
            .with_onnx_provider_preferences(
                onnx.use_cpu_provider,
                onnx.prefer_coreml,
                onnx.prefer_cuda,
            ),
    )
}

/// PII labels recognized by the current [`Detector::detect`] NER layer.
#[must_use]
pub fn pii_label_set() -> Vec<&'static str> {
    GDPR_NER_LABELS.iter().map(|(l, _, _)| *l).collect()
}

/// Returns true when `name` is one of the detector's GDPR NER labels.
#[must_use]
pub fn is_pii_label(name: &str) -> bool {
    GDPR_NER_LABELS.iter().any(|(l, _, _)| *l == name)
}

/// Return the union of PII NER labels and `extra_labels`, de-duplicated.
#[must_use]
pub fn combined_label_set(extra_labels: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = GDPR_NER_LABELS.iter().map(|(l, _, _)| *l).collect();
    for label in extra_labels {
        if let Some(static_ref) = static_label_ref(label) {
            if !out.contains(&static_ref) {
                out.push(static_ref);
            }
        }
    }
    out
}

fn static_label_ref(label: &str) -> Option<&'static str> {
    crate::legal::default_legal_labels()
        .into_iter()
        .find(|candidate| candidate.name == label)
        .map(|candidate| candidate.name)
}

/// Aggregate PII detector: FR regex pack + anno NER.
enum NerBackend {
    Onnx(GLiNER2Fastino),
    #[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
    Candle(anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle),
}

impl NerBackend {
    fn extract_with_types(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> anno::Result<Vec<anno::Entity>> {
        match self {
            Self::Onnx(model) => model.extract_with_types(text, labels, threshold),
            #[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
            Self::Candle(model) => model.extract_with_types(text, labels, threshold),
        }
    }

    fn extract_with_label_thresholds(
        &self,
        text: &str,
        label_thresholds: &[(&str, f32)],
    ) -> anno::Result<Vec<anno::Entity>> {
        match self {
            Self::Onnx(model) => model.extract_with_label_thresholds(text, label_thresholds),
            #[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
            Self::Candle(model) => model.extract_with_label_thresholds(text, label_thresholds),
        }
    }

    fn extract_with_label_descriptions(
        &self,
        text: &str,
        labeled: &[(&str, &str)],
        threshold: f32,
    ) -> anno::Result<Vec<anno::Entity>> {
        match self {
            Self::Onnx(model) => model.extract_with_label_descriptions(text, labeled, threshold),
            #[cfg(any(feature = "gpu-metal", feature = "gliner2-candle-cpu"))]
            Self::Candle(model) => model.extract_with_label_descriptions(text, labeled, threshold),
        }
    }
}

/// PII detector: GLiNER2 NER plus, on defense+ layers, the deterministic
/// French heuristics backend.
pub struct Detector {
    /// Legal/generalist NER backend (SemplificaAI/gliner2-multi-v1-onnx).
    /// Used by detect_with_labels and the legal extraction pipeline.
    ner: NerBackend,
    /// PII-specialized NER backend (fastino/gliner2-privacy-filter-PII-multi ONNX).
    /// Used by detect() / detect_inner() for GDPR privacy pipelines.
    /// Always ONNX — no Candle variant exists for this model.
    pii_ner: GLiNER2Fastino,
    #[cfg(feature = "heuristic-fr")]
    heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer,
    gdpr_layers: GdprLayerSet,
    /// HuggingFace model ID used by the active NER backend — surfaced in
    /// audit events so operators can reconcile behaviour against a model version.
    ner_model_id: String,
    /// PII model ID surfaced in audit events.
    pii_ner_model_id: String,
}

impl Detector {
    /// Load the PII-specialized NER model (always ONNX — no Candle variant).
    fn load_pii_ner(cfg: &crate::config::AnnoRagConfig) -> Result<GLiNER2Fastino> {
        let model_root = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| cfg.models_cache());
        let pii_path = model_root.join(cfg.ner_pii_onnx_dir());
        if pii_path.exists() {
            GLiNER2Fastino::from_local_with_config(
                &pii_path,
                anno::backends::gliner2_fastino::GLiNER2FastinoConfig::default(),
            )
            .map_err(|e| Error::Detect(format!("pii ner load (local): {e}")))
        } else {
            GLiNER2Fastino::from_pretrained_with_config(
                &cfg.ner_pii_model_id,
                anno::backends::gliner2_fastino::GLiNER2FastinoConfig::default(),
            )
            .map_err(|e| Error::Detect(format!("pii ner load: {e}")))
        }
    }

    /// Build a new detector. Loads the GLiNER2Fastino multi-v1 ONNX model
    /// (multilingual, FR-aware) from the HF Hub cache.
    pub fn new(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let requested = crate::accelerator::AcceleratorPreference::from_env_or(cfg.accelerator)?;
        let decision = crate::accelerator::resolve(requested)?;
        if matches!(
            decision.selected,
            crate::accelerator::SelectedAccelerator::Metal
        ) {
            return Self::new_candle_metal(cfg, &decision);
        }

        // CPU Candle path: when gliner2-candle-cpu is compiled in and the accelerator
        // resolved to CPU, use the Candle backend instead of ONNX so the feature flag
        // actually changes runtime behaviour.
        #[cfg(feature = "gliner2-candle-cpu")]
        if matches!(
            decision.selected,
            crate::accelerator::SelectedAccelerator::Cpu
        ) {
            return Self::new_candle_cpu(cfg);
        }

        let model_cfg = detector_model_config_for(cfg.accelerator)?;
        let pii_ner = Self::load_pii_ner(cfg)?;

        // ── Local model fast-path ─────────────────────────────────────────────
        let model_root = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| cfg.models_cache());
        let model_path = model_root.join(cfg.ner_onnx_dir());
        if model_path.exists() {
            let ner = anno::backends::gliner2_fastino::GLiNER2Fastino::from_local_with_config(
                &model_path,
                model_cfg.clone(),
            )
            .map_err(|e| Error::Detect(format!("gliner2_fastino load (local): {e}")))?;
            return Ok(Self {
                ner: NerBackend::Onnx(ner),
                pii_ner,
                #[cfg(feature = "heuristic-fr")]
                heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
                gdpr_layers: cfg.gdpr_layers,
                ner_model_id: cfg.ner_model_id.clone(),
                pii_ner_model_id: cfg.ner_pii_model_id.clone(),
            });
        }
        // ─────────────────────────────────────────────────────────────────────
        let ner = GLiNER2Fastino::from_pretrained_with_config(&cfg.ner_model_id, model_cfg)
            .map_err(|e| Error::Detect(format!("gliner2_fastino load: {e}")))?;
        Ok(Self {
            ner: NerBackend::Onnx(ner),
            pii_ner,
            #[cfg(feature = "heuristic-fr")]
            heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
            gdpr_layers: cfg.gdpr_layers,
            ner_model_id: cfg.ner_model_id.clone(),
            pii_ner_model_id: cfg.ner_pii_model_id.clone(),
        })
    }

    #[cfg(feature = "gpu-metal")]
    fn new_candle_metal(
        _cfg: &crate::config::AnnoRagConfig,
        decision: &crate::accelerator::AcceleratorDecision,
    ) -> Result<Self> {
        let device = crate::accelerator::candle_device(decision)?;
        let pii_ner = Self::load_pii_ner(_cfg)?;
        let model_root = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| _cfg.models_cache());
        let model_path = model_root.join(_cfg.ner_candle_dir());
        if model_path.exists() {
            let ner =
                anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_local_with_device(
                    &model_path,
                    &device,
                )
                .map_err(|e| {
                    Error::Detect(format!("gliner2_fastino_candle load (local): {e}"))
                })?;
            return Ok(Self {
                ner: NerBackend::Candle(ner),
                pii_ner,
                #[cfg(feature = "heuristic-fr")]
                heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
                gdpr_layers: _cfg.gdpr_layers,
                ner_model_id: _cfg.ner_candle_model_id.clone(),
                pii_ner_model_id: _cfg.ner_pii_model_id.clone(),
            });
        }
        let ner =
            anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_pretrained_with_device(
                &_cfg.ner_candle_model_id,
                &device,
            )
            .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load: {e}")))?;
        Ok(Self {
            ner: NerBackend::Candle(ner),
            pii_ner,
            #[cfg(feature = "heuristic-fr")]
            heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
            gdpr_layers: _cfg.gdpr_layers,
            ner_model_id: _cfg.ner_candle_model_id.clone(),
            pii_ner_model_id: _cfg.ner_pii_model_id.clone(),
        })
    }

    #[cfg(not(feature = "gpu-metal"))]
    fn new_candle_metal(
        _cfg: &crate::config::AnnoRagConfig,
        _decision: &crate::accelerator::AcceleratorDecision,
    ) -> Result<Self> {
        Err(Error::Config(
            "ANNO_ACCELERATOR=metal requires a binary built with feature gpu-metal".into(),
        ))
    }

    #[cfg(feature = "gliner2-candle-cpu")]
    fn new_candle_cpu(cfg: &crate::config::AnnoRagConfig) -> Result<Self> {
        let device = candle_core::Device::Cpu;
        let pii_ner = Self::load_pii_ner(cfg)?;
        let model_root = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| cfg.models_cache());
        let model_path = model_root.join(cfg.ner_candle_dir());
        if model_path.exists() {
            let ner = anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_local_with_device(
                &model_path,
                &device,
            )
            .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load (local cpu): {e}")))?;
            return Ok(Self {
                ner: NerBackend::Candle(ner),
                pii_ner,
                #[cfg(feature = "heuristic-fr")]
                heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
                gdpr_layers: cfg.gdpr_layers,
                ner_model_id: cfg.ner_candle_model_id.clone(),
                pii_ner_model_id: cfg.ner_pii_model_id.clone(),
            });
        }
        let ner = anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_pretrained_with_device(
            &cfg.ner_candle_model_id,
            &device,
        )
        .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load (cpu): {e}")))?;
        Ok(Self {
            ner: NerBackend::Candle(ner),
            pii_ner,
            #[cfg(feature = "heuristic-fr")]
            heuristic_fr: anno::backends::heuristic_fr::HeuristicFrNer::new(),
            gdpr_layers: cfg.gdpr_layers,
            ner_model_id: cfg.ner_candle_model_id.clone(),
            pii_ner_model_id: cfg.ner_pii_model_id.clone(),
        })
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
        emit_detect_audit(input_chars, elapsed_us, &out, &self.pii_ner_model_id);
        Ok(out)
    }

    /// Run the detector with a caller-controlled NER label set and threshold.
    ///
    /// The French regex PII layer still runs, and output uses the same
    /// char-to-byte translation, sorting, deduplication, and cleartext-free
    /// audit event as [`Self::detect`].
    pub fn detect_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<DetectedEntity>> {
        let started = std::time::Instant::now();
        let input_chars = text.chars().count();
        let out = self.detect_inner_with(text, labels, threshold)?;
        let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        emit_detect_audit(input_chars, elapsed_us, &out, &self.ner_model_id);
        Ok(out)
    }

    fn detect_inner(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        // 1. FR regex layer (model-free).
        let mut all = detect_patterns(text);

        // 1b. FR heuristics (defense layer and above).
        #[cfg(feature = "heuristic-fr")]
        if self.gdpr_layers.includes_heuristics() {
            let labels = pii_label_set();
            let label_refs: Vec<&str> = labels.iter().copied().collect();
            if let Ok(heur_entities) = self.heuristic_fr.extract_with_types(text, &label_refs, 0.5)
            {
                all.extend(anno_entities_to_detected(text, heur_entities)?);
            }
        }

        // 2. NER with GDPR label descriptions — use the PII-specialized model.
        //    Floor threshold 0.25 passes everything through; per-label thresholds
        //    from GDPR_NER_LABELS are applied as a post-filter so Art. 9 labels
        //    (0.30) and basic identifiers (0.35) use different bounds.
        let described = gdpr_described();
        let thresholds = gdpr_label_thresholds();
        let mut anno_entities = self
            .pii_ner
            .extract_with_label_descriptions(text, &described, 0.25)
            .map_err(|e| Error::Detect(e.to_string()))?;
        anno_entities.retain(|e| {
            let label = e.entity_type.as_label();
            f64::from(e.confidence) >= f64::from(thresholds.get(label).copied().unwrap_or(0.50))
        });
        all.extend(anno_entities_to_detected(text, anno_entities)?);

        // 3. Sort + dedup overlaps.
        all.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        dedup_overlaps(&mut all, text);

        // 4. Validators (defense layer and above).
        let rejection_counts = if self.gdpr_layers.includes_validators() {
            let validators = default_validators();
            let (kept, counts) = apply_validators(all, text, &validators);
            all = kept;
            counts
        } else {
            RejectionCounts::new()
        };
        let _ = rejection_counts; // emitted in audit via detect() wrapper in a future task

        Ok(all)
    }

    fn detect_inner_with(
        &self,
        text: &str,
        labels: &[&str],
        threshold: f32,
    ) -> Result<Vec<DetectedEntity>> {
        // 1. FR regex set (model-free layer).
        let mut all = detect_patterns(text);

        // 2. anno NER. anno reports *character* offsets; cloakpipe (and Rust
        // string slicing) want *byte* offsets. `anno_entities_to_detected`
        // builds the once-per-doc char→byte lookup and translates every anno
        // entity through it. Without this, any non-ASCII text (€, accents)
        // triggers a "not a char boundary" panic inside Replacer::pseudonymize.
        let anno_entities = self
            .ner
            .extract_with_types(text, labels, threshold)
            .map_err(|e| Error::Detect(e.to_string()))?;
        all.extend(anno_entities_to_detected(text, anno_entities)?);

        // 3. Sort + dedup overlaps.
        // Sort by (start asc, span-length desc, Pattern-before-Ner).
        all.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        dedup_overlaps(&mut all, text);
        Ok(all)
    }

    /// Run one model pass for ingest and split results into PII and legal layers.
    ///
    /// PII and legal outputs are deduplicated independently so a legal role can
    /// overlap the span that the vault must pseudonymize.
    pub fn detect_for_ingest(
        &self,
        text: &str,
        legal_labels: &[LegalLabel],
        legal_thresholds: &HashMap<&'static str, f32>,
    ) -> Result<IngestDetectionBundle> {
        let started = std::time::Instant::now();
        let input_chars = text.chars().count();

        let mut pii = detect_patterns(text);
        let mut label_thresholds: Vec<(&str, f32)> =
            GDPR_NER_LABELS.iter().map(|(l, _, t)| (*l, *t)).collect();
        for label in legal_labels {
            let threshold = legal_thresholds.get(label.name).copied().unwrap_or(0.5);
            if !label_thresholds.iter().any(|(name, _)| *name == label.name) {
                label_thresholds.push((label.name, threshold));
            }
        }

        let anno_entities = self
            .ner
            .extract_with_label_thresholds(text, &label_thresholds)
            .map_err(|e| Error::Detect(e.to_string()))?;
        let raw_model_spans = anno_entities_to_detected(text, anno_entities)?;

        for entity in &raw_model_spans {
            if is_pii_entity(entity) {
                pii.push(entity.clone());
            }
        }
        pii.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
                .then_with(|| pattern_priority(&a.source).cmp(&pattern_priority(&b.source)))
        });
        dedup_overlaps(&mut pii, text);

        let mut legal: Vec<LegalEntity> = raw_model_spans
            .iter()
            .filter_map(|entity| {
                let EntityCategory::Custom(label) = &entity.category else {
                    return None;
                };
                if !legal_labels.iter().any(|candidate| candidate.name == label) {
                    return None;
                }
                Some(LegalEntity {
                    label: label.clone(),
                    text: entity.original.clone(),
                    byte_start: entity.start as u32,
                    byte_end: entity.end as u32,
                    confidence: entity.confidence as f32,
                })
            })
            .collect();
        dedup_legal_overlaps(&mut legal);

        let out = IngestDetectionBundle {
            pii,
            legal,
            raw_model_spans,
        };
        let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        emit_detect_audit(input_chars, elapsed_us, &out.pii, &self.ner_model_id);
        Ok(out)
    }
}

/// Emit the AI Act Art. 12 / Art. 72 detector audit event. Cleartext-free:
/// only counts, durations, and model ids. Deployers pipe the
/// `anno_rag::detect::audit` target to their SIEM / Art. 30 register.
fn emit_detect_audit(
    input_chars: usize,
    elapsed_us: u64,
    out: &[DetectedEntity],
    ner_model_id: &str,
) {
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
        ner_model_id = ner_model_id,
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

fn default_validators() -> Vec<Box<dyn EntityValidator>> {
    vec![
        Box::new(LuhnValidator::new("SIRET")),
        Box::new(LuhnValidator::new("card_number")),
        Box::new(LuhnValidator::new("bank_account")),
        Box::new(Iban97Validator::new("IBAN_FR")),
        Box::new(Iban97Validator::new("iban")),
        Box::new(NirControlKeyValidator),
        Box::new(DateRangeValidator),
        Box::new(IpAddressValidator),
        Box::new(EmailRfcValidator),
        Box::new(PostalCodeValidator),
    ]
}

fn pattern_priority(s: &DetectionSource) -> u8 {
    match s {
        DetectionSource::Pattern => 0,
        DetectionSource::Financial => 1,
        DetectionSource::Custom => 2,
        DetectionSource::Ner => 3,
    }
}

fn dedup_overlaps(entities: &mut Vec<DetectedEntity>, text: &str) {
    debug_assert!(
        entities.windows(2).all(|w| w[0].start <= w[1].start),
        "dedup_overlaps requires entities sorted by start"
    );
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        if let Some(last) = out.last_mut() {
            if entity.start < last.end {
                // Fusion: extend coverage to the max of both spans.
                // For PII masking, over-masking is safer than under-masking.
                last.end = last.end.max(entity.end);
                // Re-derive original from source text so it matches [start..end].
                last.original = text[last.start..last.end].to_string();
                continue;
            }
        }
        out.push(entity);
    }
    *entities = out;
}

fn map_anno_category(t: &EntityType) -> EntityCategory {
    match t {
        EntityType::Person => EntityCategory::Person,
        EntityType::Organization => EntityCategory::Organization,
        EntityType::Location => EntityCategory::Location,
        _ => EntityCategory::Custom(t.as_label().to_ascii_lowercase()),
    }
}

/// Returns true when a detected entity should be treated as PII for vault
/// pseudonymization. Pattern-detected entities (IBAN, phone, email, …) are
/// always PII. NER entities are PII when their category is a known GDPR type
/// (Art. 4 named entities, Art. 9 special categories, Art. 10 criminal data).
#[must_use]
pub fn is_pii_entity(e: &cloakpipe_core::DetectedEntity) -> bool {
    matches!(e.source, cloakpipe_core::DetectionSource::Pattern)
        || matches!(
            e.category,
            cloakpipe_core::EntityCategory::Person
                | cloakpipe_core::EntityCategory::Organization
                | cloakpipe_core::EntityCategory::Location
        )
        || matches!(&e.category, cloakpipe_core::EntityCategory::Custom(name) if is_pii_label(name))
}

/// Layer-aware detection result for document ingest.
#[derive(Debug, Clone, Default)]
pub struct IngestDetectionBundle {
    /// Spans used for vault pseudonymization.
    pub pii: Vec<DetectedEntity>,
    /// Legal facts and roles. These may overlap PII spans.
    pub legal: Vec<LegalEntity>,
    /// Raw model spans after char-to-byte translation and before layer filtering.
    pub raw_model_spans: Vec<DetectedEntity>,
}

fn dedup_legal_overlaps(entities: &mut Vec<LegalEntity>) {
    entities.sort_by(|a, b| {
        a.byte_start
            .cmp(&b.byte_start)
            .then_with(|| b.confidence.total_cmp(&a.confidence))
            .then_with(|| (b.byte_end - b.byte_start).cmp(&(a.byte_end - a.byte_start)))
    });

    let mut out: Vec<LegalEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        let same_label_overlap = out.iter().any(|selected| {
            selected.label == entity.label
                && entity.byte_start < selected.byte_end
                && entity.byte_end > selected.byte_start
        });
        if !same_label_overlap {
            out.push(entity);
        }
    }

    out.sort_by(|a, b| {
        a.byte_start
            .cmp(&b.byte_start)
            .then_with(|| a.byte_end.cmp(&b.byte_end))
            .then_with(|| a.label.cmp(&b.label))
    });
    *entities = out;
}

fn anno_entities_to_detected(
    text: &str,
    anno_entities: Vec<anno::Entity>,
) -> Result<Vec<DetectedEntity>> {
    let mut char_to_byte: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    char_to_byte.push(text.len());

    let mut out = Vec::with_capacity(anno_entities.len());
    for e in anno_entities {
        let s_char = e.start();
        let n_char = e.end();
        if s_char >= char_to_byte.len() || n_char > char_to_byte.len() || s_char >= n_char {
            continue;
        }
        out.push(DetectedEntity {
            original: e.text.clone(),
            start: char_to_byte[s_char],
            end: char_to_byte[n_char],
            category: map_anno_category(&e.entity_type),
            confidence: f64::from(e.confidence),
            source: DetectionSource::Ner,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod combined_label_tests {
    use super::*;

    #[test]
    fn legal_overlap_dedup_preserves_different_labels_on_same_span() {
        let mut legal = vec![
            crate::legal::LegalEntity {
                label: "organization".to_string(),
                text: "Société ABC".to_string(),
                byte_start: 10,
                byte_end: 21,
                confidence: 0.91,
            },
            crate::legal::LegalEntity {
                label: "contract_party".to_string(),
                text: "Société ABC".to_string(),
                byte_start: 10,
                byte_end: 21,
                confidence: 0.84,
            },
        ];

        dedup_legal_overlaps(&mut legal);

        assert_eq!(legal.len(), 2);
        assert!(legal.iter().any(|e| e.label == "organization"));
        assert!(legal.iter().any(|e| e.label == "contract_party"));
    }

    #[test]
    fn legal_overlap_dedup_drops_lower_confidence_same_label_overlap() {
        let mut legal = vec![
            crate::legal::LegalEntity {
                label: "contract_party".to_string(),
                text: "Société ABC".to_string(),
                byte_start: 10,
                byte_end: 21,
                confidence: 0.70,
            },
            crate::legal::LegalEntity {
                label: "contract_party".to_string(),
                text: "Société ABC SAS".to_string(),
                byte_start: 10,
                byte_end: 25,
                confidence: 0.88,
            },
        ];

        dedup_legal_overlaps(&mut legal);

        assert_eq!(legal.len(), 1);
        assert_eq!(legal[0].text, "Société ABC SAS");
        assert_eq!(legal[0].confidence, 0.88);
    }

    #[test]
    fn pii_and_legal_outputs_can_overlap() {
        let pii = vec![DetectedEntity {
            original: "Société ABC".to_string(),
            start: 10,
            end: 21,
            category: EntityCategory::Organization,
            confidence: 0.91,
            source: DetectionSource::Ner,
        }];
        let legal = vec![crate::legal::LegalEntity {
            label: "contract_party".to_string(),
            text: "Société ABC".to_string(),
            byte_start: 10,
            byte_end: 21,
            confidence: 0.84,
        }];

        let bundle = IngestDetectionBundle {
            pii,
            legal,
            raw_model_spans: Vec::new(),
        };

        assert_eq!(bundle.pii.len(), 1);
        assert_eq!(bundle.legal.len(), 1);
        assert_eq!(bundle.pii[0].start as u32, bundle.legal[0].byte_start);
        assert_eq!(bundle.pii[0].end as u32, bundle.legal[0].byte_end);
    }

    #[test]
    fn combined_label_set_is_union_of_pii_and_legal() {
        let pii = pii_label_set();
        let legal: Vec<&str> = crate::legal::default_legal_labels()
            .iter()
            .map(|label| label.name)
            .collect();
        let combined = combined_label_set(&legal);

        for pii_label in &pii {
            assert!(
                combined.contains(pii_label),
                "missing PII label {pii_label}"
            );
        }
        for legal_label in &legal {
            assert!(
                combined.contains(legal_label),
                "missing legal label {legal_label}"
            );
        }
    }

    #[test]
    fn is_pii_label_distinguishes_pii_from_legal() {
        assert!(is_pii_label("person"));
        assert!(!is_pii_label("clause_type"));
        assert!(!is_pii_label("obligation"));
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
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector builds");
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
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector builds");
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
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector builds");
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
        let d = Detector::new(&crate::config::AnnoRagConfig::default()).expect("detector builds");
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
    fn anno_models_dir_missing_ner_dir_does_not_satisfy_local_fast_path() {
        // ANNO_MODELS_DIR is set but the NER dir does NOT exist.
        // Verify the local readiness condition without calling Detector::new,
        // because the fallback path may touch Hugging Face cache/network.
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig::default();
        // deliberately do NOT create dir.path()/<ner_onnx_dir>

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::set(dir.path());
        let model_root = std::env::var_os("ANNO_MODELS_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| cfg.models_cache());
        assert!(!model_root.join(cfg.ner_onnx_dir()).exists());
    }

    #[test]
    fn anno_models_dir_local_path_entered_when_ner_dir_exists() {
        // When ANNO_MODELS_DIR/<ner_onnx_dir>/ exists (even empty),
        // the fast-path IS taken and from_local errors with a typed error.
        // This proves the branch is entered without requiring real model files.
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig::default();
        let ner_dir = dir.path().join(cfg.ner_onnx_dir());
        std::fs::create_dir_all(&ner_dir).expect("mkdir");
        // Also create PII model dir so load_pii_ner takes the local path
        // (avoids a 404 network attempt and produces a "(local)" error).
        std::fs::create_dir_all(dir.path().join(cfg.ner_pii_onnx_dir())).expect("mkdir pii");

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::set(dir.path());
        let result = Detector::new(&crate::config::AnnoRagConfig::default());

        // from_local on an empty dir returns an error (no ONNX files)
        let err = match result {
            Ok(_) => panic!("must fail on empty model dir"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("(local)"),
            "error must come from local path, got: {msg}"
        );
    }

    #[test]
    fn default_models_cache_local_path_entered_when_ner_dir_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = crate::config::AnnoRagConfig {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        let ner_dir = cfg.models_cache().join(cfg.ner_onnx_dir());
        std::fs::create_dir_all(&ner_dir).expect("mkdir");
        // Also create PII model dir so load_pii_ner takes the local path.
        std::fs::create_dir_all(cfg.models_cache().join(cfg.ner_pii_onnx_dir()))
            .expect("mkdir pii");

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::unset();
        let result = Detector::new(&cfg);

        let err = match result {
            Ok(_) => panic!("must fail on empty model dir"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("(local)"),
            "error must come from default local path, got: {msg}"
        );
    }

    #[test]
    fn detector_new_uses_cfg_ner_onnx_dir() {
        // Verifies that ner_onnx_dir() from config drives the directory
        // that Detector::new() would join to model_root.
        let cfg = crate::config::AnnoRagConfig::default();
        assert_eq!(cfg.ner_onnx_dir(), "SemplificaAI/gliner2-multi-v1-onnx");
        assert_eq!(cfg.ner_candle_dir(), "fastino/gliner2-multi-v1-candle");
        let mut custom = cfg.clone();
        custom.ner_model_id = "org/my-ner-onnx".to_string();
        assert_eq!(custom.ner_onnx_dir(), "org/my-ner-onnx");
    }

    #[test]
    fn cpu_detector_uses_cpu_provider() {
        let cfg = detector_onnx_config_for(crate::accelerator::AcceleratorPreference::Cpu)
            .expect("cpu config");
        assert!(cfg.use_cpu_provider);
        assert!(!cfg.prefer_cuda);
        assert!(!cfg.prefer_coreml);
    }

    #[test]
    fn cuda_detector_requires_cuda_feature() {
        if !cfg!(feature = "gpu-cuda") {
            let err = detector_onnx_config_for(crate::accelerator::AcceleratorPreference::Cuda)
                .expect_err("cuda unavailable");
            assert!(err.to_string().contains("gpu-cuda"));
        }
    }

    // Phase A integration: validators + heuristics + layers
    #[test]
    fn layer_basic_disables_validators() {
        let _env = crate::env_guard::ScopedEnvVar::set("ANNO_GDPR_LAYERS", "basic");
        assert!(!crate::layers::GdprLayerSet::from_env().includes_validators());
    }

    #[test]
    fn layer_defense_enables_validators() {
        let _env = crate::env_guard::ScopedEnvVar::set("ANNO_GDPR_LAYERS", "defense");
        assert!(crate::layers::GdprLayerSet::from_env().includes_validators());
        assert!(crate::layers::GdprLayerSet::from_env().includes_heuristics());
    }

    #[test]
    fn empty_validators_keeps_all() {
        use cloakpipe_core::{DetectedEntity, DetectionSource, EntityCategory};
        let entities = vec![DetectedEntity {
            original: "test".into(),
            start: 0,
            end: 4,
            category: EntityCategory::Custom("x".into()),
            confidence: 0.9,
            source: DetectionSource::Ner,
        }];
        let (kept, counts) = crate::validators::apply_validators(entities, "", &[]);
        assert_eq!(kept.len(), 1);
        assert!(counts.is_empty());
    }

    #[test]
    fn dedup_overlaps_no_overlap_unchanged() {
        let mut entities = vec![
            DetectedEntity {
                original: "Jean".to_string(),
                start: 0,
                end: 4,
                category: EntityCategory::Person,
                confidence: 0.9,
                source: DetectionSource::Ner,
            },
            DetectedEntity {
                original: "Paris".to_string(),
                start: 10,
                end: 15,
                category: EntityCategory::Location,
                confidence: 0.85,
                source: DetectionSource::Ner,
            },
        ];
        dedup_overlaps(&mut entities, "");
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].start, 0);
        assert_eq!(entities[0].end, 4);
        assert_eq!(entities[1].start, 10);
        assert_eq!(entities[1].end, 15);
    }

    #[test]
    fn dedup_overlaps_total_containment_absorbs_inner() {
        let mut entities = vec![
            DetectedEntity {
                original: "Jean Dupont".to_string(),
                start: 0,
                end: 11,
                category: EntityCategory::Person,
                confidence: 0.9,
                source: DetectionSource::Ner,
            },
            DetectedEntity {
                original: "Dupont".to_string(),
                start: 5,
                end: 11,
                category: EntityCategory::Person,
                confidence: 0.8,
                source: DetectionSource::Ner,
            },
        ];
        let text = "Jean Dupont"; // 11 bytes, matches [0..11]
        dedup_overlaps(&mut entities, text);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].start, 0);
        assert_eq!(entities[0].end, 11);
        assert_eq!(entities[0].original, "Jean Dupont");
    }

    #[test]
    fn dedup_overlaps_partial_overlap_fuses_spans() {
        let mut entities = vec![
            DetectedEntity {
                original: "Jean Dupont".to_string(),
                start: 0,
                end: 11,
                category: EntityCategory::Person,
                confidence: 0.9,
                source: DetectionSource::Ner,
            },
            DetectedEntity {
                original: "Dupont SA".to_string(),
                start: 5,
                end: 14,
                category: EntityCategory::Organization,
                confidence: 0.85,
                source: DetectionSource::Ner,
            },
        ];
        let text = "Jean Dupont SA"; // 14 bytes, matches [0..14]
        dedup_overlaps(&mut entities, text);
        assert_eq!(entities.len(), 1, "partial overlap must fuse into one span");
        assert_eq!(entities[0].start, 0);
        assert_eq!(entities[0].end, 14, "end must extend to cover both spans");
        assert_eq!(entities[0].original, "Jean Dupont SA");
    }

    #[test]
    fn dedup_overlaps_adjacent_spans_preserved() {
        let mut entities = vec![
            DetectedEntity {
                original: "Jean".to_string(),
                start: 0,
                end: 4,
                category: EntityCategory::Person,
                confidence: 0.9,
                source: DetectionSource::Ner,
            },
            DetectedEntity {
                original: " Dupont".to_string(),
                start: 4,
                end: 11,
                category: EntityCategory::Person,
                confidence: 0.8,
                source: DetectionSource::Ner,
            },
        ];
        let text = "Jean Dupont"; // 11 bytes, matches [0..11]
        dedup_overlaps(&mut entities, text);
        assert_eq!(
            entities.len(),
            2,
            "adjacent (non-overlapping) spans must stay separate"
        );
    }
}
