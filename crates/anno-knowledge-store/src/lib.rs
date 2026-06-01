//! SQLite-backed local knowledge control and FTS store.

pub mod control_store;
pub mod error;
pub mod fts_query;
pub mod migrations;

pub use control_store::{KnowledgeControlStore, TestChunkInput};
pub use error::{KnowledgeStoreError, Result};
