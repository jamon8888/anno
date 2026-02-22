//! Pre-computed label embeddings and semantic registry for zero-shot NER.
//!
//! `SemanticRegistry` stores relation labels and class embeddings used by
//! the encoder-based extraction pipeline.

use std::collections::HashMap;

// Semantic Registry (Pre-computed Label Embeddings)
// =============================================================================

/// A frozen, pre-computed registry of entity and relation types.
///
/// # Motivation
///
/// The `SemanticRegistry` is the "knowledge base" of a bi-encoder NER system.
/// It stores pre-computed embeddings for all entity/relation types, enabling:
///
/// - **Zero-shot**: Add new types without retraining
/// - **Speed**: Encode labels once, reuse forever
/// - **Semantics**: Rich descriptions enable better matching
///
/// # Architecture
///
/// ```text
/// ┌────────────────────────────────────────────────────────────────┐
/// │                     SemanticRegistry                           │
/// ├────────────────────────────────────────────────────────────────┤
/// │  labels: [                                                     │
/// │    { slug: "person", description: "named individual human" }   │
/// │    { slug: "organization", description: "company or group" }   │
/// │    { slug: "CEO_OF", description: "leads organization" }       │
/// │  ]                                                             │
/// │                                                                │
/// │  embeddings: [768 floats] [768 floats] [768 floats]            │
/// │              └────┬────┘  └────┬────┘  └────┬────┘             │
/// │                   ▲            ▲            ▲                  │
/// │              person        organization   CEO_OF               │
/// │                                                                │
/// │  label_index: { "person" → 0, "organization" → 1, ... }        │
/// └────────────────────────────────────────────────────────────────┘
/// ```
///
/// # Bi-Encoder Efficiency
///
/// The key insight from GLiNER is that label embeddings can be computed once
/// and reused across all inference requests:
///
/// | Approach | Cost per query | Benefit |
/// |----------|----------------|---------|
/// | Cross-encoder | O(N × L) | Better accuracy |
/// | Bi-encoder | O(N) + O(L) | Much faster, labels cached |
///
/// # Example
///
/// ```ignore
/// use anno::SemanticRegistry;
///
/// // Build registry (expensive, do once at startup)
/// let registry = SemanticRegistry::builder()
///     .add_entity("person", "A named individual human being")
///     .add_entity("organization", "A company, institution, or organized group")
///     .add_relation("CEO_OF", "Chief executive officer of an organization")
///     .build(&label_encoder)?;
///
/// // Use registry for all inference (cheap, cached embeddings)
/// for document in documents {
///     let entities = engine.extract(&document, &registry)?;
/// }
/// ```
///
/// # Adding Custom Types
///
/// ```ignore
/// // Domain-specific medical entities
/// let medical_registry = SemanticRegistry::builder()
///     .add_entity("drug", "A pharmaceutical compound or medication")
///     .add_entity("disease", "A medical condition or illness")
///     .add_entity("gene", "A genetic sequence encoding a protein")
///     .add_relation("TREATS", "Drug is used to treat disease")
///     .add_relation("CAUSES", "Factor causes or leads to condition")
///     .build(&label_encoder)?;
/// ```
#[derive(Debug, Clone)]
pub struct SemanticRegistry {
    /// Pre-computed embeddings for all labels.
    /// Shape: [num_labels, hidden_dim]
    /// Stored as flattened f32 for simplicity without tensor deps.
    pub embeddings: Vec<f32>,
    /// Hidden dimension of embeddings
    pub hidden_dim: usize,
    /// Metadata for each label (index corresponds to embedding row)
    pub labels: Vec<LabelDefinition>,
    /// Index mapping from label slug to embedding row
    pub label_index: HashMap<String, usize>,
}

/// Definition of a semantic label (entity type or relation type).
#[derive(Debug, Clone)]
pub struct LabelDefinition {
    /// Unique identifier (e.g., "person", "CEO_OF")
    pub slug: String,
    /// Human-readable description (used for encoding)
    pub description: String,
    /// Category: Entity or Relation
    pub category: LabelCategory,
    /// Expected source modality
    pub modality: ModalityHint,
    /// Minimum confidence threshold for this label
    pub threshold: f32,
}

/// Category of semantic label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelCategory {
    /// Named entity (Person, Organization, Location, etc.)
    Entity,
    /// Relation between entities (CEO_OF, LOCATED_IN, etc.)
    Relation,
    /// Attribute of an entity (date of birth, revenue, etc.)
    Attribute,
}

