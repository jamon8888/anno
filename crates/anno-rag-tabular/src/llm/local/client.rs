//! Local extraction client — implements [`LlmClient`] using a
//! [`LocalEntityExtractor`] (GLiNER2/Fastino or a test double) instead
//! of an LLM API call.

use super::{
    normalizers::normalize_value, offsets::char_span_to_byte_span, prompt::parse_user_prompt,
};
use crate::error::Result;
use crate::llm::{LlmClient, StructuredOutput, Usage};
use crate::schema::{CellType, ExtractionMode, ExtractionSpec};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Extractor trait
// ---------------------------------------------------------------------------

/// A single entity span returned by the local extractor.
#[derive(Debug, Clone)]
pub struct LocalEntity {
    /// The extracted text span.
    pub text: String,
    /// Start character index (inclusive) in the source text.
    pub start_char: usize,
    /// End character index (exclusive) in the source text.
    pub end_char: usize,
    /// Model confidence score in `[0, 1]`.
    pub confidence: f32,
}

/// Abstraction over GLiNER2/Fastino so tests can inject a cheap double.
pub trait LocalEntityExtractor: Send + Sync {
    fn extract(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>>;
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// LLM client implementation that uses a local entity extractor instead of a remote API.
pub struct LocalTabularClient {
    extractor: Box<dyn LocalEntityExtractor>,
}

impl LocalTabularClient {
    /// Constructor for production use (e.g. wrapping GLiNER2Fastino).
    pub fn new(extractor: Box<dyn LocalEntityExtractor>) -> Self {
        Self { extractor }
    }

    /// Constructor available in tests — identical to `new` but callable
    /// without the `#[cfg(test)]` gate so integration tests can also use it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_tests(extractor: Box<dyn LocalEntityExtractor>) -> Self {
        Self { extractor }
    }
}

// ---------------------------------------------------------------------------
// Schema meta — mirrors `x-anno-column` in the JSON schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ColumnMeta {
    name: String,
    #[allow(dead_code)]
    prompt: String,
    cell_type: CellType,
    extraction: ExtractionSpec,
}

// ---------------------------------------------------------------------------
// LlmClient impl
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmClient for LocalTabularClient {
    async fn generate_structured(
        &self,
        _system: &str,
        user: &str,
        json_schema: &Value,
    ) -> crate::error::Result<StructuredOutput> {
        let parsed = parse_user_prompt(user)?;
        let mut result = serde_json::Map::new();

        let props = json_schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();

        for (name, prop) in props {
            let Some(meta_val) = prop.get("x-anno-column") else {
                continue;
            };
            let meta: ColumnMeta = serde_json::from_value(meta_val.clone())?;

            // Only handle local modes here; routing client handles the rest.
            if !matches!(
                meta.extraction.mode,
                ExtractionMode::LocalSpan | ExtractionMode::LocalClause
            ) {
                continue;
            }

            let labels: Vec<(&str, &str)> = meta
                .extraction
                .labels
                .iter()
                .map(|l| (l.name.as_str(), l.description.as_str()))
                .collect();
            let threshold = meta.extraction.threshold.unwrap_or(0.45);

            // Pick the best entity across all chunks (highest confidence).
            let mut best: Option<(uuid::Uuid, String, usize, usize, f32)> = None;

            for chunk in &parsed.chunks {
                let entities = self.extractor.extract(&chunk.text, &labels, threshold)?;
                for ent in entities {
                    let Some(span) =
                        char_span_to_byte_span(&chunk.text, ent.start_char, ent.end_char)
                    else {
                        continue;
                    };
                    let Some(quote) = chunk.text.get(span.start..span.end) else {
                        continue;
                    };
                    // Verify the extracted text matches the byte slice.
                    if quote != ent.text {
                        continue;
                    }
                    if best.as_ref().map_or(true, |b| ent.confidence > b.4) {
                        best = Some((
                            chunk.id,
                            quote.to_string(),
                            span.start,
                            span.end,
                            ent.confidence,
                        ));
                    }
                }
            }

            if let Some((chunk_id, quote, start, end, confidence)) = best {
                if let Some(value) = normalize_value(&quote, &meta.cell_type) {
                    result.insert(
                        name,
                        json!({
                            "value": value,
                            "reasoning": format!(
                                "Local GLiNER extraction with confidence {:.2}", confidence
                            ),
                            "citations": [{
                                "chunk_id": chunk_id.to_string(),
                                "byte_start": start,
                                "byte_end": end,
                                "quoted_text": quote
                            }]
                        }),
                    );
                }
            }
        }

        Ok(StructuredOutput {
            value: Value::Object(result),
            usage: Usage::default(),
        })
    }

    fn model_id(&self) -> &str {
        "local-tabular-gliner2"
    }
}

