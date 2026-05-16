//! `CellType` — the closed taxonomy of cell value shapes.
//!
//! Serialised with `#[serde(tag = "kind")]` so a TOML / JSON template can
//! discriminate variants by the `kind` field. Variants that carry payload
//! (`Currency`, `Enum`) flatten their fields alongside `kind`.

use serde::{Deserialize, Serialize};

/// One cell value shape. The extractor's JSON-Schema generator (Task 7)
/// turns each variant into a constrained-decoding clause for the LLM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellType {
    /// Free-form text answer.
    Text,
    /// ISO-8601 date.
    Date,
    /// Decimal currency amount with ISO 4217 code (EUR, USD, …).
    Currency {
        /// ISO 4217 currency code.
        code: String,
    },
    /// Exact quote from source — must be a verbatim substring of a chunk.
    Verbatim,
    /// One of a closed set of options.
    Enum {
        /// Allowed values (canonical surface form).
        options: Vec<String>,
    },
    /// True/false.
    Boolean,
    /// Decimal number (no currency).
    Number,
}

impl CellType {
    /// Lowercase identifier suitable for logs + storage encoding. Stable
    /// across the serde tag and the on-disk Arrow string column.
    #[must_use]
    pub fn discriminant_name(&self) -> &'static str {
        match self {
            CellType::Text => "text",
            CellType::Date => "date",
            CellType::Currency { .. } => "currency",
            CellType::Verbatim => "verbatim",
            CellType::Enum { .. } => "enum",
            CellType::Boolean => "boolean",
            CellType::Number => "number",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_with_kind_tag() {
        let t = CellType::Currency {
            code: "EUR".into(),
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v, json!({"kind": "currency", "code": "EUR"}));
    }

    #[test]
    fn deserializes_enum_options() {
        let v = json!({"kind": "enum", "options": ["paris", "lyon"]});
        let t: CellType = serde_json::from_value(v).unwrap();
        match t {
            CellType::Enum { options } => assert_eq!(options, vec!["paris", "lyon"]),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trips_text() {
        let t = CellType::Text;
        let s = serde_json::to_string(&t).unwrap();
        let back: CellType = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn discriminant_names_stable() {
        assert_eq!(CellType::Text.discriminant_name(), "text");
        assert_eq!(CellType::Boolean.discriminant_name(), "boolean");
        assert_eq!(
            CellType::Enum { options: vec![] }.discriminant_name(),
            "enum"
        );
    }
}
