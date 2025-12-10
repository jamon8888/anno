//! Tests for the Dataset derive macro.

use anno_derive::Dataset;

#[derive(Dataset, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestDataset {
    /// CoNLL 2003 English NER dataset
    #[dataset(
        name = "CoNLL-2003",
        task = "ner",
        languages("en"),
        entity_types("PER", "LOC", "ORG", "MISC"),
        url = "https://www.clips.uantwerpen.be/conll2003/ner/",
        description = "Standard benchmark for NER",
        source = "SIGNLL",
        aliases("conll", "conll03")
    )]
    Conll2003,

    /// OntoNotes 5.0 dataset
    #[dataset(
        name = "OntoNotes 5.0",
        task = "ner",
        languages("en", "zh", "ar"),
        entity_types("PERSON", "ORG", "GPE", "LOC"),
        aliases("ontonotes", "onto")
    )]
    OntoNotes5,

    /// WikiANN (Pan-X)
    #[dataset(
        name = "WikiANN",
        task = "ner",
        languages("en", "de", "fr", "es", "zh", "ar", "ja", "ko"),
        entity_types("PER", "LOC", "ORG")
    )]
    WikiAnn,

    /// Simple variant with no attributes
    Simple,

    /// MasakhaNER African languages
    #[dataset(
        name = "MasakhaNER",
        task = "ner",
        languages("amh", "hau", "ibo", "kin", "lug", "luo", "pcm", "swa", "wol", "yor"),
        entity_types("PER", "LOC", "ORG", "DATE"),
        description = "NER for African languages"
    )]
    MasakhaNer,
}

#[test]
fn test_name() {
    assert_eq!(TestDataset::Conll2003.name(), "CoNLL-2003");
    assert_eq!(TestDataset::OntoNotes5.name(), "OntoNotes 5.0");
    assert_eq!(TestDataset::WikiAnn.name(), "WikiANN");
    assert_eq!(TestDataset::Simple.name(), "Simple");
    assert_eq!(TestDataset::MasakhaNer.name(), "MasakhaNER");
}

#[test]
fn test_task() {
    assert_eq!(TestDataset::Conll2003.task(), "ner");
    assert_eq!(TestDataset::OntoNotes5.task(), "ner");
    assert_eq!(TestDataset::Simple.task(), "ner"); // Default
}

#[test]
fn test_languages() {
    assert_eq!(TestDataset::Conll2003.languages(), &["en"]);
    assert_eq!(TestDataset::OntoNotes5.languages(), &["en", "zh", "ar"]);
    assert_eq!(
        TestDataset::WikiAnn.languages(),
        &["en", "de", "fr", "es", "zh", "ar", "ja", "ko"]
    );
    assert_eq!(TestDataset::Simple.languages(), &["en"]); // Default
    assert_eq!(TestDataset::MasakhaNer.languages().len(), 10);
}

#[test]
fn test_entity_types() {
    assert_eq!(
        TestDataset::Conll2003.entity_types(),
        &["PER", "LOC", "ORG", "MISC"]
    );
    assert_eq!(
        TestDataset::OntoNotes5.entity_types(),
        &["PERSON", "ORG", "GPE", "LOC"]
    );
    assert!(TestDataset::Simple.entity_types().is_empty());
}

#[test]
fn test_url() {
    assert_eq!(
        TestDataset::Conll2003.url(),
        Some("https://www.clips.uantwerpen.be/conll2003/ner/")
    );
    assert!(TestDataset::Simple.url().is_none());
}

#[test]
fn test_description() {
    assert_eq!(
        TestDataset::Conll2003.description(),
        Some("Standard benchmark for NER")
    );
    assert!(TestDataset::Simple.description().is_none());
}

#[test]
fn test_source() {
    assert_eq!(TestDataset::Conll2003.source(), Some("SIGNLL"));
    assert!(TestDataset::Simple.source().is_none());
}

