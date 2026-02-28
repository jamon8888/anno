//! Graph-Based Coreference Resolution with Iterative Refinement.
//!
//! This module implements a graph-based approach to coreference resolution,
//! inspired by the Graph-to-Graph Transformer (G2GT) architecture from
//! Miculicich & Henderson (2022). The key insight: model coreference as a
//! graph where nodes are mentions and edges are coref links, then iteratively
//! refine predictions until convergence.
//!
//! # Historical Context
//!
//! Coreference resolution evolved through distinct paradigms:
//!
//! ```text
//! 1995-2010  Rule-based: Hobbs algorithm, centering theory
//! 2010-2016  Mention-pair: Classify (m_i, m_j) independently
//! 2017       Lee et al.: End-to-end span-based, O(N⁴) complexity
//! 2018       Lee et al.: Higher-order with representation refinement
//! 2022       G2GT: Graph refinement with global decisions, O(N² × T)
//! ```
//!
//! **The core problem with pairwise models**: Decisions are independent.
//! If P(A~B)=0.9 and P(B~C)=0.9, transitivity implies A~C, but pairwise
//! models can output P(A~C)=0.1. The G2GT approach addresses this by
//! conditioning each iteration on the full predicted graph from the previous
//! iteration, enabling global consistency.
//!
//! # Architecture
//!
//! ```text
//! Input: Detected mentions M = [m₁, m₂, ..., mₙ]
//!    ↓
//! ┌─────────────────────────────────────────────────────────┐
//! │ Iteration 0: Initialize empty graph G₀                  │
//! │    - Nodes: all mentions                                │
//! │    - Edges: none                                        │
//! └─────────────────────────────────────────────────────────┘
//!    ↓
//! ┌─────────────────────────────────────────────────────────┐
//! │ Iteration t: Refine graph Gₜ₋₁ → Gₜ                     │
//! │    For each mention pair (mᵢ, mⱼ) where j < i:          │
//! │    1. Compute pairwise score s(mᵢ, mⱼ)                  │
//! │    2. Add graph context from Gₜ₋₁ (transitivity bonus)  │
//! │    3. Update edge if score exceeds threshold            │
//! └─────────────────────────────────────────────────────────┘
//!    ↓ (repeat until Gₜ = Gₜ₋₁ or t = max_iterations)
//! ┌─────────────────────────────────────────────────────────┐
//! │ Extract clusters via connected components               │
//! └─────────────────────────────────────────────────────────┘
//!    ↓
//! Output: Coreference chains (clusters)
//! ```
//!
//! # Approximation vs. Full G2GT
//!
//! This implementation is a **heuristic approximation**, not a full neural model.
//!
//! | Aspect | G2GT (Miculicich 2022) | This Implementation |
//! |--------|------------------------|---------------------|
//! | **Graph nodes** | Tokens | Mentions (pre-detected) |
//! | **Graph encoding** | Attention modification: `Lk = E(G)·Wk` | Explicit transitivity bonus |
//! | **Pairwise scoring** | Learned neural scorer | String/head heuristics |
//! | **Refinement** | Full neural re-prediction | Score adjustment |
//! | **Training** | End-to-end backprop | None (heuristic) |
//!
//! ## What We Preserve
//!
//! The key insight from G2GT: **iterative refinement with graph conditioning**
//! enables global consistency that independent pairwise models lack. Even with
//! heuristic scoring, the refinement loop propagates transitivity constraints.
//!
//! ## What We Lose
//!
//! - **Learned representations**: G2GT embeds graph structure directly into
//!   transformer attention. We approximate this with explicit bonuses.
//! - **End-to-end optimization**: G2GT trains the full system. We use fixed heuristics.
//! - **Token-level granularity**: G2GT operates on tokens; we operate on mentions.
//!
//! For production use with high accuracy requirements, consider a full neural
//! implementation or the T5-based coreference in `crate::backends::coref::t5`.
//!
//! # Usage with MentionType
//!
//! For best results, provide mentions with `mention_type` set:
//!
//! ```rust,ignore
//! use anno::backends::coref::graph::GraphCoref;
//! use anno::eval::coref::{Mention, MentionType};
//!
//! // Properly annotated mentions work better
//! let mut john = Mention::new("John", 0, 4);
//! john.mention_type = Some(MentionType::Proper);
//!
//! let mut he = Mention::new("he", 20, 22);
//! he.mention_type = Some(MentionType::Pronominal);
//!
//! let coref = GraphCoref::new();
//! let chains = coref.resolve(&[john, he]);
//! ```
//!
//! # Graph Initialization: Syntactic vs Semantic
//!
//! SpanEIT (Hossain et al. 2025) constructs a combined graph:
//! - **Syntactic edges** (`E_syn`): From dependency parse (adjectival modifiers, etc.)
//! - **Semantic edges** (`E_sem`): From co-occurrence statistics
//!
//! Use [`CorefGraph::seed_cooccurrence_edges`] to initialize with proximity-based
//! priors before running iterative refinement.
//!
//! # References
//!
//! - Miculicich & Henderson (2022): "Graph Refinement for Coreference Resolution"
//!   [arXiv:2203.16574](https://arxiv.org/abs/2203.16574)
//! - Lee et al. (2017): "End-to-end Neural Coreference Resolution"
//! - Lee et al. (2018): "Higher-Order Coreference Resolution"
//! - Mohammadshahi & Henderson (2021): "Graph-to-Graph Transformer for Dependency Parsing"
//! - Hossain et al. (2025): "SpanEIT: Dynamic Span Interaction and Graph-Aware Memory"
//!   [arXiv:2509.11604](https://arxiv.org/abs/2509.11604)
//!
//! # Future Direction: Sheaf Neural Networks
//!
//! This implementation uses explicit transitivity bonuses to approximate global consistency.
//! A more principled approach: **Sheaf Neural Networks** replace scalar edge weights with
//! learned linear maps (restriction maps) and minimize the sheaf Dirichlet energy:
//!
//! ```text
//! E(x) = Σ_{(u,v) ∈ E} || F(u→v) · x_u - F(v→u) · x_v ||²
//! ```
//!
//! This enforces transitivity at the gradient level, not post-hoc. See:
//! - `archive/geometric-2024-12/sheaf.rs` for stub implementation and trait definitions
//! - Bodnar et al. (2023): "Neural Sheaf Diffusion" - NeurIPS
//! - twitter-research/neural-sheaf-diffusion (Apache 2.0): reference implementation

