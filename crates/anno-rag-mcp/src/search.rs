//! Search planning logic for the anno-rag MCP server.

use crate::indexer::SyncSummary;
use crate::legal::LegalSearchParams;
use crate::params::SearchUnifiedParams;

pub(crate) fn knowledge_sync_issue(summary: &SyncSummary) -> Option<String> {
    (summary.failed > 0 || summary.truncated).then(|| {
        format!(
            "knowledge sync incomplete: failed={}, truncated={}",
            summary.failed, summary.truncated
        )
    })
}

pub(crate) fn normalize_search_scope(scope: Option<String>, warnings: &mut Vec<String>) -> String {
    match scope.as_deref().unwrap_or("all") {
        "all" => "all".to_string(),
        "knowledge" => "knowledge".to_string(),
        "legal" => "legal".to_string(),
        other => {
            warnings.push(format!(
                "unsupported search scope '{other}'; using scope='all'"
            ));
            "all".to_string()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchBackendMode {
    Fast,
    Semantic,
    Skipped,
}

impl SearchBackendMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Semantic => "semantic",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchExecutionPlan {
    pub(crate) mode_used: &'static str,
    pub(crate) knowledge: SearchBackendMode,
    pub(crate) legal: SearchBackendMode,
    pub(crate) explicit_fast_legal_error: bool,
}

pub(crate) fn search_execution_plan(
    mode: Option<String>,
    scope: &str,
    warnings: &mut Vec<String>,
) -> SearchExecutionPlan {
    match (mode.as_deref(), scope) {
        (None, "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "all") => SearchExecutionPlan {
            mode_used: "auto",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (None, "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (None, other) => {
            warnings.push(format!(
                "unsupported normalized search scope '{other}'; using scope='all'"
            ));
            SearchExecutionPlan {
                mode_used: "auto",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Semantic,
                explicit_fast_legal_error: false,
            }
        }
        (Some("fast"), "legal") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: true,
        },
        (Some("fast"), "all") => {
            warnings.push(
                "legal scope skipped in fast mode (requires models). Use mode='semantic' to include legal results."
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "fast",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("fast"), "knowledge") => SearchExecutionPlan {
            mode_used: "fast",
            knowledge: SearchBackendMode::Fast,
            legal: SearchBackendMode::Skipped,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "knowledge") => {
            warnings.push(
                "knowledge index uses FTS only; running fast mode as fallback for semantic query"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Skipped,
                explicit_fast_legal_error: false,
            }
        }
        (Some("semantic"), "legal") => SearchExecutionPlan {
            mode_used: "semantic",
            knowledge: SearchBackendMode::Skipped,
            legal: SearchBackendMode::Semantic,
            explicit_fast_legal_error: false,
        },
        (Some("semantic"), "all") => {
            warnings.push(
                "knowledge index uses FTS only; running fast mode as fallback for semantic query"
                    .to_string(),
            );
            SearchExecutionPlan {
                mode_used: "semantic",
                knowledge: SearchBackendMode::Fast,
                legal: SearchBackendMode::Semantic,
                explicit_fast_legal_error: false,
            }
        }
        (Some(other), _) => {
            warnings.push(format!(
                "unsupported search mode '{other}'; using implicit mode for scope='{scope}'"
            ));
            search_execution_plan(None, scope, warnings)
        }
    }
}

pub(crate) fn build_legal_search_params(p: &SearchUnifiedParams) -> LegalSearchParams {
    let filters = p.filters.as_ref().and_then(serde_json::Value::as_object);

    LegalSearchParams {
        query: p.query.clone(),
        top_k: p.top_k,
        doc_type: filter_string(filters, "doc_type"),
        legal_domain: filter_string(filters, "legal_domain"),
        jurisdiction: filter_string(filters, "jurisdiction"),
        dossier_id: filter_string(filters, "dossier_id"),
        parties: filter_string_vec(filters, "parties"),
        party_roles: filter_string_vec(filters, "party_roles"),
        legal_refs: filter_string_vec(filters, "legal_refs"),
        clause_types: filter_string_vec(filters, "clause_types"),
        obligation_kinds: filter_string_vec(filters, "obligation_kinds"),
        risk_flags: filter_string_vec(filters, "risk_flags"),
        min_confidence: filter_f32(filters, "min_confidence"),
        corpus_id: p.corpus_id.clone(),
        allow_cross_corpus: p.allow_cross_corpus,
        rerank: true,
    }
}

pub(crate) fn filter_string(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<String> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

pub(crate) fn filter_string_vec(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Vec<String> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn filter_f32(
    filters: Option<&serde_json::Map<String, serde_json::Value>>,
    key: &str,
) -> Option<f32> {
    filters
        .and_then(|values| values.get(key))
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
}

/// Build a `alias/relative_path` document handle, or `None` if either part
/// is missing. Spec C §10 (U1).
pub(crate) fn build_handle(alias: Option<&str>, relative_path: Option<&str>) -> Option<String> {
    match (alias, relative_path) {
        (Some(a), Some(p)) => Some(format!("{a}/{p}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn handle_built_from_alias_and_relative_path() {
        assert_eq!(
            crate::search::build_handle(Some("corpus-01"), Some("contrats/x.txt")),
            Some("corpus-01/contrats/x.txt".to_string())
        );
        assert_eq!(crate::search::build_handle(None, Some("x.txt")), None);
        assert_eq!(crate::search::build_handle(Some("corpus-01"), None), None);
    }
}
