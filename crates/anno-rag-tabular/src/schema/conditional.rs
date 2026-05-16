//! Conditional-column gates. v0.1 stub: defines the type surface used by
//! [`super::column::Column::conditional`]. T6 fleshes out predicate
//! evaluation; T31/T32 (Phase 7) wire the DAG into the extraction engine.

use crate::ids::ColumnId;
use serde::{Deserialize, Serialize};

/// Marker on a [`Column`](super::column::Column) saying "only extract me
/// when the named parent column's value satisfies `predicate`".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalSpec {
    /// The id of the column whose value gates this child.
    pub parent_col: ColumnId,
    /// The predicate evaluated against the parent cell's value.
    pub predicate: Predicate,
}

/// Predicate over a JSON cell value. Tagged with `op` so a TOML template
/// can carry it as `{ op = "equals", value = "..." }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Predicate {
    /// Parent value equals the supplied literal (JSON-compared).
    Equals {
        /// The literal to compare against.
        value: serde_json::Value,
    },
    /// Parent value is not equal to the supplied literal.
    NotEquals {
        /// The literal to compare against.
        value: serde_json::Value,
    },
    /// Parent cell is non-null (any value extracted, of any type).
    NonNull,
    /// Parent value (coerced to string) matches the regex.
    Matches {
        /// Regex source string (Rust `regex` crate syntax).
        regex: String,
    },
}

impl Predicate {
    /// Evaluate this predicate against a parent cell's JSON value.
    ///
    /// Semantics around the missing parent (`None`):
    /// - `NonNull` is `false` (no value can't satisfy "non-null").
    /// - `Equals` is `false` (nothing to compare against).
    /// - `NotEquals` is `true` (vacuously not-equal to the excluded literal).
    /// - `Matches` is `false` (nothing to match against).
    ///
    /// Invalid regex strings degrade to `false` rather than panicking —
    /// the audit log catches the malformed template at load time; runtime
    /// stays safe.
    #[must_use]
    pub fn eval(&self, parent_value: Option<&serde_json::Value>) -> bool {
        match (self, parent_value) {
            (Predicate::NonNull, Some(serde_json::Value::Null)) => false,
            (Predicate::NonNull, None) => false,
            (Predicate::NonNull, Some(_)) => true,

            (Predicate::Equals { value }, Some(actual)) => actual == value,
            (Predicate::Equals { .. }, None) => false,

            (Predicate::NotEquals { value }, Some(actual)) => actual != value,
            (Predicate::NotEquals { .. }, None) => true,

            (Predicate::Matches { regex }, Some(serde_json::Value::String(s))) => {
                regex::Regex::new(regex)
                    .map(|r| r.is_match(s))
                    .unwrap_or(false)
            }
            (Predicate::Matches { .. }, _) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nonnull_predicate() {
        let p = Predicate::NonNull;
        assert!(!p.eval(None));
        assert!(!p.eval(Some(&serde_json::Value::Null)));
        assert!(p.eval(Some(&json!("anything"))));
        assert!(p.eval(Some(&json!(0))));
        assert!(p.eval(Some(&json!(false))));
    }

    #[test]
    fn equals_predicate() {
        let p = Predicate::Equals { value: json!("FR") };
        assert!(p.eval(Some(&json!("FR"))));
        assert!(!p.eval(Some(&json!("DE"))));
        assert!(!p.eval(None));
    }

    #[test]
    fn matches_predicate() {
        let p = Predicate::Matches {
            regex: r"^Cass\.".to_string(),
        };
        assert!(p.eval(Some(&json!("Cass. civ. 1, 2024-03-12"))));
        assert!(!p.eval(Some(&json!("CE, 2024-04-01"))));
        // Non-string parent: false.
        assert!(!p.eval(Some(&json!(42))));
        // Missing parent: false.
        assert!(!p.eval(None));
    }

    #[test]
    fn matches_invalid_regex_degrades_to_false() {
        let p = Predicate::Matches {
            regex: "[".to_string(), // malformed: unclosed character class.
        };
        assert!(!p.eval(Some(&json!("anything"))));
    }

    #[test]
    fn not_equals_treats_missing_as_satisfying() {
        let p = Predicate::NotEquals {
            value: json!("excluded"),
        };
        assert!(p.eval(None), "missing parent should satisfy != X");
        assert!(p.eval(Some(&json!("other"))));
        assert!(!p.eval(Some(&json!("excluded"))));
    }
}
