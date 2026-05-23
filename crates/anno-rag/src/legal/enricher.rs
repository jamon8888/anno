//! Legal entity extraction. Trait + GLiNER2 adapter + in-memory fake.

use crate::error::Result;
use crate::legal::offsets::PseudoOffsetMap;
use crate::legal::rules::{apply_all as apply_rules, TypedFact, VaultForwardMap};
use crate::legal::kg::{EdgeWrite, NodeWrite};
use crate::legal::types::LegalChunkEnrichment;
use crate::legal::types::{LegalEntity, LegalLabel, PartyKind};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Abstraction over legal entity extraction so tests do not load model weights.
pub trait LegalEntityExtractor: Send + Sync {
    /// Extract legal entities from `text` using the given labels and thresholds.
    ///
    /// # Errors
    /// Returns an error when the underlying extractor fails.
    fn extract(
        &self,
        text: &str,
        labels: &[LegalLabel],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>>;

    /// Underlying model identifier.
    fn model_id(&self) -> &'static str;

    /// Extractor implementation version.
    fn extractor_version(&self) -> &'static str;
}

/// Minimal session trait needed by [`GlinerLegalExtractor`].
pub trait GlinerSession: Send + Sync {
    /// Extract legal entities using raw GLiNER label names.
    ///
    /// # Errors
    /// Returns an error when the underlying GLiNER session fails.
    fn extract_with_labels(
        &self,
        text: &str,
        labels: &[&str],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>>;

    /// Underlying GLiNER model identifier.
    fn model_id(&self) -> &'static str;
}

/// GLiNER2-backed legal extractor.
pub struct GlinerLegalExtractor {
    inner: std::sync::Arc<dyn GlinerSession>,
    extractor_version: &'static str,
}

impl GlinerLegalExtractor {
    /// Build a GLiNER-backed legal extractor from an existing session.
    #[must_use]
    pub fn new(inner: std::sync::Arc<dyn GlinerSession>) -> Self {
        Self {
            inner,
            extractor_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

impl LegalEntityExtractor for GlinerLegalExtractor {
    fn extract(
        &self,
        text: &str,
        labels: &[LegalLabel],
        thresholds: &HashMap<&'static str, f32>,
    ) -> Result<Vec<LegalEntity>> {
        let names: Vec<&str> = labels.iter().map(|label| label.name).collect();
        self.inner.extract_with_labels(text, &names, thresholds)
    }

    fn model_id(&self) -> &'static str {
        self.inner.model_id()
    }

    fn extractor_version(&self) -> &'static str {
        self.extractor_version
    }
}

/// Orchestrates Layer-1 entities plus deterministic Layer-2 rules.
pub struct LegalEnricher {
    extractor: Arc<dyn LegalEntityExtractor>,
    labels: Vec<LegalLabel>,
    thresholds: HashMap<&'static str, f32>,
}

/// One chunk passed to [`LegalEnricher::enrich_chunks_batched`].
pub struct LegalChunkInput<'a> {
    /// Deterministic chunk id.
    pub chunk_id: Uuid,
    /// Raw chunk text, before pseudonymization.
    pub raw_text: &'a str,
    /// Pseudonymized chunk text.
    pub pseudo_text: &'a str,
    /// Raw-to-pseudonymized offset map for this chunk.
    pub offset_map: &'a PseudoOffsetMap,
}

/// Enrichment output for one chunk.
pub struct LegalChunkOutput {
    /// Denormalized LanceDB projection row.
    pub row: LegalChunkEnrichment,
    /// Deterministic rule facts extracted for graph writing.
    pub facts: Vec<TypedFact>,
    /// Legal entities translated into pseudonymized coordinates.
    pub entities: Vec<LegalEntity>,
    /// Node writes derived from `facts`, ready for [`LegalKnowledgeGraph::upsert_batch`].
    pub node_writes: Vec<NodeWrite>,
    /// Edge writes derived from `facts`, ready for [`LegalKnowledgeGraph::upsert_batch`].
    pub edge_writes: Vec<EdgeWrite>,
}

impl LegalEnricher {
    /// Build a legal enricher around a shared extractor.
    #[must_use]
    pub fn new(extractor: Arc<dyn LegalEntityExtractor>) -> Self {
        Self {
            extractor,
            labels: crate::legal::default_legal_labels(),
            thresholds: crate::legal::default_thresholds(),
        }
    }

