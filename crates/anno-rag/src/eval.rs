//! Legal RAG evaluation harness: graded-relevance metrics, fixture loader,
//! and a runner that replays queries against a `Pipeline`.

use std::collections::HashMap;

/// recall@k — fraction of relevant docs (grade ≥ 1) present in the top-k
/// ranked docs. Vacuous queries (no relevant docs) score 1.0.
#[must_use]
pub fn recall_at_k(ranked_docs: &[String], relevant: &HashMap<String, u8>, k: usize) -> f64 {
    let rel_total = relevant.values().filter(|&&g| g >= 1).count();
    if rel_total == 0 {
        return 1.0;
    }
    let hits = ranked_docs
        .iter()
        .take(k)
        .filter(|d| relevant.get(*d).is_some_and(|&g| g >= 1))
        .count();
    hits as f64 / rel_total as f64
}

/// nDCG@k over graded relevance (grades 1..=3; absent ⇒ 0). Gain `2^g − 1`,
/// discount `log2(rank + 2)`. Empty ideal DCG scores 1.0.
#[must_use]
pub fn ndcg_at_k(ranked_docs: &[String], relevant: &HashMap<String, u8>, k: usize) -> f64 {
    let dcg: f64 = ranked_docs
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, d)| {
            let g = f64::from(relevant.get(d).copied().unwrap_or(0));
            (2f64.powf(g) - 1.0) / ((i + 2) as f64).log2()
        })
        .sum();
    let mut ideal: Vec<u8> = relevant.values().copied().collect();
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(k)
        .enumerate()
        .map(|(i, &g)| (2f64.powf(f64::from(g)) - 1.0) / ((i + 2) as f64).log2())
        .sum();
    if idcg == 0.0 {
        1.0
    } else {
        dcg / idcg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(pairs: &[(&str, u8)]) -> HashMap<String, u8> {
        pairs.iter().map(|(d, g)| ((*d).to_string(), *g)).collect()
    }

    #[test]
    fn recall_counts_relevant_in_topk() {
        let ranked = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let relevant = rel(&[("a", 3), ("c", 1), ("x", 2)]);
        // top-2 = [a,b]; only `a` is relevant; 3 relevant total ⇒ 1/3.
        assert!((recall_at_k(&ranked, &relevant, 2) - 1.0 / 3.0).abs() < 1e-9);
        // top-4 = [a,b,c,d]; a + c relevant ⇒ 2/3.
        assert!((recall_at_k(&ranked, &relevant, 4) - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn recall_vacuous_query_is_one() {
        let ranked = vec!["a".into()];
        assert!((recall_at_k(&ranked, &HashMap::new(), 10) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ndcg_perfect_ranking_is_one() {
        let ranked = vec!["a".into(), "b".into(), "c".into()];
        let relevant = rel(&[("a", 3), ("b", 2), ("c", 1)]);
        assert!((ndcg_at_k(&ranked, &relevant, 10) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ndcg_reversed_ranking_is_below_one() {
        let ranked = vec!["c".into(), "b".into(), "a".into()];
        let relevant = rel(&[("a", 3), ("b", 2), ("c", 1)]);
        let score = ndcg_at_k(&ranked, &relevant, 10);
        assert!(score < 1.0 && score > 0.0, "got {score}");
    }

    #[test]
    fn ndcg_empty_relevant_is_one() {
        let ranked = vec!["a".into()];
        assert!((ndcg_at_k(&ranked, &HashMap::new(), 10) - 1.0).abs() < 1e-9);
    }
}
