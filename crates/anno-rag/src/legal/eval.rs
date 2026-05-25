//! Gold-corpus evaluation suite for the French legal RAG pipeline.
//!
//! Each gold document is a plain-text file `<name>.txt` paired with a JSON
//! sidecar `<name>.gold.json` describing the expected extractions. The
//! [`load_corpus`] function reads both from a directory.
//!
//! # Metrics
//! | Function | What it measures |
//! |---|---|
//! | [`entity_precision_recall`] | NER precision/recall per entity label |
//! | [`obligation_f1`] | Obligation extraction F1 |
//! | [`deadline_accuracy`] | Deadline normalization accuracy |
//! | [`amount_normalization_accuracy`] | EUR-cents normalization accuracy |
//! | [`mandatory_clause_f1`] | Mandatory-clause checklist F1 |
//! | [`prescription_accuracy`] | Prescription deadline accuracy (± 1 day) |
//! | [`citation_validity_rate`] | Fraction of citations matching a known ref |
//! | [`graph_traversal_latency_p95`] | 95th-percentile graph-intent latency |

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

// ── Gold document types ────────────────────────────────────────────────────

/// One expected entity mention in the gold annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldEntity {
    /// Entity label (e.g. `"person"`, `"obligation"`).
    pub label: String,
    /// Expected extracted text.
    pub text: String,
}

/// One expected obligation in the gold annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldObligation {
    /// Obligation kind.
    pub kind: String,
    /// Expected pseudo text.
    pub text_fragment: String,
}

/// One expected deadline in ISO-8601 format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldDeadline {
    /// ISO-8601 date string.
    pub date: String,
    /// Human context.
    pub context: String,
}

/// One expected monetary amount in EUR cents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldAmount {
    /// Expected EUR cents value.
    pub eur_cents: i64,
    /// Human context.
    pub context: String,
}

/// One expected mandatory-clause result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldMandatoryClause {
    /// Requirement key (e.g. `"penalites_de_retard"`).
    pub requirement: String,
    /// Expected status: `"present"` or `"missing"`.
    pub status: String,
}

/// One expected prescription anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldPrescription {
    /// Prescription category.
    pub category: String,
    /// ISO-8601 anchor date (event date).
    pub anchor_date: String,
    /// Expected prescription year.
    pub expected_year: i32,
}

/// One expected legal citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldCitation {
    /// Normalized ref, e.g. `"code_civil:1240"`.
    pub normalized_ref: String,
}

/// Gold annotation for one document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldAnnotation {
    /// Document type.
    pub doc_type: Option<String>,
    /// Legal domain.
    pub legal_domain: Option<String>,
    /// Expected entities.
    #[serde(default)]
    pub entities: Vec<GoldEntity>,
    /// Expected obligations.
    #[serde(default)]
    pub obligations: Vec<GoldObligation>,
    /// Expected deadlines.
    #[serde(default)]
    pub deadlines: Vec<GoldDeadline>,
    /// Expected amounts.
    #[serde(default)]
    pub amounts: Vec<GoldAmount>,
    /// Expected mandatory-clause results.
    #[serde(default)]
    pub mandatory_clauses: Vec<GoldMandatoryClause>,
    /// Expected prescription anchors.
    #[serde(default)]
    pub prescriptions: Vec<GoldPrescription>,
    /// Expected citations.
    #[serde(default)]
    pub citations: Vec<GoldCitation>,
}

/// One loaded gold document: source text + annotation.
#[derive(Debug, Clone)]
pub struct GoldDocument {
    /// Base name (without extension).
    pub name: String,
    /// Source text.
    pub text: String,
    /// Gold annotation.
    pub gold: GoldAnnotation,
}

/// A loaded corpus.
pub type GoldCorpus = Vec<GoldDocument>;

// ── Corpus loader ─────────────────────────────────────────────────────────

