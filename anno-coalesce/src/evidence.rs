//! # Evidence-Based Confidence Mediation
//!
//! This module provides types for accumulating and combining evidence
//! from multiple sources when making coreference decisions.
//!
//! ## Historical Context
//!
//! The challenge of combining conflicting coreference evidence has deep roots:
//!
//! - **Kehler (1997)** first formalized probabilistic coreference for IE systems,
//!   showing that pairwise evidence often conflicts when extended to larger sets.
//!   His use of **Dempster-Shafer theory** to resolve conflicts anticipated modern
//!   evidence aggregation approaches.
//!
//! - **Dempster (1968)** introduced the rule for combining belief functions from
//!   independent sources. The key insight: when sources disagree, normalize away
//!   the conflicting mass rather than averaging. This handles "one source says yes,
//!   one says no" more gracefully than naive combination.
//!
//! - **Maximum entropy models** (Berger et al. 1996) — used by Kehler for pairwise
//!   probabilities — find the least-committed distribution consistent with feature
//!   constraints. This principle underlies modern log-linear and neural models.
//!
//! - **Correlation belief functions** (2023) extend Dempster-Shafer to handle
//!   correlated evidence, resolving counterintuitive fusion results.
//!
//! ## The Problem
//!
//! In entity resolution, we often have multiple signals:
//! - String similarity (Jaccard, Jaro-Winkler, trigrams)
//! - Embedding similarity (cosine distance)
//! - Type matching (both are PERSON?)
//! - Knowledge base linkage (same Wikidata ID?)
//! - Contextual coreference (ML model prediction)
//!
//! These signals may disagree. How do we combine them?
//!
//! Kehler's key observation: pairwise probabilities can be **globally inconsistent**.
//! If P(A~D)=0.505 and P(C~D)=0.504, but A and C are incompatible, then we can't
//! have both be true. Naive combination ignores this; evidential reasoning addresses it.
//!
//! ## Type-Theoretic Perspective (Curry-Howard)
//!
//! Evidence combination has a natural logical interpretation:
//!
//! ```text
//! EvidenceSource ≈ Atomic proof: "source S says P(A~B) = x"
//! PairEvidence   ≈ Conjunction: "all these sources provide evidence"
//! MediationStrategy ≈ Proof combinator: "given evidence, derive conclusion"
//! ```
//!
//! Each `EvidenceSource` is a **witness** from an independent proof system:
//! - `StringSimilarity`: Syntactic evidence (edit distance proof)
//! - `Embedding`: Semantic evidence (vector space proof)
//! - `TypeMatch`: Ontological evidence (category proof)
//! - `KnowledgeBase`: External authority (KB entailment proof)
//!
//! The mediation strategy determines how to **combine proofs**:
//! - `Average`: Equal weight to all proof systems
//! - `Bayesian`: Treat each as likelihood ratio, combine via Bayes
//! - `Product`: Require all proofs to agree (conjunction)
//! - `Max`: Any single strong proof suffices (disjunction)
//!
//! ## Evidence Accumulation
//!
//! Instead of taking max or average, we accumulate **evidence** for and
//! against coreference, weighted by source reliability:
//!
//! ```text
//! P(coref | evidence) ∝ Σᵢ wᵢ · scoreᵢ · reliabilityᵢ
//! ```
//!
//! ## Modern Descendants
//!
//! This approach evolved into several research directions:
//!
//! - **Mention-ranking** (Clark & Manning 2016): Model P(antecedent | mention)
//!   directly, avoiding explicit configuration enumeration
//!
//! - **End-to-end coref** (Lee et al. 2017): Learn span representations and
//!   antecedent scores jointly, with cluster-level features
//!
//! - **Triad networks** (Meng & Rumshisky 2018): Score mention triples to capture
//!   transitivity constraints that pairwise models miss
//!
//! - **Bayesian entity resolution** (Steorts 2014): Partition priors (Ewens-Pitman)
//!   with likelihood models for record comparisons
//!
//! ## References
//!
//! - Kehler (1997). "Probabilistic Coreference in Information Extraction". ACL.
//! - Dempster (1968). "A Generalization of Bayesian Inference". JRSS.
//! - Berger, Della Pietra, Della Pietra (1996). "A Maximum Entropy Approach to NLP".
//! - Clark & Manning (2016). "Deep Reinforcement Learning for Mention-Ranking". EMNLP.
//! - Lee et al. (2017). "End-to-End Neural Coreference Resolution". EMNLP.
//! - Steorts (2014). "Entity Resolution with Empirically Motivated Priors".
//!
//! ## Example
//!
//! ```
//! use anno_coalesce::evidence::{PairEvidence, EvidenceSource, MediationStrategy};
//!
//! let mut evidence = PairEvidence::new();
//! evidence.add_source(EvidenceSource::StringSimilarity {
//!     method: "trigram".into(),
//!     score: 0.85,
//! });
//! evidence.add_source(EvidenceSource::Embedding {
//!     model: "all-MiniLM-L6-v2".into(),
//!     score: 0.72,
//! });
//! evidence.add_source(EvidenceSource::TypeMatch {
//!     matched: true,
//!     type_a: "PERSON".into(),
//!     type_b: "PER".into(),
//! });
//!
//! let score = evidence.mediate(&MediationStrategy::default());
//! assert!(score > 0.5);
//! ```

