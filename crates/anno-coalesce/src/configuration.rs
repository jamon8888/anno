//! # Probabilistic Coreference Configurations
//!
//! This module provides types for representing probability distributions over
//! *coreference configurations* — the set of all possible partitions of mentions.
//!
//! ## Type-Theoretic Perspective (Curry-Howard)
//!
//! Coreference configurations have a natural logical interpretation:
//!
//! ```text
//! Configuration = Partition of mentions into clusters
//!               ≈ Conjunction of cluster memberships
//!               ≈ ∧ᵢ (mention_i ∈ cluster_j)
//! ```
//!
//! A `CorefConfiguration` is a **proof** that a specific clustering is valid:
//! - Each mention appears in exactly one cell (partition property)
//! - Incompatibility constraints are satisfied
//!
//! A `ConfigurationDistribution` is a **weighted disjunction** over configurations:
//!
//! ```text
//! Distribution ≈ Σᵢ wᵢ · Configᵢ  (weighted sum of alternatives)
//! ```
//!
//! This differs from a simple `Vec<Configuration>` (unweighted disjunction) by
//! attaching credence to each alternative—essential for downstream data fusion.
//!
//! ## Logical Connectives in Coreference
//!
//! | Concept | Logical Form | Implementation |
//! |---------|--------------|----------------|
//! | "A and B corefer" | Atomic relation | Two mentions in same cell |
//! | "A, B, C all corefer" | Conjunction | All in same cell |
//! | "Either config₁ or config₂" | Disjunction | Multiple entries in distribution |
//! | "A and C are incompatible" | Negation constraint | Prune configs where A~C |
//!
//! ## Historical Context
//!
//! The idea of treating coreference as a distribution over configurations rather
//! than point estimates dates to Kehler (1997), "Probabilistic Coreference in
//! Information Extraction". Kehler observed that:
//!
//! > "The merging phase is where most of the ambiguities (as well as most of the
//! > errors) lie. However, most IE systems... have pursued a deterministic strategy
//! > for merging and report only a single possible state of affairs."
//!
//! This creates problems for downstream systems that need to fuse information from
//! multiple sources with varying reliability. A system that assigns P=0.9 to the
//! correct configuration is more useful than one that assigns P=0.6, even if both
//! select the same most-likely answer.
//!
//! ## The Configuration Space
//!
//! Given n mentions, the number of possible partitions is the n-th Bell number:
//!
//! | n | Bell(n) | Example partitions of {A,B,C} |
//! |---|---------|-------------------------------|
//! | 1 | 1       | {A} |
//! | 2 | 2       | {A,B}, {A}{B} |
//! | 3 | 5       | {A,B,C}, {A,B}{C}, {A,C}{B}, {A}{B,C}, {A}{B}{C} |
//! | 4 | 15      | ... |
//! | 5 | 52      | ... |
//! | 10| 115,975 | ... |
//!
//! This combinatorial explosion means we cannot enumerate all configurations for
//! large mention sets. Instead, we use:
//!
//! 1. **Pruning via incompatibility constraints** — if A and C cannot corefer
//!    (e.g., different entity types), eliminate configurations where they share a cell
//!
//! 2. **Pairwise-to-configuration inference** — estimate P(config) from P(A~B), P(A~C), ...
//!    using evidential reasoning (Dempster-Shafer) or decision modeling
//!
//! 3. **Beam search** — maintain only the top-k most probable configurations
//!
//! ## Evolution of the Field
//!
//! Kehler's two approaches anticipated major research directions:
//!
//! - **Evidential approach** → Bayesian entity resolution (Steorts 2014), correlation
//!   clustering with LP relaxations, and modern probabilistic record linkage
//!
//! - **Merging decision approach** → mention-ranking models (Clark & Manning 2016),
//!   which model the sequence of antecedent decisions rather than all pairwise probs
//!
//! Modern neural systems (Lee et al. 2017, "End-to-End Neural Coreference Resolution")
//! learn P(mention_j is antecedent of mention_i) directly, then use greedy clustering.
//! This loses configuration-level uncertainty, though some work (e.g., reinforcement
//! learning for coref) optimizes partition-level metrics directly.
//!
//! The "triad" approach (Meng & Rumshisky 2018) addresses Kehler's insight about
//! pairwise inconsistency by scoring mention triples jointly, capturing transitivity
//! constraints that pairwise models miss.
//!
//! ## References
//!
//! - Kehler (1997). "Probabilistic Coreference in Information Extraction". ACL.
//! - Steorts (2014). "Entity Resolution with Empirically Motivated Priors". Bayesian Analysis.
//! - Clark & Manning (2016). "Deep Reinforcement Learning for Mention-Ranking". EMNLP.
//! - Lee, He, Lewis, Zettlemoyer (2017). "End-to-End Neural Coreference Resolution". EMNLP.
//! - Meng & Rumshisky (2018). "Triad-based Neural Network for Coreference Resolution". COLING.
//! - Rahman & Ng (2014). "Narrowing the Modeling Gap: A Cluster-Ranking Approach".