/// Load all gold documents from `dir`.
///
/// For each `<name>.txt` file, the loader expects a sibling `<name>.gold.json`.
/// Files without a sidecar are silently skipped.
///
/// # Errors
/// Returns an IO error if the directory cannot be read.
pub fn load_corpus(dir: &Path) -> Result<GoldCorpus> {
    let mut corpus = Vec::new();
    let read_dir = std::fs::read_dir(dir)
        .map_err(|e| crate::error::Error::Store(format!("load_corpus: {e}")))?;

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let gold_path = path.with_extension("gold.json");
        if !gold_path.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| crate::error::Error::Store(format!("read {}: {e}", path.display())))?;
        let gold_str = std::fs::read_to_string(&gold_path).map_err(|e| {
            crate::error::Error::Store(format!("read {}: {e}", gold_path.display()))
        })?;
        let gold: GoldAnnotation = serde_json::from_str(&gold_str).map_err(|e| {
            crate::error::Error::Store(format!("parse {}: {e}", gold_path.display()))
        })?;
        corpus.push(GoldDocument {
            name: stem,
            text,
            gold,
        });
    }
    corpus.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(corpus)
}

// ── Metric types ──────────────────────────────────────────────────────────

/// Precision, recall, and F1 for a label or category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelMetrics {
    /// True positives.
    pub tp: usize,
    /// False positives.
    pub fp: usize,
    /// False negatives.
    pub fn_: usize,
    /// Precision (0.0–1.0).
    pub precision: f64,
    /// Recall (0.0–1.0).
    pub recall: f64,
    /// F1 score (0.0–1.0).
    pub f1: f64,
}

impl LabelMetrics {
    fn new(tp: usize, fp: usize, fn_: usize) -> Self {
        let precision = if tp + fp == 0 {
            1.0
        } else {
            tp as f64 / (tp + fp) as f64
        };
        let recall = if tp + fn_ == 0 {
            1.0
        } else {
            tp as f64 / (tp + fn_) as f64
        };
        let f1 = if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        };
        Self {
            tp,
            fp,
            fn_,
            precision,
            recall,
            f1,
        }
    }
}

/// Per-label metrics map.
pub type PerLabelMetrics = HashMap<String, LabelMetrics>;

// ── Metric functions ──────────────────────────────────────────────────────

/// Compute entity precision/recall against the gold corpus.
///
/// The `predicted` closure receives the document text and returns the set of
/// `(label, text)` tuples extracted by the system under test.
pub fn entity_precision_recall<F>(corpus: &GoldCorpus, mut predicted: F) -> PerLabelMetrics
where
    F: FnMut(&GoldDocument) -> Vec<(String, String)>,
{
    let mut per_label: HashMap<String, (usize, usize, usize)> = HashMap::new();

    for doc in corpus {
        let preds = predicted(doc);
        let gold_set: Vec<_> = doc
            .gold
            .entities
            .iter()
            .map(|e| (e.label.clone(), e.text.to_lowercase()))
            .collect();
        let pred_set: Vec<_> = preds
            .into_iter()
            .map(|(l, t)| (l, t.to_lowercase()))
            .collect();

        // TP/FP per label
        for (label, text) in &pred_set {
            let entry = per_label.entry(label.clone()).or_default();
            if gold_set.iter().any(|(gl, gt)| gl == label && gt == text) {
                entry.0 += 1; // tp
            } else {
                entry.1 += 1; // fp
            }
        }
        // FN per label
        for (label, text) in &gold_set {
            let entry = per_label.entry(label.clone()).or_default();
            if !pred_set.iter().any(|(pl, pt)| pl == label && pt == text) {
                entry.2 += 1; // fn
            }
        }
    }

    per_label
        .into_iter()
        .map(|(label, (tp, fp, fn_))| (label, LabelMetrics::new(tp, fp, fn_)))
        .collect()
}

/// Compute obligation extraction F1 across the corpus.
pub fn obligation_f1<F>(corpus: &GoldCorpus, mut predicted: F) -> LabelMetrics
where
    F: FnMut(&GoldDocument) -> Vec<String>,
{
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;

    for doc in corpus {
        let preds: Vec<_> = predicted(doc)
            .into_iter()
            .map(|k| k.to_lowercase())
            .collect();
        let gold: Vec<_> = doc
            .gold
            .obligations
            .iter()
            .map(|o| o.kind.to_lowercase())
            .collect();

        for p in &preds {
            if gold.contains(p) {
                tp += 1;
            } else {
                fp += 1;
            }
        }
        for g in &gold {
            if !preds.contains(g) {
                fn_ += 1;
            }
        }
    }

    LabelMetrics::new(tp, fp, fn_)
}