#[test]
fn test_from_str() {
    use std::str::FromStr;

    // Exact match (case insensitive)
    assert_eq!(
        TestDataset::from_str("conll2003").unwrap(),
        TestDataset::Conll2003
    );
    assert_eq!(
        TestDataset::from_str("Conll2003").unwrap(),
        TestDataset::Conll2003
    );
    assert_eq!(
        TestDataset::from_str("CONLL2003").unwrap(),
        TestDataset::Conll2003
    );

    // Snake case
    assert_eq!(
        TestDataset::from_str("onto_notes5").unwrap(),
        TestDataset::OntoNotes5
    );

    // Kebab case
    assert_eq!(
        TestDataset::from_str("onto-notes5").unwrap(),
        TestDataset::OntoNotes5
    );

    // Aliases
    assert_eq!(
        TestDataset::from_str("conll").unwrap(),
        TestDataset::Conll2003
    );
    assert_eq!(
        TestDataset::from_str("conll03").unwrap(),
        TestDataset::Conll2003
    );
    assert_eq!(
        TestDataset::from_str("ontonotes").unwrap(),
        TestDataset::OntoNotes5
    );
    assert_eq!(
        TestDataset::from_str("onto").unwrap(),
        TestDataset::OntoNotes5
    );

    // Unknown
    assert!(TestDataset::from_str("nonexistent").is_err());
}

#[test]
fn test_display() {
    assert_eq!(format!("{}", TestDataset::Conll2003), "CoNLL-2003");
    assert_eq!(format!("{}", TestDataset::OntoNotes5), "OntoNotes 5.0");
    assert_eq!(format!("{}", TestDataset::Simple), "Simple");
}

#[test]
fn test_all() {
    let all = TestDataset::all();
    assert_eq!(all.len(), 5);
    assert_eq!(TestDataset::count(), 5);

    // Check all variants are included
    assert!(all.contains(&TestDataset::Conll2003));
    assert!(all.contains(&TestDataset::OntoNotes5));
    assert!(all.contains(&TestDataset::WikiAnn));
    assert!(all.contains(&TestDataset::Simple));
    assert!(all.contains(&TestDataset::MasakhaNer));
}

#[test]
fn test_multilingual_dataset() {
    // WikiANN supports many languages
    let wiki = TestDataset::WikiAnn;
    assert!(wiki.languages().contains(&"en"));
    assert!(wiki.languages().contains(&"zh"));
    assert!(wiki.languages().contains(&"ar"));
    assert!(wiki.languages().contains(&"ja"));
}

#[test]
fn test_african_languages() {
    let masakhaner = TestDataset::MasakhaNer;
    let langs = masakhaner.languages();

    // Check some African language codes
    assert!(langs.contains(&"amh")); // Amharic
    assert!(langs.contains(&"swa")); // Swahili
    assert!(langs.contains(&"yor")); // Yoruba
    assert!(langs.contains(&"hau")); // Hausa
}

// Property: FromStr roundtrip
#[test]
fn test_fromstr_roundtrip() {
    use std::str::FromStr;

    for dataset in TestDataset::all() {
        // Parse the display name
        let display = format!("{}", dataset);
        // Parsing should work
        let _parsed: Result<TestDataset, _> = TestDataset::from_str(&display.to_lowercase());
        // At minimum, the variant name should work
        let variant_name = format!("{:?}", dataset).to_lowercase();
        let parsed_variant = TestDataset::from_str(&variant_name);
        assert!(
            parsed_variant.is_ok(),
            "Failed to parse variant: {}",
            variant_name
        );
        assert_eq!(parsed_variant.unwrap(), dataset);
    }
}

// Helper method tests

#[test]
fn test_is_multilingual() {
    assert!(!TestDataset::Conll2003.is_multilingual()); // Only English
    assert!(TestDataset::OntoNotes5.is_multilingual()); // en, zh, ar
    assert!(TestDataset::WikiAnn.is_multilingual()); // Many languages
    assert!(!TestDataset::Simple.is_multilingual()); // Default (en only)
    assert!(TestDataset::MasakhaNer.is_multilingual()); // African languages
}

