//! CLI-side document ingestion: URL resolution, format conversion, and file reading.
//!
//! The anno library takes `&str` (clean text). This module handles converting
//! URLs, HTML files, PDFs, and other formats into plain text for the library.

#[cfg(feature = "eval")]
pub mod url_resolver;

#[cfg(feature = "eval")]
pub use url_resolver::{CompositeResolver, UrlResolver};
