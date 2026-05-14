//! Measures search latency on a warm Pipeline. Hard gate: p95 < 200ms.
// Bench harness: unwrap panics are acceptable (a failed bench is a failed
// run), and criterion_group! expands to an undocumented `pub fn benches`.
#![allow(clippy::unwrap_used, missing_docs)]
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

const QUERY: &str = "résiliation du contrat avec préavis";

fn bench_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (pipeline, _tmp) = rt.block_on(async {
        let (p, tmp) = common::pipeline_in_tempdir().await;
        let n = p
            .ingest_folder(
                &common::bench_corpus_dir(),
                true,
                &tmp.path().join("outputs"),
            )
            .await
            .expect("ingest");
        assert!(
            n > 0,
            "bench corpus ingested 0 documents — warm the HF cache first"
        );
        (p, tmp)
    });
    c.bench_function("search_p95", |b| {
        b.to_async(&rt)
            .iter(|| async { pipeline.search(QUERY, 10).await.unwrap() });
    });
}
criterion_group!(benches, bench_search);
criterion_main!(benches);
