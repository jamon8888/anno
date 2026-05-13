//! Recall@10 over 10 reference queries. Hard gate: ≥ 95% of main baseline.
//! Emits RECALL_AT_10=<float> on stderr for CI scraping.
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

const REFERENCE_QUERIES: &[(&str, &str)] = &[
    ("résiliation contrat avec préavis", "doc1.md"),
    ("conseil juridique fourniture", "doc1.md"),
    ("identifiant numéro de sécurité", "doc2.md"),
    ("coordonnées bancaires IBAN", "doc2.md"),
    ("clause confidentialité informations", "doc3.md"),
    ("non concurrence employé", "doc3.md"),
    ("modalités de paiement facture", "doc4.md"),
    ("indemnité licenciement", "doc4.md"),
    ("droit applicable juridiction", "doc5.md"),
    ("résolution amiable litige", "doc5.md"),
];

fn bench_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (pipeline, _tmp) = rt.block_on(async {
        let (p, tmp) = common::pipeline_in_tempdir().await;
        p.ingest_folder(&common::bench_corpus_dir(), true, &tmp.path().join("outputs"))
            .await
            .expect("ingest");
        (p, tmp)
    });
    c.bench_function("recall_at_10", |b| {
        b.to_async(&rt).iter(|| async {
            let mut hits = 0_usize;
            for (q, expected) in REFERENCE_QUERIES {
                let res = pipeline.search(q, 10).await.unwrap();
                if res.iter().any(|h| h.source_path.ends_with(expected)) {
                    hits += 1;
                }
            }
            let recall = hits as f64 / REFERENCE_QUERIES.len() as f64;
            eprintln!("RECALL_AT_10={}", recall);
        });
    });
}
criterion_group!(benches, bench_recall);
criterion_main!(benches);
