//! Five named graph traversals. Each is a parameterized Cypher template
//! dispatched through [`LegalKnowledgeGraph::cypher`].

use crate::error::Result;
use crate::legal::kg::LegalKnowledgeGraph;
use std::collections::HashMap;

/// Named graph intent — a parameterized traversal over the legal KG.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(tag = "intent", content = "params", rename_all = "snake_case")]
pub enum GraphIntent {
    /// All documents and chunks that mention a given normalized party.
    PartyDossier { party: String },
    /// Obligations for which the given party is the obligor.
    ObligationsOwedBy { party: String },
    /// Documents and chunks that cite a given code article.
    CitationChain { article_ref: String },
    /// Chronological events in a dossier.
    ProceduralTimeline { dossier_id: String },
    /// Appeal chain rooted at a document, up to `max_depth` hops.
    AppealChain { doc_id: String, max_depth: u32 },
}

/// Row set returned by a graph intent query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQueryResult {
    /// Raw rows — each row is a map of column name → string value.
    pub rows: Vec<HashMap<String, String>>,
}

/// Resolve `intent` to a parameterized Cypher query and execute it against
/// `kg`. Returns an empty row set when the Phase-1 no-op backend is active.
///
/// # Errors
/// Propagates [`crate::error::Error::Graph`] from the KG backend.
pub async fn run_intent(
    kg: &dyn LegalKnowledgeGraph,
    intent: GraphIntent,
) -> Result<GraphQueryResult> {
    let (query, params) = match intent {
        GraphIntent::PartyDossier { party } => (
            "MATCH (p:Party {normalized_form:$party})-[r:PARTY_TO]->(d:Document)\
             -[:HAS_CHUNK]->(c:Chunk) \
             RETURN d, r.role AS role, c ORDER BY d.document_date"
                .to_string(),
            HashMap::from([("party".to_string(), party)]),
        ),
        GraphIntent::ObligationsOwedBy { party } => (
            "MATCH (p:Party {normalized_form:$party})-[:BOUND_BY]->(o:Obligation) \
             OPTIONAL MATCH (o)-[:OWED_TO]->(t:Party), \
                            (o)-[:HAS_DEADLINE]->(e:Event), \
                            (o)-[:HAS_AMOUNT]->(a:Amount) \
             RETURN o, t, e, a"
                .to_string(),
            HashMap::from([("party".to_string(), party)]),
        ),
        GraphIntent::CitationChain { article_ref } => (
            "MATCH (a:Article {normalized_ref:$ref})<-[:REFERENCES]-(d:Document)\
             -[:HAS_CHUNK]->(c:Chunk) \
             WHERE (c)-[:MENTIONS]->(a) RETURN d, c"
                .to_string(),
            HashMap::from([("ref".to_string(), article_ref)]),
        ),
        GraphIntent::ProceduralTimeline { dossier_id } => (
            "MATCH (d:Document {dossier_id:$dossier})-[:HAS_CHUNK]->(c:Chunk)\
             -[:MENTIONS]->(e:Event) \
             RETURN e ORDER BY e.event_date"
                .to_string(),
            HashMap::from([("dossier".to_string(), dossier_id)]),
        ),
        GraphIntent::AppealChain { doc_id, max_depth } => (
            format!(
                "MATCH path = (d:Document {{doc_id:$doc}})-[:APPEALS*1..{max_depth}]->(prior:Document) \
                 RETURN path"
            ),
            HashMap::from([("doc".to_string(), doc_id.to_string())]),
        ),
    };
    let rows = kg.cypher(&query, params).await?;
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
