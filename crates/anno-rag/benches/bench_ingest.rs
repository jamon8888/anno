//! Measures ingest throughput on the 5-doc fixture corpus.
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use tokio::runtime::Runtime;

fn bench_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest");
    group.sample_size(10).measurement_time(Duration::from_secs(120));
    group.bench_function("five_doc_corpus", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let (p, tmp) = common::pipeline_in_tempdir().await;
            p.ingest_folder(&common::bench_corpus_dir(), true, &tmp.path().join("outputs"))
                .await
                .expect("ingest");
        });
    });
    group.finish();
}
criterion_group!(benches, bench_ingest);
criterion_main!(benches);
