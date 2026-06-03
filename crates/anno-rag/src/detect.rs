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
use crate::legal::{LegalEntity, LegalLabel};
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

/// Candle/PyTorch GLiNER2 repo used for Apple Metal detector acceleration.
pub const CANDLE_NER_MODEL_ID: &str = "fastino/gliner2-multi-v1";
const CANDLE_NER_MODEL_DIR: &str = "gliner2-multi-v1-candle";

/// PII labels recognized by the current [`Detector::detect`] NER layer.
const PII_NER_LABELS: &[&str] = &["person", "organization", "location"];

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
    PII_NER_LABELS.to_vec()
}

/// Returns true when `name` is one of the detector's PII NER labels.
#[must_use]
pub fn is_pii_label(name: &str) -> bool {
    PII_NER_LABELS.contains(&name)
}

/// Return the union of PII NER labels and `extra_labels`, de-duplicated.
#[must_use]
pub fn combined_label_set(extra_labels: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = PII_NER_LABELS.to_vec();
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
    #[cfg(feature = "gpu-metal")]
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
            #[cfg(feature = "gpu-metal")]
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
            #[cfg(feature = "gpu-metal")]
            Self::Candle(model) => model.extract_with_label_thresholds(text, label_thresholds),
        }
    }
}

pub struct Detector {
    ner: NerBackend,
}

impl Detector {
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

        let model_cfg = detector_model_config_for(cfg.accelerator)?;

        // ── ANNO_MODELS_DIR fast-path ─────────────────────────────────────────
        if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
            let model_path = PathBuf::from(models_dir).join("gliner2-multi-v1-onnx");
            if model_path.exists() {
                let ner = anno::backends::gliner2_fastino::GLiNER2Fastino::from_local_with_config(
                    &model_path,
                    model_cfg.clone(),
                )
                .map_err(|e| Error::Detect(format!("gliner2_fastino load (local): {e}")))?;
                return Ok(Self {
                    ner: NerBackend::Onnx(ner),
                });
            }
        }
        // ─────────────────────────────────────────────────────────────────────
        let ner = GLiNER2Fastino::from_pretrained_with_config(NER_MODEL_ID, model_cfg)
            .map_err(|e| Error::Detect(format!("gliner2_fastino load: {e}")))?;
        Ok(Self {
            ner: NerBackend::Onnx(ner),
        })
    }

    #[cfg(feature = "gpu-metal")]
    fn new_candle_metal(
        _cfg: &crate::config::AnnoRagConfig,
        decision: &crate::accelerator::AcceleratorDecision,
    ) -> Result<Self> {
        let device = crate::accelerator::candle_device(decision)?;
        if let Some(models_dir) = std::env::var_os("ANNO_MODELS_DIR") {
            let model_path = PathBuf::from(models_dir).join(CANDLE_NER_MODEL_DIR);
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
                });
            }
        }
        let ner =
            anno::backends::gliner2_fastino_candle::GLiNER2FastinoCandle::from_pretrained_with_device(
                CANDLE_NER_MODEL_ID,
                &device,
            )
            .map_err(|e| Error::Detect(format!("gliner2_fastino_candle load: {e}")))?;
        Ok(Self {
            ner: NerBackend::Candle(ner),
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
        emit_detect_audit(input_chars, elapsed_us, &out);
        Ok(out)
    }

    fn detect_inner(&self, text: &str) -> Result<Vec<DetectedEntity>> {
        self.detect_inner_with(text, PII_NER_LABELS, 0.5)
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
        dedup_overlaps(&mut all);
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
            PII_NER_LABELS.iter().map(|label| (*label, 0.5)).collect();
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
        dedup_overlaps(&mut pii);

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
        emit_detect_audit(input_chars, elapsed_us, &out.pii);
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

fn dedup_overlaps(entities: &mut Vec<DetectedEntity>) {
    let mut out: Vec<DetectedEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        if let Some(last) = out.last() {
            if entity.start < last.end {
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
/// always PII. NER entities are PII only for person/organisation/location.
#[must_use]
pub fn is_pii_entity(e: &cloakpipe_core::DetectedEntity) -> bool {
    matches!(e.source, cloakpipe_core::DetectionSource::Pattern)
        || matches!(
            e.category,
            cloakpipe_core::EntityCategory::Person
                | cloakpipe_core::EntityCategory::Organization
                | cloakpipe_core::EntityCategory::Location
        )
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
    fn anno_models_dir_missing_ner_dir_fast_path_not_taken() {
        // ANNO_MODELS_DIR is set but gliner2-multi-v1-onnx/ does NOT exist.
        // The fast-path must not be taken; Detector::new falls through to
        // from_pretrained. Two outcomes are valid:
        //   - Ok(_): from_pretrained loaded from HF cache (models cached locally) — fast-path not taken ✓
        //   - Err(e): from_pretrained failed (no cache / network) — error must NOT contain "(local)"
        let dir = tempfile::tempdir().expect("tempdir");
        // deliberately do NOT create dir.path()/gliner2-multi-v1-onnx

        let _models_dir = crate::env_guard::ScopedAnnoModelsDir::set(dir.path());
        let result = Detector::new(&crate::config::AnnoRagConfig::default());

        match result {
            Ok(_) => {
                // from_pretrained succeeded (models are HF-cached) — fast-path was not taken ✓
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("(local)"),
                    "fast-path must NOT be taken when gliner2 dir absent, got: {msg}"
                );
            }
        }
    }

    #[test]
    fn anno_models_dir_local_path_entered_when_ner_dir_exists() {
        // When ANNO_MODELS_DIR/gliner2-multi-v1-onnx/ exists (even empty),
        // the fast-path IS taken and from_local errors with a typed error.
        // This proves the branch is entered without requiring real model files.
        let dir = tempfile::tempdir().expect("tempdir");
        let ner_dir = dir.path().join("gliner2-multi-v1-onnx");
        std::fs::create_dir_all(&ner_dir).expect("mkdir");

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
}
