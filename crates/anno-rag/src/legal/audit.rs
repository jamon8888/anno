//! Structured audit-event helpers for the French legal RAG pipeline.
//!
//! Each helper emits a structured tracing event to the `anno_rag::legal::audit`
//! target so that log aggregators (Loki, CloudWatch, etc.) can parse and alert
//! on legal-tool activity without parsing free-form log messages.
//!
//! All helpers are synchronous thin wrappers around `tracing::info!` and are
//! designed to be sprinkled at tool entry/exit points with zero allocation on
//! the hot path.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Emit an ingest audit event.
///
/// Call at the start of each `legal_ingest` tool invocation, once per folder.
pub fn audit_ingest(doc_id: Uuid, outcome: &str) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "ingest",
        doc_id = %doc_id,
        outcome = outcome,
    );
}

/// Emit a search audit event.
///
/// Call at the start of each `legal_search` tool invocation.
pub fn audit_search(query_len: usize, filter_count: usize, top_k: usize) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "search",
        query_len = query_len,
        filter_count = filter_count,
        top_k = top_k,
    );
}

/// Emit a citation-rehydration audit event.
///
/// Call at the start of each `legal_rehydrate_citation` tool invocation.
pub fn audit_rehydrate(chunk_id: Uuid, byte_start: u32, byte_end: u32) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "rehydrate",
        chunk_id = %chunk_id,
        byte_start = byte_start,
        byte_end = byte_end,
    );
}

/// Emit a field-validation audit event.
///
/// `actor` is the human reviewer or automated system that triggered the
/// validation. `action` is one of `"confirm"`, `"reject"`, `"correct"`.
pub fn audit_validation(actor: Option<&str>, action: &str) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "validation",
        actor = actor.unwrap_or("unknown"),
        action = action,
    );
}

/// Emit a prescription-check audit event.
///
/// Call once per prescription anchor computed during `legal_prescription_check`.
pub fn audit_prescription(event_id: Uuid, prescribes_on: DateTime<Utc>) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "prescription",
        event_id = %event_id,
        prescribes_on = %prescribes_on.to_rfc3339(),
    );
}

/// Emit a mandatory-clause audit event.
///
/// `status` is the aggregate status returned by
/// [`crate::legal::mandatory::aggregate_status`]: `"complete"`, `"partial"`,
/// or `"missing"`.
pub fn audit_mandatory(doc_id: Uuid, status: &str) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "mandatory_clause",
        doc_id = %doc_id,
        status = status,
    );
}

/// Emit a graph-query audit event.
///
/// `intent` is the `GraphIntent` discriminator string (e.g. `"party_dossier"`).
pub fn audit_graph_query(intent: &str, rows: usize, duration_ms: u64) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "graph_query",
        intent = intent,
        rows = rows,
        duration_ms = duration_ms,
    );
}

/// Emit an extract-contract audit event.
pub fn audit_extract_contract(doc_id: &str, rows: usize, duration_ms: u64) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "extract_contract",
        doc_id = doc_id,
        rows = rows,
        duration_ms = duration_ms,
    );
}

/// Emit a risk-review audit event.
pub fn audit_risk_review(scope_id: &str, findings: usize, duration_ms: u64) {
    tracing::info!(
        target: "anno_rag::legal::audit",
        event = "risk_review",
        scope_id = scope_id,
        findings = findings,
        duration_ms = duration_ms,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke-tests: verify all helpers can be called without panicking.
    /// The tracing subscriber is not installed here so events are discarded.

    #[test]
    fn audit_ingest_does_not_panic() {
        audit_ingest(Uuid::new_v4(), "ok");
    }

    #[test]
    fn audit_search_does_not_panic() {
        audit_search(42, 3, 10);
    }

    #[test]
    fn audit_rehydrate_does_not_panic() {
        audit_rehydrate(Uuid::new_v4(), 0, 100);
    }

    #[test]
    fn audit_validation_with_actor_does_not_panic() {
        audit_validation(Some("lawyer@example.com"), "confirm");
    }

    #[test]
    fn audit_validation_without_actor_does_not_panic() {
        audit_validation(None, "reject");
    }

    #[test]
    fn audit_prescription_does_not_panic() {
        audit_prescription(Uuid::new_v4(), Utc::now());
    }

    #[test]
    fn audit_mandatory_does_not_panic() {
        audit_mandatory(Uuid::new_v4(), "partial");
    }

    #[test]
    fn audit_graph_query_does_not_panic() {
        audit_graph_query("party_dossier", 5, 12);
    }

    #[test]
    fn audit_extract_contract_does_not_panic() {
        audit_extract_contract("doc:123", 8, 45);
    }

    #[test]
    fn audit_risk_review_does_not_panic() {
        audit_risk_review("dossier:456", 3, 22);
    }
}
