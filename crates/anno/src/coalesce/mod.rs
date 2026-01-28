//! # `anno::coalesce`
//!
//! Cross-document coalescing groups extracted mentions/tracks into “identities” across documents.
//! This module is implementation-focused: similarity primitives, candidate generation, and
//! clustering utilities used by the `anno` pipeline.
//!
//! ## What is stable
//!
//! - The public surface is the code in this module and its submodules (`lsh`, `resolver`,
//!   `streaming`, `similarity`).
//! - Quantitative claims (runtime/quality) belong in generated eval artifacts, not in doc comments.
//!
//! ## Where to look
//!
//! - `resolver`: batch coalescing utilities
//! - `lsh`: candidate generation for scaling beyond all-pairs comparison
//! - `streaming`: incremental coalescing for arriving documents
//! - `similarity`: string/script heuristics and similarity composition

#![warn(missing_docs)]

pub mod lsh;
pub mod resolver;
pub mod script;
pub mod similarity;
pub mod streaming;

// =============================================================================
// Public surface (keep small; the rest is reachable via submodules)
// =============================================================================

pub use lsh::{LSHConfig, LSHItem, MinHashLSH, SimHashLSH};
pub use resolver::{embedding_similarity, string_similarity, Resolver};
pub use similarity::{
    cross_lingual_similarity, is_acronym_match, jaro_similarity, jaro_winkler_similarity,
    levenshtein_distance, levenshtein_similarity, multilingual_similarity, normalize,
    ChainedSynonyms, NoSynonyms, Script, Similarity, SimilarityConfig, SynonymMatch, SynonymSource,
};
pub use streaming::{
    trigram_similarity, EntityCluster, EntityMention, StreamingConfig, StreamingResolver,
};

// NOTE: additional clustering / evidence aggregation experiments were intentionally
// removed to keep this module paper-grounded and reduce bespoke surface area.
