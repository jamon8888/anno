//! SQLite-backed local knowledge control and FTS store.

pub mod control_store;
pub mod error;
pub mod fts_query;
pub mod migrations;

pub use control_store::{
    hex32, CommitChunk, CommitObjectInput, KnowledgeControlStore, LocalFolderRegistered,
    LocalFolderRegistration, ScopeRow, SourceRow, TestChunkInput,
};
pub use error::{KnowledgeStoreError, Result};
