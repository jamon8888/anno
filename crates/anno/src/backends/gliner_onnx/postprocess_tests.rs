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
