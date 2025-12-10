//! Core types for relation extraction.

use anno_core::Entity;

/// A relation triple (head, relation, tail).
#[derive(Debug, Clone, PartialEq)]
pub struct RelationTriple {
    /// Index of head entity in the entity list.
    pub head_idx: usize,
    /// Index of tail entity in the entity list.
    pub tail_idx: usize,
    /// Relation type.
    pub relation: String,
    /// Confidence score [0, 1].
    pub confidence: f32,
}

impl RelationTriple {
    /// Create a new relation triple.
    pub fn new(
        head_idx: usize,
        tail_idx: usize,
        relation: impl Into<String>,
        confidence: f32,
    ) -> Self {
        Self {
            head_idx,
            tail_idx,
            relation: relation.into(),
            confidence,
        }
    }

    /// Create a triple with high confidence (useful for gold data).
    pub fn certain(head_idx: usize, tail_idx: usize, relation: impl Into<String>) -> Self {
        Self::new(head_idx, tail_idx, relation, 1.0)
    }
}

/// A relation type with metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct RelationType {
    /// Relation name (e.g., "works_for").
    pub name: String,
    /// Human-readable label.
    pub label: String,
    /// Valid head entity types.
    pub valid_head_types: Vec<String>,
    /// Valid tail entity types.
    pub valid_tail_types: Vec<String>,
    /// Whether the relation is symmetric.
    pub symmetric: bool,
    /// Inverse relation (if any).
    pub inverse: Option<String>,
}

impl RelationType {
    /// Create a new relation type.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            label: name.clone(),
            name,
            valid_head_types: Vec::new(),
            valid_tail_types: Vec::new(),
            symmetric: false,
            inverse: None,
        }
    }

    /// Set the human-readable label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Set valid head entity types.
    pub fn with_head_types(mut self, types: Vec<String>) -> Self {
        self.valid_head_types = types;
        self
    }

    /// Set valid tail entity types.
    pub fn with_tail_types(mut self, types: Vec<String>) -> Self {
        self.valid_tail_types = types;
        self
    }

    /// Mark as symmetric (A rel B implies B rel A).
    pub fn symmetric(mut self) -> Self {
        self.symmetric = true;
        self
    }

    /// Set the inverse relation.
    pub fn with_inverse(mut self, inverse: impl Into<String>) -> Self {
        self.inverse = Some(inverse.into());
        self
    }

    /// Check if head entity type is valid.
    pub fn is_valid_head(&self, entity_type: &str) -> bool {
        self.valid_head_types.is_empty() || self.valid_head_types.iter().any(|t| t == entity_type)
    }

    /// Check if tail entity type is valid.
    pub fn is_valid_tail(&self, entity_type: &str) -> bool {
        self.valid_tail_types.is_empty() || self.valid_tail_types.iter().any(|t| t == entity_type)
    }
}

/// Configuration for relation extraction.
#[derive(Debug, Clone)]
pub struct RelationConfig {
    /// Confidence threshold for relation extraction.
    pub threshold: f32,
    /// Maximum number of relations to extract per entity pair.
    pub max_relations_per_pair: usize,
    /// Whether to extract symmetric relations in both directions.
    pub bidirectional: bool,
    /// Whether to filter by entity type constraints.
    pub use_type_constraints: bool,
}

impl Default for RelationConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            max_relations_per_pair: 1,
            bidirectional: false,
            use_type_constraints: true,
        }
    }
}

/// Result of relation extraction on a document.
#[derive(Debug, Clone)]
pub struct RelationDocument {
    /// Source text.
    pub text: String,
    /// Extracted entities.
    pub entities: Vec<Entity>,
    /// Extracted triples.
    pub triples: Vec<RelationTriple>,
}

impl RelationDocument {
    /// Create a new relation document.
    pub fn new(
        text: impl Into<String>,
        entities: Vec<Entity>,
        triples: Vec<RelationTriple>,
    ) -> Self {
        Self {
            text: text.into(),
            entities,
            triples,
        }
    }

    /// Get all triples for a given head entity.
    pub fn triples_for_head(&self, head_idx: usize) -> Vec<&RelationTriple> {
        self.triples
            .iter()
            .filter(|t| t.head_idx == head_idx)
            .collect()
    }

    /// Get all triples for a given tail entity.
    pub fn triples_for_tail(&self, tail_idx: usize) -> Vec<&RelationTriple> {
        self.triples
            .iter()
            .filter(|t| t.tail_idx == tail_idx)
            .collect()
    }

    /// Get all triples of a given relation type.
    pub fn triples_of_type(&self, relation: &str) -> Vec<&RelationTriple> {
        self.triples
            .iter()
            .filter(|t| t.relation == relation)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relation_triple() {
        let triple = RelationTriple::new(0, 1, "works_for", 0.95);
        assert_eq!(triple.head_idx, 0);
        assert_eq!(triple.tail_idx, 1);
        assert_eq!(triple.relation, "works_for");
        assert!((triple.confidence - 0.95).abs() < 1e-6);
    }

    #[test]
    fn test_relation_type_constraints() {
        let rel = RelationType::new("works_for")
            .with_head_types(vec!["PERSON".into()])
            .with_tail_types(vec!["ORGANIZATION".into()]);

        assert!(rel.is_valid_head("PERSON"));
        assert!(!rel.is_valid_head("LOCATION"));
        assert!(rel.is_valid_tail("ORGANIZATION"));
    }
}
