//! Template loader. A `Template` is a reusable schema preset: TOML on
//! disk for the five M&A-flavoured presets we ship, or arbitrary string
//! for user-supplied templates.
//!
//! Wire shape uses an explicit `type` discriminant (matching `CellType`'s
//! `kind` tag idiom but renamed to `type` because that reads better in
//! a hand-edited TOML file). `CellTypeWire` exists solely to absorb that
//! difference at deserialise time; runtime code only sees `CellType`.

use crate::error::{Error, Result};
use crate::ids::ReviewId;
use crate::schema::column::{ColumnBuilder, ExtractionSpec};
use crate::schema::{CellType, Column};
use serde::{Deserialize, Serialize};

/// A reusable schema preset. Loaded from TOML on disk (shipped) or from
/// arbitrary string (user templates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Stable string id, e.g. `"nda-v1"`. Surfaces in MCP tool output.
    pub id: String,
    /// Display name for the grid UI.
    pub name: String,
    /// Semver of this template — bumping invalidates cached extractions.
    pub version: String,
    /// Short human description.
    pub description: String,
    /// Vertical tag (e.g. `"legal-fr"`) for filtering in the picker.
    pub vertical: String,
    /// Column list. TOML uses `[[column]]` blocks — rename absorbs that.
    #[serde(rename = "column")]
    pub columns: Vec<TemplateColumn>,
}

/// One column entry in a template — wire form, before `ReviewId` binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateColumn {
    /// Column key.
    pub name: String,
    /// Per-cell prompt the extractor passes to the LLM.
    pub prompt: String,
    /// Cell type (flattened so TOML carries `type = "..."` plus shape-
    /// specific fields like `code = "EUR"` or `options = [...]`).
    #[serde(flatten)]
    pub cell_type: CellTypeWire,
    /// Optional local-extraction metadata. Absent from older templates
    /// — `#[serde(default)]` gives `ExtractionSpec::default()` in that case.
    #[serde(default)]
    pub extraction: ExtractionSpec,
}

/// TOML-friendly cell type. Tagged with `type` to keep the on-disk
/// templates readable (`type = "currency"` plus `code = "EUR"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CellTypeWire {
    /// Free text.
    Text,
    /// ISO-8601 date.
    Date,
    /// Verbatim quote — extractor must echo the chunk text.
    Verbatim,
    /// Boolean.
    Boolean,
    /// Number.
    Number,
    /// Currency — `code` is the ISO 4217 currency code (e.g. `"EUR"`).
    Currency {
        /// ISO 4217 currency code.
        code: String,
    },
    /// Enum — `options` is the closed list of allowed values.
    Enum {
        /// Allowed values for the enum.
        options: Vec<String>,
    },
}

impl From<CellTypeWire> for CellType {
    fn from(w: CellTypeWire) -> Self {
        match w {
            CellTypeWire::Text => CellType::Text,
            CellTypeWire::Date => CellType::Date,
            CellTypeWire::Verbatim => CellType::Verbatim,
            CellTypeWire::Boolean => CellType::Boolean,
            CellTypeWire::Number => CellType::Number,
            CellTypeWire::Currency { code } => CellType::Currency { code },
            CellTypeWire::Enum { options } => CellType::Enum { options },
        }
    }
}

impl Template {
    /// Parse a Template from a TOML string. Used for user-supplied
    /// templates and as the inner helper for [`Self::builtin`].
    pub fn from_toml(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(Error::from)
    }

    /// Load a shipped (built-in) template by id. Returns
    /// [`Error::TemplateNotFound`] for unknown ids.
    pub fn builtin(id: &str) -> Result<Self> {
        let s = match id {
            "nda-v1" => include_str!("../templates/nda-v1.toml"),
            "customer-contract-v1" => include_str!("../templates/customer-contract-v1.toml"),
            "real-estate-v1" => include_str!("../templates/real-estate-v1.toml"),
            "employment-v1" => include_str!("../templates/employment-v1.toml"),
            "ip-v1" => include_str!("../templates/ip-v1.toml"),
            _ => return Err(Error::TemplateNotFound { name: id.into() }),
        };
        Self::from_toml(s)
    }

