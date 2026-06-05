//! anno-rag — local GDPR-compliant document anonymizer + RAG service for French legal docs.
//!
//! v0.1 walking skeleton: ingest a folder, anonymize PII, index in LanceDB, search.

#![warn(missing_docs)]

pub mod accelerator;
pub mod bench_cli;
pub mod canonicalize;
pub mod config;
pub mod conflict;
pub mod detect;
pub mod download_models;
pub mod embed;
#[cfg(test)]
pub(crate) mod env_guard;
pub mod error;
pub mod eval;
pub mod ingest;
pub mod knowledge_privacy;
pub mod layers;
pub mod legal;
pub mod memory;
#[cfg(test)]
pub(crate) mod ocr;
pub mod pii_eval;
pub mod pipeline;
#[cfg(feature = "rerank")]
pub mod rerank;
pub mod store;
pub mod validators;
pub mod vault;
pub mod vault_admin;

pub use config::AnnoRagConfig;
pub use error::{Error, Result};
pub use pipeline::Pipeline;
