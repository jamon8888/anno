//! TPLinker: Single-stage Joint Entity-Relation Extraction
//!
//! TPLinker uses a handshaking tagging scheme for joint entity-relation extraction.
//! It models entity boundaries and relations simultaneously using a unified tagging matrix.
//!
//! # Implementation Status
//!
//! **⚠️ PLACEHOLDER IMPLEMENTATION**: This is currently a placeholder that uses
//! simple heuristics for entity and relation extraction. A full implementation would:
//! - Integrate ONNX model for handshaking matrix prediction
//! - Decode entity boundaries from SH2OH/OH2SH tags
//! - Decode relations from handshaking between entity pairs
//!
//! The placeholder provides the interface and basic functionality for testing,
//! but does not use the actual TPLinker model architecture.
//!
//! # Research
//!
//! - **Paper**: [TPLinker: Single-stage Joint Extraction](https://aclanthology.org/2020.coling-main.138/)
//! - **Architecture**: Handshaking matrix where each cell (i,j) encodes:
//!   - Entity boundaries (SH2OH, OH2SH, ST2OT, OT2ST)
//!   - Relations (handshaking between entity pairs)
//!
//! # Usage
//!
//! ```rust,ignore
//! use anno::backends::tplinker::TPLinker;
//!
//! let extractor = TPLinker::new()?;
//! let result = extractor.extract_with_relations(
//!     "Steve Jobs founded Apple in 1976.",
//!     &["person", "organization"],
//!     &["founded", "works_for"],
//!     0.5
//! )?;
//!
//! for entity in &result.entities {
//!     println!("Entity: {} ({})", entity.text, entity.entity_type);
//! }
//!
//! for relation in &result.relations {
//!     let head = &result.entities[relation.head_idx];
//!     let tail = &result.entities[relation.tail_idx];
//!     println!("Relation: {} --[{}]--> {}", head.text, relation.relation_type, tail.text);
//! }
//! ```

use crate::backends::inference::{ExtractionWithRelations, RelationExtractor, RelationTriple};
use crate::{Entity, EntityType, Model, Result};
use std::borrow::Cow;

/// TPLinker backend for joint entity-relation extraction.
///
/// Uses handshaking matrix to simultaneously extract entities and relations.
/// Currently a placeholder implementation - full ONNX model integration pending.
pub struct TPLinker {
    /// Confidence threshold for entity extraction
    #[allow(dead_code)]
    entity_threshold: f32,
    /// Confidence threshold for relation extraction
    #[allow(dead_code)]
    relation_threshold: f32,
}

