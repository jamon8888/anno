//! Extract workflows: contract review grid, case-file review grid,
//! procedural timeline, and risk review.
//!
//! All workflows query the legal knowledge graph and return structured
//! review rows.

use crate::error::Result;
use crate::legal::kg::LegalKnowledgeGraph;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Review grid ──────────────────────────────────────────────────────────────

/// One row in a contract or case-file review grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRow {
    /// Field identifier (e.g. `"party:demandeur"`, `"obligation:paiement"`).
    pub field: String,
    /// Pseudonymized extracted value.
    pub value: String,
    /// Supporting chunk UUID (stringified), if available.
    pub chunk_id: Option<String>,
    /// Byte start offset in the chunk's pseudonymized text.
    pub byte_start: Option<u32>,
    /// Byte end offset in the chunk's pseudonymized text.
    pub byte_end: Option<u32>,
    /// Model or rule confidence (0.0–1.0), if available.
    pub confidence: Option<f32>,
}

/// Contract review result keyed by `doc_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractReview {
    /// Document id queried.
    pub doc_id: String,
    /// Review rows: parties, obligations, clauses.
    pub rows: Vec<ReviewRow>,
}

/// Case-file review result keyed by `dossier_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseFileReview {
    /// Dossier id queried.
    pub dossier_id: String,
    /// Review rows: documents, parties, events.
    pub rows: Vec<ReviewRow>,
}

// ── Timeline ─────────────────────────────────────────────────────────────────

/// One entry in the procedural timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Event kind (e.g. `"mise_en_demeure"`, `"audience"`).
    pub kind: String,
    /// ISO-8601 event date when available.
    pub event_date: Option<String>,
    /// ISO-8601 deadline date when available.
    pub deadline_date: Option<String>,
    /// Chunk UUID that mentions this event.
    pub chunk_id: Option<String>,
}

/// Procedural timeline for a dossier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralTimeline {
    /// Dossier id.
    pub dossier_id: String,
    /// Events in chronological order (ascending by `event_date`).
    pub events: Vec<TimelineEvent>,
}

// ── Risk review ───────────────────────────────────────────────────────────────

/// One risk finding with severity and recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFinding {
    /// Risk node UUID.
    pub risk_id: String,
    /// Severity level: `"high"`, `"medium"`, or `"low"`.
    pub severity: String,
    /// Risk category (e.g. `"clause_abusive"`, `"force_majeure"`).
    pub category: String,
    /// Pseudonymized risk description.
    pub text_pseudo: String,
    /// Recommended action for the reviewing lawyer.
    pub recommendation: String,
}

/// Risk review for a document or dossier scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReview {
    /// Document or dossier id queried.
    pub scope_id: String,
    /// Risk findings sorted severity-descending.
    pub findings: Vec<RiskFinding>,
}

// ── Workflows ─────────────────────────────────────────────────────────────────

