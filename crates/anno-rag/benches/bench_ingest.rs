//! Measures ingest throughput on the 5-doc fixture corpus.
// Bench harness: unwrap panics are acceptable (a failed bench is a failed
// run), and criterion_group! expands to an undocumented `pub fn benches`.
#![allow(clippy::unwrap_used, missing_docs)]
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use tokio::runtime::Runtime;

fn bench_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest");
    group
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(30));
    group.bench_function("five_doc_corpus", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
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
        });
    });
    group.finish();
}
criterion_group!(benches, bench_ingest);
criterion_main!(benches);
