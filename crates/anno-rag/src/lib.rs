//! anno-rag — local GDPR-compliant document anonymizer + RAG service for French legal docs.
//!
//! v0.1 walking skeleton: ingest a folder, anonymize PII, index in LanceDB, search.
//!
//! # GDPR Detection Layers
//!
//! Entity detection uses a tiered, configurable approach via the `ANNO_GDPR_LAYERS` environment variable:
//!
//! | Layer | Features | Use Case |
//! |-------|----------|----------|
//! | `basic` | Regex + GLiNER2 NER | Original v0.1 behavior, high recall |
//! | `defense` | + FR heuristics + validators | Production: balanced precision/recall |
//! | `shadow` | + multi-task composition | Phase C: experimental calibration |
//! | `full` | + calibration + review queue | Phase D: operator-driven curation |
//!
//! **Default:** `defense`
//!
//! Validators (Luhn, IBAN mod-97, NIR, date range, IP, email, postal code) run on defense+ layers.
//! Heuristics (SAS/SARL orgs, FR addresses, dates with context, intl IBANs) run on defense+ layers.

#![warn(missing_docs)]

pub mod accelerator;
pub mod bench_cli;
pub mod canonicalize;
pub mod config;
pub mod config_meta_types;
pub mod conflict;
pub mod detect;
pub mod docx_instructions;
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
pub mod model_cache;
pub mod pii_eval;
pub mod pipeline;
pub mod privacy_artifacts;
pub mod privacy_decisions;
pub mod privacy_docx;
pub mod privacy_workspace;
#[cfg(feature = "rerank")]
pub mod rerank;
pub mod store;
pub mod validators;
pub mod vault;
pub mod vault_admin;

pub use config::AnnoRagConfig;
pub use error::{Error, Result};
pub use pipeline::{Pipeline, WarmupOutcome};
