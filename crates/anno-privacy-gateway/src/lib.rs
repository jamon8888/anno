//! Local Anthropic-compatible privacy gateway for Cowork.
//!
//! v0.3 owns the privacy boundary and composes with an upstream
//! Anthropic-compatible sidecar such as `anthropic-proxy-rs`.

pub mod anthropic;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod config;
pub mod document_blocks;
pub mod document_extract;
pub mod error;
pub mod file_registry;
pub mod model_catalog;
pub mod openai_compat;
pub mod policy;
pub mod privacy;
pub mod privacy_mode;
pub mod provider;
pub mod server;
pub mod stream;
pub mod subjects;
pub mod upstream;

pub use config::GatewayConfig;
pub use error::{Error, Result};
pub use privacy::{PrivacyEngine, PrivacyReport};
