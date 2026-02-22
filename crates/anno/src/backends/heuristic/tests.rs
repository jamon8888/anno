use super::*;

#[test]
fn test_basic_person_detection() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Dr. John Smith met with Mary.", None)
        .unwrap();

    let names: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
    assert!(
        names
            .iter()
            .any(|n| n.contains("John") || n.contains("Smith")),
        "Should detect John Smith: {:?}",
        names
    );
}

#[test]
fn test_organization_suffix_detection() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Apple Inc. announced new products.", None)
        .unwrap();

    let orgs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Organization))
        .collect();
    assert!(!orgs.is_empty(), "Should detect Apple Inc. as organization");
}

#[test]
fn test_location_preposition_context() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("She lived in Paris for years.", None)
        .unwrap();

    let locs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Location))
        .collect();
    assert!(!locs.is_empty(), "Should detect Paris as location");
}

#[test]
fn test_known_organizations() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Google and Microsoft competed.", None)
        .unwrap();

    let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
    assert!(
        texts.iter().any(|t| t.contains("Google")),
        "Should detect Google"
    );
    assert!(
        texts.iter().any(|t| t.contains("Microsoft")),
        "Should detect Microsoft"
    );
}

#[test]
fn test_cjk_organization_detection() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("ソニーが新製品を発表しました。", None)
        .unwrap();

    let orgs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Organization))
        .collect();
    assert!(
        !orgs.is_empty(),
        "Should detect Sony (ソニー) as organization"
    );
}

#[test]
fn test_cjk_location_detection() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("東京オリンピックが開催された。", None)
        .unwrap();

    let locs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Location))
        .collect();
    assert!(!locs.is_empty(), "Should detect Tokyo (東京) as location");
}

#[test]
fn test_empty_text() {
    let ner = HeuristicNER::new();
    let entities = ner.extract_entities("", None).unwrap();
    assert!(entities.is_empty());
}

#[test]
fn test_no_entities() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("the quick brown fox jumps over the lazy dog", None)
        .unwrap();
    // All lowercase, no entities expected
    assert!(
        entities.is_empty(),
        "Lowercase text should have no entities"
    );
}

#[test]
fn test_threshold_filtering() {
    let low_threshold = HeuristicNER::with_threshold(0.1);
    let high_threshold = HeuristicNER::with_threshold(0.9);

    let text = "John works at Google.";
    let low_entities = low_threshold.extract_entities(text, None).unwrap();
    let high_entities = high_threshold.extract_entities(text, None).unwrap();

    // Lower threshold should capture more or equal entities
    assert!(low_entities.len() >= high_entities.len());
}

#[test]
fn test_sentence_starter_filtering() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("The dog ran. It was fast.", None)
        .unwrap();

    // "The" and "It" should be filtered as common sentence starters
    let texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
    assert!(
        !texts.contains(&"The"),
        "Should filter 'The' as sentence starter"
    );
    assert!(!texts.contains(&"It"), "Should filter 'It' as pronoun");
}

#[test]
fn test_person_prefix_detection() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Prof. Einstein presented the theory.", None)
        .unwrap();

    let persons: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Person))
        .collect();
    assert!(
        !persons.is_empty(),
        "Should detect Prof. Einstein as person"
    );
}

#[test]
fn test_multi_word_organization() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Bank of America provides services.", None)
        .unwrap();

    let orgs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Organization))
        .collect();
    assert!(!orgs.is_empty(), "Should detect 'Bank of America' pattern");
}

#[test]
fn test_location_indicators() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("New Zealand is beautiful.", None)
        .unwrap();

    let locs: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.entity_type, EntityType::Location))
        .collect();
    assert!(!locs.is_empty(), "Should detect 'New Zealand' as location");
}

#[test]
fn test_model_trait_implementation() {
    let ner = HeuristicNER::new();

    assert_eq!(ner.name(), "heuristic");
    assert!(ner.is_available());
    assert!(!ner.supported_types().is_empty());
    assert!(ner.description().contains("Heuristic"));
}

#[test]
fn test_entity_offsets_are_valid() {
    let ner = HeuristicNER::new();
    let text = "Barack Obama visited Berlin yesterday.";
    let entities = ner.extract_entities(text, None).unwrap();

    let char_count = text.chars().count();
    for entity in &entities {
        assert!(entity.start <= entity.end, "start should be <= end");
        assert!(entity.end <= char_count, "end should be within text");

        // Verify text matches span
        let extracted: String = text
            .chars()
            .skip(entity.start)
            .take(entity.end - entity.start)
            .collect();
        assert_eq!(
            extracted, entity.text,
            "Extracted text should match entity text"
        );
    }
}

