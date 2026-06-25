//! Shared response-envelope convention for MCP tool outputs.
//!
//! Every non-trivial tool response carries a top-level machine-stable `status`,
//! a human `message`, an actionable `hint`, plus a status-specific payload.
//! See `docs/superpowers/specs/2026-06-24-mcp-ux-surface-design.md` §0.

use serde_json::{json, Value};

/// Closed set of machine-stable tool statuses.
pub(crate) mod status {
    pub(crate) const OK: &str = "ok";
    pub(crate) const EMPTY: &str = "empty";
    pub(crate) const NOT_ENRICHED: &str = "not_enriched";
    pub(crate) const UNKNOWN_DOCUMENT: &str = "unknown_document";
    pub(crate) const CORPUS_REQUIRED: &str = "corpus_required";
    pub(crate) const SETUP_REQUIRED: &str = "setup_required";
    pub(crate) const NOT_READY: &str = "not_ready";
    pub(crate) const DEGRADED: &str = "degraded";
}

/// Build a status envelope: `{status, message, hint, ...payload}`.
/// `payload` must be a JSON object (or `Value::Null` for none); its keys are
/// merged at the top level so callers can attach `available`, `next_step`, etc.
pub(crate) fn envelope(status: &str, message: &str, hint: &str, payload: Value) -> Value {
    let mut base = json!({ "status": status, "message": message, "hint": hint });
    if let (Some(obj), Some(extra)) = (base.as_object_mut(), payload.as_object()) {
        for (k, v) in extra {
            obj.insert(k.clone(), v.clone());
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_merges_payload_at_top_level() {
        let v = envelope(
            status::NOT_ENRICHED,
            "Document non enrichi.",
            "Réindexez via index(profile=legal).",
            json!({ "doc_id": "abc" }),
        );
        assert_eq!(v["status"], "not_enriched");
        assert_eq!(v["message"], "Document non enrichi.");
        assert_eq!(v["hint"], "Réindexez via index(profile=legal).");
        assert_eq!(v["doc_id"], "abc");
    }

    #[test]
    fn envelope_tolerates_null_payload() {
        let v = envelope(
            status::EMPTY,
            "Aucun résultat.",
            "Élargissez la requête.",
            Value::Null,
        );
        assert_eq!(v["status"], "empty");
        assert!(v.get("doc_id").is_none());
    }
}