use std::collections::{HashMap, HashSet};

/// A coreference configuration: a partition of mentions into clusters.
///
/// Each cluster (cell) contains mention indices that are claimed to corefer.
/// The configuration is valid if every mention appears in exactly one cell.
///
/// # Example
///
/// For mentions {A, B, C, D} where A=0, B=1, C=2, D=3:
/// - Configuration `{{0,1,3}, {2}}` means A, B, D corefer; C is separate
/// - Configuration `{{0,1}, {2,3}}` means A~B and C~D but not across
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorefConfiguration {
    /// Cells of the partition, each containing mention indices.
    /// Cells are sorted by their minimum element for canonical ordering.
    pub cells: Vec<Vec<usize>>,
    /// Total number of mentions in this configuration.
    pub n_mentions: usize,
}

impl CorefConfiguration {
    /// Create a configuration from cells.
    ///
    /// Normalizes by sorting cells and their contents for canonical form.
    pub fn new(mut cells: Vec<Vec<usize>>) -> Self {
        let n_mentions = cells.iter().map(|c| c.len()).sum();

        // Sort within cells and sort cells by minimum element
        for cell in &mut cells {
            cell.sort_unstable();
        }
        cells.sort_by_key(|c| c.first().copied().unwrap_or(usize::MAX));

        Self { cells, n_mentions }
    }

    /// Create the "all separate" configuration (each mention in its own cell).
    pub fn all_singletons(n: usize) -> Self {
        let cells: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
        Self {
            cells,
            n_mentions: n,
        }
    }

    /// Create the "all same" configuration (all mentions in one cell).
    pub fn all_same(n: usize) -> Self {
        Self {
            cells: vec![(0..n).collect()],
            n_mentions: n,
        }
    }

    /// Check if two mentions are in the same cell.
    pub fn same_cell(&self, i: usize, j: usize) -> bool {
        self.cells
            .iter()
            .any(|cell| cell.contains(&i) && cell.contains(&j))
    }

    /// Number of non-singleton cells.
    pub fn num_non_singletons(&self) -> usize {
        self.cells.iter().filter(|c| c.len() > 1).count()
    }

    /// Check if this configuration is valid (partition property).
    pub fn is_valid(&self) -> bool {
        let mut seen = HashSet::new();
        for cell in &self.cells {
            for &i in cell {
                if !seen.insert(i) {
                    return false; // Duplicate
                }
            }
        }
        seen.len() == self.n_mentions
    }

    /// Convert to a compact string representation.
    ///
    /// Example: `(0 1 3)(2)` for `{{0,1,3}, {2}}`
    pub fn to_compact_string(&self) -> String {
        self.cells
            .iter()
            .map(|cell| {
                let inner: String = cell
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("({})", inner)
            })
            .collect()
    }
}

impl std::fmt::Display for CorefConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_compact_string())
    }
}

