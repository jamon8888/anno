//! Local folder source connector for Anno knowledge indexing.
//!
//! This crate is pure discovery + change detection. It does NOT load models,
//! call Kreuzberg, or touch SQLite. It depends only on `anno-knowledge-core`.

pub mod error;
pub mod folder;

pub use error::{LocalSourceError, Result};
pub use folder::{DiscoverBudget, DiscoveredObject, LocalFolderSource};