use std::collections::HashMap;

/// Source of evidence for a coreference decision.
#[derive(Debug, Clone)]
pub enum EvidenceSource {
    /// String-based similarity measurement
    StringSimilarity {
        /// Method used (jaccard, jaro_winkler, trigram, etc.)
        method: String,
        /// Similarity score in [0, 1]
        score: f32,
    },

    /// Embedding-based similarity
    Embedding {
        /// Model that produced the embeddings
        model: String,
        /// Cosine similarity in [-1, 1], normalized to [0, 1]
        score: f32,
    },

    /// Entity type matching
    TypeMatch {
        /// Whether the types matched (exact or compatible)
        matched: bool,
        /// Type of entity A
        type_a: String,
        /// Type of entity B
        type_b: String,
    },

    /// Knowledge base linkage
    KnowledgeBase {
        /// If both entities link to a KB, what's the ID?
        kb_id: Option<String>,
        /// Whether they link to the same entity
        linked: bool,
    },

    /// Contextual coreference model prediction
    ContextualCoref {
        /// Model name
        model: String,
        /// Coreference probability
        score: f32,
    },

    /// Explicit negative evidence (blocking signals)
    NegativeEvidence {
        /// Reason for blocking
        reason: String,
        /// Confidence that this is a blocker
        confidence: f32,
    },

    /// Custom evidence source
    Custom {
        /// Source identifier
        source: String,
        /// Score in [0, 1]
        score: f32,
        /// Additional metadata
        metadata: HashMap<String, String>,
    },

    /// Temporal consistency for diachronic entities.
    ///
    /// Used to check if entities could co-exist at the same time period.
    /// E.g., "USSR" and "Russia" have non-overlapping validity periods.
    TemporalConsistency {
        /// Score: 1.0 if consistent, 0.0 if anachronistic, 0.5 if uncertain
        score: f32,
        /// Whether the mention has temporal bounds
        mention_valid: bool,
        /// Whether the cluster has temporal bounds
        cluster_valid: bool,
    },

    /// Acronym expansion match.
    ///
    /// Detects when one string is the acronym of another:
    /// - "WHO" ↔ "World Health Organization"
    /// - "MRSA" ↔ "Methicillin-resistant Staphylococcus aureus"
    ///
    /// This is language-agnostic: the algorithm checks if the short form
    /// consists of the first letters of words in the long form.
    Acronym {
        /// The short form (potential acronym)
        short_form: String,
        /// The long form (potential expansion)
        long_form: String,
        /// Whether the match was successful
        matched: bool,
    },

    /// Synonym relationship between entities.
    ///
    /// Evidence that two surface forms are synonymous, based on:
    /// - Knowledge base lookups (UMLS, WordNet, Wikidata aliases)
    /// - Custom synonym tables
    /// - Cross-lingual equivalent detection
    ///
    /// Unlike string similarity, synonyms may have very different surface forms:
    /// - "heart attack" ↔ "myocardial infarction"
    /// - "USA" ↔ "United States" ↔ "America"
    Synonym {
        /// Source of the synonym relationship (e.g., "umls", "wordnet", "custom")
        source: String,
        /// Confidence in the synonym relationship [0, 1]
        confidence: f32,
        /// The canonical form if available (e.g., UMLS CUI)
        canonical_id: Option<String>,
    },
}