use anno_core::{CorefChain, Mention, MentionType};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

// =============================================================================
// Types
// =============================================================================

/// Edge type in the coreference graph.
///
/// Following G2GT's three-way classification:
/// - 0 (None): No relationship
/// - 1 (Mention): Within-mention link (not used here since we operate on mentions, not tokens)
/// - 2 (Coref): Coreference link between mentions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EdgeType {
    /// No link between mentions.
    None = 0,
    /// Coreference link (both mentions refer to the same entity).
    Coref = 2,
}

/// A coreference graph representing mention relationships.
///
/// This is the core data structure for iterative refinement. Nodes are mention
/// indices, edges are coreference links. The graph is stored as an adjacency
/// set for O(1) edge lookup during refinement.
///
/// # Invariants
///
/// - Graph is symmetric: if edge(i,j) exists, edge(j,i) exists
/// - No self-loops: edge(i,i) is always None
/// - Indices are valid mention indices: 0 <= i,j < num_mentions
///
/// # Example
///
/// ```rust
/// use anno::backends::coref::graph::CorefGraph;
///
/// let mut graph = CorefGraph::new(3);
/// graph.add_edge(0, 1);
/// graph.add_edge(1, 2);
///
/// assert!(graph.has_edge(0, 1));
/// assert!(graph.transitively_connected(0, 2));  // via 0-1-2
///
/// let clusters = graph.extract_clusters();
/// assert_eq!(clusters.len(), 1);  // All connected
/// ```
pub mod types;
pub use types::*;

/// Graph-based coreference resolver.
pub struct GraphCoref {
    config: GraphCorefConfig,
}

