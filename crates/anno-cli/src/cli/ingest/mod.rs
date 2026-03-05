//! CLI-side document ingestion: URL resolution, format conversion, and file reading.
//!
//! The anno library takes `&str` (clean text). This module handles converting
//! URLs, HTML files, PDFs, and other formats into plain text for the library.

pub mod url_resolver;

pub use url_resolver::{CompositeResolver, UrlResolver};
