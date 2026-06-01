//! Source-neutral domain types for Anno's local knowledge service.

pub mod error;
pub mod ids;

pub use error::{KnowledgeCoreError, Result};
pub use ids::{
    AccountId, ChunkId, ObjectId, PartId, RevisionId, ScopeId, SourceId, SourceKindForId,
};
