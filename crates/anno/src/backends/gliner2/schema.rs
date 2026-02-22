#![allow(unused_imports)]
//! GLiNER2 shared schema types: task definition, extraction results, caches.
//!
//! These are feature-agnostic — imported by both the ONNX and Candle backends.

use crate::{Entity, EntityType, Error, Result};
use anno_core::EntityCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "candle")]
use std::sync::RwLock;

use crate::backends::inference::{ExtractionWithRelations, RelationExtractor, ZeroShotNER};

// =============================================================================
// Special Token IDs (gliner-multitask-large-v0.5 vocabulary)
// Valid tokens: [MASK]=128000, [FLERT]=128001, <<ENT>>=128002, <<SEP>>=128003
// Note: [P], [C], [L] markers don't exist in this model - DO NOT USE 128004+
// =============================================================================

/// <<ENT>> token - entity type marker (class_token_index in config)
#[cfg(feature = "onnx")]
pub(super) const TOKEN_ENT: u32 = 128002;
/// <<SEP>> separator token
#[cfg(feature = "onnx")]
pub(super) const TOKEN_SEP: u32 = 128003;
/// Start token [CLS]
#[cfg(feature = "onnx")]
pub(super) const TOKEN_START: u32 = 1;
/// End token [SEP]
#[cfg(feature = "onnx")]
pub(super) const TOKEN_END: u32 = 2;

/// Default max span width
pub(super) const MAX_SPAN_WIDTH: usize = 12;
/// Max count for structure instances (0-19)
#[cfg(feature = "candle")]
pub(super) const MAX_COUNT: usize = 20;

// =============================================================================
// Label Embedding Cache
// =============================================================================

/// Cache for label embeddings to avoid recomputation
#[derive(Debug, Default)]
pub struct LabelCache {
    #[cfg(feature = "candle")]
    cache: RwLock<HashMap<String, Vec<f32>>>,
    #[cfg(not(feature = "candle"))]
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(feature = "candle")]
impl LabelCache {
    pub(super) fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub(super) fn get(&self, label: &str) -> Option<Vec<f32>> {
        self.cache.read().ok()?.get(label).cloned()
    }

    pub(super) fn insert(&self, label: String, embedding: Vec<f32>) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(label, embedding);
        }
    }
}

#[cfg(not(feature = "candle"))]
impl LabelCache {
    #[allow(dead_code)]
    fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

// =============================================================================
// Task Schema
// =============================================================================

/// Schema defining what to extract.
///
/// Use builder methods to construct complex schemas:
///
/// ```rust,ignore
/// let schema = TaskSchema::new()
///     .with_entities(&["person", "organization"])
///     .with_classification("sentiment", &["positive", "negative"], false)
///     .with_structure(
///         StructureTask::new("product")
///             .with_field("name", FieldType::String)
///             .with_field("price", FieldType::String)
///     );
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskSchema {
    /// Entity types to extract
    pub entities: Option<EntityTask>,
    /// Classification tasks
    pub classifications: Vec<ClassificationTask>,
    /// Structure extraction tasks
    pub structures: Vec<StructureTask>,
}

impl TaskSchema {
    /// Create empty schema.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add entity types to extract.
    pub fn with_entities(mut self, types: &[&str]) -> Self {
        self.entities = Some(EntityTask {
            types: types.iter().map(|s| s.to_string()).collect(),
            descriptions: HashMap::new(),
        });
        self
    }

    /// Add entity types with descriptions for better zero-shot.
    pub fn with_entities_described(mut self, types_with_desc: HashMap<String, String>) -> Self {
        let types: Vec<String> = types_with_desc.keys().cloned().collect();
        self.entities = Some(EntityTask {
            types,
            descriptions: types_with_desc,
        });
        self
    }

    /// Add a classification task.
    pub fn with_classification(mut self, name: &str, labels: &[&str], multi_label: bool) -> Self {
        self.classifications.push(ClassificationTask {
            name: name.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            multi_label,
            descriptions: HashMap::new(),
        });
        self
    }

    /// Add a structure extraction task.
    pub fn with_structure(mut self, task: StructureTask) -> Self {
        self.structures.push(task);
        self
    }
}

/// Entity extraction task configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityTask {
    /// Entity type labels
    pub types: Vec<String>,
    /// Optional descriptions for each type
    pub descriptions: HashMap<String, String>,
}

/// Classification task configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassificationTask {
    /// Task name (e.g., "sentiment")
    pub name: String,
    /// Class labels
    pub labels: Vec<String>,
    /// Whether multiple labels can be selected
    pub multi_label: bool,
    /// Optional descriptions for labels
    pub descriptions: HashMap<String, String>,
}

/// Hierarchical structure extraction task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructureTask {
    /// Structure type name (parent entity)
    pub name: String,
    /// Internal alias for compatibility
    #[serde(skip)]
    pub structure_type: String,
    /// Child fields to extract
    pub fields: Vec<StructureField>,
}

impl StructureTask {
    /// Create new structure task.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            structure_type: name.to_string(),
            fields: Vec::new(),
        }
    }

    /// Add a field to extract.
    pub fn with_field(mut self, name: &str, field_type: FieldType) -> Self {
        self.fields.push(StructureField {
            name: name.to_string(),
            field_type,
            description: None,
            choices: None,
        });
        self
    }

    /// Add a field with description.
    pub fn with_field_described(
        mut self,
        name: &str,
        field_type: FieldType,
        description: &str,
    ) -> Self {
        self.fields.push(StructureField {
            name: name.to_string(),
            field_type,
            description: Some(description.to_string()),
            choices: None,
        });
        self
    }

    /// Add a choice field with constrained options.
    pub fn with_choice_field(mut self, name: &str, choices: &[&str]) -> Self {
        self.fields.push(StructureField {
            name: name.to_string(),
            field_type: FieldType::Choice,
            description: None,
            choices: Some(choices.iter().map(|s| s.to_string()).collect()),
        });
        self
    }
}

/// Structure field configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureField {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: FieldType,
    /// Optional description
    pub description: Option<String>,
    /// For Choice type: allowed values
    pub choices: Option<Vec<String>>,
}

/// Field type for structure extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    /// Single string value
    String,
    /// List of values
    List,
    /// Choice from constrained options
    Choice,
}

// =============================================================================
// Extraction Results
// =============================================================================

/// Combined extraction result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionResult {
    /// Extracted entities
    pub entities: Vec<Entity>,
    /// Classification results by task name
    pub classifications: HashMap<String, ClassificationResult>,
    /// Extracted structures
    pub structures: Vec<ExtractedStructure>,
}

/// Classification result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Selected label(s)
    pub labels: Vec<String>,
    /// Score for each label
    pub scores: HashMap<String, f32>,
}

/// Extracted structure instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedStructure {
    /// Structure type
    pub structure_type: String,
    /// Extracted field values
    pub fields: HashMap<String, StructureValue>,
}

/// Value for a structure field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StructureValue {
    /// Single value
    Single(String),
    /// List of values
    List(Vec<String>),
}

// =============================================================================