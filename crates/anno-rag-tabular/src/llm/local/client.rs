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
    pub text: String,
    pub start_char: usize,
    pub end_char: usize,
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
                        best =
                            Some((chunk.id, quote.to_string(), span.start, span.end, ent.confidence));
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

        Ok(StructuredOutput { value: Value::Object(result), usage: Usage::default() })
    }

    fn model_id(&self) -> &str {
        "local-tabular-gliner2"
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
}
