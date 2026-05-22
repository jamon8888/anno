//! Recall-path cost: (a) cold recall (first call builds FTS index +
//! optimizes), (b) steady-state recall (no new memories — the watermark
//! gate must make this pay only a count_rows, no optimize). Establishes
//! the spec §4.3 number. Run:
//! `cargo bench -p anno-rag --bench bench_memory_recall`.
#![allow(clippy::unwrap_used, missing_docs)]

use anno_rag::{AnnoRagConfig, Pipeline};
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

fn bench_memory_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let p = rt.block_on(async {
        let p = Pipeline::new(
            AnnoRagConfig {
                data_dir: tmp.path().to_path_buf(),
                ..Default::default()
            },
            [0u8; 32],
        )
        .await
        .unwrap();
        for i in 0..50 {
            p.save_memory(
                &format!("Mémoire de test numéro {i} sur la responsabilité."),
                None,
                None,
            )
            .await
            .unwrap();
        }
        p
    });

    // Cold: first recall builds the index + optimizes. Every subsequent
    // iteration adds no new memories, so the watermark gate skips
    // optimize — steady-state time is dominated by hybrid search.
    c.bench_function("memory_recall_cold_then_steady", |b| {
        b.to_async(&rt).iter(|| async {
            let hits = p
                .recall_memory("responsabilité", 5, None, None, None, false)
                .await
                .unwrap();
            criterion::black_box(hits);
        });
    });
}

criterion_group!(benches, bench_memory_recall);
criterion_main!(benches);
