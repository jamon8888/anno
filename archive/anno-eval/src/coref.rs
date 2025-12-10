//! Coreference resolution data structures.
//!
//! This module re-exports the canonical coref types from `anno-core`.
//! See [`anno_core::coref`] for the full documentation.
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_eval::coref::{Mention, CorefChain, CorefDocument};
//!
//! let john = Mention::new("John", 0, 4);
//! let he = Mention::new("He", 25, 27);
//!
//! let chain = CorefChain::new(vec![john, he]);
//! assert_eq!(chain.len(), 2);
//! ```

// Re-export all coref types from anno_core
pub use anno_core::coref::*;
