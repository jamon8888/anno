//! Relation extraction evaluation metrics.

use crate::types::RelationTriple;
use std::collections::HashSet;

/// Metrics for relation extraction evaluation.
#[derive(Debug, Clone, Default)]
pub struct RelationMetrics {
    /// True positives.
    pub tp: usize,
    /// False positives.
    pub fp: usize,
    /// False negatives.
    pub fn_: usize,
    /// Precision.
    pub precision: f64,
    /// Recall.
    pub recall: f64,
    /// F1 score.
    pub f1: f64,
}

impl RelationMetrics {
    /// Create metrics from counts.
    pub fn from_counts(tp: usize, fp: usize, fn_: usize) -> Self {
        let precision = if tp + fp > 0 {
            tp as f64 / (tp + fp) as f64
        } else {
            0.0
        };
        let recall = if tp + fn_ > 0 {
            tp as f64 / (tp + fn_) as f64
        } else {
            0.0
        };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
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

/// Evaluation mode for relation matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Boundary match: entity spans overlap, relation type matches.
    Boundary,
    /// Strict match: exact entity spans and relation type.
    Strict,
}

/// Evaluate predicted relations against gold standard.
///
/// # Arguments
/// - `predicted`: Predicted relation triples
/// - `gold`: Gold standard triples
/// - `pred_entities`: Predicted entity spans (start, end)
/// - `gold_entities`: Gold entity spans (start, end)
/// - `mode`: Matching mode (Boundary or Strict)
pub fn evaluate_relations(
    predicted: &[RelationTriple],
    gold: &[RelationTriple],
    pred_entities: &[(usize, usize)],
    gold_entities: &[(usize, usize)],
    mode: MatchMode,
) -> RelationMetrics {
    let mut tp = 0;
    let mut matched_gold: HashSet<usize> = HashSet::new();

    for pred in predicted {
        let pred_head = pred_entities.get(pred.head_idx);
        let pred_tail = pred_entities.get(pred.tail_idx);

        if pred_head.is_none() || pred_tail.is_none() {
            continue;
        }

        let pred_head = pred_head.unwrap();
        let pred_tail = pred_tail.unwrap();

        for (gold_idx, gold_triple) in gold.iter().enumerate() {
            if matched_gold.contains(&gold_idx) {
                continue;
            }

            let gold_head = gold_entities.get(gold_triple.head_idx);
            let gold_tail = gold_entities.get(gold_triple.tail_idx);

            if gold_head.is_none() || gold_tail.is_none() {
                continue;
            }

            let gold_head = gold_head.unwrap();
            let gold_tail = gold_tail.unwrap();

            // Check relation type
            if pred.relation != gold_triple.relation {
                continue;
            }

            // Check entity match based on mode
            let head_match = match mode {
                MatchMode::Strict => pred_head == gold_head,
                MatchMode::Boundary => spans_overlap(*pred_head, *gold_head),
            };

            let tail_match = match mode {
                MatchMode::Strict => pred_tail == gold_tail,
                MatchMode::Boundary => spans_overlap(*pred_tail, *gold_tail),
            };

            if head_match && tail_match {
                tp += 1;
                matched_gold.insert(gold_idx);
                break;
            }
        }
    }

    let fp = predicted.len() - tp;
    let fn_ = gold.len() - matched_gold.len();

    RelationMetrics::from_counts(tp, fp, fn_)
}

/// Check if two spans overlap.
fn spans_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Per-relation type metrics.
#[derive(Debug, Clone, Default)]
pub struct PerRelationMetrics {
    /// Metrics per relation type.
    pub by_type: std::collections::HashMap<String, RelationMetrics>,
    /// Macro-averaged metrics.
    pub macro_avg: RelationMetrics,
    /// Micro-averaged metrics.
    pub micro_avg: RelationMetrics,
}

impl PerRelationMetrics {
    /// Compute per-relation metrics.
    pub fn compute(
        predicted: &[RelationTriple],
        gold: &[RelationTriple],
        pred_entities: &[(usize, usize)],
        gold_entities: &[(usize, usize)],
        mode: MatchMode,
    ) -> Self {
        use std::collections::HashMap;

        // Group by relation type
        let mut pred_by_type: HashMap<&str, Vec<&RelationTriple>> = HashMap::new();
        let mut gold_by_type: HashMap<&str, Vec<&RelationTriple>> = HashMap::new();

        for p in predicted {
            pred_by_type.entry(&p.relation).or_default().push(p);
        }
        for g in gold {
            gold_by_type.entry(&g.relation).or_default().push(g);
        }

        // All relation types
        let mut all_types: HashSet<&str> = HashSet::new();
        all_types.extend(pred_by_type.keys());
        all_types.extend(gold_by_type.keys());

        let mut by_type = HashMap::new();
        let mut total_tp = 0;
        let mut total_fp = 0;
        let mut total_fn = 0;

        for rel_type in all_types {
            let pred_for_type: Vec<RelationTriple> = pred_by_type
                .get(rel_type)
                .map(|v| v.iter().cloned().cloned().collect())
                .unwrap_or_default();
            let gold_for_type: Vec<RelationTriple> = gold_by_type
                .get(rel_type)
                .map(|v| v.iter().cloned().cloned().collect())
                .unwrap_or_default();

            let metrics = evaluate_relations(
                &pred_for_type,
                &gold_for_type,
                pred_entities,
                gold_entities,
                mode,
            );

            total_tp += metrics.tp;
            total_fp += metrics.fp;
            total_fn += metrics.fn_;

            by_type.insert(rel_type.to_string(), metrics);
        }

        // Micro average
        let micro_avg = RelationMetrics::from_counts(total_tp, total_fp, total_fn);

        // Macro average
        let num_types = by_type.len().max(1);
        let macro_precision: f64 =
            by_type.values().map(|m| m.precision).sum::<f64>() / num_types as f64;
        let macro_recall: f64 = by_type.values().map(|m| m.recall).sum::<f64>() / num_types as f64;
        let macro_f1 = if macro_precision + macro_recall > 0.0 {
            2.0 * macro_precision * macro_recall / (macro_precision + macro_recall)
        } else {
            0.0
        };

        let macro_avg = RelationMetrics {
            tp: 0, // Not meaningful for macro
            fp: 0,
            fn_: 0,
            precision: macro_precision,
            recall: macro_recall,
            f1: macro_f1,
        };

        Self {
            by_type,
            macro_avg,
            micro_avg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_match() {
        let pred = vec![RelationTriple::new(0, 1, "works_for", 0.9)];
        let gold = vec![RelationTriple::new(0, 1, "works_for", 1.0)];
        let entities = vec![(0, 5), (10, 15)];

        let metrics = evaluate_relations(&pred, &gold, &entities, &entities, MatchMode::Strict);
        assert_eq!(metrics.tp, 1);
        assert_eq!(metrics.fp, 0);
        assert_eq!(metrics.fn_, 0);
        assert!((metrics.f1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_match() {
        let pred = vec![RelationTriple::new(0, 1, "works_for", 0.9)];
        let gold = vec![RelationTriple::new(0, 1, "founded", 1.0)];
        let entities = vec![(0, 5), (10, 15)];

        let metrics = evaluate_relations(&pred, &gold, &entities, &entities, MatchMode::Strict);
        assert_eq!(metrics.tp, 0);
        assert_eq!(metrics.fp, 1);
        assert_eq!(metrics.fn_, 1);
    }

    #[test]
    fn test_boundary_match() {
        let pred = vec![RelationTriple::new(0, 1, "works_for", 0.9)];
        let gold = vec![RelationTriple::new(0, 1, "works_for", 1.0)];
        let pred_entities = vec![(0, 5), (10, 15)];
        let gold_entities = vec![(0, 4), (11, 15)]; // Slightly different

        let strict = evaluate_relations(
            &pred,
            &gold,
            &pred_entities,
            &gold_entities,
            MatchMode::Strict,
        );
        assert_eq!(strict.tp, 0);

        let boundary = evaluate_relations(
            &pred,
            &gold,
            &pred_entities,
            &gold_entities,
            MatchMode::Boundary,
        );
        assert_eq!(boundary.tp, 1);
    }
}
