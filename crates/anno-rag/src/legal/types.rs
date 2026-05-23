//! Legal types, label catalog, thresholds, and search-filter struct.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A legal extraction label with its natural-language model prompt label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LegalLabel {
    /// Stable snake_case label name used in code and storage.
    pub name: &'static str,
    /// Short human-readable description for extraction prompts and docs.
    pub description: &'static str,
}

/// A legal entity extracted from source or pseudonymized text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalEntity {
    /// Stable label name from [`default_legal_labels`].
    pub label: String,
    /// Extracted text span.
    pub text: String,
    /// Byte start offset in the relevant text coordinate space.
    pub byte_start: u32,
    /// Byte end offset in the relevant text coordinate space.
    pub byte_end: u32,
    /// Model or rule confidence in the inclusive range 0.0..=1.0.
    pub confidence: f32,
}

/// Denormalized legal metadata for one indexed chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalChunkEnrichment {
    /// Chunk UUID matching the primary chunks table.
    pub chunk_id: Uuid,
    /// Parent document UUID.
    pub doc_id: Uuid,
    /// Optional document type, for example `contract` or `litigation`.
    pub doc_type: Option<String>,
    /// Optional French legal domain, for example `commercial` or `employment`.
    pub legal_domain: Option<String>,
    /// Optional normalized jurisdiction or court key.
    pub jurisdiction: Option<String>,
    /// Optional source document date.
    pub document_date: Option<DateTime<Utc>>,
    /// Optional dossier grouping key.
    pub dossier_id: Option<String>,
    /// Normalized party references mentioned in this chunk.
    pub parties: Vec<String>,
    /// Parallel role labels for [`Self::parties`].
    pub party_roles: Vec<String>,
    /// Normalized legal references, for example `code_civil:1240`.
    pub legal_refs: Vec<String>,
    /// Clause types detected in the chunk.
    pub clause_types: Vec<String>,
    /// Obligation kinds detected in the chunk.
    pub obligation_kinds: Vec<String>,
    /// Monetary values normalized to EUR cents when possible.
    pub amounts_eur_cents: Vec<i64>,
    /// Deadlines detected in the chunk.
    pub deadlines: Vec<DateTime<Utc>>,
    /// Legal or procedural event kinds detected in the chunk.
    pub event_kinds: Vec<String>,
    /// Risk flags detected in the chunk.
    pub risk_flags: Vec<String>,
    /// Mandatory-clause status for the chunk or document context.
    pub mandatory_clause_status: Option<String>,
    /// Minimum confidence across extracted fields.
    pub confidence_min: f32,
    /// Average confidence across extracted fields.
    pub confidence_avg: f32,
    /// Extractor implementation version.
    pub extractor_version: String,
    /// Underlying model identifier.
    pub model_id: String,
}

/// Filters accepted by legal search and graph-backed lookup flows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LegalSearchFilters {
    /// Restrict to one document type.
    pub doc_type: Option<String>,
    /// Restrict to one legal domain.
    pub legal_domain: Option<String>,
    /// Restrict to one normalized jurisdiction.
    pub jurisdiction: Option<String>,
    /// Restrict to one dossier id.
    pub dossier_id: Option<String>,
    /// Restrict to normalized party refs.
    pub parties: Vec<String>,
    /// Restrict to party roles.
    pub party_roles: Vec<String>,
    /// Restrict to normalized legal references.
    pub legal_refs: Vec<String>,
    /// Restrict to clause types.
    pub clause_types: Vec<String>,
    /// Restrict to obligation kinds.
    pub obligation_kinds: Vec<String>,
    /// Restrict to event kinds.
    pub event_kinds: Vec<String>,
    /// Restrict to risk flags.
    pub risk_flags: Vec<String>,
    /// Inclusive lower bound for document date.
    pub date_from: Option<DateTime<Utc>>,
    /// Inclusive upper bound for document date.
    pub date_to: Option<DateTime<Utc>>,
    /// Restrict to minimum average confidence.
    pub min_confidence: Option<f32>,
    /// Restrict to mandatory-clause status.
    pub mandatory_clause_status: Option<String>,
}

