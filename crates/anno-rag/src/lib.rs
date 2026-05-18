//! anno-rag — local GDPR-compliant document anonymizer + RAG service for French legal docs.
//!
//! v0.1 walking skeleton: ingest a folder, anonymize PII, index in LanceDB, search.

#![warn(missing_docs)]

pub mod bench_cli;
pub mod canonicalize;
pub mod config;
pub mod conflict;
pub mod detect;
pub mod embed;
pub mod error;
pub mod eval;
pub mod ingest;
pub mod memory;
pub(crate) mod ocr;
pub mod pii_eval;
pub mod pipeline;
pub mod store;
pub mod vault;

pub use config::AnnoRagConfig;
pub use error::{Error, Result};
pub use pipeline::Pipeline;