/// Compute deadline normalization accuracy.
///
/// A deadline is correct when the predicted ISO-8601 date string matches the
/// gold date exactly.
pub fn deadline_accuracy<F>(corpus: &GoldCorpus, mut predicted: F) -> f64
where
    F: FnMut(&GoldDocument) -> Vec<String>,
{
    let mut correct = 0usize;
    let mut total = 0usize;

    for doc in corpus {
        let preds = predicted(doc);
        for gold_dl in &doc.gold.deadlines {
            total += 1;
            if preds.iter().any(|p| p == &gold_dl.date) {
                correct += 1;
            }
        }
    }

    if total == 0 {
        1.0
    } else {
        correct as f64 / total as f64
    }
}

/// Compute EUR-cents normalization accuracy.
///
/// An amount is correct when the predicted cents value equals the gold value
/// exactly.
pub fn amount_normalization_accuracy<F>(corpus: &GoldCorpus, mut predicted: F) -> f64
where
    F: FnMut(&GoldDocument) -> Vec<i64>,
{
    let mut correct = 0usize;
    let mut total = 0usize;

    for doc in corpus {
        let preds = predicted(doc);
        for gold_amt in &doc.gold.amounts {
            total += 1;
            if preds.contains(&gold_amt.eur_cents) {
                correct += 1;
            }
        }
    }

    if total == 0 {
        1.0
    } else {
        correct as f64 / total as f64
    }
}

/// Compute mandatory-clause checklist F1.
pub fn mandatory_clause_f1<F>(corpus: &GoldCorpus, mut predicted: F) -> LabelMetrics
where
    F: FnMut(&GoldDocument) -> Vec<(String, String)>,
{
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;

    for doc in corpus {
        let preds = predicted(doc);
        let gold: Vec<_> = doc
            .gold
            .mandatory_clauses
            .iter()
            .map(|mc| (mc.requirement.as_str(), mc.status.as_str()))
            .collect();

        for (req, status) in &preds {
            if gold
                .iter()
                .any(|(gr, gs)| *gr == req.as_str() && *gs == status.as_str())
            {
                tp += 1;
            } else {
                fp += 1;
            }
        }
        for (req, status) in &gold {
            if !preds
                .iter()
                .any(|(pr, ps)| pr.as_str() == *req && ps.as_str() == *status)
            {
                fn_ += 1;
            }
        }
    }

    LabelMetrics::new(tp, fp, fn_)
}

/// Compute prescription accuracy: fraction of anchors where the predicted
/// year matches the gold year (tolerance: ± 0 years).
pub fn prescription_accuracy<F>(corpus: &GoldCorpus, mut predicted: F) -> f64
where
    F: FnMut(&GoldDocument, &GoldPrescription) -> Option<i32>,
{
    let mut correct = 0usize;
    let mut total = 0usize;

    for doc in corpus {
        for presc in &doc.gold.prescriptions {
            total += 1;
            if let Some(year) = predicted(doc, presc) {
                if year == presc.expected_year {
                    correct += 1;
                }
            }
        }
    }

    if total == 0 {
        1.0
    } else {
        correct as f64 / total as f64
    }
}

/// Compute the fraction of predicted citations that match a known gold ref.
pub fn citation_validity_rate<F>(corpus: &GoldCorpus, mut predicted: F) -> f64
where
    F: FnMut(&GoldDocument) -> Vec<String>,
{
    let mut valid = 0usize;
    let mut total = 0usize;

    for doc in corpus {
        let preds = predicted(doc);
        let gold_refs: Vec<_> = doc
            .gold
            .citations
            .iter()
            .map(|c| c.normalized_ref.as_str())
            .collect();
        for pred in &preds {
            total += 1;
            if gold_refs.contains(&pred.as_str()) {
                valid += 1;
            }
        }
    }

    if total == 0 {
        1.0
    } else {
        valid as f64 / total as f64
    }
}

