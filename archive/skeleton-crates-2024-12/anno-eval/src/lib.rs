//! Convenience re-export crate for anno's evaluation framework.
//!
//! # Overview
//!
//! This crate provides a convenient way to use anno's evaluation functionality.
//! It re-exports `anno::eval` and related types, so you can use evaluation
//! features without needing to specify the `eval` feature flag directly.
//!
//! # Usage
//!
//! ```toml
//! [dependencies]
//! anno-eval = "0.2"
//! ```
//!
//! This is equivalent to:
//!
//! ```toml
//! [dependencies]
//! anno = { version = "0.2", features = ["eval"] }
//! ```
//!
//! # Why This Crate Exists
//!
//! 1. **Convenience**: Users who primarily need evaluation don't need to remember
//!    the feature flag.
//!
//! 2. **Cleaner dependencies**: For crates that only need evaluation, this makes
//!    the Cargo.toml more explicit about intent.
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_eval::{evaluate_ner_model, GoldEntity};
//! use anno::RegexNER;
//!
//! let model = RegexNER::new();
//! let test_cases = vec![
//!     ("Meeting on January 15".to_string(), vec![
//!         GoldEntity::new("January 15", anno::EntityType::Date, 11),
//!     ]),
//! ];
//!
//! let results = evaluate_ner_model(&model, &test_cases)?;
//! println!("F1: {:.1}%", results.f1 * 100.0);
//! ```

// Re-export the entire eval module from anno
pub use anno::eval::*;

// Also re-export commonly used types from anno itself for convenience
pub use anno::{Entity, EntityType, Error, Model, Result};

// Re-export anno-core types that are commonly used with eval
pub use anno_core::{CoreferenceResolver, MentionType, PhiFeatures};