impl LegalSearchFilters {
    /// Returns true when any filter would constrain the chunk search.
    #[must_use]
    pub fn has_any_filter(&self) -> bool {
        self.doc_type.is_some()
            || self.legal_domain.is_some()
            || self.jurisdiction.is_some()
            || self.dossier_id.is_some()
            || !self.parties.is_empty()
            || !self.party_roles.is_empty()
            || !self.legal_refs.is_empty()
            || !self.clause_types.is_empty()
            || !self.obligation_kinds.is_empty()
            || !self.event_kinds.is_empty()
            || !self.risk_flags.is_empty()
            || self.date_from.is_some()
            || self.date_to.is_some()
            || self.min_confidence.is_some()
            || self.mandatory_clause_status.is_some()
    }
}

/// A legal search result with provenance and enrichment metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalSearchHit {
    /// Matching chunk UUID.
    pub chunk_id: Uuid,
    /// Parent document UUID.
    pub doc_id: Uuid,
    /// Pseudonymized text returned by default.
    pub text_pseudo: String,
    /// Retrieval score from vector, full-text, or hybrid search.
    pub score: f32,
    /// Optional legal enrichment row for the same chunk.
    pub enrichment: Option<LegalChunkEnrichment>,
}

/// Extracted legal fact with provenance, threshold state, and validation flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedFact<T> {
    /// Extracted normalized value.
    pub value: T,
    /// Model or rule confidence.
    pub confidence: f32,
    /// Source chunk UUID.
    pub source_chunk_id: Uuid,
    /// Byte start offset in the source coordinate space.
    pub byte_start: u32,
    /// Byte end offset in the source coordinate space.
    pub byte_end: u32,
    /// Extractor implementation version.
    pub extractor_version: String,
    /// Underlying model or rule identifier.
    pub model_id: String,
    /// Confidence threshold used for this label, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// True when confidence is within the human-validation band.
    pub needs_validation: bool,
}

impl<T> ExtractedFact<T> {
    /// Builds an extracted fact and computes whether it needs human validation.
    pub fn new(
        value: T,
        confidence: f32,
        source_chunk_id: Uuid,
        byte_start: u32,
        byte_end: u32,
        extractor_version: String,
        model_id: String,
        threshold: f32,
        validation_band_width: f32,
    ) -> Self {
        let validation_ceiling = threshold + validation_band_width;
        let needs_validation = confidence >= threshold && confidence < validation_ceiling;

        Self {
            value,
            confidence,
            source_chunk_id,
            byte_start,
            byte_end,
            extractor_version,
            model_id,
            threshold: Some(threshold),
            needs_validation,
        }
    }

    /// Returns true when the fact confidence is below its label threshold.
    #[must_use]
    pub fn below_threshold(&self) -> bool {
        self.threshold
            .map(|threshold| self.confidence < threshold)
            .unwrap_or(false)
    }
}

/// Normalized party kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartyKind {
    /// A natural person.
    Person,
    /// A legal organization.
    Organization,
}

/// Normalized French court level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CourtLevel {
    /// First-instance judicial or commercial court.
    Tribunal,
    /// Court of appeal.
    CourAppel,
    /// Cour de cassation.
    CourCassation,
    /// Conseil d'Etat.
    ConseilEtat,
    /// Administrative court or appeal court.
    Administratif,
}

/// Normalized French court reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CourtRef {
    /// Stable normalized court id.
    pub id: String,
    /// Display name as extracted or normalized.
    pub name: String,
    /// Court level.
    pub level: CourtLevel,
    /// Optional jurisdiction key.
    pub jurisdiction: Option<String>,
}

/// Normalized French code/article reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArticleRef {
    /// Stable code id, for example `code_civil`.
    pub code: String,
    /// Article number as written in normalized form.
    pub article_num: String,
}

impl ArticleRef {
    /// Combined normalized reference, for example `code_civil:1240`.
    #[must_use]
    pub fn normalized_ref(&self) -> String {
        format!("{}:{}", self.code, self.article_num)
    }
}

/// Start point used by prescription calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrescStart {
    /// Starts when damage becomes known.
    KnowledgeOfDamage,
    /// Starts on the event date.
    EventDate,
    /// Starts on the due date.
    DueDate,
}

