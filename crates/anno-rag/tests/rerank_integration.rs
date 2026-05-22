//! Integration tests for the reranked search path. Heavy (model
//! download + LanceDB); ignored by default.
#![cfg(feature = "rerank")]

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranker_lazy_inits_only_on_demand() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");
    assert!(
        !p.reranker_loaded(),
        "reranker must not load at construction"
    );
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB; ingests a small corpus"]
async fn reranked_search_reorders_vs_rrf() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    let docs = [
        (
            "a.txt",
            "La responsabilité contractuelle suppose une obligation de moyen et un dommage.",
        ),
        (
            "b.txt",
            "Le bail commercial fixe la durée et le loyer du local.",
        ),
        (
            "c.txt",
            "L'obligation de moyen engage la responsabilité contractuelle du débiteur négligent.",
        ),
        (
            "d.txt",
            "Les congés payés sont calculés sur la base de cinq semaines annuelles.",
        ),
    ];
    for (name, body) in docs {
        std::fs::write(corpus.join(name), body).unwrap();
    }
    let out = tmp.path().join("out");
    p.ingest_folder(&corpus, false, &out).await.expect("ingest");

    let q = "responsabilité contractuelle obligation de moyen";
    let rrf = p.search(q, 4).await.expect("rrf search");
    let reranked = p.search_reranked(q, 3, 4).await.expect("reranked");

    assert_eq!(reranked.len(), 3);
    let top2: Vec<&str> = reranked
        .iter()
        .take(2)
        .map(|h| h.source_path.as_str())
        .collect();
    assert!(
        top2.iter().any(|s| s.ends_with("a.txt")) && top2.iter().any(|s| s.ends_with("c.txt")),
        "expected a.txt + c.txt in reranked top-2, got {top2:?}"
    );
    let rrf_order: Vec<&str> = rrf.iter().take(3).map(|h| h.source_path.as_str()).collect();
    let rr_order: Vec<&str> = reranked.iter().map(|h| h.source_path.as_str()).collect();
    assert_ne!(rrf_order, rr_order, "rerank must change the ordering");
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranked_memory_recall_returns_topk() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    for body in [
        "La prescription quinquennale court à compter de la connaissance du dommage.",
        "Le café de la machine est trop amer ce matin.",
        "La prescription de l'action en responsabilité est de cinq ans.",
    ] {
        p.save_memory(body, None, None).await.expect("save");
    }

    let hits = p
        .recall_memory_reranked(
            "délai de prescription en responsabilité",
            2,
            None,
            None,
            None,
            false,
            10,
        )
        .await
        .expect("reranked recall");
    assert_eq!(hits.len(), 2);
    assert!(
        hits[0].text.contains("prescription"),
        "top hit must be about prescription, got: {}",
        hits[0].text
    );
}

#[tokio::test]
#[ignore = "downloads model + opens LanceDB"]
async fn reranked_search_preserves_privacy_boundary() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    let corpus = tmp.path().join("corpus");
    std::fs::create_dir_all(&corpus).unwrap();
    // Contains a PII email the vault pseudonymizes at ingest.
    std::fs::write(
        corpus.join("contract.txt"),
        "Le contrat engage Jean Dupont (jean.dupont@example.fr) au titre de \
         la responsabilité contractuelle.",
    )
    .unwrap();
    let out = tmp.path().join("out");
    p.ingest_folder(&corpus, false, &out).await.expect("ingest");

    let hits = p
        .search_reranked("responsabilité contractuelle", 1, 4)
        .await
        .expect("reranked");
    assert_eq!(hits.len(), 1);

    // The returned hit's stored text must remain pseudonymized:
    // rehydration inside search_reranked is transient (for scoring only)
    // and must never be written back into the SearchHit.
    assert!(
        !hits[0].text_pseudo.contains("jean.dupont@example.fr"),
        "stored text must remain pseudonymized: {}",
        hits[0].text_pseudo
    );
    assert!(
        hits[0].text_pseudo.contains('<') || hits[0].text_pseudo.contains("EMAIL"),
        "expected a pseudo-token in stored text: {}",
        hits[0].text_pseudo
    );
}