    /// Extract legal entities from raw text.
    ///
    /// # Errors
    /// Returns an error when the configured extractor fails.
    pub fn extract_raw(&self, text: &str) -> Result<Vec<LegalEntity>> {
        self.extractor.extract(text, &self.labels, &self.thresholds)
    }

    /// Translate raw-coordinate legal entities into pseudonymized coordinates.
    #[must_use]
    pub fn translate_to_pseudo(
        &self,
        raw_ents: Vec<LegalEntity>,
        map: &PseudoOffsetMap,
        pseudo_text: &str,
    ) -> Vec<LegalEntity> {
        let mut out = Vec::with_capacity(raw_ents.len());
        for entity in raw_ents {
            let Some((pseudo_start, pseudo_end)) =
                map.translate(entity.byte_start, entity.byte_end)
            else {
                continue;
            };
            if pseudo_start >= pseudo_end {
                continue;
            }
            let Some(text) = pseudo_text.get(pseudo_start as usize..pseudo_end as usize) else {
                continue;
            };
            out.push(LegalEntity {
                byte_start: pseudo_start,
                byte_end: pseudo_end,
                text: text.to_string(),
                ..entity
            });
        }
        out
    }

    /// Produce legal enrichment and typed facts for one pseudonymized chunk.
    #[must_use]
    pub fn enrich_one(
        &self,
        chunk_id: Uuid,
        doc_id: Uuid,
        pseudo_text: &str,
        legal_ents: &[LegalEntity],
        fwd: &VaultForwardMap,
    ) -> (LegalChunkEnrichment, Vec<TypedFact>, Vec<NodeWrite>, Vec<EdgeWrite>) {
        let facts = apply_rules(chunk_id, pseudo_text, legal_ents, fwd);
        let row = projection_from_facts(chunk_id, doc_id, legal_ents, &facts, &self.extractor);
        let (node_writes, edge_writes) = facts_to_graph_writes(chunk_id, doc_id, &facts);
        (row, facts, node_writes, edge_writes)
    }