/// A probability distribution over coreference configurations.
///
/// This is the key type for probabilistic coreference resolution à la Kehler (1997).
/// Instead of committing to a single partition, we maintain beliefs over alternatives.
///
/// # Kehler's Insight
///
/// From the 1997 paper:
///
/// > "A system that assigns a probability of 0.9 to correct answers is more successful
/// > than one that assigns a probability of 0.6 to them."
///
/// This matters for:
/// - **Data fusion**: Downstream systems can weigh coref output against other sources
/// - **Active learning**: Query humans about high-uncertainty configurations
/// - **Interpretability**: Explain why mentions were/weren't merged
///
/// # Representation
///
/// We store only configurations with non-negligible probability. A `smoothing_mass`
/// covers the (potentially huge) set of unrepresented configurations.
#[derive(Debug, Clone)]
pub struct ConfigurationDistribution {
    /// Map from configuration to probability mass.
    probabilities: HashMap<CorefConfiguration, f64>,
    /// Mass assigned uniformly to all unrepresented configurations.
    /// This provides smoothing per Kehler's recommendation.
    smoothing_mass: f64,
    /// Number of mentions (defines the configuration space).
    n_mentions: usize,
    /// Incompatibility constraints: pairs that cannot corefer.
    /// Configurations violating these have zero probability.
    incompatible_pairs: HashSet<(usize, usize)>,
}

impl ConfigurationDistribution {
    /// Create a new distribution with uniform smoothing.
    pub fn new(n_mentions: usize) -> Self {
        Self {
            probabilities: HashMap::new(),
            smoothing_mass: 0.0,
            n_mentions,
            incompatible_pairs: HashSet::new(),
        }
    }

    /// Add an incompatibility constraint: mentions i and j cannot corefer.
    ///
    /// This is crucial for making the configuration space tractable.
    /// Kehler used type incompatibility (e.g., "Ammunition" vs "Rail" depot types).
    pub fn add_incompatibility(&mut self, i: usize, j: usize) {
        let pair = if i < j { (i, j) } else { (j, i) };
        self.incompatible_pairs.insert(pair);
    }

    /// Check if a configuration respects all incompatibility constraints.
    pub fn is_compatible(&self, config: &CorefConfiguration) -> bool {
        for &(i, j) in &self.incompatible_pairs {
            if config.same_cell(i, j) {
                return false;
            }
        }
        true
    }

    /// Set probability for a configuration (unnormalized).
    pub fn set_prob(&mut self, config: CorefConfiguration, prob: f64) {
        if prob > 0.0 && self.is_compatible(&config) {
            self.probabilities.insert(config, prob);
        }
    }

    /// Set the smoothing mass for unrepresented configurations.
    pub fn set_smoothing(&mut self, mass: f64) {
        self.smoothing_mass = mass;
    }

    /// Normalize to a proper probability distribution.
    pub fn normalize(&mut self) {
        let total: f64 = self.probabilities.values().sum::<f64>() + self.smoothing_mass;
        if total > 0.0 {
            for prob in self.probabilities.values_mut() {
                *prob /= total;
            }
            self.smoothing_mass /= total;
        }
    }

    /// Get probability of a specific configuration.
    pub fn prob(&self, config: &CorefConfiguration) -> f64 {
        if !self.is_compatible(config) {
            return 0.0;
        }
        self.probabilities.get(config).copied().unwrap_or_else(|| {
            // Smoothing: uniform over unrepresented compatible configs
            // This is a simplification; exact count requires Bell number calculation
            if self.smoothing_mass > 0.0 {
                self.smoothing_mass / self.estimate_unrepresented_count() as f64
            } else {
                0.0
            }
        })
    }

    /// Get the most probable configuration.
    pub fn mode(&self) -> Option<&CorefConfiguration> {
        self.probabilities
            .iter()
            .max_by(|(_, p1), (_, p2)| {
                p1.partial_cmp(p2)
                    .expect("probabilities should be comparable")
            })
            .map(|(config, _)| config)
    }

    /// Get top-k configurations by probability.
    pub fn top_k(&self, k: usize) -> Vec<(&CorefConfiguration, f64)> {
        let mut items: Vec<_> = self.probabilities.iter().collect();
        items.sort_by(|(_, p1), (_, p2)| {
            p2.partial_cmp(p1)
                .expect("probabilities should be comparable")
        });
        items.into_iter().take(k).map(|(c, &p)| (c, p)).collect()
    }

