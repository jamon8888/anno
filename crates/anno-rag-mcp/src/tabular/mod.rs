//! Tabular review MCP tools — wired into [`crate::AnnoRagServer`].
//!
//! [`PipelineChunkSource`] adapts [`anno_rag::pipeline::Pipeline`] to the
//! `anno_rag_tabular::extract::ChunkSource` trait so the extraction engine
//! can fetch pseudonymized chunks from the existing anno-rag index.

pub mod chunk_source;
pub use chunk_source::PipelineChunkSource;
