//! Regression: `recall_memory` must work through the public API with no
//! manual index setup, including for memories saved *after* the FTS
//! index is first built (the staleness case). Heavy (LanceDB + model);
//! ignored by default.

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "opens LanceDB + loads embedder"]
async fn recall_memory_works_without_manual_index_setup() {
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
        .recall_memory(
            "délai de prescription en responsabilité",
            3,
            None,
            None,
            None,
            false,
        )
        .await
        .expect("recall_memory must not error on a fresh store");
    assert!(!hits.is_empty(), "expected at least one recalled memory");
    assert!(
        hits.iter().any(|h| h.text.contains("prescription")),
        "expected a prescription memory in hits, got: {:?}",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[ignore = "opens LanceDB + loads embedder"]
async fn recall_finds_memories_saved_after_first_index_build() {
    let tmp = tempfile::tempdir().expect("tmp");
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");

    // First save + recall: forces the initial FTS index build.
    p.save_memory(
        "La résiliation du bail commercial obéit à un préavis de six mois.",
        None,
        None,
    )
    .await
    .expect("save 1");
    let _ = p
        .recall_memory("bail commercial", 5, None, None, None, false)
        .await
        .expect("recall 1 (builds index)");

    // Save MORE memories AFTER the index already exists, then recall.
    // This is the staleness case: pre-fix these were never indexed.
    for body in [
        "Le congé doit être délivré par acte extrajudiciaire.",
        "Le preneur dispose d'un droit au renouvellement du bail.",
    ] {
        p.save_memory(body, None, None).await.expect("save more");
    }
    let hits = p
        .recall_memory(
            "droit au renouvellement du bail",
            5,
            None,
            None,
            None,
            false,
        )
        .await
        .expect("recall 2");
    assert!(
        hits.iter().any(|h| h.text.contains("renouvellement")),
        "memory saved AFTER first index build must be recallable; got: {:?}",
        hits.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}
