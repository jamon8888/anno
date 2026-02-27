#![allow(deprecated)]
use super::*;

#[test]
fn test_rule_based_ner() {
    let ner = RuleBasedNER::new();
    let text =
        "John Smith works at Acme Corp. He earns $100,000 per year. The meeting is on 2024-01-15.";
    let entities = ner.extract_entities(text, None).unwrap();

    // Should extract capitalized names, money, dates
    assert!(!entities.is_empty());
    assert!(entities.iter().any(|e| e.text == "John Smith"));
    assert!(entities.iter().any(|e| e.entity_type == EntityType::Money));
    assert!(entities.iter().any(|e| e.entity_type == EntityType::Date));
}

#[test]
fn test_common_word_filtering() {
    let ner = RuleBasedNER::new();
    let text = "The Figure shows the Results. However, the Introduction was clear.";
    let entities = ner.extract_entities(text, None).unwrap();

    // Common words should be filtered out
    assert!(!entities.iter().any(|e| e.text == "The"));
    assert!(!entities.iter().any(|e| e.text == "Figure"));
    assert!(!entities.iter().any(|e| e.text == "Results"));
    assert!(!entities.iter().any(|e| e.text == "However"));
    assert!(!entities.iter().any(|e| e.text == "Introduction"));
}

#[test]
fn test_without_filtering() {
    let ner = RuleBasedNER::without_filtering();
    // Use text where common words are NOT followed by other capitalized words
    // (the pattern greedily matches multi-word phrases like "The Figure")
    let text = "The cat sat on Figure today.";
    let entities = ner.extract_entities(text, None).unwrap();

    // Without filtering, common words should be included as standalone entities
    assert!(
        entities.iter().any(|e| e.text == "The"),
        "Expected 'The' in entities: {:?}",
        entities
    );
    assert!(
        entities.iter().any(|e| e.text == "Figure"),
        "Expected 'Figure' in entities: {:?}",
        entities
    );
}

#[test]
fn test_percentage_extraction() {
    let ner = RuleBasedNER::new();
    let text = "Accuracy improved by 15.5% and recall by 20%.";
    let entities = ner.extract_entities(text, None).unwrap();

    let percents: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Percent)
        .collect();
    assert_eq!(percents.len(), 2);
}

#[test]
fn test_model_interface() {
    let ner = RuleBasedNER::new();
    assert!(ner.is_available());
    assert_eq!(ner.name(), "rule");
    assert!(!ner.supported_types().is_empty());
}

// ============================================================================
// Pure function tests
// ============================================================================

#[test]
fn test_empty_text_returns_no_entities() {
    let ner = RuleBasedNER::new();
    let entities = ner.extract_entities("", None).unwrap();
    assert!(entities.is_empty(), "empty text should yield no entities");
}

#[test]
fn test_spans_overlap_helper() {
    // Overlapping spans
    assert!(spans_overlap(0, 5, 3, 8));
    assert!(spans_overlap(3, 8, 0, 5));
    // Contained span
    assert!(spans_overlap(0, 10, 2, 5));
    // Adjacent (non-overlapping) spans
    assert!(!spans_overlap(0, 5, 5, 10));
    assert!(!spans_overlap(5, 10, 0, 5));
    // Disjoint spans
    assert!(!spans_overlap(0, 3, 5, 8));
    // Zero-width span at boundary
    assert!(!spans_overlap(0, 5, 5, 5));
}

#[test]
fn test_strip_leading_article_helper() {
    assert_eq!(strip_leading_article("The Company"), "Company");
    assert_eq!(strip_leading_article("A Person"), "Person");
    assert_eq!(strip_leading_article("An Entity"), "Entity");
    // No article -- unchanged
    assert_eq!(strip_leading_article("Microsoft"), "Microsoft");
    // "The" alone should strip to empty
    assert_eq!(strip_leading_article("The "), "");
    // "There" should NOT be stripped (not "The " prefix)
    assert_eq!(strip_leading_article("There"), "There");
}

#[test]
fn test_starts_with_noise_helper() {
    assert!(starts_with_noise("According to the study"));
    assert!(starts_with_noise("Based on evidence"));
    assert!(starts_with_noise("Following the experiment"));
    assert!(starts_with_noise("Attention Is All You Need"));
    // Not noise
    assert!(!starts_with_noise("Google Research"));
    assert!(!starts_with_noise("John Smith"));
}

#[test]
fn test_infer_entity_type_persons() {
    // Common surname triggers Person
    assert_eq!(infer_entity_type("Wang Lei"), EntityType::Person);
    assert_eq!(infer_entity_type("Kim Yoon"), EntityType::Person);
    // Single common surname
    assert_eq!(infer_entity_type("Tanaka"), EntityType::Person);
    // Not a known surname -- falls through
    assert_ne!(infer_entity_type("Zephyr"), EntityType::Person);
}

