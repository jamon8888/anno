//! Model-free anonymization eval: scores the five regex-detected PII
//! categories with `detect_patterns` over the annotated corpus and hard-gates
//! recall against `pii_baseline.toml`. Runs on every CI build (no model).

use anno_rag::detect::detect_patterns;
use anno_rag::pii_eval::{
    check_pii_corpus, load_pii_annotations, pii_corpus_dir, resolve_span, score_detections,
    CategoryScore, TrueSpan,
};
use std::collections::HashMap;

/// The categories produced by the regex layer (`detect_patterns`).
const REGEX_CATEGORIES: &[&str] = &["NIR", "SIRET", "IBAN_FR", "PhoneNumber", "Email"];

fn load_baseline_recall() -> HashMap<String, f64> {
    let path = pii_corpus_dir()
        .parent()
        .unwrap()
        .join("pii_baseline.toml");
    let text = std::fs::read_to_string(&path).expect("pii_baseline.toml");
    let parsed: toml::Value = toml::from_str(&text).expect("parse baseline");
    let recall = parsed.get("recall").expect("[recall] table");
    let mut out = HashMap::new();
    for cat in REGEX_CATEGORIES {
        let v = recall
            .get(cat)
            .and_then(toml::Value::as_float)
            .unwrap_or(0.0);
        out.insert((*cat).to_string(), v);
    }
    out
}

#[test]
fn regex_pii_recall_meets_baseline() {
    let dir = pii_corpus_dir();
    let ann = load_pii_annotations(&dir).expect("annotations");
    check_pii_corpus(&dir, &ann).expect("corpus consistent");

    // Aggregate detections and truth across the whole corpus, scoping truth
    // to the regex categories only.
    let mut agg: HashMap<String, CategoryScore> = HashMap::new();
    for doc in &ann.docs {
        let content = std::fs::read_to_string(dir.join(&doc.file)).expect("read doc");
        let detected = detect_patterns(&content);
        let truth: Vec<TrueSpan> = doc
            .pii
            .iter()
            .filter(|e| REGEX_CATEGORIES.contains(&e.category.as_str()))
            .map(|e| resolve_span(&content, e).expect("resolve"))
            .collect();
        let scores = score_detections(&detected, &truth);
        for cat in REGEX_CATEGORIES {
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

    let baseline = load_baseline_recall();
    let mut failures = Vec::new();
    for cat in REGEX_CATEGORIES {
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
        let base = baseline.get(*cat).copied().unwrap_or(0.0);
        eprintln!(
            "{cat}: recall={recall:.4} precision={precision:.4} (baseline recall {base})"
        );
        if recall < base * 0.98 {
            failures.push(format!(
                "{cat} recall {recall:.4} below 98% of baseline {base}"
            ));
        }
    }
    assert!(failures.is_empty(), "PII recall regressions: {failures:?}");
}
