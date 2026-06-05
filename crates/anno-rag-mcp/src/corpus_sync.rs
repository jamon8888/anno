//! Corpus-level sync request and response models.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// Parameters for `sync_corpus`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SyncCorpusParams {
    /// Corpus id to synchronize.
    pub corpus_id: String,
    /// Optional source ids to sync. Defaults to all bound sources.
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    /// Requested output kinds. Defaults to `knowledge_fast`.
    #[serde(default = "default_outputs")]
    pub outputs: Vec<String>,
    /// Optional maximum files budget. Plumbed into sync in a later phase.
    #[serde(default)]
    pub max_files: Option<usize>,
    /// Optional maximum time budget in milliseconds. Plumbed in a later phase.
    #[serde(default)]
    pub max_millis: Option<u64>,
}

fn default_outputs() -> Vec<String> {
    vec!["knowledge_fast".to_string()]
}

/// Structured result for `sync_corpus`.
#[derive(Debug, Clone, Serialize)]
pub struct SyncCorpusResult {
    /// True when the sync request completed.
    pub ok: bool,
    /// Synchronized corpus id.
    pub corpus_id: String,
    /// Corpus freshness after the sync attempt.
    pub freshness: String,
    /// Source binding counts.
    pub sources: SyncSourceSummary,
    /// Knowledge output summary.
    pub knowledge: serde_json::Value,
    /// Legal output summary.
    pub legal: serde_json::Value,
    /// Non-fatal sync warnings.
    pub warnings: Vec<String>,
}

/// Source binding counts for a corpus sync.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncSourceSummary {
    /// Knowledge sources bound to the corpus.
    pub bound_sources: usize,
    /// Sources selected and attempted.
    pub synced_sources: usize,
    /// Bound sources skipped by source filtering.
    pub skipped_sources: usize,
}

/// Parsed requested output kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestedOutputs {
    /// Sync the fast knowledge output.
    pub knowledge_fast: bool,
    /// Sync the legal semantic output.
    pub legal_semantic: bool,
}

/// Parse requested output names.
pub fn parse_requested_outputs(outputs: &[String]) -> Result<RequestedOutputs, String> {
    let mut requested = RequestedOutputs {
        knowledge_fast: false,
        legal_semantic: false,
    };
    for output in outputs {
        match output.as_str() {
            "knowledge_fast" => requested.knowledge_fast = true,
            "legal_semantic" => requested.legal_semantic = true,
            other => {
                return Err(format!(
                    "unsupported output '{other}'. Expected knowledge_fast or legal_semantic"
                ));
            }
        }
    }
    if !requested.knowledge_fast && !requested.legal_semantic {
        requested.knowledge_fast = true;
    }
    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_outputs_defaults_empty_to_knowledge_fast() {
        let outputs = parse_requested_outputs(&[]).expect("parse");
        assert!(outputs.knowledge_fast);
        assert!(!outputs.legal_semantic);
    }

    #[test]
    fn parse_outputs_rejects_unknown_output() {
        let err = parse_requested_outputs(&["deep_magic".to_string()]).expect_err("unknown");
        assert!(err.contains("unsupported output"));
    }
}
