//! # anno-coalesce
//!
//! **Cross-document entity resolution: determining when records from different
//! documents refer to the same real-world entity.**
//!
//! This crate provides a suite of algorithms for *entity resolution* (also known as
//! record linkage, deduplication, or entity matching). Given entities extracted from
//! multiple documents, these algorithms cluster mentions that refer to the same
//! underlying entity.
//!
//! **Extract. Coalesce. Stratify.** — This crate implements the "Coalesce" step.
//!
//! ---
//!
//! # Historical Perspective
//!
//! Entity resolution has evolved through several paradigms:
//!
//! ```text
//! 1959  Fellegi-Sunter: probabilistic record linkage theory
//! 1964  Galler-Fischer: union-find data structure
//! 1997  Kehler: probabilistic coreference configurations (Dempster-Shafer)
//! 1997  Broder: MinHash/LSH for scalable similarity
//! 2004  Bansal et al: correlation clustering
//! 2014  Steorts: Bayesian entity resolution with partition priors
//! 2016  Clark & Manning: neural mention-ranking with RL
//! 2017  Lee et al: end-to-end neural coreference
//! 2023  Cohen-Addad et al: 1.73-approximation for correlation clustering
//! 2025  Behnezhad et al: breaking the 3-approximation barrier
//! ```
//!
//! Key insight from Kehler (1997): pairwise decisions are insufficient. When we have
//! mentions A, B, C, D and P(A~B)=0.67, P(B~D)=0.75, P(C~D)=0.50, but A/C and B/C
//! are type-incompatible, the pairwise probabilities *cannot all be satisfied*.
//! This motivates either (a) reasoning over full configurations, or (b) clustering
//! algorithms that implicitly handle transitivity.
//!
//! ---
//!
//! # The Entity Resolution Problem
//!
//! Given \( n \) entity mentions from a corpus, we seek a partition into clusters
//! where each cluster contains all and only the mentions referring to the same
//! real-world entity. Formally:
//!
//! > **Definition.** Let \( M = \{m_1, \ldots, m_n\} \) be a set of mentions.
//! > An *entity resolution* is a partition \( \mathcal{C} = \{C_1, \ldots, C_k\} \)
//! > of \( M \) such that \( m_i, m_j \in C_\ell \) if and only if \( m_i \) and
//! > \( m_j \) refer to the same entity.
//!
//! The challenge: we have only *similarity evidence*, not ground truth. Mentions
//! may have variations ("Barack Obama" vs "obama"), missing information, or
//! ambiguity ("Paris" → city or person?).
//!
//! ---
//!
//! # Modules and Their Mathematical Foundations
//!
//! ## [`resolver`] — Union-Find Batch Resolution
//!
//! **Complexity:** \( O(n^2) \) pairwise comparisons, \( O(m \cdot \alpha(n)) \) clustering
//!
//! Uses the disjoint-set (union-find) data structure with path compression and
//! union-by-rank, achieving amortized \( O(\alpha(n)) \) per operation where
//! \( \alpha \) is the inverse Ackermann function (effectively constant).
//!
//! **Historical note:** Galler & Fischer (1964) introduced disjoint-set forests;
//! Tarjan (1975) proved the \( O(m \cdot \alpha(n)) \) bound is tight.
//!
//! ## [`lsh`] — Locality-Sensitive Hashing
//!
//! **Complexity:** \( O(n \log n) \) expected for candidate generation
//!
//! Reduces the quadratic comparison space to near-linear by hashing similar items
//! to the same bucket with high probability. Based on:
//!
//! - **MinHash** (Broder 1997): \( \Pr[\min(\pi(A)) = \min(\pi(B))] = J(A,B) \)
//!   where \( J \) is Jaccard similarity and \( \pi \) is a random permutation.
//!
//! - **Banding:** With \( b \) bands of \( r \) rows each, the probability of
//!   becoming candidates is \( P = 1 - (1 - s^r)^b \) where \( s \) is similarity.
//!
//! ## [`streaming`] — Incremental Resolution (Doubling Algorithm)
//!
//! **Complexity:** \( O(1) \) amortized per document, 8-approximation guarantee
//!
//! For streaming scenarios where documents arrive continuously. Based on
//! Charikar et al. (1997), maintains active clusters and periodically merges.
//!
//! ## [`correlation`] — Correlation Clustering
//!
//! **Complexity:** \( O(n + m) \) for Pivot algorithm
//!
//! When you have explicit positive/negative edge labels (from a matcher or oracle),
//! correlation clustering finds the partition minimizing *disagreements*:
//!
//! \[
//! \text{cost}(\mathcal{C}) = \sum_{\substack{(u,v) \in E^+ \\ u, v \text{ in different clusters}}} 1
//!                          + \sum_{\substack{(u,v) \in E^- \\ u, v \text{ in same cluster}}} 1
//! \]
//!
//! Algorithms implemented:
//! - **Pivot** (Ailon, Charikar, Newman 2008): 3-approximation
//! - **Modified Pivot** (Behnezhad et al. 2025): Better than 3-approx (~23% fewer errors)
//! - **Min-Max** (2024): 4-approximation, minimizes worst-case per-cluster disagreements
//! - **Chromatic**: Color-constrained clustering (no same-color nodes in cluster)
//! - **Greedy Agglomerative**: Heuristic, often competitive
//!
//! ## [`configuration`] — Probabilistic Configuration Distributions
//!
//! **Complexity:** Exponential in mentions (Bell numbers), but prunable
//!
//! For applications requiring uncertainty quantification (data fusion, active learning),
//! this module provides distributions over coreference *configurations* (partitions).
//! Based on Kehler (1997)'s observation that downstream systems need P(config), not
//! just the most-likely partition.
//!
//! Implements two combination strategies:
//! - **Evidential (Dempster-Shafer)**: Combine pairwise beliefs, normalize conflicts
//! - **Merging Decision**: Model probability of greedy merge sequence
//!
//! ## [`evidence`] — Multi-Source Evidence Aggregation
//!
//! **Complexity:** \( O(k) \) where k is number of evidence sources
//!
//! When multiple signals (string similarity, embeddings, KB links, type matching)
//! provide conflicting evidence, this module mediates them into a single decision.
//! Strategies range from simple averaging to Bayesian combination.
//!
//! ## [`hierarchical`] — Agglomerative Clustering
//!
//! **Complexity:** \( O(n^2 \log n) \) with efficient data structures
//!
//! Produces a *dendrogram* showing hierarchical cluster structure. Uses the
//! Lance-Williams (1967) recurrence formula:
//!
//! \[
//! D_{(ij),k} = \alpha_i D_{ik} + \alpha_j D_{jk} + \beta D_{ij} + \gamma |D_{ik} - D_{jk}|
//! \]
//!
//! Linkage methods:
//! - **Single** (\(\gamma = -1/2\)): min distance, creates chains
//! - **Complete** (\(\gamma = +1/2\)): max distance, compact clusters
//! - **Average (UPGMA)**: balanced, good default
//! - **Ward** (1963): minimizes within-cluster variance
//!
//! ---
//!
//! # Choosing an Approach
//!
//! | Scenario | Module | Time | Space | Guarantee |
//! |----------|--------|------|-------|-----------|
//! | Small corpus (<10K) | [`resolver`] | \( O(n^2) \) | \( O(n) \) | Exact |
//! | Large corpus (10K-1M) | [`lsh`] + [`resolver`] | \( O(n \log n) \) | \( O(n) \) | ~95% recall |
//! | Streaming documents | [`streaming`] | \( O(1) \) amort. | \( O(k) \) | 8-approx |
//! | Explicit +/- labels | [`correlation`] | \( O(n+m) \) | \( O(n+m) \) | 3-approx |
//! | Need dendrogram | [`hierarchical`] | \( O(n^2 \log n) \) | \( O(n^2) \) | Exact |
//!
//! ```text
//! Decision tree:
//!
//! Is real-time processing required?
//! ├─ YES → streaming (Doubling Algorithm)
//! └─ NO → Do you have explicit +/- labels?
//!         ├─ YES → correlation (Pivot)
//!         └─ NO → Is interpretability important?
//!                 ├─ YES → hierarchical (Ward or Average linkage)
//!                 └─ NO → Is n > 10,000?
//!                         ├─ YES → lsh + resolver
//!                         └─ NO → resolver (direct Union-Find)
//! ```
//!
//! ---
//!
//! # Quick Start
//!
//! ## Batch Resolution (Small Corpus)
//!
//! ```
//! use anno_coalesce::Resolver;
//! use anno_core::Corpus;
//!
//! let resolver = Resolver::new().with_threshold(0.7);
//! let mut corpus = Corpus::new();
//! // ... add documents with tracks to corpus ...
//!
//! let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
//! println!("Created {} cross-document identities", identity_ids.len());
//! ```
//!
//! ## LSH Blocking (Large Corpus)
//!
//! ```
//! use anno_coalesce::lsh::{MinHashLSH, LSHConfig};
//!
//! let mut lsh = MinHashLSH::new(LSHConfig::default());
//! lsh.insert_text("1", "Barack Obama");
//! lsh.insert_text("2", "obama");
//! lsh.insert_text("3", "Donald Trump");
//!
//! // Only compare candidate pairs, not all O(n²)
//! for (i, j) in lsh.candidate_pairs() {
//!     if let Some(sim) = lsh.estimated_similarity(i, j) {
//!         if sim > 0.5 {
//!             println!("Likely match: {} and {}", i, j);
//!         }
//!     }
//! }
//! ```
//!
//! ## Streaming Resolution
//!
//! ```rust,ignore
//! use anno_coalesce::streaming::{StreamingResolver, StreamingConfig};
//!
//! let mut resolver = StreamingResolver::new(StreamingConfig::default());
//!
//! // Process entities as they arrive
//! resolver.add_entity("doc1", "Barack Obama", Some("Person".into()));
//! resolver.add_entity("doc2", "obama", Some("Person".into()));
//! resolver.add_entity("doc3", "Donald Trump", Some("Person".into()));
//!
//! assert!(resolver.num_clusters() <= 2); // Obama mentions merged
//! ```
//!
//! ## Correlation Clustering
//!
//! ```
//! use anno_coalesce::correlation::{LabeledGraph, EdgeLabel, pivot_clustering};
//! use rand::SeedableRng;
//!
//! // Graph with explicit +/- edges
//! let mut graph = LabeledGraph::new(4);
//! graph.add_edge(0, 1, EdgeLabel::Positive);  // 0 and 1 should cluster
//! graph.add_edge(2, 3, EdgeLabel::Positive);  // 2 and 3 should cluster
//! graph.add_edge(0, 2, EdgeLabel::Negative);  // 0 and 2 should not
//!
//! let mut rng = rand::rngs::StdRng::seed_from_u64(42);
//! let result = pivot_clustering(&graph, &mut rng);
//! println!("Clusters: {:?}, Cost: {}", result.clusters, result.cost);
//! ```
//!
//! ## Hierarchical Clustering
//!
//! ```
//! use anno_coalesce::hierarchical::{hierarchical_from_similarity, Linkage};
//!
//! let sims = vec![
//!     vec![1.0, 0.9, 0.1],
//!     vec![0.9, 1.0, 0.1],
//!     vec![0.1, 0.1, 1.0],
//! ];
//!
//! let dendrogram = hierarchical_from_similarity(&sims, Linkage::Average);
//! let clusters = dendrogram.cut_to_k_clusters(2);
//! assert_eq!(clusters.len(), 2);
//! ```
//!
//! ---
//!
//! # References
//!
//! ## Foundational
//! - Fellegi & Sunter (1969). "A Theory for Record Linkage". JASA.
//! - Galler & Fischer (1964). "An improved equivalence algorithm"
//! - Tarjan (1975). "Efficiency of a good but not linear set union algorithm"
//!
//! ## Probabilistic Coreference
//! - **Kehler (1997). "Probabilistic Coreference in Information Extraction". ACL.**
//!   *Key paper introducing configuration-level probability distributions and
//!   Dempster-Shafer combination for coreference.*
//! - Dempster (1968). "A Generalization of Bayesian Inference". JRSS.
//! - Steorts (2014). "Entity Resolution with Empirically Motivated Priors". Bayesian Analysis.
//!
//! ## Scalability
//! - Broder (1997). "On the resemblance and containment of documents"
//! - Indyk & Motwani (1998). "Approximate nearest neighbors: towards removing
//!   the curse of dimensionality"
//! - Charikar et al. (1997). "Incremental clustering and dynamic information retrieval"
//!
//! ## Correlation Clustering
//! - Bansal, Blum, Chawla (2004). "Correlation clustering"
//! - Ailon, Charikar, Newman (2008). "Aggregating inconsistent information"
//! - Behnezhad et al. (2025). "Breaking the 3-approximation barrier" (ICML 2025)
//!
//! ## Neural Coreference
//! - Clark & Manning (2016). "Deep Reinforcement Learning for Mention-Ranking". EMNLP.
//! - Lee, He, Lewis, Zettlemoyer (2017). "End-to-End Neural Coreference Resolution". EMNLP.
//! - Meng & Rumshisky (2018). "Triad-based Neural Network for Coreference". COLING.
//!
//! ## Hierarchical Methods
//! - Lance & Williams (1967). "A general theory of classificatory sorting strategies"
//! - Ward (1963). "Hierarchical grouping to optimize an objective function"

