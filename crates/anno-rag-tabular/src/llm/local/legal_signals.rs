//! Legal signal catalog — maps column-level semantic roles to GLiNER
//! labels and per-label confidence thresholds.
//!
//! The catalog is self-contained: it ships hardcoded defaults that
//! mirror the `anno` legal label taxonomy so callers don't need to
//! import the model crate just to plan an extraction run.
//!
//! The live `Gliner2EntityExtractor` adapter (which wraps
//! `anno::backends::gliner2_fastino::GLiNER2Fastino`) lives in
//! `client.rs` and is only compiled when the `gliner2` feature is
//! enabled — tests there are `#[ignore]` to avoid downloading weights
//! in CI.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One entry in the legal signal catalog — a semantic label name plus
/// a short English description passed to GLiNER as the label text.
#[derive(Debug, Clone, PartialEq)]
pub struct LegalLabel {
    pub name: &'static str,
    pub description: &'static str,
}

/// Confidence thresholds and label metadata for a batch extraction call.
#[derive(Debug, Clone, PartialEq)]
pub struct LegalSignalPlan {
    /// `(name, description)` pairs for the GLiNER label argument.
    pub label_descriptions: Vec<(&'static str, &'static str)>,
    /// `(name, threshold)` pairs — one per label, in the same order.
    pub label_thresholds: Vec<(&'static str, f32)>,
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// Catalog of legal extraction signals with per-label thresholds.
///
/// Built from hardcoded defaults that mirror the `anno` legal taxonomy.
/// Call [`LegalSignalCatalog::default()`] for the shipped defaults.
#[derive(Debug, Clone)]
pub struct LegalSignalCatalog {
    labels: Vec<LegalLabel>,
    thresholds: HashMap<&'static str, f32>,
}

impl Default for LegalSignalCatalog {
    fn default() -> Self {
        // Labels mirror anno/gliner2_fastino legal taxonomy.
        let labels = vec![
            LegalLabel { name: "contract_party",      description: "Party to the contract (person or organisation)" },
            LegalLabel { name: "company_identifier",  description: "Company registration number (SIREN/SIRET/RCS)" },
            LegalLabel { name: "amount",              description: "Monetary amount" },
            LegalLabel { name: "date",                description: "Calendar date" },
            LegalLabel { name: "address",             description: "Postal or civic address" },
            LegalLabel { name: "obligation",          description: "Legal obligation or duty clause" },
            LegalLabel { name: "right",               description: "Legal right or entitlement clause" },
            LegalLabel { name: "jurisdiction",        description: "Governing law or jurisdiction" },
            LegalLabel { name: "duration",            description: "Contract term or duration" },
            LegalLabel { name: "penalty",             description: "Penalty, indemnity, or liquidated damages" },
        ];

        let thresholds: HashMap<&'static str, f32> = [
            ("contract_party",     0.65),
            ("company_identifier", 0.90),
            ("amount",             0.80),
            ("date",               0.75),
            ("address",            0.70),
            ("obligation",         0.55),
            ("right",              0.55),
            ("jurisdiction",       0.80),
            ("duration",           0.75),
            ("penalty",            0.70),
        ]
        .into_iter()
        .collect();

        Self { labels, thresholds }
    }
}

impl LegalSignalCatalog {
    /// Look up a label by name.
    pub fn label(&self, name: &str) -> Option<&LegalLabel> {
        self.labels.iter().find(|l| l.name == name)
    }

    /// Look up the confidence threshold for a label name.
    pub fn threshold(&self, name: &str) -> Option<f32> {
        self.thresholds.get(name).copied()
    }

    /// Build a [`LegalSignalPlan`] for the given label names.
    /// Unknown names are silently skipped.
    pub fn plan_for_labels(&self, names: &[&'static str]) -> LegalSignalPlan {
        let mut label_descriptions = Vec::new();
        let mut label_thresholds = Vec::new();

        for &name in names {
            if let Some(label) = self.label(name) {
                label_descriptions.push((label.name, label.description));
                if let Some(threshold) = self.threshold(name) {
                    label_thresholds.push((label.name, threshold));
                }
            }
        }

        LegalSignalPlan { label_descriptions, label_thresholds }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_default_legal_catalog_to_labels_and_thresholds() {
        let catalog = LegalSignalCatalog::default();

        assert!(catalog.label("contract_party").is_some());
        assert!(catalog.label("amount").is_some());
        assert_eq!(catalog.threshold("company_identifier"), Some(0.90));
        assert_eq!(catalog.threshold("obligation"), Some(0.55));
    }

    #[test]
    fn chooses_label_thresholds_for_catalog_backed_column() {
        let catalog = LegalSignalCatalog::default();
        let plan = catalog.plan_for_labels(&["contract_party", "amount"]);

        assert_eq!(
            plan.label_thresholds,
            vec![("contract_party", 0.65), ("amount", 0.80)]
        );
        assert!(plan.label_descriptions.iter().any(|(name, _)| *name == "contract_party"));
    }

    #[test]
    fn unknown_labels_silently_skipped() {
        let catalog = LegalSignalCatalog::default();
        let plan = catalog.plan_for_labels(&["contract_party", "nonexistent_label"]);
        assert_eq!(plan.label_descriptions.len(), 1);
        assert_eq!(plan.label_descriptions[0].0, "contract_party");
    }

    #[test]
    fn empty_names_gives_empty_plan() {
        let catalog = LegalSignalCatalog::default();
        let plan = catalog.plan_for_labels(&[]);
        assert!(plan.label_descriptions.is_empty());
        assert!(plan.label_thresholds.is_empty());
    }
}