    /// Enrich a document's chunks in batch.
    ///
    /// # Errors
    /// Returns an error when the configured extractor fails on any chunk.
    pub fn enrich_chunks_batched(
        &self,
        doc_id: Uuid,
        chunks: &[LegalChunkInput<'_>],
        fwd: &VaultForwardMap,
    ) -> Result<Vec<LegalChunkOutput>> {
        let mut out = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let raw_entities = self.extract_raw(chunk.raw_text)?;
            let entities =
                self.translate_to_pseudo(raw_entities, chunk.offset_map, chunk.pseudo_text);
            let (row, facts, node_writes, edge_writes) =
                self.enrich_one(chunk.chunk_id, doc_id, chunk.pseudo_text, &entities, fwd);
            out.push(LegalChunkOutput {
                row,
                facts,
                entities,
                node_writes,
                edge_writes,
            });
        }
        Ok(out)
    }
}

fn projection_from_facts(
    chunk_id: Uuid,
    doc_id: Uuid,
    entities: &[LegalEntity],
    facts: &[TypedFact],
    extractor: &Arc<dyn LegalEntityExtractor>,
) -> LegalChunkEnrichment {
    let mut parties = Vec::new();
    let mut party_roles = Vec::new();
    let mut legal_refs = Vec::new();
    let mut clause_types = Vec::new();
    let mut obligation_kinds = Vec::new();
    let mut amounts_eur_cents = Vec::new();
    let mut event_kinds = Vec::new();
    let mut deadlines = Vec::new();

    for fact in facts {
        match fact {
            TypedFact::PartyRole { party, role } => {
                parties.push(party.clone());
                party_roles.push(role.clone());
            }
            TypedFact::Obligation {
                obligor,
                kind,
                amount_cents,
                deadline,
                owed_to,
                ..
            } => {
                if let Some(obligor) = obligor {
                    parties.push(obligor.clone());
                    party_roles.push("obligor".to_string());
                }
                if let Some(owed_to) = owed_to {
                    parties.push(owed_to.clone());
                    party_roles.push("owed_to".to_string());
                }
                obligation_kinds.push(kind.clone());
                if let Some(amount) = amount_cents {
                    amounts_eur_cents.push(*amount);
                }
                if let Some(deadline) = deadline {
                    deadlines.push(*deadline);
                }
            }
            TypedFact::Reference { article } => legal_refs.push(article.normalized_ref()),
            TypedFact::Event { kind, event_date } => {
                event_kinds.push(kind.clone());
                if let Some(event_date) = event_date {
                    deadlines.push(*event_date);
                }
            }
            TypedFact::CourtRouting { .. } => {}
        }
    }

    for entity in entities
        .iter()
        .filter(|entity| entity.label == "clause_type")
    {
        clause_types.push(entity.text.to_lowercase());
    }

    let confidence_min = entities
        .iter()
        .map(|entity| entity.confidence)
        .reduce(f32::min)
        .unwrap_or(0.0);
    let confidence_avg = if entities.is_empty() {
        0.0
    } else {
        entities.iter().map(|entity| entity.confidence).sum::<f32>() / entities.len() as f32
    };

    LegalChunkEnrichment {
        chunk_id,
        doc_id,
        doc_type: None,
        legal_domain: None,
        jurisdiction: None,
        document_date: None,
        dossier_id: None,
        parties,
        party_roles,
        legal_refs,
        clause_types,
        obligation_kinds,
        amounts_eur_cents,
        deadlines,
        event_kinds,
        risk_flags: Vec::new(),
        mandatory_clause_status: None,
        confidence_min,
        confidence_avg,
        extractor_version: extractor.extractor_version().to_string(),
        model_id: extractor.model_id().to_string(),
    }
}

/// Convert a slice of [`TypedFact`]s into graph node and edge writes.
///
/// Each fact variant produces a deterministic node (via UUID v5) and one or
/// more edges. Party ids are stable across chunks because they are derived
/// solely from the normalized form string.
fn facts_to_graph_writes(
    chunk_id: Uuid,
    doc_id: Uuid,
    facts: &[TypedFact],
) -> (Vec<NodeWrite>, Vec<EdgeWrite>) {
    let doc_key = doc_id.to_string();
    let chunk_key = chunk_id.to_string();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for fact in facts {
        match fact {
            TypedFact::PartyRole { party, role } => {
                let party_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, party.as_bytes());
                let kind = if party.starts_with("org:") {
                    PartyKind::Organization
                } else {
                    PartyKind::Person
                };
                nodes.push(NodeWrite::Party {
                    party_id,
                    kind,
                    canonical_name: party.clone(),
                    normalized_form: party.clone(),
                    siren: None,
                });
                edges.push(EdgeWrite {
                    from_label: "Party",
                    from_key: party_id.to_string(),
                    to_label: "Document",
                    to_key: doc_key.clone(),
                    edge_type: "PARTY_TO",
                    props: HashMap::from([("role".to_string(), role.clone())]),
                });
            }
            TypedFact::Obligation { obligor, kind, .. } => {
                let oblig_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("{chunk_id}::{kind}").as_bytes(),
                );
                nodes.push(NodeWrite::Obligation {
                    obligation_id: oblig_id,
                    kind: kind.clone(),
                    text_pseudo: String::new(),
                });
                edges.push(EdgeWrite {
                    from_label: "Chunk",
                    from_key: chunk_key.clone(),
                    to_label: "Obligation",
                    to_key: oblig_id.to_string(),
                    edge_type: "MENTIONS",
                    props: HashMap::new(),
                });
                if let Some(party) = obligor {
                    let party_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, party.as_bytes());
                    edges.push(EdgeWrite {
                        from_label: "Party",
                        from_key: party_id.to_string(),
                        to_label: "Obligation",
                        to_key: oblig_id.to_string(),
                        edge_type: "BOUND_BY",
                        props: HashMap::new(),
                    });
                }
            }
            TypedFact::Reference { article } => {
                let article_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    article.normalized_ref().as_bytes(),
                );
                nodes.push(NodeWrite::Article {
                    article_id,
                    article: article.clone(),
                });
                edges.push(EdgeWrite {
                    from_label: "Document",
                    from_key: doc_key.clone(),
                    to_label: "Article",
                    to_key: article_id.to_string(),
                    edge_type: "REFERENCES",
                    props: HashMap::new(),
                });
            }
            TypedFact::CourtRouting { court } => {
                nodes.push(NodeWrite::Court {
                    court_id: court.id.clone(),
                    court: court.clone(),
                });
                edges.push(EdgeWrite {
                    from_label: "Document",
                    from_key: doc_key.clone(),
                    to_label: "Court",
                    to_key: court.id.clone(),
                    edge_type: "HEARS",
                    props: HashMap::new(),
                });
            }
            TypedFact::Event { kind, event_date } => {
                let event_id = Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("{chunk_id}::{kind}").as_bytes(),
                );
                nodes.push(NodeWrite::Event {
                    event_id,
                    kind: kind.clone(),
                    event_date: *event_date,
                    deadline_date: None,
                });
                edges.push(EdgeWrite {
                    from_label: "Chunk",
                    from_key: chunk_key.clone(),
                    to_label: "Event",
                    to_key: event_id.to_string(),
                    edge_type: "MENTIONS",
                    props: HashMap::new(),
                });
            }
        }
    }
    (nodes, edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legal::default_thresholds;
    use crate::legal::offsets::{PseudoOffsetMap, Substitution};
    use crate::legal::rules::VaultForwardMap;

    pub struct FakeExtractor;

    impl LegalEntityExtractor for FakeExtractor {
        fn extract(
            &self,
            text: &str,
            _labels: &[LegalLabel],
            _thresholds: &HashMap<&'static str, f32>,
        ) -> Result<Vec<LegalEntity>> {
            let start = text.find("paiement").expect("fixture contains paiement") as u32;
            Ok(vec![LegalEntity {
                label: "obligation".into(),
                text: "paiement".into(),
                byte_start: start,
                byte_end: start + 8,
                confidence: 0.91,
            }])
        }

        fn model_id(&self) -> &'static str {
            "fake"
        }

        fn extractor_version(&self) -> &'static str {
            "test-v0"
        }
    }

    #[test]
    fn fake_extractor_returns_obligation_paiement() {
        let labels = crate::legal::default_legal_labels();
        let thresholds = default_thresholds();
        let out = FakeExtractor
            .extract(
                "Le paiement intervient sous 30 jours.",
                &labels,
                &thresholds,
            )
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "obligation");
    }

    #[test]
    fn translate_to_pseudo_rewrites_entity_coordinates() {
        let enricher = LegalEnricher::new(Arc::new(FakeExtractor));
        let map = PseudoOffsetMap {
            subs: vec![Substitution {
                raw_start: 0,
                raw_end: 4,
                pseudo_start: 0,
                pseudo_end: 5,
            }],
        };
        let out = enricher.translate_to_pseudo(
            vec![LegalEntity {
                label: "contract_party".into(),
                text: "Acme".into(),
                byte_start: 0,
                byte_end: 4,
                confidence: 0.9,
            }],
            &map,
            "ORG_1 s'engage",
        );
        assert_eq!(out[0].text, "ORG_1");
        assert_eq!((out[0].byte_start, out[0].byte_end), (0, 5));
    }

    #[test]
    fn enrich_one_projects_rule_facts() {
        let enricher = LegalEnricher::new(Arc::new(FakeExtractor));
        let fwd = VaultForwardMap {
            alias_to_canonical: [("ORG_1".to_string(), "org:acme".to_string())]
                .into_iter()
                .collect(),
        };
        let (row, facts, _nodes, _edges) = enricher.enrich_one(
            Uuid::nil(),
            Uuid::nil(),
            "ORG_1 s'engage à payer 10 000 € sous 30 jours.",
            &[],
            &fwd,
        );
        assert!(!facts.is_empty());
        assert!(row.parties.contains(&"org:acme".to_string()));
        assert!(row.obligation_kinds.contains(&"performance".to_string()));
    }

    #[test]
    fn enrich_chunks_batched_extracts_translates_and_projects_each_chunk() {
        let enricher = LegalEnricher::new(Arc::new(FakeExtractor));
        let fwd = VaultForwardMap {
            alias_to_canonical: [("ORG_1".to_string(), "org:acme".to_string())]
                .into_iter()
                .collect(),
        };
        let chunk_id = Uuid::new_v4();
        let doc_id = Uuid::new_v4();
        let map = PseudoOffsetMap::default();
        let chunks = vec![LegalChunkInput {
            chunk_id,
            raw_text: "ORG_1 s'engage au paiement sous 30 jours.",
            pseudo_text: "ORG_1 s'engage au paiement sous 30 jours.",
            offset_map: &map,
        }];

        let out = enricher
            .enrich_chunks_batched(doc_id, &chunks, &fwd)
            .unwrap();

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].row.chunk_id, chunk_id);
        assert_eq!(out[0].row.doc_id, doc_id);
        assert!(out[0].row.parties.contains(&"org:acme".to_string()));
        assert!(out[0]
            .row
            .obligation_kinds
            .contains(&"performance".to_string()));
        assert_eq!(out[0].entities[0].text, "paiement");
        assert!(!out[0].facts.is_empty());
    }
}