#[test]
fn test_unicode_text_handling() {
    let ner = HeuristicNER::new();
    let text = "François Müller from München met José García.";
    let entities = ner.extract_entities(text, None).unwrap();

    // Should handle diacritics correctly
    for entity in &entities {
        let extracted: String = text
            .chars()
            .skip(entity.start)
            .take(entity.end - entity.start)
            .collect();
        assert_eq!(extracted, entity.text, "Unicode offsets should be correct");
    }
}

#[test]
fn test_provenance_is_set() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Google announced today.", None)
        .unwrap();

    for entity in &entities {
        if let Some(ref prov) = entity.provenance {
            assert_eq!(prov.source, "heuristic");
            assert!(matches!(prov.method, ExtractionMethod::Heuristic));
        }
    }
}

// =========================================================================
// Acronym signal tests (domain-agnostic, language-agnostic)
// =========================================================================

#[test]
fn test_is_acronym_word_latin() {
    assert!(is_acronym_word("PARC"));
    assert!(is_acronym_word("IBM"));
    assert!(is_acronym_word("NASA"));
    assert!(is_acronym_word("N2K"));
    assert!(is_acronym_word("DARPA."));
    assert!(is_acronym_word("(NATO)"));
    assert!(!is_acronym_word("Xerox"));
    assert!(!is_acronym_word("Lynn"));
    assert!(!is_acronym_word("A"));
    assert!(!is_acronym_word("42"));
    assert!(!is_acronym_word(""));
}

#[test]
fn test_is_acronym_word_cyrillic() {
    assert!(is_acronym_word("\u{041D}\u{0410}\u{0422}\u{041E}")); // НАТО
    assert!(is_acronym_word("\u{041C}\u{0418}\u{0414}")); // МИД
    assert!(!is_acronym_word(
        "\u{041C}\u{043E}\u{0441}\u{043A}\u{0432}\u{0430}"
    )); // Москва
}

#[test]
fn test_is_acronym_word_caseless_scripts() {
    assert!(!is_acronym_word("\u{6771}\u{4EAC}")); // 東京 (CJK)
    assert!(!is_acronym_word("\u{30BD}\u{30CB}\u{30FC}")); // ソニー (Katakana)
    assert!(!is_acronym_word("\u{062D}\u{0645}\u{0627}\u{0633}")); // حماس (Arabic)
}

#[test]
fn test_acronym_in_multi_word_span_signals_org() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities(
            "Lynn Conway worked at IBM and Xerox PARC in California.",
            None,
        )
        .unwrap();
    let xerox_parc = entities.iter().find(|e| e.text == "Xerox PARC");
    assert!(
        xerox_parc.is_some(),
        "Should detect 'Xerox PARC': {entities:?}"
    );
    assert!(
        matches!(xerox_parc.unwrap().entity_type, EntityType::Organization),
        "Xerox PARC should be ORG, got {:?}",
        xerox_parc.unwrap().entity_type,
    );
}

#[test]
fn test_acronym_no_regression_on_normal_names() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("Lynn Conway designed the processor.", None)
        .unwrap();
    let lynn = entities.iter().find(|e| e.text == "Lynn Conway");
    assert!(lynn.is_some(), "Should detect 'Lynn Conway': {entities:?}");
    assert!(
        matches!(lynn.unwrap().entity_type, EntityType::Person),
        "Lynn Conway should remain PER, got {:?}",
        lynn.unwrap().entity_type,
    );
}

#[test]
fn test_single_acronym_signals_org() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("She joined DARPA last year.", None)
        .unwrap();
    let darpa = entities.iter().find(|e| e.text == "DARPA");
    assert!(darpa.is_some(), "Should detect 'DARPA': {entities:?}");
    assert!(
        matches!(darpa.unwrap().entity_type, EntityType::Organization),
        "DARPA should be ORG, got {:?}",
        darpa.unwrap().entity_type,
    );
}

#[test]
fn test_known_loc_acronym_still_loc() {
    let ner = HeuristicNER::new();
    let entities = ner
        .extract_entities("She moved to USA last year.", None)
        .unwrap();
    let usa = entities.iter().find(|e| e.text == "USA");
    assert!(usa.is_some(), "Should detect 'USA': {entities:?}");
    assert!(
        matches!(usa.unwrap().entity_type, EntityType::Location),
        "USA should be LOC (gazetteer wins), got {:?}",
        usa.unwrap().entity_type,
    );
}
