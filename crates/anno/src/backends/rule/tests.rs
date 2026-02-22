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