    /// Enumerate built-in template ids. Surfaces in the MCP
    /// `tabular_list_templates` tool.
    #[must_use]
    pub fn list_builtin() -> &'static [&'static str] {
        &[
            "nda-v1",
            "customer-contract-v1",
            "real-estate-v1",
            "employment-v1",
            "ip-v1",
        ]
    }

    /// Materialise the template's columns under a given review id.
    /// Display order follows TOML order.
    #[must_use]
    pub fn into_columns(self, review_id: ReviewId) -> Vec<Column> {
        self.columns
            .into_iter()
            .enumerate()
            .map(|(i, tc)| {
                ColumnBuilder::new(review_id, &tc.name, &tc.prompt, tc.cell_type.into())
                    .extraction(tc.extraction)
                    .order(u32::try_from(i).unwrap_or(u32::MAX))
                    .build()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nda_v1_loads_and_has_expected_columns() {
        let t = Template::builtin("nda-v1").expect("nda-v1 ships");
        assert_eq!(t.id, "nda-v1");
        assert_eq!(t.vertical, "legal-fr");
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"parties"));
        assert!(names.contains(&"term"));
        assert!(names.contains(&"governing_law"));
    }

    #[test]
    fn unknown_template_errors() {
        let r = Template::builtin("not-a-template");
        assert!(matches!(r, Err(Error::TemplateNotFound { .. })));
    }

    #[test]
    fn into_columns_preserves_order() {
        let t = Template::builtin("nda-v1").expect("nda-v1 ships");
        let r = ReviewId::new();
        let cols = t.into_columns(r);
        for (i, c) in cols.iter().enumerate() {
            assert_eq!(c.order, u32::try_from(i).unwrap());
        }
    }

    #[test]
    fn list_builtin_matches_loader() {
        for id in Template::list_builtin() {
            Template::builtin(id).unwrap_or_else(|e| panic!("builtin {id} must load: {e}"));
        }
    }

    #[test]
    fn customer_contract_v1_has_required_columns() {
        let t = Template::builtin("customer-contract-v1").expect("loads");
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in [
            "parties",
            "term",
            "change_of_control",
            "liability_cap",
            "governing_law",
        ] {
            assert!(names.contains(&must), "missing required col {must}");
        }
    }

    #[test]
    fn real_estate_v1_has_required_columns() {
        let t = Template::builtin("real-estate-v1").expect("loads");
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["landlord", "tenant", "base_rent", "permitted_use"] {
            assert!(names.contains(&must), "missing required col {must}");
        }
    }

    #[test]
    fn employment_v1_has_required_columns() {
        let t = Template::builtin("employment-v1").expect("loads");
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in [
            "employee_name",
            "base_salary",
            "non_compete",
            "ip_assignment",
        ] {
            assert!(names.contains(&must), "missing required col {must}");
        }
    }

    #[test]
    fn template_parses_extraction_metadata() {
        use crate::schema::column::{ExtractionMode, ExtractionNormalizer};
        let raw = r#"
id = "test-template"
name = "Test"
version = "1.0.0"
description = "Test"
vertical = "legal-fr"

[[column]]
name = "landlord"
prompt = "Landlord legal name."
type = "text"

[column.extraction]
mode = "local_span"
normalizer = "legal_name"
threshold = 0.45
keywords = ["bailleur", "entre les soussignes"]
labels = [
  { name = "bailleur", description = "Nom complet du bailleur" }
]
"#;
        let t: Template = Template::from_toml(raw).expect("template wire parses");
        let review = ReviewId::new();
        let cols = t.into_columns(review);

        assert_eq!(cols[0].extraction.mode, ExtractionMode::LocalSpan);
        assert_eq!(
            cols[0].extraction.normalizer,
            Some(ExtractionNormalizer::LegalName)
        );
        assert!((cols[0].extraction.threshold.unwrap() - 0.45_f32).abs() < 1e-5);
        assert_eq!(
            cols[0].extraction.keywords,
            vec!["bailleur", "entre les soussignes"]
        );
        assert_eq!(cols[0].extraction.labels[0].name, "bailleur");
    }

    #[test]
    fn ip_v1_has_required_columns() {
        let t = Template::builtin("ip-v1").expect("loads");
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["asset_name", "owner", "status", "encumbrances"] {
            assert!(names.contains(&must), "missing required col {must}");
        }
    }
}
