//! Core extraction traits: ZeroShotNER, RelationExtractor, RelationCapable defaults,
//! and DiscontinuousNER.

#[allow(unused_imports)]
use crate::{Entity, EntityType, Relation};

// Zero-Shot NER Trait
// =============================================================================

/// Zero-shot NER for open entity types.
///
/// # Motivation
///
/// Traditional NER models are trained on fixed taxonomies (PER, ORG, LOC, etc.)
/// and cannot extract new entity types without retraining. Zero-shot NER solves
/// this by allowing **arbitrary entity types at inference time**.
///
/// Instead of asking "is this a PERSON?", zero-shot NER asks "does this text
/// span match the description 'a named individual human being'?"
///
/// # Use Cases
///
/// - **Domain adaptation**: Extract "gene names" or "legal citations" without
///   training data
/// - **Custom taxonomies**: Use your own entity hierarchy
/// - **Rapid prototyping**: Test new entity types before investing in annotation
///
/// # Research Alignment
///
/// From GLiNER (arXiv:2311.08526):
/// > "NER model capable of identifying any entity type using a bidirectional
/// > transformer encoder... provides a practical alternative to traditional
/// > NER models, which are limited to predefined entity types."
///
/// From UniversalNER (arXiv:2308.03279):
/// > "Large language models demonstrate remarkable generalizability, such as
/// > understanding arbitrary entities and relations."
///
/// # Example
///
/// ```ignore
/// use anno::ZeroShotNER;
///
/// fn extract_medical_entities(ner: &dyn ZeroShotNER, clinical_note: &str) {
///     // Define custom medical entity types at runtime
///     let types = &["drug name", "disease", "symptom", "dosage"];
///
///     let entities = ner.extract_with_types(clinical_note, types, 0.5).unwrap();
///     for e in entities {
///         println!("{}: {} (conf: {:.2})", e.entity_type, e.text, e.confidence);
///     }
/// }
///
/// fn extract_with_descriptions(ner: &dyn ZeroShotNER, text: &str) {
///     // Even richer: use natural language descriptions
///     let descriptions = &[
///         "a medication or pharmaceutical compound",
///         "a medical condition or illness",
///         "a physical sensation indicating illness",
///     ];
///
///     let entities = ner.extract_with_descriptions(text, descriptions, 0.5).unwrap();
/// }
/// ```
pub trait ZeroShotNER: Send + Sync {
    /// Extract entities with custom types.
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `entity_types` - Entity type descriptions (arbitrary text, not fixed vocabulary)
    ///   - Encoded as text embeddings via bi-encoder (semantic matching, not exact string match)
    ///   - Any string works: `"disease"`, `"pharmaceutical compound"`, `"19th century French philosopher"`
    ///   - **Replaces default types completely** - model only extracts the specified types
    ///   - To include defaults, pass them explicitly: `&["person", "organization", "disease"]`
    /// * `threshold` - Confidence threshold (0.0 - 1.0)
    ///
    /// # Returns
    /// Entities with their matched types
    ///
    /// # Behavior
    ///
    /// - **Arbitrary text**: Type hints are not fixed vocabulary. They're encoded as embeddings,
    ///   so semantic similarity determines matches (not exact string matching).
    /// - **Replace, don't union**: This method completely replaces default entity types.
    ///   The model only extracts the types you specify.
    /// - **Semantic matching**: Uses cosine similarity between text span embeddings and label embeddings.
    fn extract_with_types(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<Entity>>;

    /// Extract entities with natural language descriptions.
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `descriptions` - Natural language descriptions of what to extract
    ///   - Encoded as text embeddings (same as `extract_with_types`)
    ///   - Examples: `"companies headquartered in Europe"`, `"diseases affecting the heart"`
    ///   - **Replaces default types completely** - model only extracts the specified descriptions
    /// * `threshold` - Confidence threshold
    ///
    /// # Behavior
    ///
    /// Same as `extract_with_types`, but accepts natural language descriptions instead of
    /// short type labels. Both methods encode labels as embeddings and use semantic matching.
    fn extract_with_descriptions(
        &self,
        text: &str,
        descriptions: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<Entity>>;

    /// Get default entity types for this model.
    ///
    /// Returns the entity types used by `extract_entities()` (via `Model` trait).
    /// Useful for extending defaults: combine with custom types and pass to `extract_with_types()`.
    ///
    /// # Example: Extending defaults
    ///
    /// ```ignore
    /// use anno::ZeroShotNER;
    ///
    /// let ner: &dyn ZeroShotNER = ...;
    /// let defaults = ner.default_types();
    ///
    /// // Combine defaults with custom types
    /// let mut types: Vec<&str> = defaults.to_vec();
    /// types.extend(&["disease", "medication"]);
    ///
    /// let entities = ner.extract_with_types(text, &types, 0.5)?;
    /// ```
    fn default_types(&self) -> &[&'static str];
}

// =============================================================================
// Relation Extractor Trait
// =============================================================================

/// Joint entity and relation extraction.
///
/// # Motivation
///
/// Real-world information extraction often requires both entities AND their
/// relationships. For example, extracting "Steve Jobs" and "Apple" is useful,
/// but knowing "Steve Jobs FOUNDED Apple" is far more valuable.
///
/// Joint extraction (vs pipeline) is preferred because:
/// - **Error propagation**: Pipeline errors compound (bad entities → bad relations)
/// - **Shared context**: Entities and relations inform each other
/// - **Efficiency**: Single forward pass instead of two
///
/// # Architecture
///
/// ```text
/// Input: "Steve Jobs founded Apple in 1976."
///                │
///                ▼
/// ┌──────────────────────────────────┐
/// │     Shared Encoder (BERT)        │
/// └──────────────────────────────────┘
///                │
///         ┌──────┴──────┐
///         ▼             ▼
/// ┌───────────────┐  ┌───────────────┐
/// │ Entity Head   │  │ Relation Head │
/// │ (span class.) │  │ (pair class.) │
/// └───────┬───────┘  └───────┬───────┘
///         │                  │
///         ▼                  ▼
/// Entities:              Relations:
/// - Steve Jobs [PER]     - (Steve Jobs, FOUNDED, Apple)
/// - Apple [ORG]          - (Apple, FOUNDED_IN, 1976)
/// - 1976 [DATE]
/// ```
///
/// # Research Alignment
///
/// From GLiNER multi-task (arXiv:2406.12925):
/// > "Generalist Lightweight Model for Various Information Extraction Tasks...
/// > joint entity and relation extraction."
///
/// From W2NER (arXiv:2112.10070):
/// > "Unified Named Entity Recognition as Word-Word Relation Classification...
/// > handles flat, overlapped, and discontinuous NER."
///
/// # Example
///
/// ```ignore
/// use anno::RelationExtractor;
///
/// fn build_knowledge_graph(extractor: &dyn RelationExtractor, text: &str) {
///     let entity_types = &["person", "organization", "date"];
///     let relation_types = &["founded", "works_for", "acquired"];
///
///     let result = extractor.extract_with_relations(
///         text, entity_types, relation_types, 0.5
///     ).unwrap();
///
///     // Build graph nodes from entities
///     for e in &result.entities {
///         println!("Node: {} ({})", e.text, e.entity_type);
///     }
///
///     // Build graph edges from relations
///     for r in &result.relations {
///         let head = &result.entities[r.head_idx];
///         let tail = &result.entities[r.tail_idx];
///         println!("Edge: {} --[{}]--> {}", head.text, r.relation_type, tail.text);
///     }
/// }
/// ```
pub trait RelationExtractor: Send + Sync {
    /// Extract entities and relations jointly.
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `entity_types` - Entity types to extract
    /// * `relation_types` - Relation types to extract
    /// * `threshold` - Confidence threshold
    ///
    /// # Returns
    /// Entities and relations between them
    fn extract_with_relations(
        &self,
        text: &str,
        entity_types: &[&str],
        relation_types: &[&str],
        threshold: f32,
    ) -> crate::Result<ExtractionWithRelations>;
}

/// Output from joint entity-relation extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractionWithRelations {
    /// Extracted entities
    pub entities: Vec<Entity>,
    /// Relations between entities (indices into entities vec)
    pub relations: Vec<RelationTriple>,
}

/// A relation triple linking two entities.
#[derive(Debug, Clone)]
pub struct RelationTriple {
    /// Index of head entity in entities vec
    pub head_idx: usize,
    /// Index of tail entity in entities vec
    pub tail_idx: usize,
    /// Relation type
    pub relation_type: String,
    /// Confidence score
    pub confidence: f32,
}

// =============================================================================
// Shared defaults for RelationCapable::extract_with_relations
// =============================================================================

/// Broad default entity types for the no-arg `RelationCapable` convenience interface.
///
/// These cover the most common NER taxonomies (CoNLL, OntoNotes, ACE). Callers that need
/// precise control should use `RelationExtractor::extract_with_relations` directly.
pub(crate) const DEFAULT_ENTITY_TYPES: &[&str] =
    &["person", "organization", "location", "date", "product", "event"];

/// Broad default relation types for the no-arg `RelationCapable` convenience interface.
pub(crate) const DEFAULT_RELATION_TYPES: &[&str] = &[
    "founded",
    "works_for",
    "located_in",
    "acquired",
    "born_in",
    "member_of",
    "ceo_of",
    "part_of",
    "subsidiary_of",
];

impl ExtractionWithRelations {
    /// Convert index-based `RelationTriple`s into owned `anno_core::Relation` values.
    ///
    /// Indices that are out-of-bounds (should not happen but can in malformed output) are
    /// silently dropped.
    #[must_use]
    pub fn into_anno_relations(self) -> (Vec<Entity>, Vec<crate::Relation>) {
        let relations = self
            .relations
            .iter()
            .filter_map(|t| {
                let head = self.entities.get(t.head_idx)?.clone();
                let tail = self.entities.get(t.tail_idx)?.clone();
                Some(crate::Relation::new(
                    head,
                    tail,
                    t.relation_type.clone(),
                    t.confidence as f64,
                ))
            })
            .collect();
        (self.entities, relations)
    }
}

// =============================================================================
// Discontinuous Entity Support (W2NER Research)
// =============================================================================

/// Support for discontinuous entity spans.
///
/// # Motivation
///
/// Not all entities are contiguous text spans. In coordination structures,
/// entities can be **discontinuous** - scattered across non-adjacent positions.
///
/// # Examples of Discontinuous Entities
///
/// ```text
/// "New York and Los Angeles airports"
///  ^^^^^^^^     ^^^^^^^^^^^ ^^^^^^^^
///  └──────────────────────────┘
///     LOCATION: "New York airports" (discontinuous!)
///                ^^^^^^^^^^^ ^^^^^^^^
///                └───────────┘
///                LOCATION: "Los Angeles airports" (contiguous)
///
/// "protein A and B complex"
///  ^^^^^^^^^ ^^^ ^^^^^^^^^
///  └────────────────────┘
///     PROTEIN: "protein A ... complex" (discontinuous!)
/// ```
///
/// # NER Complexity Hierarchy
///
/// | Type | Description | Example |
/// |------|-------------|---------|
/// | Flat | Non-overlapping spans | "John works at Google" |
/// | Nested | Overlapping spans | "\[New \[York\] City\]" |
/// | Discontinuous | Non-contiguous | "New York and LA \[airports\]" |
///
/// # Research Alignment
///
/// From W2NER (arXiv:2112.10070):
/// > "Named entity recognition has been involved with three major types,
/// > including flat, overlapped (aka. nested), and discontinuous NER...
/// > we propose a novel architecture to model NER as word-word relation
/// > classification."
///
/// W2NER achieves this by building a **handshaking matrix** where each cell
/// (i, j) indicates whether tokens i and j are part of the same entity.
///
/// # Example
///
/// ```ignore
/// use anno::DiscontinuousNER;
///
/// fn extract_complex_entities(ner: &dyn DiscontinuousNER, text: &str) {
///     let types = &["location", "protein"];
///     let entities = ner.extract_discontinuous(text, types, 0.5).unwrap();
///
///     for e in entities {
///         if e.is_contiguous() {
///             println!("Contiguous {}: '{}'", e.entity_type, e.text);
///         } else {
///             println!("Discontinuous {}: '{}' spans: {:?}",
///                      e.entity_type, e.text, e.spans);
///         }
///     }
/// }
/// ```
pub trait DiscontinuousNER: Send + Sync {
    /// Extract entities including discontinuous spans.
    ///
    /// # Arguments
    /// * `text` - Input text
    /// * `entity_types` - Entity types to extract
    /// * `threshold` - Confidence threshold
    ///
    /// # Returns
    /// Entities, potentially with multiple non-contiguous spans
    fn extract_discontinuous(
        &self,
        text: &str,
        entity_types: &[&str],
        threshold: f32,
    ) -> crate::Result<Vec<DiscontinuousEntity>>;
}

/// An entity that may span multiple non-contiguous regions.
#[derive(Debug, Clone)]
pub struct DiscontinuousEntity {
    /// The spans that make up this entity (may be non-contiguous)
    pub spans: Vec<(usize, usize)>,
    /// Concatenated text from all spans
    pub text: String,
    /// Entity type
    pub entity_type: String,
    /// Confidence score
    pub confidence: f32,
}

impl DiscontinuousEntity {
    /// Check if this entity is contiguous (single span).
    pub fn is_contiguous(&self) -> bool {
        self.spans.len() == 1
    }

    /// Convert to a standard Entity if contiguous.
    pub fn to_entity(&self) -> Option<Entity> {
        if self.is_contiguous() {
            let (start, end) = self.spans[0];
            Some(Entity::new(
                self.text.clone(),
                EntityType::from_label(&self.entity_type),
                start,
                end,
                self.confidence as f64,
            ))
        } else {
            None
        }
    }
}

// =============================================================================