impl TPLinker {
    /// Create a new TPLinker instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            entity_threshold: 0.5,
            relation_threshold: 0.5,
        })
    }

    /// Create with custom thresholds.
    pub fn with_thresholds(entity_threshold: f32, relation_threshold: f32) -> Self {
        Self {
            entity_threshold,
            relation_threshold,
        }
    }

    /// Extract entities and relations using handshaking matrix.
    ///
    /// This is a placeholder implementation. Full TPLinker would:
    /// 1. Run ONNX model to get handshaking matrix predictions
    /// 2. Decode entity boundaries from SH2OH/OH2SH tags
    /// 3. Decode relations from handshaking between entity pairs
    #[allow(dead_code)] // Placeholder helper; kept for future TPLinker ONNX decoding work.
    fn extract_with_handshaking(
        &self,
        text: &str,
        _entity_types: &[&str],
        relation_types: &[&str],
    ) -> Result<ExtractionWithRelations> {
        // Placeholder: Use HeuristicNER for entity extraction
        // This properly handles multi-word entity names
        let heuristic = crate::HeuristicNER::new();
        let mut entities = heuristic.extract_entities(text, None)?;

        // Add provenance to indicate placeholder
        for entity in &mut entities {
            entity.provenance = Some(crate::Provenance {
                source: Cow::Borrowed("tplinker"),
                method: crate::ExtractionMethod::Heuristic,
                pattern: None,
                raw_confidence: Some(entity.confidence),
                model_version: Some(Cow::Borrowed("placeholder")),
                timestamp: None,
            });
        }

        // Extract relations (placeholder: simple proximity-based)
        // Performance: Pre-allocate relations vec with estimated capacity
        let mut relations = Vec::with_capacity(entities.len().min(8));
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len().min(i + 3) {
                // Check for relation triggers between entities
                let head = &entities[i];
                let tail = &entities[j];

                // Find text between entities (handle both orderings)
                let (between_start, between_end) = if head.end <= tail.start {
                    (head.end, tail.start)
                } else if tail.end <= head.start {
                    (tail.end, head.start)
                } else {
                    // Overlapping entities - skip
                    continue;
                };

                // `Entity` offsets in anno are **character offsets**.
                // Convert char offsets to byte offsets safely before slicing.
                let char_len = text.chars().count();
                let between_start = between_start.min(char_len);
                let between_end = between_end.min(char_len);
                let between_text = if between_start < between_end {
                    crate::offset::TextSpan::from_chars(text, between_start, between_end)
                        .extract(text)
                } else {
                    ""
                };

                // Simple relation detection (placeholder)
                let between_lower = between_text.to_lowercase();
                let relation_type = if between_lower.contains("founded") {
                    "founded".to_string()
                } else if between_lower.contains("works") || between_lower.contains("employee") {
                    "works_for".to_string()
                } else if relation_types.contains(&"related") {
                    "related".to_string()
                } else if !relation_types.is_empty() {
                    relation_types[0].to_string()
                } else {
                    continue; // No relation detected
                };

                relations.push(RelationTriple {
                    head_idx: i,
                    tail_idx: j,
                    relation_type,
                    confidence: self.relation_threshold + 0.1, // Above threshold for placeholder
                });
            }
        }

        Ok(ExtractionWithRelations {
            entities,
            relations,
        })
    }
}

impl Model for TPLinker {
    fn extract_entities(&self, text: &str, _language: Option<&str>) -> Result<Vec<Entity>> {
        // Extract entities only (no relations)
        let result =
            self.extract_with_handshaking(text, &["person", "organization", "location"], &[])?;
        Ok(result.entities)
    }

    fn supported_types(&self) -> Vec<EntityType> {
        vec![
            EntityType::Person,
            EntityType::Organization,
            EntityType::Location,
        ]
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "tplinker"
    }

    fn description(&self) -> &'static str {
        "TPLinker joint entity-relation extraction (placeholder - full ONNX implementation pending)"
    }
}

impl RelationExtractor for TPLinker {
    fn extract_with_relations(
        &self,
        text: &str,
        entity_types: &[&str],
        relation_types: &[&str],
        _threshold: f32,
    ) -> Result<ExtractionWithRelations> {
        self.extract_with_handshaking(text, entity_types, relation_types)
    }
}

// Make TPLinker implement BatchCapable and StreamingCapable for consistency
impl crate::BatchCapable for TPLinker {
    fn extract_entities_batch(
        &self,
        texts: &[&str],
        _language: Option<&str>,
    ) -> Result<Vec<Vec<Entity>>> {
        texts
            .iter()
            .map(|text| self.extract_entities(text, None))
            .collect()
    }
}

impl crate::StreamingCapable for TPLinker {
    fn extract_entities_streaming(&self, chunk: &str, offset: usize) -> Result<Vec<Entity>> {
        let mut entities = self.extract_entities(chunk, None)?;
        for entity in &mut entities {
            entity.start += offset;
            entity.end += offset;
        }
        Ok(entities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tplinker_creation() {
        let tplinker = TPLinker::new().unwrap();
        assert!(tplinker.is_available());
        assert_eq!(tplinker.name(), "tplinker");
    }

    #[test]
    fn test_tplinker_entity_extraction() {
        let tplinker = TPLinker::new().unwrap();
        let entities = tplinker
            .extract_entities("Steve Jobs founded Apple.", None)
            .unwrap();
        assert!(!entities.is_empty());
    }

    #[test]
    fn test_tplinker_relation_extraction() {
        let tplinker = TPLinker::new().unwrap();
        let result = tplinker
            .extract_with_relations(
                "Steve Jobs founded Apple in 1976.",
                &["person", "organization"],
                &["founded"],
                0.5,
            )
            .unwrap();
        assert!(!result.entities.is_empty());
    }
}
