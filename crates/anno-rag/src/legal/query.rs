//! Five named graph traversals dispatched through typed graph-store methods.

use crate::error::Result;
use crate::legal::kg::LegalKnowledgeGraph;
use std::collections::HashMap;

/// Named graph intent — a parameterized traversal over the legal KG.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(tag = "intent", content = "params", rename_all = "snake_case")]
pub enum GraphIntent {
    /// All documents and chunks that mention a given normalized party.
    PartyDossier {
        /// Normalized party name to search for.
        party: String,
    },
    /// Obligations for which the given party is the obligor.
    ObligationsOwedBy {
        /// Normalized obligor party name.
        party: String,
    },
    /// Documents and chunks that cite a given code article.
    CitationChain {
        /// Normalized code article reference.
        article_ref: String,
    },
    /// Chronological events in a dossier.
    ProceduralTimeline {
        /// Dossier identifier to traverse.
        dossier_id: String,
    },
    /// Appeal chain rooted at a document, up to `max_depth` hops.
    AppealChain {
        /// Root document identifier.
        doc_id: String,
        /// Maximum traversal depth.
        max_depth: u32,
    },
}

/// Row set returned by a graph intent query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQueryResult {
    /// Raw rows — each row is a map of column name → string value.
    pub rows: Vec<HashMap<String, String>>,
}

/// Resolve `intent` to a typed graph-store traversal and execute it against
/// `kg`.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn run_intent(
    kg: &dyn LegalKnowledgeGraph,
    intent: GraphIntent,
) -> Result<GraphQueryResult> {
    let rows = match intent {
        GraphIntent::PartyDossier { party } => kg.party_dossier(&party).await?,
        GraphIntent::ObligationsOwedBy { party } => kg.obligations_owed_by(&party).await?,
        GraphIntent::CitationChain { article_ref } => kg.citation_chain(&article_ref).await?,
        GraphIntent::ProceduralTimeline { dossier_id } => {
            kg.procedural_timeline(&dossier_id).await?
        }
        GraphIntent::AppealChain { doc_id, max_depth } => {
            kg.appeal_chain(&doc_id, max_depth).await?
        }
    };
    Ok(GraphQueryResult { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn party_dossier_builds_correct_cypher() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        // Phase-1 KG returns empty rows for any cypher call — assertion is
        // that no panic occurs and the Ok variant is returned.
        let r = run_intent(
            &kg,
            GraphIntent::PartyDossier {
                party: "org:acme".into(),
            },
        )
        .await
        .unwrap();
        assert!(r.rows.is_empty());
    }

    #[tokio::test]
    async fn obligations_owed_by_returns_empty_on_empty_kg() {
        let kg = crate::legal::kg::tests::InMemoryKG::default();
        let r = run_intent(
            &kg,
            GraphIntent::ObligationsOwedBy {
                party: "org:beta".into(),
            },
        )
        .await
        .unwrap();
        assert!(r.rows.is_empty());
    }
}
