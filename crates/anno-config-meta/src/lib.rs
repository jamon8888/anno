//! Derive macros for `AnnoRagConfig` metadata and CLI args generation.
//!
//! - `#[derive(ConfigMeta)]`     → generates `config_schema() -> &'static [FieldMeta]`
//! - `#[derive(ConfigCliArgs)]`  → generates `ConfigOverrides` clap-compatible struct

mod config_cli_args;
mod config_meta;

use proc_macro::TokenStream;

/// Derive `config_schema() -> &'static [anno_rag::config_meta_types::FieldMeta]`.
///
/// Every field of the annotated struct must carry `#[config_meta(...)]`.
/// Missing the attribute is a compile error.
#[proc_macro_derive(ConfigMeta, attributes(config_meta))]
pub fn derive_config_meta(input: TokenStream) -> TokenStream {
    config_meta::derive(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive `pub struct ConfigOverrides` — a clap-compatible struct with every
/// field wrapped in `Option<T>` and `#[arg(long, env)]` attributes.
///
/// Every field of the annotated struct must carry `#[config_meta(...)]`.
#[proc_macro_derive(ConfigCliArgs, attributes(config_meta))]
pub fn derive_config_cli_args(input: TokenStream) -> TokenStream {
    config_cli_args::derive(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
