//! CloakTree — Vectorless, reasoning-based retrieval with privacy-first indexing.
//!
//! Inspired by PageIndex (VectifyAI), CloakTree builds hierarchical tree indices
//! from documents and uses LLM-driven tree search for retrieval. This eliminates
//! 3 out of 4 data leak points in traditional RAG pipelines:
//!
//! - No embedding API calls (no LEAK #1)
//! - No vector database (no LEAK #2, no embedding inversion risk)
//! - No query embedding (no LEAK #3)
//! - Only the final LLM generation call remains (LEAK #4, pseudonymized by CloakPipe)
//!
//! ## Architecture
//!
//! 1. **Indexer**: Ingests documents and builds hierarchical JSON tree indices
//! 2. **Search**: LLM reasons over tree structure to find relevant nodes
//! 3. **Extractor**: Pulls full text from selected nodes for context
//! 4. **Storage**: Persists tree indices as local JSON files

pub mod tree;
pub mod indexer;
pub mod search;
pub mod extractor;
pub mod storage;
pub mod parser;

pub use tree::{TreeIndex, TreeNode, NodeSummary};
pub use indexer::TreeIndexer;
pub use search::TreeSearcher;
