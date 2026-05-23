//! Legal RAG domain layer: extraction labels, metadata, storage, search, graph.

pub mod audit;
pub mod codes;
pub mod courts;
pub mod enricher;
pub mod eval;
pub mod extract;
pub mod kg;
pub mod mandatory;
pub mod normalize;
pub mod offsets;
pub mod prescription;
pub mod query;
pub mod rules;
pub mod status;
pub mod store;
pub mod types;

pub use types::{
    default_legal_labels, default_thresholds, ArticleRef, CourtLevel, CourtRef, ExtractedFact,
    LegalChunkEnrichment, LegalEntity, LegalLabel, LegalSearchFilters, LegalSearchHit, PartyKind,
    PrescStart,
};
