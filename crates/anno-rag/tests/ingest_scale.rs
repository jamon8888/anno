//! End-to-end resumable ingest: idempotent re-ingest (no duplication),
//! resume/skip, and content-change supersede. Heavy (LanceDB + NER model);
//! ignored by default.

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ..Default::default()
    }
}

fn write_corpus(dir: &std::path::Path, n: usize) {
    std::fs::create_dir_all(dir).unwrap();
    for i in 0..n {
        std::fs::write(
            dir.join(format!("doc_{i}.txt")),
            format!("Contrat numéro {i}. Responsabilité contractuelle du débiteur."),
        )
        .unwrap();
    }
}

#[tokio::test]
#[ignore = "LanceDB + NER models; heavy"]
async fn reingest_is_idempotent_and_resumable() {
    let tmp = tempfile::tempdir().unwrap();
    let p = Pipeline::new(cfg(tmp.path()), [0u8; 32])
        .await
        .expect("pipeline");
    let corpus = tmp.path().join("corpus");
    write_corpus(&corpus, 5);
    let out = tmp.path().join("out");

    let n1 = p
        .ingest_folder(&corpus, false, &out)
        .await
        .expect("ingest 1");
    assert_eq!(n1, 5, "first run ingests all 5");

    let n2 = p
        .ingest_folder(&corpus, false, &out)
        .await
        .expect("ingest 2");
    assert_eq!(n2, 0, "second run skips all 5 (idempotent/resumable)");

    std::fs::write(corpus.join("doc_new.txt"), "Nouveau contrat de bail.").unwrap();
    let n3 = p
        .ingest_folder(&corpus, false, &out)
        .await
        .expect("ingest 3");
    assert_eq!(n3, 1, "third run ingests only the new file");

    std::fs::write(
        corpus.join("doc_0.txt"),
        "Contenu modifié: clause de non-concurrence.",
    )
    .unwrap();
    let n4 = p
        .ingest_folder(&corpus, false, &out)
        .await
        .expect("ingest 4");
    assert_eq!(n4, 1, "fourth run re-ingests only the changed file");

    let hits = p
        .search("clause de non-concurrence", 5)
        .await
        .expect("search");
    assert!(
        hits.iter().any(|h| h.source_path.ends_with("doc_0.txt")),
        "changed doc_0 is findable by its new content"
    );
    let stale = p.search("Contrat numéro 0", 5).await.expect("search2");
    assert!(
        !stale.iter().any(|h| h.source_path.ends_with("doc_0.txt")),
        "old content of doc_0 must not orphan (delete_doc_rows worked)"
    );
}