#[test]
fn test_infer_entity_type_concepts_and_acronyms() {
    // Technical/concept terms
    assert_eq!(
        infer_entity_type("Neural Network"),
        EntityType::Other("concept".to_string())
    );
    assert_eq!(
        infer_entity_type("Deep Learning"),
        EntityType::Other("concept".to_string())
    );
    // Short all-caps acronym
    assert_eq!(
        infer_entity_type("BERT"),
        EntityType::Other("acronym".to_string())
    );
    assert_eq!(
        infer_entity_type("GPT"),
        EntityType::Other("acronym".to_string())
    );
    // Too long for acronym heuristic (>5 chars)
    assert_ne!(
        infer_entity_type("ABCDEF"),
        EntityType::Other("acronym".to_string())
    );
}

#[test]
fn test_is_common_surname_helper() {
    assert!(is_common_surname("Wang"));
    assert!(is_common_surname("Kim"));
    assert!(is_common_surname("Suzuki"));
    assert!(is_common_surname("Smith"));
    assert!(!is_common_surname("Xyzzyplugh"));
    assert!(!is_common_surname("wang")); // case-sensitive
}

#[test]
fn test_known_org_extraction() {
    let ner = RuleBasedNER::new();
    let text = "NASA and CERN announced a joint project with MIT.";
    let entities = ner.extract_entities(text, None).unwrap();

    let orgs: Vec<&str> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Organization)
        .map(|e| e.text.as_str())
        .collect();
    assert!(orgs.contains(&"NASA"), "missing NASA in {orgs:?}");
    assert!(orgs.contains(&"CERN"), "missing CERN in {orgs:?}");
    assert!(orgs.contains(&"MIT"), "missing MIT in {orgs:?}");
}

#[test]
fn test_org_suffix_extraction() {
    let ner = RuleBasedNER::new();
    let text = "She joined Acme Corporation last year.";
    let entities = ner.extract_entities(text, None).unwrap();

    assert!(
        entities
            .iter()
            .any(|e| e.text == "Acme Corporation" && e.entity_type == EntityType::Organization),
        "expected 'Acme Corporation' as Organization, got: {entities:?}"
    );
}

#[test]
fn test_unicode_text_char_offsets() {
    let ner = RuleBasedNER::new();
    // Multi-byte prefix so byte vs char offsets diverge.
    let text = "\u{00e9}\u{00e9}\u{00e9} NASA rocks";
    let entities = ner.extract_entities(text, None).unwrap();

    let nasa = entities.iter().find(|e| e.text == "NASA");
    assert!(nasa.is_some(), "NASA not found in: {entities:?}");
    let nasa = nasa.unwrap();
    // "eee " is 4 chars; NASA spans chars 4..8
    assert_eq!(nasa.start, 4, "char start");
    assert_eq!(nasa.end, 8, "char end");
}

#[test]
fn test_min_confidence_filters() {
    // High threshold should drop low-confidence entities
    let ner = RuleBasedNER::with_min_confidence(0.9);
    let text = "John Smith visited Berlin.";
    let entities = ner.extract_entities(text, None).unwrap();

    // Person matches at 0.7 confidence should be filtered
    assert!(
        !entities.iter().any(|e| e.entity_type == EntityType::Person),
        "person entities should be filtered at min_confidence=0.9: {entities:?}"
    );
}

#[test]
fn test_money_extraction_variants() {
    let ner = RuleBasedNER::new();
    let text = "The deal was worth $3.5 billion and the fee was $200.";
    let entities = ner.extract_entities(text, None).unwrap();

    let money: Vec<&str> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Money)
        .map(|e| e.text.as_str())
        .collect();
    assert_eq!(money.len(), 2, "expected 2 money entities, got: {money:?}");
}

#[test]
fn test_date_extraction_formats() {
    let ner = RuleBasedNER::new();
    // ISO date and quarter format (no month-name dates since capitalized
    // month names get captured by the capitalized-word pattern first).
    let text = "filed on 2024-01-15 for Q1 review and again on 12/25/2024.";
    let entities = ner.extract_entities(text, None).unwrap();

    let dates: Vec<&str> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Date)
        .map(|e| e.text.as_str())
        .collect();
    assert!(
        dates.contains(&"2024-01-15"),
        "missing ISO date in {dates:?}"
    );
    assert!(dates.contains(&"Q1"), "missing quarter date in {dates:?}");
    assert!(
        dates.contains(&"12/25/2024"),
        "missing MM/DD/YYYY date in {dates:?}"
    );
}

#[test]
fn test_location_gazetteer() {
    let ner = RuleBasedNER::new();
    let text = "Offices in Tokyo and Berlin serve the Asia-Pacific region.";
    let entities = ner.extract_entities(text, None).unwrap();

    let locs: Vec<&str> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Location)
        .map(|e| e.text.as_str())
        .collect();
    assert!(locs.contains(&"Tokyo"), "missing Tokyo in {locs:?}");
    assert!(locs.contains(&"Berlin"), "missing Berlin in {locs:?}");
}
