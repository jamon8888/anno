//! DD template library — load, evidence-triplet, enum-consistency, shape tests.

use anno_rag_tabular::schema::template::Template;

const DD_TEMPLATES: &[&str] = &[
    "spa-v1",
    "sha-v1",
    "jva-v1",
    "senior-facilities-v1",
    "security-package-v1",
    "intercreditor-v1",
    "material-commercial-v1",
    "commercial-lease-v1",
    "employment-exec-v1",
    "ip-portfolio-v1",
    "litigation-v1",
    "insurance-v1",
    "data-protection-v1",
    "debt-finance-v1",
    "corporate-captable-v1",
    "data-room-index-v1",
    "red-flags-register-v1",
];

/// Abstraction grids only (registers excluded — they have no evidence triplet).
const GRIDS: &[&str] = &[
    "spa-v1",
    "sha-v1",
    "jva-v1",
    "senior-facilities-v1",
    "security-package-v1",
    "intercreditor-v1",
    "material-commercial-v1",
    "commercial-lease-v1",
    "employment-exec-v1",
    "ip-portfolio-v1",
    "litigation-v1",
    "insurance-v1",
    "data-protection-v1",
    "debt-finance-v1",
    "corporate-captable-v1",
];

#[test]
fn all_dd_templates_load() {
    for id in DD_TEMPLATES {
        let t = Template::builtin(id).unwrap_or_else(|e| panic!("template {id} must load: {e}"));
        assert_eq!(t.id, *id, "template id mismatch in {id}");
        assert_eq!(t.vertical, "legal-intl", "{id} must be legal-intl");
        assert!(!t.columns.is_empty(), "{id} has no columns");
    }
}

#[test]
fn grids_have_evidence_triplet() {
    for id in GRIDS {
        let t = Template::builtin(id).expect("grid must load");
        let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        for must in ["source_reference", "key_risk_flag", "risk_note"] {
            assert!(names.contains(&must), "{id} missing evidence column {must}");
        }
    }
}

fn enum_options(t: &Template, col: &str) -> Option<Vec<String>> {
    use anno_rag_tabular::schema::template::CellTypeWire;
    let c = t.columns.iter().find(|c| c.name == col)?;
    match &c.cell_type {
        CellTypeWire::Enum { options } => Some(options.clone()),
        _ => None,
    }
}

#[test]
fn governing_law_enum_is_consistent() {
    let expected = vec![
        "FR", "DE", "UK", "US", "CH", "BE", "LU", "NL", "ES", "IT", "SE", "OTHER",
    ];
    for id in GRIDS {
        let t = Template::builtin(id).expect("loads");
        if let Some(opts) = enum_options(&t, "governing_law") {
            assert_eq!(opts, expected, "{id} governing_law enum drifted");
        }
    }
}

#[test]
fn workstream_enum_is_consistent() {
    let expected = vec![
        "corporate",
        "commercial",
        "employment",
        "ip_it",
        "real_estate",
        "litigation",
        "regulatory",
        "finance_debt",
        "tax",
        "data_protection",
        "insurance",
        "environmental",
    ];
    for id in ["data-room-index-v1", "red-flags-register-v1"] {
        let t = Template::builtin(id).expect("loads");
        let opts = enum_options(&t, "workstream").expect("has workstream enum");
        assert_eq!(opts, expected, "{id} workstream enum drifted");
    }
}

#[test]
fn spa_v1_has_expected_shape() {
    let t = Template::builtin("spa-v1").expect("loads");
    // 23 domain columns + 3 evidence-triplet columns = 26.
    assert_eq!(t.columns.len(), 26, "spa-v1 column count changed");
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    for must in [
        "purchase_price_amount",
        "mac_clause",
        "warranty_cap_amount",
        "governing_law",
    ] {
        assert!(names.contains(&must), "spa-v1 missing {must}");
    }
    // last three columns are the evidence triplet, in order
    assert_eq!(
        &names[names.len() - 3..],
        &["source_reference", "key_risk_flag", "risk_note"]
    );
}
