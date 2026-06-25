//! Ground-truth PII detection eval harness. Loads labeled fixtures and
//! computes per-category precision/recall/F1 against `Detector` output.
//! Fixtures contain only synthetic (fictitious) PII.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One labeled PII span (byte offsets into `text`).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GoldSpan {
    pub category: String,
    pub start: usize,
    pub end: usize,
}

/// One eval fixture: a text and its gold PII spans.
#[derive(Debug, Clone, Deserialize)]
pub struct PiiFixture {
    pub text: String,
    pub spans: Vec<GoldSpan>,
}

/// A predicted PII span (byte offsets), category lowercased to match gold.
#[derive(Debug, Clone)]
pub struct PredSpan {
    pub category: String,
    pub start: usize,
    pub end: usize,
}

/// Per-category confusion counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
}

impl CategoryCounts {
    #[must_use]
    pub fn recall(&self) -> f64 {
        let denom = self.true_positive + self.false_negative;
        if denom == 0 {
            1.0
        } else {
            self.true_positive as f64 / denom as f64
        }
    }

    #[must_use]
    pub fn precision(&self) -> f64 {
        let denom = self.true_positive + self.false_positive;
        if denom == 0 {
            1.0
        } else {
            self.true_positive as f64 / denom as f64
        }
    }

    #[must_use]
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }
}

/// Root of the eval fixtures (env override, else versioned dir).
#[must_use]
pub fn pii_eval_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ANNO_PII_EVAL_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pii_eval")
}

/// Load every `*.json` fixture under `dir` (recursively into short/ and long/).
pub fn load_fixtures(dir: &Path) -> std::io::Result<Vec<PiiFixture>> {
    let mut out = Vec::new();
    for sub in ["short", "long"] {
        let subdir = dir.join(sub);
        if !subdir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(subdir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let raw = std::fs::read_to_string(&path)?;
                let fixture: PiiFixture = serde_json::from_str(&raw)
                    .unwrap_or_else(|e| panic!("bad fixture {}: {e}", path.display()));
                out.push(fixture);
            }
        }
    }
    Ok(out)
}

fn overlaps(a: &GoldSpan, b: &PredSpan) -> bool {
    a.category.eq_ignore_ascii_case(&b.category) && a.start < b.end && b.start < a.end
}

/// Score one fixture: greedy 1:1 overlap match per category.
#[must_use]
pub fn score_one(gold: &[GoldSpan], pred: &[PredSpan]) -> BTreeMap<String, CategoryCounts> {
    let mut counts: BTreeMap<String, CategoryCounts> = BTreeMap::new();
    let mut matched_pred = vec![false; pred.len()];
    for g in gold {
        let entry = counts
            .entry(g.category.to_ascii_lowercase())
            .or_default();
        match pred
            .iter()
            .enumerate()
            .position(|(i, p)| !matched_pred[i] && overlaps(g, p))
        {
            Some(i) => {
                matched_pred[i] = true;
                entry.true_positive += 1;
            }
            None => entry.false_negative += 1,
        }
    }
    for (i, p) in pred.iter().enumerate() {
        if !matched_pred[i] {
            counts
                .entry(p.category.to_ascii_lowercase())
                .or_default()
                .false_positive += 1;
        }
    }
    counts
}

/// Merge per-fixture counts into an aggregate.
#[must_use]
pub fn merge(
    mut acc: BTreeMap<String, CategoryCounts>,
    one: BTreeMap<String, CategoryCounts>,
) -> BTreeMap<String, CategoryCounts> {
    for (cat, c) in one {
        let e = acc.entry(cat).or_default();
        e.true_positive += c.true_positive;
        e.false_positive += c.false_positive;
        e.false_negative += c.false_negative;
    }
    acc
}

/// Recall for a specific category in an aggregate map.
#[must_use]
pub fn recall_of(agg: &BTreeMap<String, CategoryCounts>, cat: &str) -> f64 {
    agg.get(cat).map(|c| c.recall()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_at_least_one_fixture() {
        let fx = load_fixtures(&pii_eval_dir()).expect("load fixtures");
        assert!(!fx.is_empty(), "expected at least one fixture");
        assert!(fx.iter().any(|f| !f.spans.is_empty()), "fixtures carry spans");
    }

    #[test]
    fn scores_overlap_match_by_category() {
        // gold: one person span 0..18
        let gold = vec![GoldSpan {
            category: "person".into(),
            start: 0,
            end: 18,
        }];
        // predicted: overlapping person span 0..11 (partial overlap, same category)
        let pred = vec![PredSpan {
            category: "person".into(),
            start: 0,
            end: 11,
        }];
        let s = score_one(&gold, &pred);
        assert_eq!(s.get("person").map(|c| c.true_positive), Some(1));
        assert_eq!(s.get("person").map(|c| c.false_negative), Some(0));
        assert_eq!(s.get("person").map(|c| c.false_positive), Some(0));
    }

    #[test]
    fn scores_count_false_positive_and_negative() {
        let gold = vec![GoldSpan {
            category: "person".into(),
            start: 0,
            end: 5,
        }];
        let pred = vec![PredSpan {
            category: "organization".into(),
            start: 10,
            end: 15,
        }];
        let s = score_one(&gold, &pred);
        assert_eq!(s.get("person").map(|c| c.false_negative), Some(1));
        assert_eq!(s.get("organization").map(|c| c.false_positive), Some(1));
    }
}
