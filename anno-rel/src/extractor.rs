//! Relation extractor trait and implementations.

use crate::types::{RelationConfig, RelationTriple};
use anno_core::Entity;
use std::fmt::Debug;

/// Error type for relation extraction.
#[derive(Debug, Clone)]
pub enum RelationError {
    /// Model initialization failed.
    InitError(String),
    /// Extraction failed.
    ExtractionError(String),
    /// Invalid input.
    InvalidInput(String),
}

impl std::fmt::Display for RelationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitError(msg) => write!(f, "Init error: {}", msg),
            Self::ExtractionError(msg) => write!(f, "Extraction error: {}", msg),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for RelationError {}

/// Result of relation extraction.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Extracted entities.
    pub entities: Vec<Entity>,
    /// Extracted relation triples.
    pub triples: Vec<RelationTriple>,
}

impl ExtractionResult {
    /// Create a new extraction result.
    pub fn new(entities: Vec<Entity>, triples: Vec<RelationTriple>) -> Self {
        Self { entities, triples }
    }

    /// Check if any relations were extracted.
    pub fn has_relations(&self) -> bool {
        !self.triples.is_empty()
    }

    /// Get the number of unique relation types.
    pub fn num_relation_types(&self) -> usize {
        let mut types: Vec<&str> = self.triples.iter().map(|t| t.relation.as_str()).collect();
        types.sort();
        types.dedup();
        types.len()
    }
}

/// Trait for relation extractors.
pub trait RelationExtractor: Debug + Send + Sync {
    /// Get the extractor name.
    fn name(&self) -> &str;

    /// Extract relations from text.
    ///
    /// # Arguments
    /// - `text`: Input text
    /// - `entity_types`: Entity types to extract (for NER)
    /// - `relation_types`: Relation types to extract
    ///
    /// # Returns
    /// Extraction result with entities and relation triples.
    fn extract(
        &self,
        text: &str,
        entity_types: &[&str],
        relation_types: &[&str],
    ) -> Result<ExtractionResult, RelationError>;

    /// Extract relations given pre-extracted entities.
    ///
    /// This is useful when entities are already extracted by a separate NER model.
    fn extract_with_entities(
        &self,
        text: &str,
        entities: &[Entity],
        relation_types: &[&str],
    ) -> Result<Vec<RelationTriple>, RelationError>;

    /// Get the configuration.
    fn config(&self) -> &RelationConfig;

    /// Check if the extractor is available.
    fn is_available(&self) -> bool;
}

/// A simple rule-based relation extractor.
///
/// Uses pattern matching to extract relations based on entity types
/// and proximity. Useful for baselines and simple use cases.
#[derive(Debug, Clone)]
pub struct RuleBasedExtractor {
    config: RelationConfig,
    /// Maximum distance (in characters) between entities to consider.
    pub max_distance: usize,
}

impl RuleBasedExtractor {
    /// Create a new rule-based extractor.
    pub fn new() -> Self {
        Self {
            config: RelationConfig::default(),
            max_distance: 200,
        }
    }

    /// Set the maximum distance.
    pub fn with_max_distance(mut self, max_distance: usize) -> Self {
        self.max_distance = max_distance;
        self
    }
}

impl Default for RuleBasedExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationExtractor for RuleBasedExtractor {
    fn name(&self) -> &str {
        "RuleBasedExtractor"
    }

    fn extract(
        &self,
        _text: &str,
        _entity_types: &[&str],
        _relation_types: &[&str],
    ) -> Result<ExtractionResult, RelationError> {
        // Placeholder - would need NER integration
        Ok(ExtractionResult::new(Vec::new(), Vec::new()))
    }

    fn extract_with_entities(
        &self,
        text: &str,
        entities: &[Entity],
        relation_types: &[&str],
    ) -> Result<Vec<RelationTriple>, RelationError> {
        let mut triples = Vec::new();

        // Simple proximity-based heuristic
        for (i, head) in entities.iter().enumerate() {
            for (j, tail) in entities.iter().enumerate() {
                if i == j {
                    continue;
                }

                // Check distance
                let distance = if head.end <= tail.start {
                    tail.start - head.end
                } else if tail.end <= head.start {
                    head.start - tail.end
                } else {
                    0 // Overlapping
                };

                if distance > self.max_distance {
                    continue;
                }

                // Apply type-based heuristics
                if let Some(relation) = self.infer_relation(head, tail, text, relation_types) {
                    triples.push(RelationTriple::new(i, j, relation, 0.5));
                }
            }
        }

        Ok(triples)
    }

    fn config(&self) -> &RelationConfig {
        &self.config
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl RuleBasedExtractor {
    fn infer_relation(
        &self,
        head: &Entity,
        tail: &Entity,
        text: &str,
        allowed_relations: &[&str],
    ) -> Option<String> {
        let head_type = head.entity_type.as_label().to_uppercase();
        let tail_type = tail.entity_type.as_label().to_uppercase();

        // Get text between entities
        let between_start = head.end.min(tail.end);
        let between_end = head.start.max(tail.start);
        let between: String = if between_end > between_start {
            text.chars()
                .skip(between_start)
                .take(between_end - between_start)
                .collect()
        } else {
            String::new()
        };
        let between_lower = between.to_lowercase();

        // Simple pattern matching
        let relation = if head_type == "PERSON" && tail_type == "ORGANIZATION" {
            if between_lower.contains("founded") || between_lower.contains("started") {
                Some("founded")
            } else if between_lower.contains("works") || between_lower.contains("employee") {
                Some("works_for")
            } else if between_lower.contains("ceo") || between_lower.contains("president") {
                Some("leads")
            } else {
                None
            }
        } else if head_type == "PERSON" && tail_type == "LOCATION" {
            if between_lower.contains("born") {
                Some("born_in")
            } else if between_lower.contains("lives") || between_lower.contains("resides") {
                Some("lives_in")
            } else {
                None
            }
        } else if head_type == "ORGANIZATION" && tail_type == "LOCATION" {
            if between_lower.contains("headquarter") || between_lower.contains("based") {
                Some("headquartered_in")
            } else if between_lower.contains("located") {
                Some("located_in")
            } else {
                None
            }
        } else {
            None
        };

        // Filter by allowed relations
        relation
            .filter(|r| allowed_relations.is_empty() || allowed_relations.contains(r))
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_based_extractor() {
        let extractor = RuleBasedExtractor::new();
        assert!(extractor.is_available());
        assert_eq!(extractor.name(), "RuleBasedExtractor");
    }

    #[test]
    fn test_extraction_result() {
        let result = ExtractionResult::new(
            vec![],
            vec![
                RelationTriple::new(0, 1, "works_for", 0.9),
                RelationTriple::new(1, 2, "works_for", 0.8),
            ],
        );
        assert!(result.has_relations());
        assert_eq!(result.num_relation_types(), 1);
    }
}
