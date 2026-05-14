//! Measures Pipeline::new + first vault_stats call. Hard gate: p95 < 2s.
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use tokio::runtime::Runtime;

fn bench_cold_start(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold_start");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));
    group.bench_function("pipeline_new_plus_stats", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let (p, _tmp) = common::pipeline_in_tempdir().await;
            let _ = p.vault_stats().await;
        });
    });
    group.finish();
}
criterion_group!(benches, bench_cold_start);
criterion_main!(benches);
