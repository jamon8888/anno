//! Runtime types for config schema metadata.

/// Metadata for a single `AnnoRagConfig` field.
#[derive(Debug, Clone)]
pub struct FieldMeta {
    /// The Rust field name.
    pub name: &'static str,
    /// The environment variable name (e.g. `ANNO_RAG_LANCEDB_PATH`).
    pub env_var: &'static str,
    /// The CLI long flag (e.g. `--lancedb-path`).
    pub cli_flag: &'static str,
    /// Human-readable description.
    pub doc: &'static str,
    /// Serialised default value (empty string if no default).
    pub default_value: &'static str,
    /// Semver at which this field was introduced.
    pub since: &'static str,
    /// Rust type as a string (spaces stripped).
    pub type_name: &'static str,
}