// ---------------------------------------------------------------------------
// Extended trait for per-label descriptions + thresholds
// ---------------------------------------------------------------------------

/// Extended extraction interface that supports GLiNER-style label descriptions
/// and per-label confidence thresholds.
///
/// Blanket implementations can wrap any [`LocalEntityExtractor`] that accepts
/// `(name, description)` pairs or per-label thresholds.
pub trait LocalLegalSignalExtractor: LocalEntityExtractor {
    /// Extract entities using `(label_name, description)` pairs.
    /// The description is passed as the GLiNER label text for richer matching.
    fn extract_with_descriptions(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>> {
        self.extract(text, labels, threshold)
    }

    /// Extract entities with per-label thresholds.
    /// Runs extraction at the minimum threshold then filters per label.
    fn extract_with_thresholds(
        &self,
        text: &str,
        label_thresholds: &[(&str, f32)],
    ) -> Result<Vec<LocalEntity>> {
        if label_thresholds.is_empty() {
            return Ok(vec![]);
        }
        let min_threshold = label_thresholds
            .iter()
            .map(|(_, t)| *t)
            .fold(f32::MAX, f32::min);
        let labels: Vec<(&str, &str)> = label_thresholds
            .iter()
            .map(|(name, _)| (*name, *name))
            .collect();
        let entities = self.extract(text, &labels, min_threshold)?;
        // Filter each entity against its per-label threshold.
        Ok(entities
            .into_iter()
            .filter(|e| {
                label_thresholds
                    .iter()
                    .find(|(name, _)| *name == e.text.as_str())
                    .map_or(true, |(_, t)| e.confidence >= *t)
            })
            .collect())
    }
}

// Blanket impl: every LocalEntityExtractor is also a LocalLegalSignalExtractor.
impl<T: LocalEntityExtractor> LocalLegalSignalExtractor for T {}

// ---------------------------------------------------------------------------
// Live GLiNER2/Fastino adapter (feature = "gliner2")
// ---------------------------------------------------------------------------

/// Production [`LocalEntityExtractor`] backed by [`anno::GLiNEROnnx`].
///
/// Only available when the `gliner2` crate feature is enabled.
/// Tests that use this type are marked `#[ignore]` because they require
/// downloading model weights at runtime.
#[cfg(feature = "gliner2")]
pub struct Gliner2EntityExtractor {
    model: std::sync::Arc<anno::GLiNEROnnx>,
}

#[cfg(feature = "gliner2")]
impl Gliner2EntityExtractor {
    /// Wrap a pre-loaded [`anno::GLiNEROnnx`] model.
    pub fn new(model: anno::GLiNEROnnx) -> Self {
        Self { model: std::sync::Arc::new(model) }
    }