impl EvidenceSource {
    /// Get the score contribution of this evidence source.
    ///
    /// Returns a value in [-1, 1]:
    /// - Positive: evidence FOR coreference
    /// - Negative: evidence AGAINST coreference
    /// - Zero: no evidence either way
    pub fn score_contribution(&self) -> f32 {
        match self {
            Self::StringSimilarity { score, .. } => {
                // Map [0, 1] to [-1, 1] centered at 0.5
                2.0 * score - 1.0
            }
            Self::Embedding { score, .. } => {
                // Embeddings already in meaningful range
                2.0 * score - 1.0
            }
            Self::TypeMatch { matched, .. } => {
                if *matched {
                    0.5
                } else {
                    -0.5
                }
            }
            Self::KnowledgeBase { linked, .. } => {
                if *linked {
                    1.0
                } else {
                    0.0
                } // Strong positive if linked, neutral otherwise
            }
            Self::ContextualCoref { score, .. } => 2.0 * score - 1.0,
            Self::NegativeEvidence { confidence, .. } => {
                -confidence // Always negative
            }
            Self::Custom { score, .. } => 2.0 * score - 1.0,
            Self::TemporalConsistency { score, .. } => {
                // 1.0 (consistent) → +1.0, 0.0 (anachronistic) → -1.0, 0.5 (uncertain) → 0.0
                2.0 * score - 1.0
            }
            Self::Acronym { matched, .. } => {
                // Strong positive if matched, neutral otherwise
                if *matched {
                    0.8 // High confidence when acronym matches
                } else {
                    0.0
                }
            }
            Self::Synonym { confidence, .. } => {
                // Synonyms are strong positive evidence
                // Map [0, 1] confidence to [0, 1] contribution
                *confidence
            }
        }
    }

    /// Get the name of this evidence source for weighting.
    pub fn source_name(&self) -> &str {
        match self {
            Self::StringSimilarity { method, .. } => method,
            Self::Embedding { model, .. } => model,
            Self::TypeMatch { .. } => "type_match",
            Self::KnowledgeBase { .. } => "knowledge_base",
            Self::ContextualCoref { model, .. } => model,
            Self::NegativeEvidence { reason, .. } => reason,
            Self::Custom { source, .. } => source,
            Self::TemporalConsistency { .. } => "temporal_consistency",
            Self::Acronym { .. } => "acronym",
            Self::Synonym { source, .. } => source,
        }
    }
}

/// Accumulated evidence for a potential coreference link.
#[derive(Debug, Clone, Default)]
pub struct PairEvidence {
    /// All evidence sources
    pub sources: Vec<EvidenceSource>,
    /// Cached positive evidence sum
    positive_cache: Option<f32>,
    /// Cached negative evidence sum
    negative_cache: Option<f32>,
}

impl PairEvidence {
    /// Create empty evidence container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an evidence source.
    pub fn add_source(&mut self, source: EvidenceSource) {
        self.sources.push(source);
        // Invalidate caches
        self.positive_cache = None;
        self.negative_cache = None;
    }

    /// Get total positive evidence.
    pub fn positive(&self) -> f32 {
        if let Some(cached) = self.positive_cache {
            return cached;
        }

        self.sources
            .iter()
            .map(|s| s.score_contribution())
            .filter(|&s| s > 0.0)
            .sum()
    }

    /// Get total negative evidence.
    pub fn negative(&self) -> f32 {
        if let Some(cached) = self.negative_cache {
            return cached;
        }

        self.sources
            .iter()
            .map(|s| s.score_contribution())
            .filter(|&s| s < 0.0)
            .map(|s| -s) // Make positive for summing
            .sum()
    }

    /// Get net evidence (positive - negative).
    pub fn net(&self) -> f32 {
        self.positive() - self.negative()
    }

    /// Combine evidence using a mediation strategy.
    pub fn mediate(&self, strategy: &MediationStrategy) -> f32 {
        strategy.combine(self)
    }

    /// Check if there's any blocking evidence.
    pub fn has_blocker(&self) -> bool {
        self.sources.iter().any(|s| matches!(s, EvidenceSource::NegativeEvidence { confidence, .. } if *confidence > 0.9))
    }

    /// Get evidence breakdown by source type.
    pub fn by_source(&self) -> HashMap<&str, f32> {
        let mut result = HashMap::new();
        for source in &self.sources {
            let name = source.source_name();
            *result.entry(name).or_insert(0.0) += source.score_contribution();
        }
        result
    }
}