#![warn(missing_docs)]

pub mod alignment;
pub mod canonical;
pub mod configuration;
pub mod correlation;
pub mod evidence;
pub mod hierarchical;
pub mod lsh;
pub mod resolver;
pub mod similarity;
pub mod streaming;

pub use alignment::{
    entity_type_nameability, AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient,
    Nameability, NameabilityLevel,
};
pub use canonical::{
    detect_mention_type, is_pronoun, CanonicalSelector, FirstMentionSelector,
    LongestMentionSelector, MentionFeatures, MentionType, NamedFirstSelector,
    SalienceBasedSelector,
};
pub use configuration::{
    bell_number, ConfigurationBuilder, ConfigurationDistribution, CorefConfiguration,
};
pub use correlation::{
    // Chromatic Correlation Clustering: color-constrained clustering
    chromatic_clustering,
    compare_algorithms,
    compare_algorithms_extended,
    greedy_agglomerative,
    // Min-Max Correlation Clustering (2024): minimizes worst-case per-cluster disagreements
    min_max_clustering,
    modified_pivot_clustering,
    pivot_clustering,
    pivot_clustering_best_of,
    ChromaticClusteringConfig,
    ClusteringResult,
    EdgeLabel,
    LabeledGraph,
    MinMaxClusteringResult,
};
pub use evidence::{
    EvidenceSource, MediationStrategy, PairEvidence, TransitivityAnalyzer, TransitivityViolation,
};
pub use hierarchical::{
    cluster_entities, cluster_with_threshold, hierarchical_clustering,
    hierarchical_from_similarity, similarity_to_distance, Dendrogram, DendrogramStep, Linkage,
};
pub use lsh::{LSHConfig, LSHItem, MinHashLSH, SimHashLSH};
pub use resolver::{embedding_similarity, string_similarity, Resolver};
pub use similarity::{
    is_acronym_match, jaro_similarity, jaro_winkler_similarity, levenshtein_distance,
    levenshtein_similarity, multilingual_similarity, normalize, ChainedSynonyms, NoSynonyms,
    Script, Similarity, SimilarityConfig, SynonymMatch, SynonymSource,
};
pub use streaming::{
    trigram_similarity, EntityCluster, EntityMention, StreamingConfig, StreamingResolver,
};