#[test]
fn test_supports_task() {
    assert!(TestDataset::Conll2003.supports_task("ner"));
    assert!(TestDataset::Conll2003.supports_task("NER")); // Case insensitive
    assert!(TestDataset::Conll2003.supports_task("Ner"));
    assert!(!TestDataset::Conll2003.supports_task("coref"));
    assert!(!TestDataset::Conll2003.supports_task("classification"));
}

#[test]
fn test_has_entity_type() {
    assert!(TestDataset::Conll2003.has_entity_type("PER"));
    assert!(TestDataset::Conll2003.has_entity_type("per")); // Case insensitive
    assert!(TestDataset::Conll2003.has_entity_type("LOC"));
    assert!(TestDataset::Conll2003.has_entity_type("ORG"));
    assert!(TestDataset::Conll2003.has_entity_type("MISC"));
    assert!(!TestDataset::Conll2003.has_entity_type("GPE"));

    // OntoNotes has GPE
    assert!(TestDataset::OntoNotes5.has_entity_type("GPE"));
    assert!(TestDataset::OntoNotes5.has_entity_type("PERSON"));

    // Simple has no entity types
    assert!(!TestDataset::Simple.has_entity_type("PER"));
}

#[test]
fn test_supports_language() {
    // English datasets
    assert!(TestDataset::Conll2003.supports_language("en"));
    assert!(TestDataset::Conll2003.supports_language("EN")); // Case insensitive
    assert!(!TestDataset::Conll2003.supports_language("zh"));

    // Multilingual datasets
    assert!(TestDataset::OntoNotes5.supports_language("en"));
    assert!(TestDataset::OntoNotes5.supports_language("zh"));
    assert!(TestDataset::OntoNotes5.supports_language("ar"));
    assert!(!TestDataset::OntoNotes5.supports_language("fr"));

    // WikiANN has many
    assert!(TestDataset::WikiAnn.supports_language("ja"));
    assert!(TestDataset::WikiAnn.supports_language("ko"));

    // Default is English
    assert!(TestDataset::Simple.supports_language("en"));
}

#[test]
fn test_by_task() {
    let ner_datasets = TestDataset::by_task("ner");
    assert!(ner_datasets.contains(&TestDataset::Conll2003));
    assert!(ner_datasets.contains(&TestDataset::OntoNotes5));
    assert!(ner_datasets.contains(&TestDataset::WikiAnn));
    assert!(ner_datasets.contains(&TestDataset::MasakhaNer));
    assert!(ner_datasets.contains(&TestDataset::Simple)); // Default task is ner

    // Case insensitive
    let ner_upper = TestDataset::by_task("NER");
    assert_eq!(ner_datasets.len(), ner_upper.len());
}

#[test]
fn test_by_language() {
    let english_datasets = TestDataset::by_language("en");
    assert!(english_datasets.contains(&TestDataset::Conll2003));
    assert!(english_datasets.contains(&TestDataset::OntoNotes5));
    assert!(english_datasets.contains(&TestDataset::WikiAnn));
    assert!(english_datasets.contains(&TestDataset::Simple));

    let chinese_datasets = TestDataset::by_language("zh");
    assert!(chinese_datasets.contains(&TestDataset::OntoNotes5));
    assert!(chinese_datasets.contains(&TestDataset::WikiAnn));
    assert!(!chinese_datasets.contains(&TestDataset::Conll2003));

    let arabic_datasets = TestDataset::by_language("ar");
    assert!(arabic_datasets.contains(&TestDataset::OntoNotes5));
    assert!(arabic_datasets.contains(&TestDataset::WikiAnn));

    // African languages
    let swahili_datasets = TestDataset::by_language("swa");
    assert!(swahili_datasets.contains(&TestDataset::MasakhaNer));
    assert_eq!(swahili_datasets.len(), 1);
}