/// Strategy for combining multiple evidence sources.
///
/// These strategies reflect different assumptions about how evidence sources relate:
///
/// | Strategy | Assumption | When to Use |
/// |----------|------------|-------------|
/// | Average | Sources equally reliable, errors cancel | Homogeneous sources |
/// | Voting | Count matters more than magnitude | Many weak signals |
/// | SourceWeighted | Some sources more reliable | Heterogeneous sources |
/// | Bayesian | Sources provide likelihood ratios | Well-calibrated scores |
/// | Max | One strong signal sufficient | High-precision sources |
/// | Min | Need agreement from all | High-recall requirement |
/// | Product | Independence assumption | Diverse, uncorrelated sources |
///
/// ## Historical Note
///
/// Kehler (1997) used Dempster's Rule for combining pairwise evidence, which is
/// closest to `Bayesian` here but operates on mass functions rather than
/// probabilities directly. The key insight: when evidence conflicts (P(A~B)
/// suggests merge, P(A~C) suggests don't, but B~C), normalize away the
/// conflicting mass rather than averaging.
///
/// Modern neural systems typically use learned combination (attention, MLP over
/// concatenated features) rather than fixed strategies. The `Custom` evidence
/// source can wrap such learned scores.
#[derive(Debug, Clone)]
pub enum MediationStrategy {
    /// Simple average of all scores.
    ///
    /// Assumes errors are random and cancel out. Works well when sources have
    /// similar reliability and independent errors.
    Average,

    /// Majority voting (count positive vs negative sources).
    ///
    /// Ignores magnitude, only counts direction. Robust to outliers but loses
    /// information from confident scores.
    Voting,

    /// Weighted by source reliability.
    ///
    /// The most flexible strategy. Weights can be learned from held-out data
    /// or set based on domain knowledge. Kehler's maximum entropy model
    /// implicitly learns such weights.
    SourceWeighted {
        /// Weights per source name (default 1.0 if not specified)
        weights: HashMap<String, f32>,
        /// Default weight for unknown sources
        default_weight: f32,
    },

    /// Bayesian combination with prior.
    ///
    /// Treats scores as log-likelihood ratios and combines via addition in
    /// log-odds space. The prior represents base rate of coreference.
    ///
    /// This is closest to Kehler's evidential approach when sources are
    /// well-calibrated (scores reflect true probabilities).
    Bayesian {
        /// Prior probability of coreference
        prior: f32,
    },

    /// Maximum confidence (optimistic).
    ///
    /// "One strong signal is enough." Use when false negatives are costly
    /// and you have high-precision sources.
    Max,

    /// Minimum confidence (conservative).
    ///
    /// "Need agreement from all." Use when false positives are costly
    /// (e.g., merging records that shouldn't be merged is hard to undo).
    Min,

    /// Product of normalized scores (requires all signals to agree).
    ///
    /// Assumes independence: P(coref | all evidence) ∝ ∏ P(coref | source_i).
    /// Very conservative — one low score drags everything down.
    Product,
}

impl Default for MediationStrategy {
    fn default() -> Self {
        // Source-weighted with reasonable defaults
        let mut weights = HashMap::new();
        weights.insert("knowledge_base".into(), 2.0); // KB links are strong
        weights.insert("type_match".into(), 0.5); // Type is weak signal
        weights.insert("trigram".into(), 1.0);
        weights.insert("jaccard".into(), 0.8);

        Self::SourceWeighted {
            weights,
            default_weight: 1.0,
        }
    }
}

