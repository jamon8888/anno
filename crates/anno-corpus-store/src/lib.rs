//! SQLite-backed corpus registry for local MCP scoping.

pub mod error;
pub mod migrations;
pub mod store;

pub use error::{Error, Result};
pub use store::{
    CorpusBindingRow, CorpusDocumentRow, CorpusRow, CorpusStore, CorpusSyncStateRow,
    RegisterCorpusResult,
};