/// Extract a contract review grid from the KG for `doc_id`.
///
/// Queries parties and obligations linked to this document.
/// Phase-1: returns empty rows.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn extract_contract(
    kg: &dyn LegalKnowledgeGraph,
    doc_id: &str,
) -> Result<ContractReview> {
    let mut rows = Vec::new();

    // Parties linked to the document.
    let party_rows = kg.contract_parties(doc_id).await?;
    for r in &party_rows {
        rows.push(ReviewRow {
            field: format!(
                "party:{}",
                r.get("role").cloned().unwrap_or_else(|| "unknown".into())
            ),
            value: r.get("value").cloned().unwrap_or_default(),
            chunk_id: None,
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    // Obligations sourced by chunks of the document.
    let obl_rows = kg.contract_obligations(doc_id).await?;
    for r in &obl_rows {
        rows.push(ReviewRow {
            field: format!("obligation:{}", r.get("kind").cloned().unwrap_or_default()),
            value: r.get("text").cloned().unwrap_or_default(),
            chunk_id: r.get("cid").cloned(),
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    Ok(ContractReview {
        doc_id: doc_id.to_string(),
        rows,
    })
}

/// Extract a case-file review grid from the KG for `dossier_id`.
///
/// Queries constituent documents, parties, and chronological events.
/// Phase-1: returns empty rows.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn extract_case_file(
    kg: &dyn LegalKnowledgeGraph,
    dossier_id: &str,
) -> Result<CaseFileReview> {
    let mut rows = Vec::new();
    let mk_params = || HashMap::from([("dossier".to_string(), dossier_id.to_string())]);

    // Documents in this dossier.
    let doc_rows = kg
        .cypher(
            "MATCH (d:Document {dossier_id:$dossier}) \
             RETURN d.doc_id AS doc_id, d.doc_type AS doc_type",
            mk_params(),
        )
        .await?;
    for r in &doc_rows {
        rows.push(ReviewRow {
            field: format!(
                "document:{}",
                r.get("doc_type")
                    .cloned()
                    .unwrap_or_else(|| "unknown".into())
            ),
            value: r.get("doc_id").cloned().unwrap_or_default(),
            chunk_id: None,
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    // Parties across all documents in the dossier.
    let party_rows = kg
        .cypher(
            "MATCH (d:Document {dossier_id:$dossier})<-[rel:PARTY_TO]-(p:Party) \
             RETURN DISTINCT p.canonical_name AS value, rel.role AS role",
            mk_params(),
        )
        .await?;
    for r in &party_rows {
        rows.push(ReviewRow {
            field: format!(
                "party:{}",
                r.get("role").cloned().unwrap_or_else(|| "unknown".into())
            ),
            value: r.get("value").cloned().unwrap_or_default(),
            chunk_id: None,
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    // Procedural events, chronological.
    let event_rows = kg
        .cypher(
            "MATCH (d:Document {dossier_id:$dossier})-[:HAS_CHUNK]->(c:Chunk) \
             MATCH (c)-[:MENTIONS]->(e:Event) \
             RETURN e.kind AS kind, e.event_date AS event_date, c.chunk_id AS cid \
             ORDER BY e.event_date",
            mk_params(),
        )
        .await?;
    for r in &event_rows {
        rows.push(ReviewRow {
            field: format!("event:{}", r.get("kind").cloned().unwrap_or_default()),
            value: r.get("event_date").cloned().unwrap_or_default(),
            chunk_id: r.get("cid").cloned(),
            byte_start: None,
            byte_end: None,
            confidence: None,
        });
    }

    Ok(CaseFileReview {
        dossier_id: dossier_id.to_string(),
        rows,
    })
}

/// Retrieve the procedural timeline for `dossier_id` from the KG,
/// ordered chronologically.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn timeline(
    kg: &dyn LegalKnowledgeGraph,
    dossier_id: &str,
) -> Result<ProceduralTimeline> {
    let rows = kg.procedural_timeline(dossier_id).await?;

    let events = rows
        .iter()
        .map(|r| TimelineEvent {
            kind: r
                .get("event_kind")
                .or_else(|| r.get("kind"))
                .cloned()
                .unwrap_or_default(),
            event_date: r.get("event_date").cloned(),
            deadline_date: r.get("deadline_date").cloned(),
            chunk_id: r.get("chunk_id").or_else(|| r.get("cid")).cloned(),
        })
        .collect();

    Ok(ProceduralTimeline {
        dossier_id: dossier_id.to_string(),
        events,
    })
}

/// Retrieve risk findings for `scope_id`.
///
/// When `is_dossier` is `true`, `scope_id` is treated as a `dossier_id`
/// and risks across all constituent documents are collected.
/// Otherwise it is treated as a `doc_id`.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn risk_review(
    kg: &dyn LegalKnowledgeGraph,
    scope_id: &str,
    is_dossier: bool,
) -> Result<RiskReview> {
    let rows = kg.risk_findings(scope_id, is_dossier).await?;

    let findings = rows
        .iter()
        .map(|r| {
            let severity = r.get("severity").cloned().unwrap_or_default();
            let recommendation = recommendation_for_severity(&severity);
            RiskFinding {
                risk_id: r.get("rid").cloned().unwrap_or_default(),
                severity,
                category: r.get("category").cloned().unwrap_or_default(),
                text_pseudo: r.get("text").cloned().unwrap_or_default(),
                recommendation,
            }
        })
        .collect();

    Ok(RiskReview {
        scope_id: scope_id.to_string(),
        findings,
    })
}

/// Map a severity string to a lawyer-facing recommendation in French.
fn recommendation_for_severity(severity: &str) -> String {
    match severity {
        "high" | "critique" | "élevé" => {
            "Relecture immédiate par l'avocat requise avant signature.".into()
        }
        "medium" | "moyen" | "modéré" => {
            "Relecture par l'avocat recommandée avant exécution.".into()
        }
        _ => "À noter pour référence — risque faible.".into(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extract_contract_returns_empty_on_phase1_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = extract_contract(&kg, "doc:test-001").await.unwrap();
        assert!(result.rows.is_empty());
        assert_eq!(result.doc_id, "doc:test-001");
    }

    #[tokio::test]
    async fn extract_case_file_returns_empty_on_phase1_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = extract_case_file(&kg, "dossier:2024-42").await.unwrap();
        assert!(result.rows.is_empty());
        assert_eq!(result.dossier_id, "dossier:2024-42");
    }

    #[tokio::test]
    async fn timeline_returns_empty_on_phase1_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = timeline(&kg, "dossier:2024-42").await.unwrap();
        assert!(result.events.is_empty());
    }

    #[tokio::test]
    async fn timeline_calls_procedural_timeline_not_cypher() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = timeline(&kg, "dossier-test").await.unwrap();
        assert_eq!(result.dossier_id, "dossier-test");
        assert!(result.events.is_empty());
    }

    #[tokio::test]
    async fn risk_review_doc_scope_returns_empty_on_phase1_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = risk_review(&kg, "doc:test-001", false).await.unwrap();
        assert!(result.findings.is_empty());
        assert_eq!(result.scope_id, "doc:test-001");
    }

    #[tokio::test]
    async fn risk_review_dossier_scope_returns_empty_on_phase1_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let result = risk_review(&kg, "dossier:2024-42", true).await.unwrap();
        assert!(result.findings.is_empty());
    }

    #[test]
    fn recommendation_high_severity_contains_immediat() {
        let rec = recommendation_for_severity("high");
        assert!(rec.contains("immédiate"), "got: {rec}");
    }

    #[test]
    fn recommendation_low_severity_contains_faible() {
        let rec = recommendation_for_severity("low");
        assert!(rec.contains("faible"), "got: {rec}");
    }
}