impl MediationStrategy {
    /// Combine evidence using this strategy.
    pub fn combine(&self, evidence: &PairEvidence) -> f32 {
        if evidence.sources.is_empty() {
            return 0.5; // No evidence → neutral
        }

        // Check for blockers first
        if evidence.has_blocker() {
            return 0.0;
        }

        match self {
            Self::Average => {
                let sum: f32 = evidence
                    .sources
                    .iter()
                    .map(|s| s.score_contribution())
                    .sum();
                let avg = sum / evidence.sources.len() as f32;
                // Map [-1, 1] back to [0, 1]
                (avg + 1.0) / 2.0
            }

            Self::Voting => {
                let positive_count = evidence
                    .sources
                    .iter()
                    .filter(|s| s.score_contribution() > 0.0)
                    .count();
                let negative_count = evidence
                    .sources
                    .iter()
                    .filter(|s| s.score_contribution() < 0.0)
                    .count();

                if positive_count + negative_count == 0 {
                    0.5
                } else {
                    positive_count as f32 / (positive_count + negative_count) as f32
                }
            }

            Self::SourceWeighted {
                weights,
                default_weight,
            } => {
                let mut weighted_sum = 0.0;
                let mut total_weight = 0.0;

                for source in &evidence.sources {
                    let weight = weights
                        .get(source.source_name())
                        .copied()
                        .unwrap_or(*default_weight);
                    weighted_sum += weight * source.score_contribution();
                    total_weight += weight;
                }

                if total_weight == 0.0 {
                    0.5
                } else {
                    let avg = weighted_sum / total_weight;
                    (avg + 1.0) / 2.0
                }
            }

            Self::Bayesian { prior } => {
                // Simple Bayesian update
                // P(coref | evidence) ∝ P(evidence | coref) * P(coref)
                // We approximate P(evidence | coref) as product of individual likelihoods

                let mut log_likelihood_ratio = 0.0;
                for source in &evidence.sources {
                    let score = source.score_contribution();
                    // Convert to likelihood ratio
                    // score > 0 → more likely coref
                    // score < 0 → less likely coref
                    if score.abs() > 0.01 {
                        log_likelihood_ratio += score;
                    }
                }

                // Apply prior using logit transformation
                let prior_logit = (prior / (1.0 - prior)).ln();
                let posterior_logit = prior_logit + log_likelihood_ratio;

                // Convert back to probability
                1.0 / (1.0 + (-posterior_logit).exp())
            }

            Self::Max => evidence
                .sources
                .iter()
                .map(|s| s.score_contribution())
                .fold(f32::NEG_INFINITY, f32::max)
                .clamp(-1.0, 1.0)
                .pipe(|x| (x + 1.0) / 2.0),

            Self::Min => evidence
                .sources
                .iter()
                .map(|s| s.score_contribution())
                .fold(f32::INFINITY, f32::min)
                .clamp(-1.0, 1.0)
                .pipe(|x| (x + 1.0) / 2.0),

            Self::Product => {
                // Normalize scores to [0.1, 0.9] to avoid zeros
                let product: f32 = evidence
                    .sources
                    .iter()
                    .map(|s| {
                        let score = (s.score_contribution() + 1.0) / 2.0; // [0, 1]
                        score.clamp(0.1, 0.9)
                    })
                    .product();

                // Geometric mean
                product.powf(1.0 / evidence.sources.len() as f32)
            }
        }
    }

    /// Create a source-weighted strategy with custom weights.
    pub fn weighted(weights: impl IntoIterator<Item = (String, f32)>) -> Self {
        Self::SourceWeighted {
            weights: weights.into_iter().collect(),
            default_weight: 1.0,
        }
    }
}

// Helper trait for pipe syntax
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl Pipe for f32 {}

// =============================================================================
// Transitivity Analysis
// =============================================================================

/// Analyzer for detecting transitivity violations in similarity judgments.
#[derive(Debug, Clone)]
pub struct TransitivityAnalyzer {
    /// Number of items
    n: usize,
    /// Pairwise similarities (upper triangular)
    similarities: Vec<f32>,
}

/// A transitivity violation: sim(a,b) and sim(b,c) are high but sim(a,c) is low.
#[derive(Debug, Clone)]
pub struct TransitivityViolation {
    /// First item
    pub a: usize,
    /// Bridge item
    pub b: usize,
    /// Third item
    pub c: usize,
    /// Similarity between a and b
    pub sim_ab: f32,
    /// Similarity between b and c
    pub sim_bc: f32,
    /// Similarity between a and c (unexpectedly low)
    pub sim_ac: f32,
    /// Severity: how bad is the violation?
    pub severity: f32,
}

impl TransitivityAnalyzer {
    /// Create from a similarity matrix.
    pub fn from_matrix(sims: &[Vec<f32>]) -> Self {
        let n = sims.len();
        let mut similarities = vec![0.0; n * (n - 1) / 2];
        let mut idx = 0;
        for (i, row) in sims.iter().enumerate() {
            for &sim in row.iter().skip(i + 1) {
                similarities[idx] = sim;
                idx += 1;
            }
        }
        Self { n, similarities }
    }

    /// Get similarity between items i and j.
    fn get_sim(&self, i: usize, j: usize) -> f32 {
        if i == j {
            return 1.0;
        }
        let (i, j) = if i < j { (i, j) } else { (j, i) };
        let idx = i * (2 * self.n - i - 1) / 2 + (j - i - 1);
        self.similarities.get(idx).copied().unwrap_or(0.0)
    }