/// Measure the 95th-percentile latency (in milliseconds) of a graph-intent
/// traversal over the corpus.
///
/// `traverse` is called once per document. The returned durations are sorted
/// and the P95 value is extracted.
pub fn graph_traversal_latency_p95<F>(corpus: &GoldCorpus, mut traverse: F) -> Duration
where
    F: FnMut(&GoldDocument),
{
    if corpus.is_empty() {
        return Duration::ZERO;
    }

    let mut latencies: Vec<Duration> = corpus
        .iter()
        .map(|doc| {
            let start = Instant::now();
            traverse(doc);
            start.elapsed()
        })
        .collect();

    latencies.sort();
    let idx = ((latencies.len() as f64) * 0.95) as usize;
    latencies[idx.min(latencies.len() - 1)]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn synthetic_corpus() -> GoldCorpus {
        vec![GoldDocument {
            name: "doc1".into(),
            text: "Société Acme. Obligation de payer 5000 €.".into(),
            gold: GoldAnnotation {
                doc_type: Some("b2b_contract".into()),
                legal_domain: Some("commercial".into()),
                entities: vec![GoldEntity {
                    label: "organization".into(),
                    text: "Société Acme".into(),
                }],
                obligations: vec![GoldObligation {
                    kind: "paiement".into(),
                    text_fragment: "Obligation de payer".into(),
                }],
                deadlines: vec![GoldDeadline {
                    date: "2025-06-01".into(),
                    context: "deadline".into(),
                }],
                amounts: vec![GoldAmount {
                    eur_cents: 500_000,
                    context: "5000 EUR".into(),
                }],
                mandatory_clauses: vec![GoldMandatoryClause {
                    requirement: "penalites_de_retard".into(),
                    status: "missing".into(),
                }],
                prescriptions: vec![GoldPrescription {
                    category: "contractuel".into(),
                    anchor_date: "2020-01-01T00:00:00Z".into(),
                    expected_year: 2025,
                }],
                citations: vec![GoldCitation {
                    normalized_ref: "code_civil:1240".into(),
                }],
            },
        }]
    }

    #[test]
    fn obligation_f1_perfect_prediction() {
        let corpus = synthetic_corpus();
        let f1 = obligation_f1(&corpus, |_| vec!["paiement".into()]);
        assert_eq!(f1.tp, 1);
        assert_eq!(f1.fp, 0);
        assert!((f1.f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn deadline_accuracy_correct() {
        let corpus = synthetic_corpus();
        let acc = deadline_accuracy(&corpus, |_| vec!["2025-06-01".into()]);
        assert!((acc - 1.0).abs() < 1e-9);
    }

    #[test]
    fn amount_normalization_accuracy_correct() {
        let corpus = synthetic_corpus();
        let acc = amount_normalization_accuracy(&corpus, |_| vec![500_000_i64]);
        assert!((acc - 1.0).abs() < 1e-9);
    }

    #[test]
    fn citation_validity_rate_correct() {
        let corpus = synthetic_corpus();
        let rate = citation_validity_rate(&corpus, |_| vec!["code_civil:1240".into()]);
        assert!((rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn citation_validity_rate_invalid_ref() {
        let corpus = synthetic_corpus();
        let rate = citation_validity_rate(&corpus, |_| vec!["code_civil:9999".into()]);
        assert!((rate - 0.0).abs() < 1e-9);
    }

    #[test]
    fn prescription_accuracy_perfect() {
        let corpus = synthetic_corpus();
        let acc = prescription_accuracy(&corpus, |_, p| {
            // Simulate the prescription engine: 2020 + 5 = 2025.
            use chrono::{TimeZone, Utc};
            let anchor: chrono::DateTime<Utc> = p
                .anchor_date
                .parse()
                .unwrap_or_else(|_| Utc.timestamp_opt(0, 0).unwrap());
            crate::legal::prescription::compute_prescription(&p.category, anchor, &[])
                .map(|r| r.prescribes_on.year())
        });
        assert!((acc - 1.0).abs() < 1e-9);
    }

    #[test]
    fn load_corpus_returns_empty_on_missing_dir() {
        let dir = std::path::PathBuf::from("/nonexistent_path_xyzzy_42");
        assert!(load_corpus(&dir).is_err());
    }
}
