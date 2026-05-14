//! Local Anthropic-compatible privacy gateway for Cowork.
//!
//! v0.3 owns the privacy boundary and composes with an upstream
//! Anthropic-compatible sidecar such as `anthropic-proxy-rs`.

pub mod anthropic;
pub mod audit;
pub mod config;
pub mod error;
pub mod policy;
pub mod privacy;
pub mod server;
pub mod stream;
pub mod upstream;

pub use config::GatewayConfig;
pub use error::{Error, Result};
pub use privacy::{PrivacyEngine, PrivacyReport};