    /// Find all triangles where transitivity is violated.
    pub fn find_violations(&self, threshold: f32) -> Vec<TransitivityViolation> {
        let mut violations = Vec::new();

        for a in 0..self.n {
            for b in (a + 1)..self.n {
                let sim_ab = self.get_sim(a, b);
                if sim_ab < threshold {
                    continue;
                }

                for c in (b + 1)..self.n {
                    let sim_bc = self.get_sim(b, c);
                    if sim_bc < threshold {
                        continue;
                    }

                    let sim_ac = self.get_sim(a, c);

                    // Violation: a~b and b~c but not a~c
                    if sim_ac < threshold {
                        let expected_min = (sim_ab * sim_bc).sqrt(); // Geometric mean as baseline
                        let severity = expected_min - sim_ac;

                        if severity > 0.1 {
                            // Only report significant violations
                            violations.push(TransitivityViolation {
                                a,
                                b,
                                c,
                                sim_ab,
                                sim_bc,
                                sim_ac,
                                severity,
                            });
                        }
                    }
                }
            }
        }

        violations.sort_by(|x, y| {
            y.severity
                .partial_cmp(&x.severity)
                .expect("severities should be comparable")
        });
        violations
    }

    /// Score how much a clustering respects transitivity.
    ///
    /// Returns a score in [0, 1] where 1 is perfect transitivity.
    pub fn transitivity_score(&self, clusters: &[Vec<usize>]) -> f32 {
        let mut violations = 0;
        let mut total_triangles = 0;

        for cluster in clusters {
            // Check all triangles within each cluster
            for (i, &a) in cluster.iter().enumerate() {
                for (j, &b) in cluster.iter().enumerate().skip(i + 1) {
                    for &c in cluster.iter().skip(j + 1) {
                        total_triangles += 1;
                        let sim_ab = self.get_sim(a, b);
                        let sim_bc = self.get_sim(b, c);
                        let sim_ac = self.get_sim(a, c);

                        // Check triangle inequality
                        // If all are in same cluster, all similarities should be "high"
                        let min_sim = sim_ab.min(sim_bc).min(sim_ac);
                        if min_sim < 0.3 {
                            // Violation: one edge is weak
                            violations += 1;
                        }
                    }
                }
            }
        }

        if total_triangles == 0 {
            1.0
        } else {
            1.0 - (violations as f32 / total_triangles as f32)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_accumulation() {
        let mut evidence = PairEvidence::new();
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "trigram".into(),
            score: 0.9,
        });
        evidence.add_source(EvidenceSource::TypeMatch {
            matched: true,
            type_a: "PER".into(),
            type_b: "PERSON".into(),
        });

        assert!(evidence.positive() > 0.0);
        assert!(evidence.net() > 0.0);
    }

    #[test]
    fn test_voting_strategy() {
        let mut evidence = PairEvidence::new();
        // 3 positive signals
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "a".into(),
            score: 0.8,
        });
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "b".into(),
            score: 0.7,
        });
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "c".into(),
            score: 0.6,
        });
        // 1 negative signal
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "d".into(),
            score: 0.2,
        });

        let score = evidence.mediate(&MediationStrategy::Voting);
        assert!((score - 0.75).abs() < 0.01); // 3 of 4 positive
    }

    #[test]
    fn test_blocker_evidence() {
        let mut evidence = PairEvidence::new();
        evidence.add_source(EvidenceSource::StringSimilarity {
            method: "a".into(),
            score: 0.9,
        });
        evidence.add_source(EvidenceSource::NegativeEvidence {
            reason: "different_wikidata_id".into(),
            confidence: 0.95,
        });

        assert!(evidence.has_blocker());
        assert!(evidence.mediate(&MediationStrategy::default()) < 0.1);
    }

    #[test]
    fn test_transitivity_detection() {
        // Create similarity matrix with a violation
        // a~b (0.9), b~c (0.9), but a~c (0.2)
        let sims = vec![
            vec![1.0, 0.9, 0.2],
            vec![0.9, 1.0, 0.9],
            vec![0.2, 0.9, 1.0],
        ];

        let analyzer = TransitivityAnalyzer::from_matrix(&sims);
        let violations = analyzer.find_violations(0.5);

        assert!(!violations.is_empty());
        assert_eq!(violations[0].a, 0);
        assert_eq!(violations[0].b, 1);
        assert_eq!(violations[0].c, 2);
    }
}
