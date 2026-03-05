//! Exit codes for CLI commands - semantic codes for pipeline orchestration
//!
//! Pipeline orchestrators (make, snakemake, airflow) depend on exit codes.
//! These are semantic, not just 0/1.

/// General/unknown error
pub const ERROR_GENERAL: u8 = 1;

/// Invalid command-line arguments
pub const ERROR_ARGS: u8 = 2;
