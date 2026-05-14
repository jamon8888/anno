//! Legal RAG evaluation harness: graded-relevance metrics, fixture loader,
//! and a runner that replays queries against a `Pipeline`.

use crate::error::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One graded query: free-text plus the documents considered relevant.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalQuery {
    /// Concept-oriented query text.
    pub text: String,
    /// Relevant documents with graded relevance (grade 1..=3).
    pub relevant: Vec<RelevantDoc>,
}

/// A relevant document reference inside an [`EvalQuery`].
#[derive(Debug, Clone, Deserialize)]
pub struct RelevantDoc {
    /// File name within the eval corpus directory (e.g. `prestation_03.txt`).
    pub doc: String,
    /// Graded relevance: 3 directly addresses, 2 partial, 1 marginal.
    pub grade: u8,
}

/// Parsed `queries.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalQueries {
    /// All queries, keyed by the `[[query]]` array-of-tables.
    #[serde(rename = "query")]
    pub queries: Vec<EvalQuery>,
}

/// Resolve the eval corpus directory. Honours the `ANNO_RAG_EVAL_CORPUS`
/// environment variable (a path to a `{*.txt, queries.toml}` directory);
/// otherwise falls back to the versioned fixtures under the crate.
#[must_use]
pub fn eval_corpus_dir() -> PathBuf {
    if let Ok(p) = std::env::var("ANNO_RAG_EVAL_CORPUS") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval_corpus")
}

/// Load and parse `queries.toml` from `dir`.
///
/// # Errors
/// Returns [`Error::Config`] if the file is missing or malformed.
pub fn load_queries(dir: &Path) -> Result<EvalQueries> {
    let path = dir.join("queries.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("read {}: {e}", path.display())))?;
    toml::from_str(&text).map_err(|e| Error::Config(format!("parse {}: {e}", path.display())))
}

/// Verify every referenced document exists on disk and every grade is 1..=3.
///
/// # Errors
/// Returns [`Error::Config`] listing the first inconsistency found.
pub fn check_queries(dir: &Path, queries: &EvalQueries) -> Result<()> {
    for q in &queries.queries {
        for r in &q.relevant {
            if !(1..=3).contains(&r.grade) {
                return Err(Error::Config(format!(
                    "query {:?}: grade {} for {} out of range 1..=3",
                    q.text, r.grade, r.doc
                )));
            }
            if !dir.join(&r.doc).is_file() {
                return Err(Error::Config(format!(
                    "query {:?}: referenced doc {} not found in {}",
                    q.text,
                    r.doc,
                    dir.display()
                )));
            }
        }
    }
    Ok(())
}

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

    #[test]
    fn parses_queries_toml() {
        let toml = r#"
[[query]]
text = "résiliation anticipée"
relevant = [
  { doc = "a.txt", grade = 3 },
  { doc = "b.txt", grade = 1 },
]
"#;
        let q: EvalQueries = toml::from_str(toml).expect("parse");
        assert_eq!(q.queries.len(), 1);
        assert_eq!(q.queries[0].relevant.len(), 2);
        assert_eq!(q.queries[0].relevant[0].grade, 3);
    }

    #[test]
    fn check_queries_rejects_bad_grade() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        let q = EvalQueries {
            queries: vec![EvalQuery {
                text: "t".into(),
                relevant: vec![RelevantDoc { doc: "a.txt".into(), grade: 9 }],
            }],
        };
        assert!(check_queries(dir.path(), &q).is_err());
    }

    #[test]
    fn check_queries_rejects_missing_doc() {
        let dir = tempfile::tempdir().unwrap();
        let q = EvalQueries {
            queries: vec![EvalQuery {
                text: "t".into(),
                relevant: vec![RelevantDoc { doc: "missing.txt".into(), grade: 2 }],
            }],
        };
        assert!(check_queries(dir.path(), &q).is_err());
    }
}
