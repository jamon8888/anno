//! Legal Graph v0 SQLite backend behavior tests.

use anno_rag::config::AnnoRagConfig;
use anno_rag::legal::kg::{
    EdgeBatch, EdgeWrite, LegalKnowledgeGraph, NodeBatch, NodeWrite, SqliteLegalGraphStore,
};
use anno_rag::legal::query::{run_intent, GraphIntent};
use anno_rag::legal::types::{ArticleRef, PartyKind};
use chrono::Utc;
use std::collections::HashMap;
use tempfile::TempDir;
use uuid::Uuid;

fn cfg_in(dir: &TempDir) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.path().join("data"),
        ..AnnoRagConfig::default()
    }
}

async fn store() -> (TempDir, SqliteLegalGraphStore) {
    let dir = TempDir::new().expect("tempdir");
    let cfg = cfg_in(&dir);
    let kg = SqliteLegalGraphStore::open(&cfg)
        .await
        .expect("open sqlite legal graph");
    (dir, kg)
}

fn seeded_contract_graph(doc_id: Uuid, chunk_id: Uuid) -> (NodeBatch, EdgeBatch) {
    let party_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"org:acme");
    let obligation_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"obligation:pay:acme");
    let article_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"code_civil:1103");

    let mut nodes = NodeBatch::new();
    nodes.add_document(
        doc_id,
        Some("contract".into()),
        Some("civil".into()),
        Some("fr".into()),
        Some(Utc::now()),
        Some("dossier-1".into()),
    );
    nodes.add_chunk(chunk_id, doc_id, 10, 80, Some(1));
    nodes.nodes.push(NodeWrite::Party {
        party_id,
        kind: PartyKind::Organization,
        canonical_name: "org:acme".into(),
        normalized_form: "org:acme".into(),
        siren: None,
    });
    nodes.nodes.push(NodeWrite::Obligation {
        obligation_id,
        kind: "payment".into(),
        text_pseudo: "payer la facture sous trente jours".into(),
    });
    nodes.nodes.push(NodeWrite::Article {
        article_id,
        article: ArticleRef {
            code: "code_civil".into(),
            article_num: "1103".into(),
        },
    });

    let mut edges = EdgeBatch::new();
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Chunk",
        to_key: chunk_id.to_string(),
        edge_type: "HAS_CHUNK",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Party",
        from_key: party_id.to_string(),
        to_label: "Document",
        to_key: doc_id.to_string(),
        edge_type: "PARTY_TO",
        props: HashMap::from([("role".into(), "client".into())]),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Party",
        from_key: party_id.to_string(),
        to_label: "Obligation",
        to_key: obligation_id.to_string(),
        edge_type: "BOUND_BY",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Obligation",
        to_key: obligation_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::from([
            ("byte_start".into(), "10".into()),
            ("byte_end".into(), "80".into()),
        ]),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Article",
        to_key: article_id.to_string(),
        edge_type: "REFERENCES",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Article",
        to_key: article_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    (nodes, edges)
}

#[tokio::test]
async fn sqlite_graph_persists_nodes_and_edges() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);

    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    let result = run_intent(
        &kg,
        GraphIntent::PartyDossier {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("party dossier");

    let doc_id = doc_id.to_string();
    let chunk_id = chunk_id.to_string();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].get("doc_id").map(String::as_str),
        Some(doc_id.as_str())
    );
    assert_eq!(
        result.rows[0].get("chunk_id").map(String::as_str),
        Some(chunk_id.as_str())
    );
    assert_eq!(
        result.rows[0].get("role").map(String::as_str),
        Some("client")
    );
}

#[tokio::test]
async fn obligations_owed_by_returns_source_chunk() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    let result = run_intent(
        &kg,
        GraphIntent::ObligationsOwedBy {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("obligations");

    let chunk_id = chunk_id.to_string();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].get("obligation_kind").map(String::as_str),
        Some("payment")
    );
    assert_eq!(
        result.rows[0].get("chunk_id").map(String::as_str),
        Some(chunk_id.as_str())
    );
}

#[tokio::test]
async fn delete_doc_removes_document_scoped_edges() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert graph");

    kg.delete_doc(doc_id).await.expect("delete doc");

    let result = run_intent(
        &kg,
        GraphIntent::PartyDossier {
            party: "org:acme".into(),
        },
    )
    .await
    .expect("party dossier after delete");

    assert!(result.rows.is_empty());
}

#[tokio::test]
async fn contract_parties_returns_party_linked_to_doc() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg
        .contract_parties(&doc_id.to_string())
        .await
        .expect("contract_parties");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("value").map(String::as_str), Some("org:acme"));
    assert_eq!(rows[0].get("role").map(String::as_str), Some("client"));
}

