//! Measures RSS after Pipeline::new but before any tool call. Hard gate: <200 MB.
//! Emits IDLE_RSS_BYTES=<n> on stderr for CI scraping.
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

fn bench_idle_rss(c: &mut Criterion) {
    c.bench_function("idle_rss_mb", |b| {
        b.to_async(Runtime::new().unwrap())
            .iter_custom(|iters| async move {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let (p, _tmp) = common::pipeline_in_tempdir().await;
                    let rss = common::current_rss_bytes();
                    eprintln!("IDLE_RSS_BYTES={}", rss);
                    drop(p);
                }
                start.elapsed()
            });
    });
}
criterion_group!(benches, bench_idle_rss);
criterion_main!(benches);