/// Hint for which modality this label applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ModalityHint {
    /// Text-only (most entity types)
    #[default]
    TextOnly,
    /// Visual-only (e.g., logos, signatures)
    VisualOnly,
    /// Works with both text and visual
    Any,
}

impl SemanticRegistry {
    /// Create a builder for constructing a registry.
    pub fn builder() -> SemanticRegistryBuilder {
        SemanticRegistryBuilder::new()
    }

    /// Get number of labels in the registry.
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// Get embedding for a label by slug.
    pub fn get_embedding(&self, slug: &str) -> Option<&[f32]> {
        let idx = self.label_index.get(slug)?;
        let start = idx * self.hidden_dim;
        let end = start + self.hidden_dim;
        if end <= self.embeddings.len() {
            Some(&self.embeddings[start..end])
        } else {
            None
        }
    }

    /// Get all entity labels (excluding relations).
    pub fn entity_labels(&self) -> impl Iterator<Item = &LabelDefinition> {
        self.labels
            .iter()
            .filter(|l| l.category == LabelCategory::Entity)
    }

    /// Get all relation labels.
    pub fn relation_labels(&self) -> impl Iterator<Item = &LabelDefinition> {
        self.labels
            .iter()
            .filter(|l| l.category == LabelCategory::Relation)
    }

    /// Create a standard NER registry with common entity types.
    pub fn standard_ner(hidden_dim: usize) -> Self {
        // Placeholder embeddings - in real use, these would be encoder outputs
        let labels = vec![
            LabelDefinition {
                slug: "person".into(),
                description: "A named individual human being".into(),
                category: LabelCategory::Entity,
                modality: ModalityHint::TextOnly,
                threshold: 0.5,
            },
            LabelDefinition {
                slug: "organization".into(),
                description: "A company, institution, agency, or other group".into(),
                category: LabelCategory::Entity,
                modality: ModalityHint::TextOnly,
                threshold: 0.5,
            },
            LabelDefinition {
                slug: "location".into(),
                description: "A geographical place, city, country, or region".into(),
                category: LabelCategory::Entity,
                modality: ModalityHint::TextOnly,
                threshold: 0.5,
            },
            LabelDefinition {
                slug: "date".into(),
                description: "A calendar date or time expression".into(),
                category: LabelCategory::Entity,
                modality: ModalityHint::TextOnly,
                threshold: 0.5,
            },
            LabelDefinition {
                slug: "money".into(),
                description: "A monetary amount with currency".into(),
                category: LabelCategory::Entity,
                modality: ModalityHint::TextOnly,
                threshold: 0.5,
            },
        ];

        let num_labels = labels.len();
        let label_index: HashMap<String, usize> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| (l.slug.clone(), i))
            .collect();

        // Initialize with zeros (placeholder)
        let embeddings = vec![0.0f32; num_labels * hidden_dim];

        Self {
            embeddings,
            hidden_dim,
            labels,
            label_index,
        }
    }
}

/// Builder for SemanticRegistry.
#[derive(Debug, Default)]
pub struct SemanticRegistryBuilder {
    labels: Vec<LabelDefinition>,
}

impl SemanticRegistryBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entity type.
    pub fn add_entity(mut self, slug: &str, description: &str) -> Self {
        self.labels.push(LabelDefinition {
            slug: slug.into(),
            description: description.into(),
            category: LabelCategory::Entity,
            modality: ModalityHint::TextOnly,
            threshold: 0.5,
        });
        self
    }

    /// Add a relation type.
    pub fn add_relation(mut self, slug: &str, description: &str) -> Self {
        self.labels.push(LabelDefinition {
            slug: slug.into(),
            description: description.into(),
            category: LabelCategory::Relation,
            modality: ModalityHint::TextOnly,
            threshold: 0.5,
        });
        self
    }

    /// Add a label with full configuration.
    pub fn add_label(mut self, label: LabelDefinition) -> Self {
        self.labels.push(label);
        self
    }

    /// Build the registry (placeholder - real impl needs encoder).
    pub fn build_placeholder(self, hidden_dim: usize) -> SemanticRegistry {
        let num_labels = self.labels.len();
        let label_index: HashMap<String, usize> = self
            .labels
            .iter()
            .enumerate()
            .map(|(i, l)| (l.slug.clone(), i))
            .collect();

        SemanticRegistry {
            embeddings: vec![0.0f32; num_labels * hidden_dim],
            hidden_dim,
            labels: self.labels,
            label_index,
        }
    }
}

// =============================================================================
