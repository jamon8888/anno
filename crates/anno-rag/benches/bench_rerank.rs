//! Rerank a pool of 30 candidates against a canonical FR-legal query.
//! Establishes the §5.6 perf floor (expect well under ~4 s/iter on CPU;
//! more than that is a regression worth investigating). Run:
//! `cargo bench -p anno-rag --features rerank --bench bench_rerank`.
#![cfg(feature = "rerank")]

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_rerank_pool_30(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let reranker = rt
        .block_on(anno_rag::rerank::Reranker::load(
            &anno_rag::AnnoRagConfig::default(),
        ))
        .expect("load reranker");

    let query = "responsabilité contractuelle et obligation de moyen";
    let passages: Vec<String> = (0..30)
        .map(|i| format!("Clause {i} relative à la responsabilité contractuelle du débiteur."))
        .collect();
    let refs: Vec<&str> = passages.iter().map(String::as_str).collect();

    c.bench_function("rerank_pool_30", |b| {
        b.iter(|| {
            let s = reranker
                .score_pairs_batched(query, &refs, 8)
                .expect("score");
            criterion::black_box(s);
        });
    });
}

criterion_group!(benches, bench_rerank_pool_30);
criterion_main!(benches);
