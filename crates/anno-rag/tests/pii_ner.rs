//! Model-requiring anonymization eval: runs the full Detector (regex + NER)
#![allow(clippy::unwrap_used)]
//! over the annotated PII corpus exactly once and scores the three NER
//! categories (Person, Organization, Location). Ignored by default — runs
//! via `cargo test --test pii_ner -- --ignored --nocapture` and in the
//! nightly bench workflow.

use anno_rag::detect::Detector;
use anno_rag::pii_eval::{
    check_pii_corpus, load_pii_annotations, pii_corpus_dir, resolve_span, score_detections,
    CategoryScore, TrueSpan,
};
use std::collections::HashMap;

const NER_CATEGORIES: &[&str] = &["Person", "Organization", "Location"];

#[test]
#[ignore = "requires the HF GLiNER2 model cache; slow (~2 min)"]
fn ner_pii_recall_meets_baseline() {
    let dir = pii_corpus_dir();
    let ann = load_pii_annotations(&dir).expect("annotations");
    check_pii_corpus(&dir, &ann).expect("corpus consistent");
    let detector = Detector::new().expect("Detector::new — warm the HF model cache first");

    let mut agg: HashMap<String, CategoryScore> = HashMap::new();
    for doc in &ann.docs {
        let content = std::fs::read_to_string(dir.join(&doc.file)).expect("read doc");
        let detected = detector.detect(&content).expect("detect");
        let truth: Vec<TrueSpan> = doc
            .pii
            .iter()
            .filter(|e| NER_CATEGORIES.contains(&e.category.as_str()))
            .map(|e| resolve_span(&content, e).expect("resolve"))
            .collect();
        let scores = score_detections(&detected, &truth);
        for cat in NER_CATEGORIES {
            if let Some(s) = scores.per_category.get(*cat) {
                let acc = agg.entry((*cat).to_string()).or_insert(CategoryScore {
                    tp: 0,
                    fp: 0,
                    fn_: 0,
                    precision: 0.0,
                    recall: 0.0,
                    f1: 0.0,
                });
                acc.tp += s.tp;
                acc.fp += s.fp;
                acc.fn_ += s.fn_;
            }
        }
    }

    // Load baselines (Person, Organization, Location keys).
    let baseline_path = pii_corpus_dir().parent().unwrap().join("pii_baseline.toml");
    let baseline_text = std::fs::read_to_string(&baseline_path).expect("pii_baseline.toml");
    let parsed: toml::Value = toml::from_str(&baseline_text).expect("parse baseline");
    let recall_tbl = parsed.get("recall").expect("[recall] table");

    let mut failures: Vec<String> = Vec::new();
    for cat in NER_CATEGORIES {
        let s = agg.get(*cat).cloned().unwrap_or(CategoryScore {
            tp: 0,
            fp: 0,
            fn_: 0,
            precision: 1.0,
            recall: 1.0,
            f1: 0.0,
        });
        let recall = if s.tp + s.fn_ == 0 {
            1.0
        } else {
            s.tp as f64 / (s.tp + s.fn_) as f64
        };
        let precision = if s.tp + s.fp == 0 {
            1.0
        } else {
            s.tp as f64 / (s.tp + s.fp) as f64
        };
        let base = recall_tbl
            .get(*cat)
            .and_then(toml::Value::as_float)
            .unwrap_or(0.0);
        eprintln!(
            "{cat}: tp={} fp={} fn={} recall={recall:.4} precision={precision:.4} (baseline recall {base})",
            s.tp, s.fp, s.fn_
        );
        if recall < base * 0.98 {
            failures.push(format!(
                "{cat} recall {recall:.4} below 98% of baseline {base}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "PII NER recall regressions: {failures:?}"
    );
}

/// Diagnostic: print every Person/Organization/Location annotation the NER
/// failed to detect, with 40 chars of context, plus every FP detection.
#[test]
#[ignore = "diagnostic — requires HF GLiNER2 cache; ~2 min"]
fn diagnose_ner_misses() {
    use anno_rag::pii_eval::category_key;
    let dir = pii_corpus_dir();
    let ann = load_pii_annotations(&dir).expect("annotations");
    let detector = Detector::new().expect("Detector::new");

    for doc in &ann.docs {
        let content = std::fs::read_to_string(dir.join(&doc.file)).expect("read doc");
        let detected = detector.detect(&content).expect("detect");
        let truth: Vec<(TrueSpan, &str)> = doc
            .pii
            .iter()
            .filter(|e| NER_CATEGORIES.contains(&e.category.as_str()))
            .map(|e| (resolve_span(&content, e).expect("resolve"), e.text.as_str()))
            .collect();

        // FN: which truths got no overlapping same-category detection?
        for (t, txt) in &truth {
            let hit = detected.iter().any(|d| {
                category_key(&d.category) == t.category && t.start < d.end && d.start < t.end
            });
            if !hit {
                let mut cstart = t.start.saturating_sub(20);
                while cstart > 0 && !content.is_char_boundary(cstart) {
                    cstart -= 1;
                }
                let mut cend = (t.end + 20).min(content.len());
                while cend < content.len() && !content.is_char_boundary(cend) {
                    cend += 1;
                }
                let ctx = content[cstart..cend].replace('\n', " ");
                eprintln!("FN [{}] {} \"{}\" ctx=…{}…", doc.file, t.category, txt, ctx);
            }
        }

        // FP: detections that overlap no same-category truth (NER cats only).
        for d in &detected {
            let dkey = category_key(&d.category);
            if !NER_CATEGORIES.contains(&dkey.as_str()) {
                continue;
            }
            let hit = truth
                .iter()
                .any(|(t, _)| t.category == dkey && t.start < d.end && d.start < t.end);
            if !hit {
                eprintln!(
                    "FP [{}] {} \"{}\" @{}..{}",
                    doc.file, dkey, &d.original, d.start, d.end
                );
            }
        }
    }
}
