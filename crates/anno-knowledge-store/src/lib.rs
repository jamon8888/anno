//! SQLite-backed local knowledge control and FTS store.

pub mod error;
pub mod migrations;

pub use error::{KnowledgeStoreError, Result};
