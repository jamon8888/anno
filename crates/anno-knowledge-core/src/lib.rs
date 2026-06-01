//! Source-neutral domain types for Anno's local knowledge service.

pub mod error;
pub mod ids;
pub mod object;
pub mod query;
pub mod source;
pub mod status;

pub use error::{KnowledgeCoreError, Result};
pub use ids::{
    AccountId, ChunkId, ObjectId, PartId, RevisionId, ScopeId, SourceId, SourceKindForId,
};
pub use object::{
    KnowledgeObject, KnowledgePart, KnowledgeRevision, ObjectState, ObjectType, PartType,
};
pub use query::{KnowledgeSearchHit, KnowledgeSearchMode, KnowledgeSearchRequest};
pub use source::{KnowledgeSource, ScopeKind, SourceAccount, SourceKind, SourceScope, SyncPolicy};
pub use status::KnowledgeStatus;
