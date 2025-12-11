//! # anno-rel
//!
//! Relation extraction for NLP: extract (head, relation, tail) triples from text.
//!
//! This crate provides models and utilities for extracting semantic relationships
//! between entity pairs, producing knowledge graph triples.
//!
//! # Pipeline Position
//!
//! ```text
//! Text → NER (anno-models) → Relation Extraction (anno-rel) → Knowledge Graph
//!             ↓                        ↓
//!        [entities]           [(head, relation, tail)]
//! ```
//!
//! # Relation Types
//!
//! Common relation extraction scenarios:
//!
//! | Domain | Example Relations |
//! |--------|-------------------|
//! | News | works_for, born_in, headquartered_in |
//! | Biomedical | treats, causes, interacts_with |
//! | Scientific | cites, extends, contradicts |
//! | Legal | party_to, jurisdiction, appeals_to |
//!
//! # Models
//!
//! - **TPLinker**: Joint entity-relation extraction using handshaking tagging
//! - **W2NER**: Word-word relation classification
//! - **PURE**: Pipeline approach (NER first, then relation)
//! - **DocRED**: Document-level relation extraction
//!
//! # Example
//!
//! ```rust,ignore
//! use anno_rel::{RelationExtractor, TPLinker};
//! use anno_models::OnnxRuntime;
//!
//! let runtime = OnnxRuntime::new()?;
//! let extractor = TPLinker::new(runtime)?;
//!
//! let result = extractor.extract(
//!     "Steve Jobs founded Apple in 1976.",
//!     &["person", "organization"],
//!     &["founded", "works_for"],
//! )?;
//!
//! for triple in &result.triples {
//!     println!("{} --[{}]--> {}",
//!         result.entities[triple.head].text,
//!         triple.relation,
//!         result.entities[triple.tail].text
//!     );
//! }
//! ```

#![warn(missing_docs)]

pub mod eval;
pub mod extractor;
pub mod types;

pub use eval::{evaluate_relations, RelationMetrics};
pub use extractor::{ExtractionResult, RelationExtractor};
pub use types::{RelationConfig, RelationTriple, RelationType};
