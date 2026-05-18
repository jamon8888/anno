//! Reranked legal-RAG eval gate. Ingests the eval corpus, replays
//! graded queries through `search_reranked`, emits RECALL_AT_10 /
//! NDCG_AT_10 on stderr, and asserts non-regression vs the RRF baseline
//! (`tests/fixtures/eval_baseline.toml`) and vs the committed reranked
//! baseline (`tests/fixtures/eval_baseline_reranked.toml`). Spec §10.5.
//!
//! Run: `cargo bench -p anno-rag --features rerank --bench bench_rerank_eval`.
// Bench harness: unwrap panics are acceptable (a failed bench is a
// failed run), and criterion_group! expands to `pub fn benches`.
#![allow(clippy::unwrap_used, missing_docs)]
#![cfg(feature = "rerank")]
mod common;
use anno_rag::eval::{eval_corpus_dir, load_queries, run_eval, run_eval_reranked};
use criterion::{criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

fn read_baseline(name: &str) -> (f64, f64) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let txt = std::fs::read_to_string(&path).unwrap();
    let v: toml::Value = toml::from_str(&txt).unwrap();
    (
        v.get("recall_at_10")
            .and_then(toml::Value::as_float)
            .unwrap(),
        v.get("ndcg_at_10").and_then(toml::Value::as_float).unwrap(),
    )
}

fn bench_rerank_eval(c: &mut Criterion) {
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
            "eval corpus ingested 0 documents — warm the HF model cache first"
        );
        (p, tmp)
    });

    // One-shot gate: measure RRF and reranked once, assert non-regression.
    let (rrf, reranked) = rt.block_on(async {
        let rrf = run_eval(&pipeline, &queries).await.unwrap();
        let rr = run_eval_reranked(&pipeline, &queries).await.unwrap();
        (rrf, rr)
    });
    eprintln!(
        "RRF      RECALL_AT_10={} NDCG_AT_10={}",
        rrf.recall_at_10, rrf.ndcg_at_10
    );
    eprintln!(
        "RERANKED RECALL_AT_10={} NDCG_AT_10={}",
        reranked.recall_at_10, reranked.ndcg_at_10
    );

    let (_base_recall, base_ndcg) = read_baseline("eval_baseline_reranked.toml");
    let tol = 0.98;
    assert!(
        reranked.ndcg_at_10 >= base_ndcg * tol,
        "reranked nDCG@10 {} regressed below committed baseline {} (tol {})",
        reranked.ndcg_at_10,
        base_ndcg,
        tol
    );
    assert!(
        reranked.ndcg_at_10 >= rrf.ndcg_at_10 * tol,
        "reranked nDCG@10 {} did not match/beat RRF {} — do not ship",
        reranked.ndcg_at_10,
        rrf.ndcg_at_10
    );

    c.bench_function("rerank_eval_recall_ndcg", |b| {
        b.to_async(&rt).iter(|| async {
            let s = run_eval_reranked(&pipeline, &queries).await.unwrap();
            criterion::black_box(s);
        });
    });
}
criterion_group!(benches, bench_rerank_eval);
criterion_main!(benches);
