//! CLI module for anno binary
//!
//! This module contains the command-line interface structure, argument parsing,
//! and command routing. Individual command implementations are in the `commands` submodule.

/// Cache management utilities.
pub mod cache;
pub mod commands;
/// Configuration management.
pub mod config;
/// CLI error handling.
pub mod error;
pub mod exit_codes;
/// Document ingestion: URL resolution, format conversion, file reading.
pub mod ingest;
pub mod output;
pub mod parser;
pub mod utils;

pub use error::CliError;
