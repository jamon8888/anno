//! End-to-end ingest scaling: idempotent re-ingest (no duplication),
//! resume/skip, content-change supersede, concurrency-safety, and a
//! recorded throughput smoke. Heavy (LanceDB + N NER models); ignored
//! by default.

use anno_rag::{AnnoRagConfig, Pipeline};

fn cfg(dir: &std::path::Path, conc: usize, pool: usize) -> AnnoRagConfig {
    AnnoRagConfig {
        data_dir: dir.to_path_buf(),
        ingest_concurrency: conc,
        ingest_ner_pool: pool,
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
    let p = Pipeline::new(cfg(tmp.path(), 4, 2), [0u8; 32])
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

#[tokio::test]
#[ignore = "LanceDB + NER models; heavy"]
async fn concurrent_ingest_matches_sequential_count() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    write_corpus(&corpus, 12);
    let out = tmp.path().join("o");

    let seq_dir = tmp.path().join("seq");
    let pseq = Pipeline::new(cfg(&seq_dir, 1, 1), [0u8; 32]).await.unwrap();
    let nseq = pseq.ingest_folder(&corpus, false, &out).await.unwrap();

    let par_dir = tmp.path().join("par");
    let ppar = Pipeline::new(cfg(&par_dir, 6, 3), [0u8; 32]).await.unwrap();
    let npar = ppar.ingest_folder(&corpus, false, &out).await.unwrap();

    assert_eq!(nseq, 12);
    assert_eq!(npar, nseq, "concurrent ingest must ingest the same set");
    let hs = pseq
        .search("responsabilité contractuelle", 20)
        .await
        .unwrap();
    let hp = ppar
        .search("responsabilité contractuelle", 20)
        .await
        .unwrap();
    assert_eq!(
        hs.len(),
        hp.len(),
        "concurrent run lost/dup'd rows vs sequential"
    );
}

#[tokio::test]
#[ignore = "throughput smoke — records timing, run manually"]
async fn throughput_smoke_parallel_faster_than_sequential() {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("c");
    write_corpus(&corpus, 50);
    let out = tmp.path().join("o");

    let s_dir = tmp.path().join("s");
    let ps = Pipeline::new(cfg(&s_dir, 1, 1), [0u8; 32]).await.unwrap();
    let t0 = std::time::Instant::now();
    ps.ingest_folder(&corpus, false, &out).await.unwrap();
    let seq = t0.elapsed();

    let p_dir = tmp.path().join("p");
    let pp = Pipeline::new(cfg(&p_dir, 8, 4), [0u8; 32]).await.unwrap();
    let t1 = std::time::Instant::now();
    pp.ingest_folder(&corpus, false, &out).await.unwrap();
    let par = t1.elapsed();

    eprintln!("INGEST_50 sequential={seq:?} parallel(conc8,pool4)={par:?}");
    assert!(
        par < seq,
        "parallel ({par:?}) should beat sequential ({seq:?})"
    );
}
