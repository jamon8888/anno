//! Legal tool parameter and result types for the anno-rag MCP server.

use crate::params::default_top_k;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// Parameters for the `legal_ingest` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalIngestParams {
    /// Absolute path to the folder containing legal documents to ingest.
    pub folder: String,
    /// When true, recurse into sub-folders. Defaults to false.
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Serialize)]
pub(crate) struct LegalIngestResult {
    pub(crate) ingested: usize,
    pub(crate) folder: String,
    pub(crate) output_root: String,
    pub(crate) output_scope: String,
}

/// Parameters for the `legal_search` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalSearchParams {
    /// Free-text query. PII is pseudonymized before embedding.
    pub query: String,
    /// Maximum number of results.
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Optional doc_type filter (e.g. `"contract"`, `"judgment"`).
    #[serde(default)]
    pub doc_type: Option<String>,
    /// Optional legal_domain filter (e.g. `"droit_commercial"`, `"droit_travail"`).
    #[serde(default)]
    pub legal_domain: Option<String>,
    /// Optional jurisdiction filter (e.g. `"France"`, `"Paris"`).
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Optional dossier_id filter.
    #[serde(default)]
    pub dossier_id: Option<String>,
    /// Filter to chunks that mention any of these normalized party forms (e.g. `["org:acme"]`).
    #[serde(default)]
    pub parties: Vec<String>,
    /// Filter to chunks with any of these party roles.
    #[serde(default)]
    pub party_roles: Vec<String>,
    /// Filter to chunks that cite any of these normalized article refs (e.g. `["code_civil:1240"]`).
    #[serde(default)]
    pub legal_refs: Vec<String>,
    /// Filter to chunks with any of these clause types.
    #[serde(default)]
    pub clause_types: Vec<String>,
    /// Filter to chunks with any of these obligation kinds.
    #[serde(default)]
    pub obligation_kinds: Vec<String>,
    /// Filter to chunks with any of these risk flags.
    #[serde(default)]
    pub risk_flags: Vec<String>,
    /// Minimum extraction confidence (0–1).
    #[serde(default)]
    pub min_confidence: Option<f32>,
    /// Optional corpus id to constrain the legal query.
    #[serde(default)]
    pub corpus_id: Option<String>,
    /// Explicitly allow cross-corpus legal search.
    #[serde(default)]
    pub allow_cross_corpus: bool,
    /// Rerank results with the cross-encoder. Default `true` (accuracy-first).
    /// Set `false` for RRF-only hybrid (faster, no model warm-up).
    #[serde(default = "default_legal_rerank")]
    pub rerank: bool,
}

fn default_legal_rerank() -> bool {
    true
}

#[derive(Serialize)]
pub(crate) struct LegalSearchHitWire {
    pub(crate) chunk_id: String,
    pub(crate) doc_id: String,
    pub(crate) text_pseudo: String,
    pub(crate) score: f32,
}

#[derive(Serialize)]
pub(crate) struct LegalSearchResult {
    pub(crate) hits: Vec<LegalSearchHitWire>,
}

/// Parameters for the `legal_graph_query` tool.
///
/// `intent` discriminator: `"party_dossier"` | `"obligations_owed_by"` |
/// `"citation_chain"` | `"procedural_timeline"` | `"appeal_chain"`.
/// The remaining fields supply the intent's required parameters.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalGraphQueryParams {
    /// Which named traversal to run. One of: party_dossier, obligations_owed_by,
    /// citation_chain, procedural_timeline, appeal_chain.
    pub intent: String,
    /// party_dossier / obligations_owed_by: normalized party identifier.
    pub party: Option<String>,
    /// citation_chain: normalized article reference (e.g. "C.civ.1240").
    pub article_ref: Option<String>,
    /// procedural_timeline: dossier identifier.
    pub dossier_id: Option<String>,
    /// appeal_chain: root document id.
    pub doc_id: Option<String>,
    /// appeal_chain: maximum appeal hops (default 10).
    pub max_depth: Option<u32>,
}

#[derive(Serialize)]
pub(crate) struct LegalGraphQueryResult {
    pub(crate) rows: Vec<std::collections::HashMap<String, String>>,
}

/// Parameters for the `legal_rehydrate_citation` tool.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalRehydrateCitationParams {
    /// Chunk UUID (stringified) to fetch.
    pub chunk_id: String,
    /// UTF-8 byte offset of the citation span start (inclusive).
    pub byte_start: u32,
    /// UTF-8 byte offset of the citation span end (exclusive).
    pub byte_end: u32,
}

#[derive(Serialize)]
pub(crate) struct LegalRehydrateCitationResult {
    pub(crate) text: String,
    pub(crate) tokens_rehydrated: usize,
}

// ---- D2 params/results ----

/// Parameters for `legal_extract_contract`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalExtractContractParams {
    /// Document id to extract a review grid for.
    pub doc_id: String,
}

/// Parameters for `legal_extract_case_file`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalExtractCaseFileParams {
    /// Dossier id to extract a review grid for.
    pub dossier_id: String,
}

/// Parameters for `legal_timeline`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalTimelineParams {
    /// Dossier id to retrieve the procedural timeline for.
    pub dossier_id: String,
}

/// Parameters for `legal_risk_review`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalRiskReviewParams {
    /// Document or dossier id to scope the risk review to.
    pub scope_id: String,
    /// When true, treat `scope_id` as a dossier id; otherwise as a doc id.
    pub is_dossier: bool,
}

// ---- D3 params/results ----

/// Parameters for `legal_mandatory_clause_audit`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalMandatoryClauseAuditParams {
    /// Document UUID (stringified).
    pub doc_id: String,
    /// Document type used to select the checklist
    /// (e.g. `"b2b_contract"`, `"employment"`, `"rgpd"`).
    pub doc_type: String,
}

// ---- D4 params/results ----

/// Parameters for `legal_prescription_check`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalPrescriptionCheckParams {
    /// Prescription category (e.g. `"contractuel"`, `"responsabilite_decennale"`).
    pub category: String,
    /// Anchor event date in ISO-8601 format (e.g. `"2020-01-15T00:00:00Z"`).
    pub event_date: String,
    /// Interrupting events (mise en demeure, assignation, etc.).
    pub interrupting_events: Vec<LegalInterruptingEventWire>,
}

/// Wire representation of an event that interrupts or suspends prescription.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalInterruptingEventWire {
    /// Event kind, e.g. `"mise_en_demeure"`, `"assignation"`.
    pub kind: String,
    /// ISO-8601 date of the interrupting event.
    pub date: String,
}

// ---- D5 params/results ----

/// Parameters for `legal_validate_field`.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct LegalValidateFieldParams {
    /// Chunk UUID (stringified) that contains the extracted fact.
    pub chunk_id: String,
    /// Field name being validated (e.g. `"obligation:paiement"`).
    pub field_name: String,
    /// Action: `"confirm"`, `"reject"`, or `"correct"`.
    pub action: String,
    /// Corrected value when action is `"correct"`.
    pub corrected_value: Option<String>,
    /// Optional free-text note from the reviewer.
    pub note: Option<String>,
    /// Optional reviewer identifier (email or system name).
    pub actor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_search_params_default_rerank_is_true() {
        let p: LegalSearchParams =
            serde_json::from_value(serde_json::json!({ "query": "clause" })).expect("parse");
        assert!(p.rerank, "legal rerank must default ON");
    }
}
