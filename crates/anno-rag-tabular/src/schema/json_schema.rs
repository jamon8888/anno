//! Compile a list of columns into a JSON Schema that the LLM must obey
//! via constrained decoding. Output targets JSON Schema draft-2020-12.
//!
//! The schema enforces the per-row envelope the extraction engine
//! expects: for every non-manual column the LLM emits a `{ value,
//! reasoning, citations }` triple, with `value` typed against the
//! column's [`CellType`] and `citations` carrying offset-accurate
//! span pointers back into source chunks.
//!
//! Manual columns (human-only) are excluded — the LLM never produces
//! a value for them.

use crate::schema::{CellType, Column};
use serde_json::{json, Value};

/// Build a JSON Schema describing the expected per-row extraction output.
///
/// Shape (abridged):
/// ```text
/// { "type":"object",
///   "required":[col_name, ...],
///   "additionalProperties":false,
///   "properties": {
///     col_name: {
///       "type":"object",
///       "required":["value","reasoning","citations"],
///       "properties": {
///         "value":     <typed by CellType>,
///         "reasoning": { "type":"string", ... },
///         "citations": { "type":"array", "minItems":1,
///                        "items": <citation object> }
///       }
///     },
///     ...
///   } }
/// ```
#[must_use]
pub fn for_columns(columns: &[Column]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    for c in columns {
        if c.manual {
            continue;
        }
        required.push(json!(c.name.clone()));
        properties.insert(c.name.clone(), cell_envelope(&c.cell_type));
    }

    json!({
        "type": "object",
        "required": required,
        "additionalProperties": false,
        "properties": properties,
    })
}

fn cell_envelope(t: &CellType) -> Value {
    json!({
        "type": "object",
        "required": ["value", "reasoning", "citations"],
        "additionalProperties": false,
        "properties": {
            "value": value_schema(t),
            "reasoning": { "type": "string", "minLength": 1, "maxLength": 1000 },
            "citations": {
                "type": "array",
                "minItems": 1,
                "items": citation_schema(),
            }
        }
    })
}

fn value_schema(t: &CellType) -> Value {
    match t {
        CellType::Text | CellType::Verbatim => json!({ "type": "string" }),
        CellType::Date => json!({
            "type": "string",
            "pattern": r"^\d{4}-\d{2}-\d{2}$"
        }),
        CellType::Currency { code } => json!({
            "type": "object",
            "required": ["amount", "code"],
            "properties": {
                "amount": { "type": "number" },
                "code":   { "type": "string", "const": code }
            }
        }),
        CellType::Enum { options } => json!({
            "type": "string",
            "enum": options
        }),
        CellType::Boolean => json!({ "type": "boolean" }),
        CellType::Number => json!({ "type": "number" }),
    }
}

fn citation_schema() -> Value {
    json!({
        "type": "object",
        "required": ["chunk_id", "char_start", "char_end", "quoted_text"],
        "additionalProperties": false,
        "properties": {
            "chunk_id":    { "type": "string", "format": "uuid" },
            "char_start":  { "type": "integer", "minimum": 0 },
            "char_end":    { "type": "integer", "minimum": 0 },
            "quoted_text": { "type": "string", "minLength": 1 }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReviewId;
    use crate::schema::column::ColumnBuilder;

    #[test]
    fn empty_columns_yields_empty_object() {
        let s = for_columns(&[]);
        assert_eq!(s["type"], "object");
        assert_eq!(s["required"], json!([]));
        assert_eq!(s["properties"], json!({}));
    }

    #[test]
    fn text_column_uses_string_value() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "term", "What term?", CellType::Text).build();
        let s = for_columns(&[c]);
        assert_eq!(s["properties"]["term"]["properties"]["value"]["type"], "string");
    }

    #[test]
    fn enum_column_lists_options() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r,
            "law",
            "Governing law jurisdiction?",
            CellType::Enum {
                options: vec!["FR".into(), "DE".into(), "UK".into()],
            },
        )
        .build();
        let s = for_columns(&[c]);
        assert_eq!(
            s["properties"]["law"]["properties"]["value"]["enum"],
            json!(["FR", "DE", "UK"])
        );
    }

    #[test]
    fn currency_column_constrains_iso_code() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(
            r,
            "cap",
            "Liability cap?",
            CellType::Currency {
                code: "EUR".into(),
            },
        )
        .build();
        let s = for_columns(&[c]);
        assert_eq!(
            s["properties"]["cap"]["properties"]["value"]["properties"]["code"]["const"],
            "EUR"
        );
    }

    #[test]
    fn date_column_has_iso_pattern() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "effective", "Effective date?", CellType::Date).build();
        let s = for_columns(&[c]);
        let pat = s["properties"]["effective"]["properties"]["value"]["pattern"]
            .as_str()
            .expect("date schema must have string pattern");
        assert!(pat.contains(r"\d{4}-\d{2}-\d{2}"));
    }

    #[test]
    fn manual_columns_excluded_from_schema() {
        let r = ReviewId::new();
        let auto = ColumnBuilder::new(r, "term", "term?", CellType::Text).build();
        let manual = ColumnBuilder::new(r, "notes", "human notes", CellType::Text)
            .manual()
            .build();
        let s = for_columns(&[auto, manual]);
        let props = s["properties"]
            .as_object()
            .expect("schema properties must be an object");
        assert!(props.contains_key("term"));
        assert!(!props.contains_key("notes"));
        // And `notes` is absent from `required` too.
        let req = s["required"].as_array().expect("required must be array");
        assert!(req.iter().all(|v| v != &json!("notes")));
    }

    #[test]
    fn citations_required_minimum_one() {
        let r = ReviewId::new();
        let c = ColumnBuilder::new(r, "term", "term?", CellType::Text).build();
        let s = for_columns(&[c]);
        assert_eq!(
            s["properties"]["term"]["properties"]["citations"]["minItems"],
            1
        );
    }
}
