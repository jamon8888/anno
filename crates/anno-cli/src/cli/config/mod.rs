//! Configuration management for CLI.
//!
//! Handles loading, saving, and merging workflow configurations.
//!
//! # Configuration Sources (Priority Order)
//!
//! 1. Command-line arguments (highest priority)
//! 2. Environment variables (`ANNO_*`)
//! 3. Project config file (`.anno.toml` in current directory)
//! 4. User config file (`~/.config/anno/config.toml`)
//! 5. Built-in defaults (lowest priority)
//!
//! # Example Configuration
//!
//! ```toml
//! # .anno.toml
//! [extraction]
//! backend = "pattern"
//! min_confidence = 0.7
//!
//! [output]
//! format = "json"
//! color = true
//! ```
//!
//! See [`crate::cli::commands::config`] for the command implementation.

// Configuration loading logic is currently inlined in individual command modules.
// This module provides the doc-level overview; implementations live in commands.
