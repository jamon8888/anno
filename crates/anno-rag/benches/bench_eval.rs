//! Legal RAG eval harness. Ingests the eval corpus, replays graded queries,
//! emits RECALL_AT_10=<f> and NDCG_AT_10=<f> on stderr for CI scraping.
// Bench harness: unwrap panics are acceptable (a failed bench is a failed
// run), and criterion_group! expands to an undocumented `pub fn benches`.
#![allow(clippy::unwrap_used, missing_docs)]
mod common;
use anno_rag::eval::{eval_corpus_dir, load_queries, run_eval};
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

fn bench_eval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = eval_corpus_dir();
    let queries = load_queries(&dir).expect("queries.toml");
    let (pipeline, _tmp) = rt.block_on(async {
        let (p, tmp) = common::pipeline_in_tempdir().await;
        let n = p
            .ingest_folder(&dir, true, &tmp.path().join("outputs"))
            .await
            .expect("ingest");
        assert!(
            n > 0,
            "eval corpus ingested 0 documents — warm the HF model cache first \
             (run `cargo run --release --example warmup_model -p anno-rag`)"
        );
        (p, tmp)
    });
    c.bench_function("eval_recall_ndcg", |b| {
        b.to_async(&rt).iter(|| async {
            let scores = run_eval(&pipeline, &queries).await.unwrap();
            eprintln!("RECALL_AT_10={}", scores.recall_at_10);
            eprintln!("NDCG_AT_10={}", scores.ndcg_at_10);
        });
    });
}
criterion_group!(benches, bench_eval);
criterion_main!(benches);
