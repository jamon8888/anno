//! Document ingestion and text preparation.
//!
//! Provides text-to-text utilities (HTML stripping, preprocessing).
//! URL resolution and format conversion live in the CLI crate (`anno-cli`).

/// HTML-to-text conversion and detection utilities.
pub mod html;
pub mod preprocessor;

pub use html::{looks_like_html, strip_html_to_text};
pub use preprocessor::{DocumentPreprocessor, PreparedDocument};