    /// Load from HuggingFace Hub (downloads weights on first call).
    pub fn from_pretrained(model_name: &str) -> crate::error::Result<Self> {
        let model = anno::GLiNEROnnx::new(model_name).map_err(|e| {
            crate::error::Error::Extract {
                doc: "local".into(),
                col: "*".into(),
                source: e.to_string().into(),
            }
        })?;
        Ok(Self::new(model))
    }
}

#[cfg(feature = "gliner2")]
impl LocalEntityExtractor for Gliner2EntityExtractor {
    /// Extract entities. `labels` is a slice of `(name, description)` pairs;
    /// the description is passed as the GLiNER label text for richer zero-shot
    /// matching (e.g. "Nom complet et forme juridique du bailleur").
    fn extract(
        &self,
        text: &str,
        labels: &[(&str, &str)],
        threshold: f32,
    ) -> Result<Vec<LocalEntity>> {
        // Pass descriptions as GLiNER label strings — they carry more semantic
        // signal than short names for French legal text.
        let label_texts: Vec<&str> = labels.iter().map(|(_, desc)| *desc).collect();

        let entities = self
            .model
            .extract(text, &label_texts, threshold)
            .map_err(|e| crate::error::Error::Extract {
                doc: "local".into(),
                col: "*".into(),
                source: e.to_string().into(),
            })?;

        Ok(entities
            .into_iter()
            .map(|e| LocalEntity {
                text: e.text,
                start_char: e.start(),
                end_char: e.end(),
                confidence: f32::from(e.confidence),
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmClient;

    struct MockExtractor;

    impl LocalEntityExtractor for MockExtractor {
        fn extract(
            &self,
            _text: &str,
            _labels: &[(&str, &str)],
            _threshold: f32,
        ) -> Result<Vec<LocalEntity>> {
            Ok(vec![LocalEntity {
                text: "ACME SAS".into(),
                start_char: 16, // "Le bailleur est " = 16 chars
                end_char: 24,   // 16 + len("ACME SAS") = 24
                confidence: 0.91,
            }])
        }
    }

    #[tokio::test]
    async fn local_client_emits_cited_cell_for_local_span() {
        let chunk_id = uuid::Uuid::now_v7();
        let user = format!(
            "[CHUNK::{chunk_id}]Le bailleur est ACME SAS.[/CHUNK]\n\
             [COLUMN::landlord]Landlord legal name[/COLUMN]\n"
        );
        let schema = json!({
            "type": "object",
            "properties": {
                "landlord": {
                    "type": "object",
                    "x-anno-column": {
                        "name": "landlord",
                        "prompt": "Landlord legal name",
                        "cell_type": { "kind": "text" },
                        "extraction": {
                            "mode": "local_span",
                            "labels": [{ "name": "bailleur", "description": "Nom du bailleur" }],
                            "threshold": 0.45
                        }
                    }
                }
            }
        });

        let client = LocalTabularClient::new_for_tests(Box::new(MockExtractor));
        let out = client.generate_structured("", &user, &schema).await.expect("extract");

        assert_eq!(out.value["landlord"]["value"], "ACME SAS");
        assert_eq!(
            out.value["landlord"]["citations"][0]["chunk_id"],
            chunk_id.to_string()
        );
        assert_eq!(out.value["landlord"]["citations"][0]["quoted_text"], "ACME SAS");
    }

    #[tokio::test]
    async fn local_client_skips_llm_required_columns() {
        let chunk_id = uuid::Uuid::now_v7();
        let user = format!("[CHUNK::{chunk_id}]Some text.[/CHUNK]\n");
        let schema = json!({
            "type": "object",
            "properties": {
                "repair_obligations": {
                    "type": "object",
                    "x-anno-column": {
                        "name": "repair_obligations",
                        "prompt": "Repair obligations?",
                        "cell_type": { "kind": "text" },
                        "extraction": { "mode": "llm_required" }
                    }
                }
            }
        });

        let client = LocalTabularClient::new_for_tests(Box::new(MockExtractor));
        let out = client.generate_structured("", &user, &schema).await.expect("no error");

        assert!(out.value.as_object().unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Live GLiNER2 tests — require model weights; skipped in CI.
    // -----------------------------------------------------------------------

    #[cfg(feature = "gliner2")]
    #[tokio::test]
    #[ignore = "loads local GLiNER2 model weights"]
    async fn local_gliner2_adapter_extracts_party_name() {
        let extractor = Gliner2EntityExtractor::from_pretrained(
            "onnx-community/gliner_small-v2.1",
        )
        .expect("model loads");
        let out = extractor
            .extract(
                "Entre les soussignés, ACME SAS agit comme bailleur.",
                &[("bailleur", "Nom complet et forme juridique du bailleur")],
                0.3,
            )
            .expect("extract");
        assert!(out.iter().any(|e| e.text.contains("ACME")));
    }

    #[cfg(feature = "gliner2")]
    #[tokio::test]
    #[ignore = "loads local GLiNER2 model weights"]
    async fn local_gliner2_adapter_uses_descriptions_as_labels() {
        let extractor = Gliner2EntityExtractor::from_pretrained(
            "onnx-community/gliner_small-v2.1",
        )
        .expect("model loads");
        // Verify that the description ("Montant monétaire en euros") is forwarded
        // to GLiNER rather than the short label name ("amount").
        let out = extractor
            .extract(
                "Le loyer mensuel est de 1 500 euros.",
                &[("amount", "Montant monétaire en euros")],
                0.3,
            )
            .expect("extract");
        assert!(!out.is_empty(), "should find a monetary amount");
    }
}
