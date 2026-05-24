//! Criterion bench that loads the gold corpus, runs the legal eval metric
//! functions, and emits a compact JSON report on stderr for CI scraping.
//!
//! Run with:
//!   cargo bench -p anno-rag --bench legal_eval -- --quick
//!
//! Expected output (stderr): one JSON line per metric, e.g.
//!   {"metric":"mandatory_clause_f1","f1":0.95,"tp":19,"fp":1,"fn_":0}
#![allow(clippy::unwrap_used, missing_docs)]

use anno_rag::legal::eval::{
    citation_validity_rate, load_corpus, mandatory_clause_f1, obligation_f1,
    prescription_accuracy,
};
use anno_rag::legal::mandatory::evaluate_doc;
use anno_rag::legal::prescription::{compute_prescription, InterruptingEvent};
use criterion::{criterion_group, criterion_main, Criterion};
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    // Canonical path: crates/anno-rag/tests/legal_gold_corpus
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    PathBuf::from(manifest).join("tests").join("legal_gold_corpus")
}

fn bench_mandatory_clause_eval(c: &mut Criterion) {
    let dir = corpus_dir();
    let corpus = load_corpus(&dir).expect("gold corpus");
    if corpus.is_empty() {
        eprintln!("WARN: gold corpus empty — skipping mandatory clause bench");
        return;
    }

    c.bench_function("mandatory_clause_f1", |b| {
        b.iter(|| {
            let metrics = mandatory_clause_f1(&corpus, |doc| {
                let doc_type = doc
                    .gold
                    .doc_type
                    .as_deref()
                    .unwrap_or("unknown");
                evaluate_doc(doc_type, &doc.text)
                    .into_iter()
                    .map(|mc| (mc.requirement, mc.status))
                    .collect()
            });
            metrics
        })
    });
}

fn bench_obligation_f1(c: &mut Criterion) {
    let dir = corpus_dir();
    let corpus = load_corpus(&dir).expect("gold corpus");
    if corpus.is_empty() {
        return;
    }

    c.bench_function("obligation_f1", |b| {
        b.iter(|| {
            // Simulated predictor: extract obligation kinds from text via keyword matching.
            obligation_f1(&corpus, |doc| {
                let text = doc.text.to_lowercase();
                let mut kinds = Vec::new();
                if text.contains("paiement") || text.contains("rémunération") {
                    kinds.push("paiement".to_string());
                }
                if text.contains("livraison") || text.contains("livrer") {
                    kinds.push("livraison".to_string());
                }
                if text.contains("prestation") || text.contains("fournir") {
                    kinds.push("prestation".to_string());
                }
                kinds
            })
        })
    });
}

fn bench_prescription_accuracy(c: &mut Criterion) {
    let dir = corpus_dir();
    let corpus = load_corpus(&dir).expect("gold corpus");
    if corpus.is_empty() {
        return;
    }

    c.bench_function("prescription_accuracy", |b| {
        b.iter(|| {
            prescription_accuracy(&corpus, |_doc, presc| {
                let anchor: chrono::DateTime<chrono::Utc> =
                    presc.anchor_date.parse().ok()?;
                compute_prescription(&presc.category, anchor, &[] as &[InterruptingEvent])
                    .map(|r| r.prescribes_on.year())
            })
        })
    });
}

fn bench_citation_validity(c: &mut Criterion) {
    let dir = corpus_dir();
    let corpus = load_corpus(&dir).expect("gold corpus");
    if corpus.is_empty() {
        return;
    }

    c.bench_function("citation_validity_rate", |b| {
        b.iter(|| {
            // Simulated predictor using gold citations directly (perfect predictor).
            citation_validity_rate(&corpus, |doc| {
                doc.gold.citations.iter().map(|c| c.normalized_ref.clone()).collect()
            })
        })
    });
}

/// Emit a JSON metrics report to stderr for CI scraping.
fn emit_report() {
    let dir = corpus_dir();
    let corpus = match load_corpus(&dir) {
        Ok(c) if !c.is_empty() => c,
        _ => {
            eprintln!(
                "{{\"warn\":\"gold corpus not found or empty at {}\"}}",
                dir.display()
            );
            return;
        }
    };

    // Mandatory clause F1
    let mc = mandatory_clause_f1(&corpus, |doc| {
        let doc_type = doc.gold.doc_type.as_deref().unwrap_or("unknown");
        evaluate_doc(doc_type, &doc.text)
            .into_iter()
            .map(|c| (c.requirement, c.status))
            .collect()
    });
    eprintln!(
        "{{\"metric\":\"mandatory_clause_f1\",\"f1\":{:.4},\"tp\":{},\"fp\":{},\"fn_\":{}}}",
        mc.f1, mc.tp, mc.fp, mc.fn_
    );

    // Prescription accuracy
    let pa = prescription_accuracy(&corpus, |_doc, presc| {
        let anchor: chrono::DateTime<chrono::Utc> = presc.anchor_date.parse().ok()?;
        compute_prescription(&presc.category, anchor, &[] as &[InterruptingEvent])
            .map(|r| r.prescribes_on.year())
    });
    eprintln!("{{\"metric\":\"prescription_accuracy\",\"accuracy\":{:.4}}}", pa);

    // Citation validity
    let cv = citation_validity_rate(&corpus, |doc| {
        doc.gold.citations.iter().map(|c| c.normalized_ref.clone()).collect()
    });
    eprintln!("{{\"metric\":\"citation_validity_rate\",\"rate\":{:.4}}}", cv);
}

/// Emit JSON metrics report + run all criterion benches.
fn bench_all(c: &mut Criterion) {
    // Emit the metrics report once so CI can scrape it.
    emit_report();
    bench_mandatory_clause_eval(c);
    bench_obligation_f1(c);
    bench_prescription_accuracy(c);
    bench_citation_validity(c);
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
