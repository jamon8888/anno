//! # anno-coalesce
//!
//! Inter-document entity coalescing with adaptive thresholds.
//!
//! This crate provides algorithms for clustering entities across multiple documents,
//! with support for dynamic threshold adjustment based on accumulated evidence.
//!
//! **Extract. Coalesce. Stratify.**
//!
//! ## Key Features
//!
//! - **Basic Resolution**: Cluster entities using string or embedding similarity
//! - **Adaptive Thresholds**: Adjust thresholds based on entity type nameability
//!   and accumulated alignment evidence
//! - **Research-Based**: Implements concepts from cognitive science research on
//!   referential conventions and conceptual alignment
//!
//! ## Basic Example
//!
//! ```rust
//! use anno_coalesce::Resolver;
//! use anno_core::Corpus;
//!
//! let resolver = Resolver::new()
//!     .with_threshold(0.8);
//! let mut corpus = Corpus::new();
//! // ... add documents to corpus ...
//!
//! // Coalesce entities across documents
//! let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
//! ```
//!
//! ## Adaptive Resolution
//!
//! Enable adaptive thresholds to dynamically adjust based on:
//! - **Nameability**: How much naming consensus exists for an entity type
//! - **Alignment Evidence**: How many successful matches a cluster has accumulated
//! - **Generalization Gradient**: Distance-based decay following Shepard's Universal Law
//!
//! ```rust
//! use anno_coalesce::{Resolver, AdaptiveResolutionConfig, GeneralizationGradient};
//!
//! let config = AdaptiveResolutionConfig {
//!     base_threshold: 0.7,
//!     min_threshold: 0.4,
//!     max_adjustment: 0.2,
//!     gradient: GeneralizationGradient::quadratic(),
//!     use_nameability: true,
//! };
//!
//! let resolver = Resolver::new().with_adaptive(config);
//! ```
//!
//! ## Research Background
//!
//! The adaptive threshold feature is based on:
//!
//! - **"Ad hoc conventions generalize to new referents"** (Ji et al., 2025):
//!   Shows that referential conventions reflect conceptual alignment that
//!   generalizes to similar entities, not just arbitrary labels.
//!
//! - **KiloGram Dataset**: Provides Shape Naming Divergence (SND) as a measure
//!   of naming consensus, which informs the [`Nameability`] type.
//!
//! - **Shepard's Universal Law**: The probability of generalizing a learned
//!   response decays nonlinearly with psychological distance.

#![warn(missing_docs)]

pub mod alignment;
pub mod resolver;
pub mod similarity;

pub use alignment::{
    entity_type_nameability, AdaptiveResolutionConfig, AlignmentScore, GeneralizationGradient,
    Nameability, NameabilityLevel,
};
pub use resolver::{embedding_similarity, Resolver};
// Re-export the new multilingual similarity
pub use similarity::{
    jaro_similarity, jaro_winkler_similarity, levenshtein_distance, levenshtein_similarity,
    normalize, string_similarity, Script, Similarity, SimilarityConfig,
};