    /// Entropy of the distribution (uncertainty measure).
    ///
    /// Lower entropy = more confident. Zero entropy = deterministic.
    pub fn entropy(&self) -> f64 {
        let mut h = 0.0;
        for &p in self.probabilities.values() {
            if p > 0.0 {
                h -= p * p.ln();
            }
        }
        // Add contribution from smoothing (approximation)
        if self.smoothing_mass > 0.0 {
            let count = self.estimate_unrepresented_count();
            if count > 0 {
                let p_each = self.smoothing_mass / count as f64;
                h -= self.smoothing_mass * p_each.ln();
            }
        }
        h
    }

    /// Cross-entropy with respect to a "true" configuration.
    ///
    /// This is Kehler's evaluation metric: -log P(correct config).
    /// Lower is better. Zero if we assign probability 1 to the truth.
    ///
    /// ## Comparison to Modern Metrics
    ///
    /// Modern coreference systems use MUC, B³, CEAF, LEA—these operate on **hard
    /// clusterings** and don't see model probabilities. Cross-entropy is orthogonal:
    /// it measures **calibration** (does confidence match reality?), not clustering
    /// structure.
    ///
    /// Kehler argued this matters for data fusion: "A system that assigns P=0.9 to
    /// correct answers is more successful than one that assigns P=0.6, even if both
    /// select the same most-likely answer."
    pub fn cross_entropy(&self, true_config: &CorefConfiguration) -> f64 {
        let p = self.prob(true_config);
        if p > 0.0 {
            -p.ln()
        } else {
            f64::INFINITY
        }
    }

    /// Number of explicitly represented configurations.
    pub fn num_represented(&self) -> usize {
        self.probabilities.len()
    }

    /// Estimate count of compatible but unrepresented configurations.
    fn estimate_unrepresented_count(&self) -> usize {
        // This is a rough estimate. For exact count, we'd need to:
        // 1. Compute Bell(n_mentions)
        // 2. Subtract configs violating incompatibility constraints
        // 3. Subtract represented configs
        //
        // For small n, we can be exact; for large n, we approximate.
        let bell = bell_number(self.n_mentions);
        bell.saturating_sub(self.probabilities.len())
    }
}

/// Compute the n-th Bell number (number of partitions of n elements).
///
/// Uses the Bell triangle recurrence. Exact for small n, approximate for large.
/// First few: B(0)=1, B(1)=1, B(2)=2, B(3)=5, B(4)=15, B(5)=52
pub fn bell_number(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    if n > 20 {
        // Bell numbers grow super-exponentially; return max for large n
        return usize::MAX;
    }

    let mut bell = vec![vec![0usize; n + 1]; n + 1];
    bell[0][0] = 1;

    for i in 1..=n {
        bell[i][0] = bell[i - 1][i - 1];
        for j in 1..=i {
            bell[i][j] = bell[i - 1][j - 1].saturating_add(bell[i][j - 1]);
        }
    }

    bell[n][0]
}

/// Builder for constructing configuration distributions from pairwise probabilities.
///
/// Implements Kehler's two approaches:
///
/// 1. **Evidential (Dempster-Shafer)**: Treat P(A~B) as mass on configuration subsets,
///    combine using Dempster's rule to get distribution over all configs
///
/// 2. **Merging Decision**: Model P(config) as product of merge decisions a greedy
///    resolver would make
///
/// Both approaches start from the same pairwise probabilities but derive different
/// configuration distributions.
///
/// ## Pairwise Inconsistency
///
/// Kehler's key insight: pairwise probabilities can be **globally inconsistent**.
/// If P(A~D)=0.505 and P(C~D)=0.504 but A/C are incompatible, no valid configuration
/// can satisfy both. The evidential approach normalizes away this conflict mass;
/// the merging decision approach avoids it by sequential processing.
///
/// ## Modern Extensions
///
/// Neural systems (Lee et al. 2017) learn P(antecedent | mention) directly, losing
/// configuration-level uncertainty but gaining scalability. Triad networks (Meng &
/// Rumshisky 2018) address transitivity by scoring mention triples jointly.
///
/// For applications needing uncertainty (data fusion, active learning), this module
/// preserves Kehler's configuration-level probabilistic output.
pub struct ConfigurationBuilder {
    n_mentions: usize,
    /// Pairwise coreference probabilities: P(mention_i ~ mention_j)
    pairwise_probs: HashMap<(usize, usize), f64>,
    /// Pairs known to be incompatible (P = 0)
    incompatible: HashSet<(usize, usize)>,
}

