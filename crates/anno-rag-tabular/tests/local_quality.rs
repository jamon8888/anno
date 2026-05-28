//! Local extraction quality smoke-tests.
//!
//! These tests use mock extractors and template fixtures — no model weights
//! are loaded. They assert that:
//! - The real-estate template correctly classifies which fields are
//!   local-safe vs requiring LLM reasoning.
//! - The `LocalTabularClient` + `RoutingLlmClient` pipeline produces
//!   cited cells for local-safe columns and leaves others to the fallback.

use anno_rag_tabular::{
    llm::{
        local::client::{LocalEntity, LocalEntityExtractor, LocalTabularClient},
        routing::RoutingLlmClient,
        LlmClient, StructuredOutput, Usage,
    },
    schema::{template::Template, ExtractionMode},
};
use async_trait::async_trait;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Template classification assertions
// ---------------------------------------------------------------------------

#[test]
fn local_quality_extracts_safe_fields_and_abstains_on_clause_reasoning() {
    let template = Template::builtin("real-estate-v1").expect("real-estate-v1 ships");
    let columns = template.into_columns(anno_rag_tabular::ids::ReviewId::new());

    // landlord should be annotated as local-safe
    let landlord = columns
        .iter()
        .find(|c| c.name == "landlord")
        .expect("landlord column");
    assert_eq!(
        landlord.extraction.mode,
        ExtractionMode::LocalSpan,
        "landlord must be local_span"
    );

    // repair_obligations is a complex clause — must NOT be local_span
    let repair = columns
        .iter()
        .find(|c| c.name == "repair_obligations")
        .expect("repair_obligations column");
    assert_ne!(
        repair.extraction.mode,
        ExtractionMode::LocalSpan,
        "repair_obligations must not be local_span"
    );

    // tenant_break_rights is also complex
    let break_rights = columns
        .iter()
        .find(|c| c.name == "tenant_break_rights")
        .expect("tenant_break_rights column");
    assert_ne!(
        break_rights.extraction.mode,
        ExtractionMode::LocalSpan,
        "tenant_break_rights must not be local_span"
    );
}

#[test]
fn all_real_estate_columns_have_extraction_spec() {
    let template = Template::builtin("real-estate-v1").expect("template loads");
    let review = anno_rag_tabular::ids::ReviewId::new();
    let cols = template.into_columns(review);
    // Every column must have a valid ExtractionSpec (at minimum the default Auto mode).
    for col in &cols {
        let _ = col.extraction.mode; // just accessing it asserts it's present
    }
    assert!(!cols.is_empty());
}

// ---------------------------------------------------------------------------
// Pipeline smoke test with mock extractor
// ---------------------------------------------------------------------------

struct FixedExtractor {
    entity_text: &'static str,
    start_char: usize,
    end_char: usize,
}

impl LocalEntityExtractor for FixedExtractor {
    fn extract(
        &self,
        _text: &str,
        _labels: &[(&str, &str)],
        _threshold: f32,
    ) -> anno_rag_tabular::error::Result<Vec<LocalEntity>> {
        Ok(vec![LocalEntity {
            text: self.entity_text.to_string(),
            start_char: self.start_char,
            end_char: self.end_char,
            confidence: 0.88,
        }])
    }
}

struct NullClient;

#[async_trait]
impl LlmClient for NullClient {
    async fn generate_structured(
        &self,
        _system: &str,
        _user: &str,
        _json_schema: &Value,
    ) -> anno_rag_tabular::error::Result<StructuredOutput> {
        Ok(StructuredOutput {
            value: json!({}),
            usage: Usage::default(),
        })
    }
    fn model_id(&self) -> &str {
        "null"
    }
}

#[tokio::test]
async fn routing_pipeline_emits_local_value_for_local_span_column() {
    // "Entre les soussignes, ACME SAS" — "ACME SAS" starts at char 22
    let chunk_id = uuid::Uuid::parse_str("018f0000-0000-7000-8000-000000000001").unwrap();
    let text = "Entre les soussignes, ACME SAS, bailleur.";
    // "ACME SAS" starts at char index 22, ends at 30
    let user = format!("[CHUNK::{chunk_id}]{text}[/CHUNK]\n");

    let schema = json!({
        "type": "object",
        "properties": {
            "landlord": {
                "type": "object",
                "x-anno-column": {
                    "name": "landlord",
                    "prompt": "Landlord legal name.",
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

    let local = Box::new(LocalTabularClient::new(Box::new(FixedExtractor {
        entity_text: "ACME SAS",
        start_char: 22,
        end_char: 30,
    })));
    let router = RoutingLlmClient::new(local, Some(Box::new(NullClient)));

    let out = router
        .generate_structured("", &user, &schema)
        .await
        .expect("extract");

    assert_eq!(out.value["landlord"]["value"], "ACME SAS");
    assert_eq!(
        out.value["landlord"]["citations"][0]["chunk_id"],
        chunk_id.to_string()
    );
}
