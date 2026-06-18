//! recall@10 + search latency gate. Ingests the bench corpus, runs reference
//! queries, computes recall@10, and writes a JSON baseline. Set
//! `ANNO_RECALL_FLOOR=<0.0-1.0>` to fail the run when recall drops below it
//! (used by CI to enforce ">= 95% of baseline" on footprint-changing PRs).
// Bench harness: unwrap panics are acceptable (a failed bench is a failed
// run), and criterion_group! expands to an undocumented `pub fn benches`.
#![allow(clippy::unwrap_used, missing_docs)]
mod common;
use criterion::{criterion_group, criterion_main, Criterion};
use serde::Deserialize;
use tokio::runtime::Runtime;

#[derive(Deserialize)]
struct RefQuery {
    query: String,
    relevant_substring: String,
}

fn load_ref_queries() -> Vec<RefQuery> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("benches/fixtures/recall_queries.json");
    let raw = std::fs::read_to_string(&path).expect("read recall_queries.json");
    serde_json::from_str(&raw).expect("parse recall_queries.json")
}

fn bench_recall(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let queries = load_ref_queries();

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

    let mut relevant = 0usize;
    for q in &queries {
        let hits = rt.block_on(pipeline.search(&q.query, 10)).unwrap();
        let needle = q.relevant_substring.to_lowercase();
        if hits
            .iter()
            .any(|h| h.text_pseudo.to_lowercase().contains(&needle))
        {
            relevant += 1;
        }
    }
    let recall = relevant as f64 / queries.len() as f64;
    eprintln!("recall@10 = {recall:.3} ({relevant}/{})", queries.len());

    let out =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/recall_baseline.json");
    let _ = std::fs::write(&out, format!("{{\"recall_at_10\": {recall}}}"));

    if let Ok(floor) = std::env::var("ANNO_RECALL_FLOOR") {
        let floor: f64 = floor.parse().expect("ANNO_RECALL_FLOOR must be a float");
        assert!(
            recall >= floor,
            "recall@10 {recall:.3} below floor {floor:.3}"
        );
    }

    let probe = queries[0].query.clone();
    c.bench_function("recall_query_latency", |b| {
        b.to_async(&rt)
            .iter(|| async { pipeline.search(&probe, 10).await.unwrap() });
    });
}
criterion_group!(benches, bench_recall);
criterion_main!(benches);