impl ConfigurationBuilder {
    /// Create a new builder for n mentions.
    pub fn new(n_mentions: usize) -> Self {
        Self {
            n_mentions,
            pairwise_probs: HashMap::new(),
            incompatible: HashSet::new(),
        }
    }

    /// Set the pairwise coreference probability P(i ~ j).
    pub fn set_pairwise(&mut self, i: usize, j: usize, prob: f64) -> &mut Self {
        let pair = if i < j { (i, j) } else { (j, i) };
        self.pairwise_probs.insert(pair, prob);
        self
    }

    /// Mark a pair as incompatible (cannot corefer).
    pub fn set_incompatible(&mut self, i: usize, j: usize) -> &mut Self {
        let pair = if i < j { (i, j) } else { (j, i) };
        self.incompatible.insert(pair);
        self
    }

    /// Get pairwise probability, defaulting to 0.5 (uncertain) if not set.
    fn get_pairwise(&self, i: usize, j: usize) -> f64 {
        if i == j {
            return 1.0;
        }
        let pair = if i < j { (i, j) } else { (j, i) };
        if self.incompatible.contains(&pair) {
            return 0.0;
        }
        self.pairwise_probs.get(&pair).copied().unwrap_or(0.5)
    }

    /// Build distribution using the evidential (Dempster-Shafer) approach.
    ///
    /// From Kehler (1997):
    /// > "We recast a probability that two templates S and T corefer as a mass
    /// > distribution over two members of the power set of coreference configurations,
    /// > namely the set containing exactly those configurations in which S and T
    /// > occupy the same cell, and the set containing those in which they do not."
    ///
    /// The final distribution is obtained by iteratively combining pairwise mass
    /// distributions using Dempster's rule, which normalizes away conflict.
    pub fn build_evidential(&self) -> ConfigurationDistribution {
        let configs = self.enumerate_compatible_configs();
        let mut dist = ConfigurationDistribution::new(self.n_mentions);

        for &(i, j) in &self.incompatible {
            dist.add_incompatibility(i, j);
        }

        // For each config, compute probability as product of consistent pairwise probs,
        // then normalize by conflict mass (Dempster's rule simplification)
        let mut raw_probs: Vec<(CorefConfiguration, f64)> = Vec::new();
        let mut conflict_mass = 0.0;

        for config in configs {
            let mut prob = 1.0;

            // For each pair, multiply by P(same cell) or P(different cell)
            for i in 0..self.n_mentions {
                for j in (i + 1)..self.n_mentions {
                    let p_coref = self.get_pairwise(i, j);
                    if config.same_cell(i, j) {
                        prob *= p_coref;
                    } else {
                        prob *= 1.0 - p_coref;
                    }
                }
            }

            if dist.is_compatible(&config) {
                raw_probs.push((config, prob));
            } else {
                conflict_mass += prob;
            }
        }

        // Normalize by (1 - conflict), which is Dempster's rule
        let normalizer = 1.0 - conflict_mass;
        if normalizer > 0.0 {
            for (config, prob) in raw_probs {
                dist.set_prob(config, prob / normalizer);
            }
        }

        // Small smoothing for numerical stability
        dist.set_smoothing(1e-10);
        dist.normalize();
        dist
    }

    /// Build distribution using the merging decision approach.
    ///
    /// From Kehler (1997):
    /// > "The second approach we consider models the likelihood of correctness of
    /// > decisions that a template merger... would make in processing a text."
    ///
    /// For each configuration, we compute the probability of the sequence of
    /// merge decisions that would produce it, processing mentions in order.
    pub fn build_merging_decision(&self) -> ConfigurationDistribution {
        let configs = self.enumerate_compatible_configs();
        let mut dist = ConfigurationDistribution::new(self.n_mentions);

        for &(i, j) in &self.incompatible {
            dist.add_incompatibility(i, j);
        }

        for config in configs {
            if !dist.is_compatible(&config) {
                continue;
            }

            // Simulate greedy merging and compute probability
            let prob = self.merging_decision_prob(&config);
            dist.set_prob(config, prob);
        }

        dist.set_smoothing(1e-10);
        dist.normalize();
        dist
    }