/// Returns the default legal extraction label catalog.
pub fn default_legal_labels() -> Vec<LegalLabel> {
    vec![
        LegalLabel {
            name: "person",
            description: "Natural person",
        },
        LegalLabel {
            name: "organization",
            description: "Organization or company",
        },
        LegalLabel {
            name: "contract_party",
            description: "Contract party",
        },
        LegalLabel {
            name: "court",
            description: "Court or tribunal",
        },
        LegalLabel {
            name: "jurisdiction",
            description: "Legal jurisdiction",
        },
        LegalLabel {
            name: "legal_reference",
            description: "Legal reference",
        },
        LegalLabel {
            name: "article",
            description: "Code article",
        },
        LegalLabel {
            name: "code",
            description: "French legal code",
        },
        LegalLabel {
            name: "case_number",
            description: "Case number",
        },
        LegalLabel {
            name: "effective_date",
            description: "Effective date",
        },
        LegalLabel {
            name: "deadline",
            description: "Deadline or time limit",
        },
        LegalLabel {
            name: "amount",
            description: "Monetary amount",
        },
        LegalLabel {
            name: "clause_type",
            description: "Contract clause type",
        },
        LegalLabel {
            name: "obligation",
            description: "Legal or contractual obligation",
        },
        LegalLabel {
            name: "sanction",
            description: "Sanction or penalty",
        },
        LegalLabel {
            name: "risk_indicator",
            description: "Legal risk indicator",
        },
        LegalLabel {
            name: "company_identifier",
            description: "SIREN, SIRET, or similar id",
        },
        LegalLabel {
            name: "lawyer",
            description: "Lawyer or counsel",
        },
        LegalLabel {
            name: "judge",
            description: "Judge or magistrate",
        },
        LegalLabel {
            name: "regulator",
            description: "Regulator or administrative authority",
        },
    ]
}

/// Returns default confidence thresholds keyed by legal label name.
pub fn default_thresholds() -> HashMap<&'static str, f32> {
    [
        ("person", 0.70),
        ("organization", 0.70),
        ("contract_party", 0.65),
        ("court", 0.80),
        ("jurisdiction", 0.75),
        ("legal_reference", 0.85),
        ("article", 0.85),
        ("code", 0.85),
        ("case_number", 0.85),
        ("effective_date", 0.75),
        ("deadline", 0.70),
        ("amount", 0.80),
        ("clause_type", 0.60),
        ("obligation", 0.55),
        ("sanction", 0.65),
        ("risk_indicator", 0.55),
        ("company_identifier", 0.90),
        ("lawyer", 0.70),
        ("judge", 0.75),
        ("regulator", 0.75),
    ]
    .into_iter()
    .collect()
}

/// Width of the borderline-confidence band that flags facts for human validation.
pub const VALIDATION_BAND: f32 = 0.10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_legal_labels_include_french_legal_core() {
        let labels = default_legal_labels();
        for expected in [
            "person",
            "organization",
            "contract_party",
            "court",
            "jurisdiction",
            "legal_reference",
            "article",
            "code",
            "case_number",
            "effective_date",
            "deadline",
            "amount",
            "clause_type",
            "obligation",
            "sanction",
            "risk_indicator",
            "company_identifier",
            "lawyer",
            "judge",
        ] {
            assert!(
                labels.iter().any(|l| l.name == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn thresholds_present_for_every_label() {
        let labels = default_legal_labels();
        let thresholds = default_thresholds();
        for l in &labels {
            assert!(
                thresholds.contains_key(l.name),
                "no threshold for {}",
                l.name
            );
            let t = thresholds[l.name];
            assert!(
                (0.0..=1.0).contains(&t),
                "threshold {t} out of range for {}",
                l.name
            );
        }
    }

    #[test]
    fn filters_empty_means_no_chunk_filter() {
        let filters = LegalSearchFilters::default();
        assert!(!filters.has_any_filter());
    }

    #[test]
    fn filters_with_party_mean_filter_required() {
        let filters = LegalSearchFilters {
            parties: vec!["org:acme".to_string()],
            ..LegalSearchFilters::default()
        };
        assert!(filters.has_any_filter());
    }

    #[test]
    fn extracted_fact_needs_validation_when_in_borderline_band() {
        let fact = ExtractedFact::new(
            "obligation".to_string(),
            0.60,
            uuid::Uuid::nil(),
            0,
            8,
            "v1".into(),
            "gliner2".into(),
            0.55,
            0.10,
        );
        assert!(fact.needs_validation);
        assert!(!fact.below_threshold());
    }
}
