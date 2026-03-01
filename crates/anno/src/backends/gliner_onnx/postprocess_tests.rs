use super::expand_ner_label;
use super::looks_like_company_name;

#[test]
fn test_looks_like_company_name() {
    assert!(looks_like_company_name("Apple Inc"));
    assert!(looks_like_company_name("Acme Corp."));
    assert!(looks_like_company_name("Example GmbH"));
    assert!(looks_like_company_name("株式会社トヨタ自動車"));
    assert!(looks_like_company_name("شركة أرامكو"));

    assert!(!looks_like_company_name("Apple"));
    assert!(!looks_like_company_name("New York"));
}

#[test]
fn test_expand_ner_label_abbreviations() {
    // Standard abbreviations -> full words
    assert_eq!(expand_ner_label("PER"), "person");
    assert_eq!(expand_ner_label("PERSON"), "person");
    assert_eq!(expand_ner_label("ORG"), "organization");
    assert_eq!(expand_ner_label("ORGANIZATION"), "organization");
    assert_eq!(expand_ner_label("LOC"), "location");
    assert_eq!(expand_ner_label("LOCATION"), "location");
    assert_eq!(expand_ner_label("GPE"), "location");
    assert_eq!(expand_ner_label("MISC"), "miscellaneous");
    assert_eq!(expand_ner_label("MISCELLANEOUS"), "miscellaneous");
    assert_eq!(expand_ner_label("DATE"), "date");
    assert_eq!(expand_ner_label("MONEY"), "money");
    assert_eq!(expand_ner_label("TIME"), "time");
    assert_eq!(expand_ner_label("PRODUCT"), "product");
    assert_eq!(expand_ner_label("EVENT"), "event");
}

#[test]
fn test_expand_ner_label_case_insensitive() {
    assert_eq!(expand_ner_label("per"), "person");
    assert_eq!(expand_ner_label("Per"), "person");
    assert_eq!(expand_ner_label("org"), "organization");
    assert_eq!(expand_ner_label("Loc"), "location");
    assert_eq!(expand_ner_label("gpe"), "location");
}

#[test]
fn test_expand_ner_label_passthrough() {
    // Unknown labels pass through as lowercase
    assert_eq!(expand_ner_label("facility"), "facility");
    assert_eq!(expand_ner_label("FACILITY"), "facility");
    assert_eq!(expand_ner_label("work_of_art"), "work_of_art");
    assert_eq!(expand_ner_label("Custom Label"), "custom label");
}

#[test]
fn test_expand_ner_label_already_expanded() {
    // Already-correct labels stay the same
    assert_eq!(expand_ner_label("person"), "person");
    assert_eq!(expand_ner_label("organization"), "organization");
    assert_eq!(expand_ner_label("location"), "location");
}