impl GraphCoref {
    /// Create a new graph coref resolver with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(GraphCorefConfig::default())
    }

    /// Create with custom configuration.
    #[must_use]
    pub fn with_config(config: GraphCorefConfig) -> Self {
        Self { config }
    }

    fn graph_fingerprint(graph: &CorefGraph) -> u64 {
        // Order-independent fingerprint of the edge set.
        // We sort per-edge hashes to avoid HashSet iteration nondeterminism.
        let mut edge_hashes: Vec<u64> = graph
            .edges
            .iter()
            .map(|e| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                e.hash(&mut h);
                h.finish()
            })
            .collect();
        edge_hashes.sort_unstable();

        let mut h = std::collections::hash_map::DefaultHasher::new();
        graph.num_mentions.hash(&mut h);
        for eh in edge_hashes {
            eh.hash(&mut h);
        }
        h.finish()
    }

    /// Resolve coreferences among mentions using iterative graph refinement.
    ///
    /// # Arguments
    ///
    /// * `mentions` - Pre-detected mentions (from NER or mention detector).
    ///   For best results, set `mention_type` on each mention.
    ///
    /// # Returns
    ///
    /// Coreference chains (clusters) where each chain contains mentions
    /// referring to the same entity. By default, singletons are filtered;
    /// set `config.include_singletons = true` to include them.
    ///
    /// # Panics
    ///
    /// Does not panic. Empty input returns empty output.
    ///
    /// # Example
    ///
    /// ```rust
    /// use anno::backends::coref::graph::GraphCoref;
    /// use anno::{Mention, MentionType};
    ///
    /// let coref = GraphCoref::new();
    ///
    /// let mut john = Mention::new("John", 0, 4);
    /// john.mention_type = Some(MentionType::Proper);
    ///
    /// let mut he = Mention::new("he", 20, 22);
    /// he.mention_type = Some(MentionType::Pronominal);
    ///
    /// let chains = coref.resolve(&[john, he]);
    /// ```
    #[must_use]
    pub fn resolve(&self, mentions: &[Mention]) -> Vec<CorefChain> {
        if mentions.is_empty() {
            return vec![];
        }

        // Validate mentions (filter empty/invalid)
        let valid_mentions: Vec<&Mention> = mentions
            .iter()
            .filter(|m| !m.text.trim().is_empty() && m.start < m.end)
            .collect();

        if valid_mentions.is_empty() {
            return vec![];
        }

        // Initialize empty graph
        let mut graph = CorefGraph::new(valid_mentions.len());

        // Iterative refinement
        let mut stagnation: usize = 0;
        let mut last_edge_count = graph.edge_count();
        let mut seen: HashMap<u64, usize> = HashMap::new();
        let mut history: VecDeque<u64> = VecDeque::new();
        if let Some(cfg) = &self.config.early_stop {
            if cfg.detect_cycles {
                let fp0 = Self::graph_fingerprint(&graph);
                seen.insert(fp0, 0);
                history.push_back(fp0);
            }
        }

        for iteration in 0..self.config.max_iterations {
            let prev_graph = graph.clone();
            graph = self.refine_iteration(&valid_mentions, &graph);

            // Check convergence
            if graph == prev_graph {
                break;
            }

            // Optional early stop: stagnation / cycles.
            if let Some(cfg) = &self.config.early_stop {
                // Stagnation: edge count stops changing.
                let ec = graph.edge_count();
                if ec == last_edge_count {
                    stagnation += 1;
                } else {
                    stagnation = 0;
                    last_edge_count = ec;
                }
                if cfg.stagnation_patience > 0 && stagnation >= cfg.stagnation_patience {
                    break;
                }

                if cfg.detect_cycles {
                    let fp = Self::graph_fingerprint(&graph);
                    if seen.contains_key(&fp) {
                        break;
                    }
                    seen.insert(fp, iteration + 1);
                    history.push_back(fp);
                    if cfg.cycle_history > 0 {
                        while history.len() > cfg.cycle_history {
                            if let Some(old) = history.pop_front() {
                                seen.remove(&old);
                            }
                        }
                    }
                }
            }
        }

        // Extract clusters and convert to CorefChains
        self.graph_to_chains(&graph, &valid_mentions)
    }

    /// Perform one iteration of graph refinement.
    ///
    /// For each mention pair, computes a score incorporating:
    /// 1. Base pairwise similarity (string match, head match, type compatibility)
    /// 2. Graph context (transitivity bonus from shared neighbors)
    ///
    /// Edges are added if score exceeds threshold.
    fn refine_iteration(&self, mentions: &[&Mention], prev_graph: &CorefGraph) -> CorefGraph {
        let mut new_graph = CorefGraph::new(mentions.len());

        for i in 0..mentions.len() {
            for j in 0..i {
                // Distance filter
                if let Some(max_dist) = self.config.max_distance {
                    let dist = mentions[i].start.saturating_sub(mentions[j].end);
                    if dist > max_dist {
                        continue;
                    }
                }

                // Compute score with graph context
                let base_score = self.pairwise_score(i, j, mentions[i], mentions[j]);
                let context_bonus = self.graph_context_bonus(i, j, prev_graph);
                let total_score = base_score + context_bonus;

                if total_score > self.config.link_threshold {
                    new_graph.add_edge(i, j);
                }
            }
        }

        new_graph
    }

    /// Compute base pairwise similarity score between two mentions.
    ///
    /// Uses multiple signals:
    /// - Exact string match (highest weight)
    /// - Substring containment
    /// - Head word match (uses `head_start`/`head_end` if available)
    /// - Pronoun-to-proper linking (uses `mention_type` if available)
    /// - Distance penalty
    fn pairwise_score(&self, i: usize, j: usize, m1: &Mention, m2: &Mention) -> f64 {
        let mut score = 0.0;

        let t1 = m1.text.to_lowercase();
        let t2 = m2.text.to_lowercase();

        // Exact match (strongest signal)
        if t1 == t2 {
            score += self.config.string_similarity_weight * 1.0;
        }
        // Substring containment
        else if t1.contains(&t2) || t2.contains(&t1) {
            score += self.config.string_similarity_weight * 0.6;
        }
        // Head word match
        else {
            let h1 = self.get_head_text(m1);
            let h2 = self.get_head_text(m2);
            if !h1.is_empty() && h1.to_lowercase() == h2.to_lowercase() {
                score += self.config.head_match_weight;
            }
        }

        // Mention type compatibility
        score += self.type_compatibility_score(m1, m2);

        // Distance penalty (log scale)
        let distance = m1.start.abs_diff(m2.end).min(m2.start.abs_diff(m1.end));
        if distance > 0 {
            score -= self.config.distance_weight * (distance as f64).ln();
        }

        // External score (e.g., box containment)
        if let Some(ref ext) = self.config.external_scores {
            let key = if i < j { (i, j) } else { (j, i) };
            if let Some(&ext_score) = ext.get(&key) {
                score += self.config.external_score_weight * ext_score;
            }
        }

        score
    }

    /// Get head text for a mention.
    ///
    /// Uses `head_start`/`head_end` if available, otherwise falls back to
    /// last word heuristic (common in English NPs where head is rightmost).
    fn get_head_text<'a>(&self, mention: &'a Mention) -> &'a str {
        // Use explicit head span if available
        if let (Some(head_start), Some(head_end)) = (mention.head_start, mention.head_end) {
            // Head offsets are relative to document, need to extract from text
            // This is complex; fall back to heuristic for now
            // In a full implementation, we'd have the document text available
            let _ = (head_start, head_end);
        }

        // Fallback: last word (head-final assumption for English NPs)
        mention.text.split_whitespace().last().unwrap_or("")
    }

    /// Compute type compatibility score between two mentions.
    ///
    /// Uses `MentionType` field if available, otherwise uses heuristics.
    fn type_compatibility_score(&self, m1: &Mention, m2: &Mention) -> f64 {
        let type1 = m1
            .mention_type
            .unwrap_or_else(|| self.infer_mention_type(m1));
        let type2 = m2
            .mention_type
            .unwrap_or_else(|| self.infer_mention_type(m2));

        match (type1, type2) {
            // Pronoun linking to proper noun: boost
            (MentionType::Pronominal, MentionType::Proper)
            | (MentionType::Proper, MentionType::Pronominal) => self.config.pronoun_proper_bonus,

            // Pronoun linking to nominal: smaller boost
            (MentionType::Pronominal, MentionType::Nominal)
            | (MentionType::Nominal, MentionType::Pronominal) => {
                self.config.pronoun_proper_bonus * 0.5
            }

            // Same type: neutral
            _ if type1 == type2 => 0.0,

            // Different non-pronoun types: slight penalty
            _ => -0.1,
        }
    }

    /// Infer mention type from text when not explicitly set.
    ///
    /// This is a fallback heuristic. For best results, set `mention_type`
    /// on mentions before calling `resolve()`.
    fn infer_mention_type(&self, mention: &Mention) -> MentionType {
        let text_lower = mention.text.to_lowercase();

        // English-only pronoun list for mention type inference
        const PRONOUNS: &[&str] = &[
            "i",
            "me",
            "my",
            "mine",
            "myself",
            "you",
            "your",
            "yours",
            "yourself",
            "yourselves",
            "he",
            "him",
            "his",
            "himself",
            "she",
            "her",
            "hers",
            "herself",
            "it",
            "its",
            "itself",
            "we",
            "us",
            "our",
            "ours",
            "ourselves",
            "they",
            "them",
            "their",
            "theirs",
            "themselves",
            "who",
            "whom",
            "whose",
            "which",
            "that",
            "this",
            "these",
            "those",
        ];

        if PRONOUNS.contains(&text_lower.as_str()) {
            return MentionType::Pronominal;
        }

        // Check for proper noun (starts with uppercase, not sentence-initial heuristic)
        let first_char = mention.text.chars().next();
        if first_char.is_some_and(|c| c.is_uppercase()) {
            // Additional check: not a common word
            let common_words = ["the", "a", "an", "this", "that", "these", "those"];
            if !common_words.contains(&text_lower.as_str()) {
                return MentionType::Proper;
            }
        }

        // Default to nominal
        MentionType::Nominal
    }

    /// Compute graph context bonus based on previous iteration's structure.
    ///
    /// This is our approximation of G2GT's graph-conditioned attention.
    ///
    /// # Transitivity Bonus
    ///
    /// If A~C and B~C in the previous graph, A and B should likely be linked.
    /// We add a bonus proportional to the number of shared neighbors.
    ///
    /// # Already Connected Bonus
    ///
    /// If A and B are already transitively connected (through a chain of
    /// coreference links), add a bonus to preserve and strengthen the connection.
    fn graph_context_bonus(&self, i: usize, j: usize, prev_graph: &CorefGraph) -> f64 {
        let mut bonus = 0.0;

        // Bonus for shared neighbors (transitivity signal)
        let shared = prev_graph.shared_neighbors(i, j);
        bonus += (shared as f64) * self.config.per_shared_neighbor_bonus;

        // Bonus if already transitively connected
        if prev_graph.transitively_connected(i, j) {
            bonus += self.config.transitivity_bonus;
        }

        bonus
    }

    /// Convert graph clusters to CorefChain format.
    fn graph_to_chains(&self, graph: &CorefGraph, mentions: &[&Mention]) -> Vec<CorefChain> {
        let clusters = graph.extract_clusters();

        clusters
            .into_iter()
            .filter(|cluster| self.config.include_singletons || cluster.len() > 1)
            .enumerate()
            .map(|(id, indices)| {
                let chain_mentions: Vec<Mention> = indices
                    .into_iter()
                    .map(|i| (*mentions[i]).clone())
                    .collect();

                let mut chain = CorefChain::new(chain_mentions);
                chain.cluster_id = Some((id as u64).into());

                // Set entity type from first proper mention
                chain.entity_type = chain
                    .mentions
                    .iter()
                    .find(|m| m.mention_type == Some(MentionType::Proper))
                    .and_then(|m| m.entity_type.clone());

                chain
            })
            .collect()
    }

    /// Get configuration.
    #[must_use]
    pub fn config(&self) -> &GraphCorefConfig {
        &self.config
    }
}