#[tokio::test]
async fn contract_obligations_returns_obligation_via_mentions() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_contract_graph(doc_id, chunk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg
        .contract_obligations(&doc_id.to_string())
        .await
        .expect("contract_obligations");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("kind").map(String::as_str), Some("payment"));
    assert_eq!(
        rows[0].get("cid").map(String::as_str),
        Some(chunk_id.to_string().as_str())
    );
}

#[tokio::test]
async fn raw_cypher_still_rejected_by_sqlite_backend() {
    let (_dir, kg) = store().await;
    let result = kg
        .cypher(
            "MATCH (n) RETURN n",
            HashMap::from([("x".to_string(), "y".to_string())]),
        )
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not supported"),
        "expected 'not supported' error, got: {err}"
    );
}

fn seeded_risk_graph(doc_id: Uuid, chunk_id: Uuid, risk_id: Uuid) -> (NodeBatch, EdgeBatch) {
    let mut nodes = NodeBatch::new();
    nodes.add_document(
        doc_id,
        Some("contract".into()),
        None,
        None,
        None,
        Some("dossier-risk".into()),
    );
    nodes.add_chunk(chunk_id, doc_id, 0, 100, None);
    nodes.nodes.push(NodeWrite::Risk {
        risk_id,
        severity: "high".into(),
        category: "clause_penale".into(),
        text_pseudo: "clause pénale forfaitaire de 50%".into(),
    });

    let mut edges = EdgeBatch::new();
    edges.edges.push(EdgeWrite {
        from_label: "Document",
        from_key: doc_id.to_string(),
        to_label: "Chunk",
        to_key: chunk_id.to_string(),
        edge_type: "HAS_CHUNK",
        props: HashMap::new(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Risk",
        to_key: risk_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });
    (nodes, edges)
}

#[tokio::test]
async fn risk_findings_by_doc_id() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let risk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_risk_graph(doc_id, chunk_id, risk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg
        .risk_findings(&doc_id.to_string(), false)
        .await
        .expect("risk_findings");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("severity").map(String::as_str), Some("high"));
    assert_eq!(
        rows[0].get("category").map(String::as_str),
        Some("clause_penale")
    );
}

#[tokio::test]
async fn risk_findings_by_dossier_id() {
    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let risk_id = Uuid::now_v7();
    let (nodes, edges) = seeded_risk_graph(doc_id, chunk_id, risk_id);
    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let rows = kg
        .risk_findings("dossier-risk", true)
        .await
        .expect("risk_findings dossier");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("category").map(String::as_str),
        Some("clause_penale")
    );
}

#[tokio::test]
async fn d2_tools_end_to_end_contract_with_risk_and_timeline() {
    use anno_rag::legal::extract::{extract_contract, risk_review, timeline};

    let (_dir, kg) = store().await;
    let doc_id = Uuid::now_v7();
    let chunk_id = Uuid::now_v7();
    let risk_id = Uuid::now_v7();
    let event_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"event:audience");

    let (mut nodes, mut edges) = seeded_contract_graph(doc_id, chunk_id);

    // Add Risk node
    nodes.nodes.push(NodeWrite::Risk {
        risk_id,
        severity: "high".into(),
        category: "responsabilite_illimitee".into(),
        text_pseudo: "exclusion totale de responsabilité".into(),
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Risk",
        to_key: risk_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    // Add Event node
    nodes.nodes.push(NodeWrite::Event {
        event_id,
        kind: "audience".into(),
        event_date: Some(Utc::now()),
        deadline_date: None,
    });
    edges.edges.push(EdgeWrite {
        from_label: "Chunk",
        from_key: chunk_id.to_string(),
        to_label: "Event",
        to_key: event_id.to_string(),
        edge_type: "MENTIONS",
        props: HashMap::new(),
    });

    kg.upsert_batch(&nodes, &edges).await.expect("upsert");

    let doc_id_str = doc_id.to_string();

    // extract_contract — should find party + obligation
    let contract = extract_contract(&kg, &doc_id_str)
        .await
        .expect("extract_contract");
    assert!(
        !contract.rows.is_empty(),
        "extract_contract returned no rows"
    );
    assert!(contract.rows.iter().any(|r| r.field.starts_with("party:")));
    assert!(contract
        .rows
        .iter()
        .any(|r| r.field.starts_with("obligation:")));

    // risk_review — should find the risk
    let risks = risk_review(&kg, &doc_id_str, false)
        .await
        .expect("risk_review");
    assert_eq!(risks.findings.len(), 1);
    assert_eq!(risks.findings[0].severity, "high");
    assert_eq!(risks.findings[0].category, "responsabilite_illimitee");

    // timeline — should find the event
    let tl = timeline(&kg, "dossier-1").await.expect("timeline");
    assert!(!tl.events.is_empty(), "timeline returned no events");
    assert!(tl.events.iter().any(|e| e.kind == "audience"));
}