    /// Compute P(config) under the merging decision model.
    ///
    /// Process mentions in order. For each mention i, consider whether to merge
    /// with each existing cluster (represented by its most recent mention).
    fn merging_decision_prob(&self, config: &CorefConfiguration) -> f64 {
        // Build mention-to-cell mapping
        let mut mention_to_cell: HashMap<usize, usize> = HashMap::new();
        for (cell_idx, cell) in config.cells.iter().enumerate() {
            for &mention in cell {
                mention_to_cell.insert(mention, cell_idx);
            }
        }

        let mut prob = 1.0;
        let mut cell_representatives: HashMap<usize, usize> = HashMap::new(); // cell_idx -> most recent mention

        for i in 0..self.n_mentions {
            let my_cell = mention_to_cell[&i];

            if let Some(&rep) = cell_representatives.get(&my_cell) {
                // This mention should merge with existing cluster
                // Probability of correctly deciding to merge with rep
                prob *= self.get_pairwise(rep, i);
            }
            // else: first mention in this cell, or creating new cluster
            // (we model the "don't merge" decisions implicitly via normalization)

            // Update representative to most recent mention
            cell_representatives.insert(my_cell, i);
        }

        prob
    }

    /// Enumerate all compatible configurations.
    ///
    /// For small n, this is feasible. For large n, use sampling or pruning.
    fn enumerate_compatible_configs(&self) -> Vec<CorefConfiguration> {
        if self.n_mentions > 10 {
            // Too many partitions; would need approximate methods
            // For now, return common configurations
            return self.enumerate_pruned_configs();
        }

        let partitions = all_partitions(self.n_mentions);
        partitions
            .into_iter()
            .map(CorefConfiguration::new)
            .filter(|c| self.is_config_compatible(c))
            .collect()
    }

    /// Enumerate a pruned set of likely configurations.
    fn enumerate_pruned_configs(&self) -> Vec<CorefConfiguration> {
        // For large n, generate configs via greedy search from high-probability pairs
        // This is a simplification; a full implementation would use beam search
        let mut configs = vec![CorefConfiguration::all_singletons(self.n_mentions)];

        // Add greedy merge configuration
        let greedy = self.greedy_config();
        if self.is_config_compatible(&greedy) {
            configs.push(greedy);
        }

        configs
    }

    /// Generate the greedy merging configuration.
    fn greedy_config(&self) -> CorefConfiguration {
        let mut parent: Vec<usize> = (0..self.n_mentions).collect();

        fn find(parent: &mut [usize], i: usize) -> usize {
            if parent[i] != i {
                parent[i] = find(parent, parent[i]);
            }
            parent[i]
        }

        fn union(parent: &mut [usize], i: usize, j: usize) {
            let pi = find(parent, i);
            let pj = find(parent, j);
            if pi != pj {
                parent[pi] = pj;
            }
        }

        // Merge pairs with P > 0.5, respecting incompatibilities
        for i in 0..self.n_mentions {
            for j in (i + 1)..self.n_mentions {
                let pair = (i, j);
                if !self.incompatible.contains(&pair) && self.get_pairwise(i, j) > 0.5 {
                    union(&mut parent, i, j);
                }
            }
        }

        // Build cells from union-find
        let mut cell_map: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..self.n_mentions {
            let root = find(&mut parent, i);
            cell_map.entry(root).or_default().push(i);
        }

        CorefConfiguration::new(cell_map.into_values().collect())
    }

    fn is_config_compatible(&self, config: &CorefConfiguration) -> bool {
        for &(i, j) in &self.incompatible {
            if config.same_cell(i, j) {
                return false;
            }
        }
        true
    }
}