impl Default for GraphCoref {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Metrics and Diagnostics
// =============================================================================

/// Statistics from a graph coref run for debugging and analysis.
///
/// Use [`GraphCoref::resolve_with_stats`] to get these alongside results.
#[derive(Debug, Clone, Default)]
pub struct GraphCorefStats {
    /// Number of iterations until convergence (1 to max_iterations).
    pub iterations: usize,
    /// Number of edges in final graph.
    pub final_edges: usize,
    /// Number of clusters (including singletons).
    pub num_clusters: usize,
    /// Number of non-singleton clusters.
    pub num_chains: usize,
    /// Per-iteration edge counts, starting from 0.
    pub edge_history: Vec<usize>,
    /// Whether the algorithm converged before max_iterations.
    pub converged: bool,
    /// Whether we stopped early for a non-fixed-point reason (cycle/stagnation).
    pub early_stopped: bool,
    /// Cycle detected (graph fingerprint repeated).
    pub cycle_detected: bool,
    /// Stagnation detected (edge count stopped changing).
    pub stagnation_detected: bool,
}

impl GraphCoref {
    /// Resolve coreferences and return detailed statistics.
    ///
    /// Useful for debugging, tuning parameters, and understanding convergence.
    ///
    /// # Example
    ///
    /// ```rust
    /// use anno::backends::coref::graph::GraphCoref;
    /// use anno::Mention;
    ///
    /// let coref = GraphCoref::new();
    /// let mentions = vec![
    ///     Mention::new("John", 0, 4),
    ///     Mention::new("John", 50, 54),
    /// ];
    ///
    /// let (chains, stats) = coref.resolve_with_stats(&mentions);
    /// println!("Converged in {} iterations", stats.iterations);
    /// println!("Edge history: {:?}", stats.edge_history);
    /// ```
    #[must_use]
    pub fn resolve_with_stats(&self, mentions: &[Mention]) -> (Vec<CorefChain>, GraphCorefStats) {
        let mut stats = GraphCorefStats::default();

        if mentions.is_empty() {
            return (vec![], stats);
        }

        let valid_mentions: Vec<&Mention> = mentions
            .iter()
            .filter(|m| !m.text.trim().is_empty() && m.start < m.end)
            .collect();

        if valid_mentions.is_empty() {
            return (vec![], stats);
        }

        let mut graph = CorefGraph::new(valid_mentions.len());
        stats.edge_history.push(0);

        let mut stagnation: usize = 0;
        let mut last_edge_count = graph.edge_count();
        let mut seen: HashMap<u64, usize> = HashMap::new();
        let mut history: VecDeque<u64> = VecDeque::new();
        if let Some(cfg) = &self.config.early_stop {
            if cfg.detect_cycles {
                let fp0 = Self::graph_fingerprint(&graph);
                seen.insert(fp0, 0);
                history.push_back(fp0);
            }
        }

        for iteration in 0..self.config.max_iterations {
            let prev_graph = graph.clone();
            graph = self.refine_iteration(&valid_mentions, &graph);
            stats.edge_history.push(graph.edge_count());
            stats.iterations = iteration + 1;

            if graph == prev_graph {
                stats.converged = true;
                break;
            }

            if let Some(cfg) = &self.config.early_stop {
                let ec = graph.edge_count();
                if ec == last_edge_count {
                    stagnation += 1;
                } else {
                    stagnation = 0;
                    last_edge_count = ec;
                }
                if cfg.stagnation_patience > 0 && stagnation >= cfg.stagnation_patience {
                    stats.early_stopped = true;
                    stats.stagnation_detected = true;
                    break;
                }

                if cfg.detect_cycles {
                    let fp = Self::graph_fingerprint(&graph);
                    if seen.contains_key(&fp) {
                        stats.early_stopped = true;
                        stats.cycle_detected = true;
                        break;
                    }
                    seen.insert(fp, iteration + 1);
                    history.push_back(fp);
                    if cfg.cycle_history > 0 {
                        while history.len() > cfg.cycle_history {
                            if let Some(old) = history.pop_front() {
                                seen.remove(&old);
                            }
                        }
                    }
                }
            }
        }

        let clusters = graph.extract_clusters();
        stats.final_edges = graph.edge_count();
        stats.num_clusters = clusters.len();
        stats.num_chains = clusters.iter().filter(|c| c.len() > 1).count();

        let chains = self.graph_to_chains(&graph, &valid_mentions);
        (chains, stats)
    }
}

// =============================================================================
// Evaluation Helpers
// =============================================================================

/// Convert GraphCoref output to format suitable for CoNLL evaluation.
///
/// This produces a `CorefDocument` that can be evaluated with standard
/// coreference metrics (MUC, B³, CEAF, LEA).
///
/// # Example
///
/// ```rust
/// use anno::backends::coref::graph::{GraphCoref, chains_to_document};
/// use anno::Mention;
///
/// let coref = GraphCoref::new();
/// let mentions = vec![
///     Mention::new("John", 0, 4),
///     Mention::new("he", 20, 22),
/// ];
///
/// let chains = coref.resolve(&mentions);
/// let doc = chains_to_document("John went to work. He was late.", chains);
/// ```
pub fn chains_to_document(
    text: impl Into<String>,
    chains: Vec<CorefChain>,
) -> anno_core::CorefDocument {
    anno_core::CorefDocument::new(text, chains)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests;
