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
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = dir.path().join("data");
    cfg
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