/// Generate all partitions of {0, 1, ..., n-1}.
///
/// Uses recursive enumeration. Only feasible for small n.
fn all_partitions(n: usize) -> Vec<Vec<Vec<usize>>> {
    if n == 0 {
        return vec![vec![]];
    }
    if n == 1 {
        return vec![vec![vec![0]]];
    }

    // Recursively partition {0..n-2}, then add n-1 to each possibility
    let smaller = all_partitions(n - 1);
    let mut result = Vec::new();

    for partition in smaller {
        // Option 1: n-1 joins an existing cell
        for (i, _) in partition.iter().enumerate() {
            let mut new_partition = partition.clone();
            new_partition[i].push(n - 1);
            result.push(new_partition);
        }

        // Option 2: n-1 forms a new singleton cell
        let mut new_partition = partition;
        new_partition.push(vec![n - 1]);
        result.push(new_partition);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bell_numbers() {
        assert_eq!(bell_number(0), 1);
        assert_eq!(bell_number(1), 1);
        assert_eq!(bell_number(2), 2);
        assert_eq!(bell_number(3), 5);
        assert_eq!(bell_number(4), 15);
        assert_eq!(bell_number(5), 52);
    }

    #[test]
    fn test_configuration_normalization() {
        // Different orderings should produce same canonical form
        let c1 = CorefConfiguration::new(vec![vec![2], vec![0, 1, 3]]);
        let c2 = CorefConfiguration::new(vec![vec![3, 1, 0], vec![2]]);

        assert_eq!(c1, c2);
        assert_eq!(c1.to_compact_string(), "(0 1 3)(2)");
    }

    #[test]
    fn test_same_cell() {
        let config = CorefConfiguration::new(vec![vec![0, 1, 3], vec![2]]);

        assert!(config.same_cell(0, 1));
        assert!(config.same_cell(0, 3));
        assert!(config.same_cell(1, 3));
        assert!(!config.same_cell(0, 2));
        assert!(!config.same_cell(1, 2));
    }

    #[test]
    fn test_kehler_example() {
        // Reproduce Kehler's example from the paper
        // Templates: A, B, C, D (indices 0, 1, 2, 3)
        // A and C are incompatible (different depot types)
        // B and C are incompatible
        //
        // Pairwise probabilities from Table 1:
        // P(A~B) = 0.671, P(A~D) = 0.505, P(B~D) = 0.752, P(C~D) = 0.504

        let mut builder = ConfigurationBuilder::new(4);
        builder
            .set_pairwise(0, 1, 0.671) // A~B
            .set_pairwise(0, 3, 0.505) // A~D
            .set_pairwise(1, 3, 0.752) // B~D
            .set_pairwise(2, 3, 0.504) // C~D
            .set_incompatible(0, 2) // A incompatible with C
            .set_incompatible(1, 2); // B incompatible with C

        let dist = builder.build_evidential();

        // The correct configuration is (A B D)(C) = (0 1 3)(2)
        let correct = CorefConfiguration::new(vec![vec![0, 1, 3], vec![2]]);

        // Should have highest probability
        let mode = dist.mode().expect("should have a mode");
        assert_eq!(*mode, correct, "Most probable config should be correct one");

        // Cross-entropy should be reasonably low
        let ce = dist.cross_entropy(&correct);
        assert!(ce < 2.0, "Cross-entropy should be reasonable: {}", ce);
    }

    #[test]
    fn test_incompatibility_pruning() {
        let mut builder = ConfigurationBuilder::new(3);
        builder.set_incompatible(0, 2); // A and C cannot corefer

        let dist = builder.build_evidential();

        // Configuration with A and C in same cell should have zero probability
        let invalid = CorefConfiguration::new(vec![vec![0, 2], vec![1]]);
        assert_eq!(dist.prob(&invalid), 0.0);
    }

    #[test]
    fn test_distribution_normalization() {
        let mut builder = ConfigurationBuilder::new(2);
        builder.set_pairwise(0, 1, 0.7);

        let dist = builder.build_evidential();

        // Total probability should be ~1
        let all_same = CorefConfiguration::all_same(2);
        let all_sep = CorefConfiguration::all_singletons(2);

        let total = dist.prob(&all_same) + dist.prob(&all_sep);
        assert!((total - 1.0).abs() < 0.01, "Should sum to 1: {}", total);
    }

    #[test]
    fn test_all_partitions_count() {
        for n in 0..=5 {
            let partitions = all_partitions(n);
            assert_eq!(partitions.len(), bell_number(n), "Bell({}) mismatch", n);
        }
    }
}
