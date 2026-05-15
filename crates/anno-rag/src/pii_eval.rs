//! Anonymization evaluation harness: per-category precision/recall/F1 over a
//! labelled PII corpus, with overlap-span matching, plus the corpus loader.

use cloakpipe_core::{DetectedEntity, EntityCategory};
use std::collections::HashMap;

/// A ground-truth PII span: byte offsets into a document plus its category
/// key (`"NIR"`, `"Person"`, …).
#[derive(Debug, Clone)]
pub struct TrueSpan {
    /// Inclusive byte start offset.
    pub start: usize,
    /// Exclusive byte end offset.
    pub end: usize,
    /// Category key — see [`category_key`].
    pub category: String,
}

/// Precision / recall / F1 plus raw counts for one category.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CategoryScore {
    /// True positives: true spans with ≥ 1 overlapping detection.
    pub tp: usize,
    /// False positives: detections overlapping no true span.
    pub fp: usize,
    /// False negatives: true spans with no overlapping detection (PII leak).
    pub fn_: usize,
    /// `tp / (tp + fp)`; `1.0` when there are no detections and no truth.
    pub precision: f64,
    /// `tp / (tp + fn)`; `1.0` when there are no true spans.
    pub recall: f64,
    /// Harmonic mean of precision and recall; `0.0` when both are `0.0`.
    pub f1: f64,
}

/// Per-category scores across the whole corpus.
#[derive(Debug, Clone, Default)]
pub struct PiiScores {
    /// Category key → score.
    pub per_category: HashMap<String, CategoryScore>,
}

/// Canonical category key for a detected entity's category.
///
/// Customs (`NIR`, `SIRET`, `IBAN_FR`) pass through as their inner string;
/// the named variants map to their PascalCase names. Any variant not
/// recognised is rendered with its `Debug` form — it simply will not match
/// any annotated category, so it counts as a false positive.
#[must_use]
pub fn category_key(c: &EntityCategory) -> String {
    match c {
        EntityCategory::Person => "Person".to_string(),
        EntityCategory::Organization => "Organization".to_string(),
        EntityCategory::Location => "Location".to_string(),
        EntityCategory::Email => "Email".to_string(),
        EntityCategory::PhoneNumber => "PhoneNumber".to_string(),
        EntityCategory::Custom(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// Two half-open byte intervals overlap.
fn overlaps(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

/// Score detections against ground truth, per category, with overlap-span
/// matching. Matching is greedy left-to-right: each detection is credited to
/// at most one true span, so one large detection cannot inflate TP.
#[must_use]
pub fn score_detections(detected: &[DetectedEntity], truth: &[TrueSpan]) -> PiiScores {
    // Collect every category appearing in either set.
    let mut categories: Vec<String> = truth.iter().map(|t| t.category.clone()).collect();
    for d in detected {
        categories.push(category_key(&d.category));
    }
    categories.sort();
    categories.dedup();

    let mut per_category = HashMap::new();
    for cat in categories {
        let truth_cat: Vec<&TrueSpan> =
            truth.iter().filter(|t| t.category == cat).collect();
        let det_cat: Vec<&DetectedEntity> = detected
            .iter()
            .filter(|d| category_key(&d.category) == cat)
            .collect();

        let mut truth_hit = vec![false; truth_cat.len()];
        let mut det_used = vec![false; det_cat.len()];
        for (ti, t) in truth_cat.iter().enumerate() {
            for (di, d) in det_cat.iter().enumerate() {
                if det_used[di] {
                    continue;
                }
                if overlaps(t.start, t.end, d.start, d.end) {
                    truth_hit[ti] = true;
                    det_used[di] = true;
                    break;
                }
            }
        }
        let tp = truth_hit.iter().filter(|h| **h).count();
        let fn_ = truth_hit.len() - tp;
        let fp = det_used.iter().filter(|u| !**u).count();
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
        per_category.insert(
            cat,
            CategoryScore { tp, fp, fn_, precision, recall, f1 },
        );
    }
    PiiScores { per_category }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloakpipe_core::DetectionSource;

    fn det(start: usize, end: usize, cat: EntityCategory) -> DetectedEntity {
        DetectedEntity {
            original: "x".to_string(),
            start,
            end,
            category: cat,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        }
    }
    fn truth(start: usize, end: usize, cat: &str) -> TrueSpan {
        TrueSpan { start, end, category: cat.to_string() }
    }

    #[test]
    fn overlap_counts_as_match() {
        let detected = vec![det(8, 14, EntityCategory::Email)];
        let truth = vec![truth(10, 20, "Email")];
        let s = score_detections(&detected, &truth);
        let e = s.per_category.get("Email").unwrap();
        assert_eq!((e.tp, e.fp, e.fn_), (1, 0, 0));
        assert!((e.recall - 1.0).abs() < 1e-9);
    }

    #[test]
    fn missed_true_span_is_a_false_negative() {
        let detected: Vec<DetectedEntity> = vec![];
        let truth = vec![truth(0, 5, "NIR")];
        let s = score_detections(&detected, &truth);
        let e = s.per_category.get("NIR").unwrap();
        assert_eq!((e.tp, e.fp, e.fn_), (0, 0, 1));
        assert!((e.recall - 0.0).abs() < 1e-9);
    }

    #[test]
    fn spurious_detection_is_a_false_positive() {
        let detected = vec![det(0, 5, EntityCategory::Person)];
        let truth: Vec<TrueSpan> = vec![];
        let s = score_detections(&detected, &truth);
        let e = s.per_category.get("Person").unwrap();
        assert_eq!((e.tp, e.fp, e.fn_), (0, 1, 0));
        assert!((e.precision - 0.0).abs() < 1e-9);
    }

    #[test]
    fn one_detection_credited_to_at_most_one_truth() {
        // A single wide detection overlapping two true spans scores TP=1,
        // FN=1 — it cannot be double-counted.
        let detected = vec![det(0, 100, EntityCategory::Person)];
        let truth = vec![truth(0, 5, "Person"), truth(50, 55, "Person")];
        let s = score_detections(&detected, &truth);
        let e = s.per_category.get("Person").unwrap();
        assert_eq!((e.tp, e.fp, e.fn_), (1, 0, 1));
    }

    #[test]
    fn category_must_match() {
        // An Organization detection over a Person true span is not a match.
        let detected = vec![det(0, 5, EntityCategory::Organization)];
        let truth = vec![truth(0, 5, "Person")];
        let s = score_detections(&detected, &truth);
        assert_eq!(s.per_category.get("Person").unwrap().fn_, 1);
        assert_eq!(s.per_category.get("Organization").unwrap().fp, 1);
    }
}
