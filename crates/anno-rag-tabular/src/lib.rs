//! anno-rag-tabular — Harvey/Legora-style tabular review for legal docs.
//!
//! Provides schema-driven extraction with per-cell citations, extractive
//! verifier, conditional columns, and LanceDB storage alongside the
//! existing chunks index.
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `schema` | Column definitions, `CellType`, JSON schema generation |
//! | `llm` | `LlmClient` trait, `AnthropicLlm`, `MockLlm` |
//! | `extract` | `Extractor` — column batching, LLM call, cell parsing |
//! | `verify` | Offset/quote round-trip + cross-encoder support scoring |
//! | `storage` | LanceDB tables: reviews, columns, rows, cells |
//! | `fanout` | Per-review concurrent row extraction with `run_review` |
//!
//! ## Phase 2 (not yet implemented)
//!
//! - `export` — CSV / XLSX / Markdown export (`rust_xlsxwriter` dep already declared)
//! - `anno-rag-mcp` tabular module — 7 MCP tools + 3 resource families
//! - `anno-rag-tabular-ui` — ag-grid MCP App bundle

pub mod error;
pub mod ids;
pub use error::{Error, Result};
pub use ids::{ColumnId, ReviewId, RowId};

pub mod schema;
pub use schema::CellType;

pub mod storage;

pub mod llm;

pub mod extract;

pub mod verify;

pub mod fanout;
pub use fanout::{run_review, FanoutConfig, RowOutcome};

pub mod export;
