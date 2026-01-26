//! African Language Dataset Integration Tests
//!
//! Tests for Masakhane community datasets:
//! - MasakhaNER / MasakhaNER 2.0 (NER for 10-20 African languages)
//! - AfriSenti (Sentiment analysis for 14 languages)
//! - AfriQA (Cross-lingual QA)
//! - MasakhaNEWS (Topic classification)
//! - MasakhaPOS (POS tagging, CoNLL-U format)
//!
//! ## Running Tests
//!
//! ```bash
//! # Unit tests (no network)
//! cargo test --test african_language_datasets
//!
//! # Integration tests with downloads (requires network)
//! cargo test --test african_language_datasets -- --ignored --nocapture
//! ```
//!
//! ## References
//!
//! - MasakhaNER: Adelani et al. (TACL 2021) https://aclanthology.org/2021.tacl-1.66/
//! - MasakhaNER 2.0: Adelani et al. (EMNLP 2022) https://aclanthology.org/2022.emnlp-main.298/
//! - AfriSenti: Muhammad et al. (SemEval 2023) https://aclanthology.org/2023.semeval-1.15/
//! - AfriQA: Ogundepo et al. (EMNLP 2023) https://aclanthology.org/2023.findings-emnlp.997/
//! - MasakhaNEWS: Adelani et al. (ACL 2023) https://aclanthology.org/2023.acl-long.574/

use anno::eval::loader::{DatasetId, DatasetLoader};

// =============================================================================
// Dataset Configuration Tests
// =============================================================================

#[test]
fn test_african_datasets_exist_in_loader() {
    // All African datasets should be defined
    let african_datasets = DatasetId::all_african_languages();
    assert!(
        african_datasets.len() >= 6,
        "Should have at least 6 African datasets"
    );

    // Specific datasets should exist
    assert!(african_datasets.contains(&&DatasetId::MasakhaNER));
    assert!(african_datasets.contains(&&DatasetId::MasakhaNER2));
    assert!(african_datasets.contains(&&DatasetId::AfriSenti));
    assert!(african_datasets.contains(&&DatasetId::AfriQA));
    assert!(african_datasets.contains(&&DatasetId::MasakhaNEWS));
    assert!(african_datasets.contains(&&DatasetId::MasakhaPOS));
}

#[test]
fn test_african_datasets_have_metadata() {
    let african_datasets = DatasetId::all_african_languages();
    for dataset in african_datasets {
        // Each should have a non-empty name
        let name = dataset.name();
        assert!(!name.is_empty(), "{:?} should have a name", dataset);

        // Each should have a non-empty description
        let desc = dataset.description();
        assert!(!desc.is_empty(), "{:?} should have a description", dataset);

        // Each should have a download URL
        let url = dataset.download_url();
        assert!(!url.is_empty(), "{:?} should have a download URL", dataset);

        // Each should be classified as African language
        assert!(
            dataset.is_african_language(),
            "{:?} should be African language",
            dataset
        );
    }
}

#[test]
#[ignore] // Requires african_language_codes() method implementation
fn test_african_language_codes_coverage() {
    // TODO: Implement african_language_codes() method on DatasetId
    // This would return language codes for multilingual African datasets
    // For now, we verify the datasets exist and are marked as African language
    assert!(DatasetId::MasakhaNER.is_african_language());
    assert!(DatasetId::MasakhaNER2.is_african_language());

    // MasakhaNER v1 should have 10 languages (documented)
    // MasakhaNER v2 should have 20 languages (documented)
    // When african_language_codes() is implemented, uncomment:
    // let v1_langs = DatasetId::MasakhaNER.african_language_codes();
    // assert_eq!(v1_langs.len(), 10, "MasakhaNER v1 should have 10 languages");
    // assert!(v1_langs.contains(&"yo"), "Should have Yoruba");
    // assert!(v1_langs.contains(&"sw"), "Should have Swahili");
    // assert!(v1_langs.contains(&"ha"), "Should have Hausa");
    // assert!(v1_langs.contains(&"ig"), "Should have Igbo");
    // assert!(v1_langs.contains(&"am"), "Should have Amharic");
    //
    // let v2_langs = DatasetId::MasakhaNER2.african_language_codes();
    // assert_eq!(v2_langs.len(), 20, "MasakhaNER v2 should have 20 languages");
    // for lang in v1_langs {
    //     assert!(v2_langs.contains(lang), "v2 should include {} from v1", lang);
    // }
    // assert!(v2_langs.contains(&"zul"), "Should have Zulu");
    // assert!(v2_langs.contains(&"xho"), "Should have Xhosa");
}

#[test]
#[ignore] // Requires african_language_url() method implementation
fn test_african_language_url_generation() {
    // TODO: Implement african_language_url(lang_code, split) method on DatasetId
    // This would generate language-specific download URLs for multilingual datasets
    // For now, we verify the datasets exist and have download URLs
    assert!(DatasetId::MasakhaNER2.is_african_language());
    assert!(!DatasetId::MasakhaNER2.download_url().is_empty());

    // When african_language_url() is implemented, uncomment:
    // let yoruba_test = DatasetId::MasakhaNER2.african_language_url("yo", "test");
    // assert!(yoruba_test.is_some(), "Should generate URL for Yoruba test split");
    // let url = yoruba_test.unwrap();
    // assert!(url.contains("yo"), "URL should contain language code");
    // assert!(url.contains("test"), "URL should contain split name");
    //
    // let invalid = DatasetId::MasakhaNER2.african_language_url("invalid", "test");
    // assert!(invalid.is_none(), "Should not generate URL for invalid language");
    //
    // let non_african = DatasetId::WikiGold.african_language_url("yo", "test");
    // assert!(non_african.is_none(), "Non-African dataset should not have language URLs");
}

#[test]
fn test_african_dataset_entity_types() {
    // MasakhaNER uses standard NER types
    let ner_types = DatasetId::MasakhaNER.entity_types();
    assert!(ner_types.contains(&"PER"), "Should have PER type");
    assert!(ner_types.contains(&"LOC"), "Should have LOC type");
    assert!(ner_types.contains(&"ORG"), "Should have ORG type");
    assert!(ner_types.contains(&"DATE"), "Should have DATE type");

    // AfriSenti uses sentiment labels
    let senti_types = DatasetId::AfriSenti.entity_types();
    assert!(
        senti_types.contains(&"positive"),
        "Should have positive label"
    );
    assert!(
        senti_types.contains(&"negative"),
        "Should have negative label"
    );
    assert!(
        senti_types.contains(&"neutral"),
        "Should have neutral label"
    );
}

// =============================================================================
// Unicode and Script Handling Tests
// =============================================================================

#[test]
fn test_yoruba_tonal_diacritics_handling() {
    // Yoruba uses Latin script with tonal diacritics
    let yoruba_text = "Olúṣẹ́gun Obásanjọ́ jẹ́ Ààrẹ ní Abẹ́òkúta";

    // Verify character counting handles combining diacritics
    let char_count = yoruba_text.chars().count();
    assert!(
        char_count > 30,
        "Should have significant char count: {}",
        char_count
    );

    // Verify we can extract substrings correctly
    let first_word: String = yoruba_text
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    assert!(first_word.contains('ú'), "Should preserve tonal marks");
}

#[test]
fn test_ethiopic_script_handling() {
    // Amharic uses Ethiopic/Ge'ez script
    let amharic_text = "ዶ/ር አብይ አህመድ የኢትዮጵያ ጠቅላይ ሚኒስትር ናቸው";

    // Verify character counting
    let char_count = amharic_text.chars().count();
    assert!(
        char_count > 20,
        "Should have valid char count: {}",
        char_count
    );

    // Ethiopic characters should be single codepoints
    let first_char = amharic_text.chars().next().unwrap();
    assert!(
        first_char as u32 >= 0x1200 && first_char as u32 <= 0x137F,
        "First char should be Ethiopic"
    );
}

#[test]
fn test_mixed_script_handling() {
    // Nigerian Pidgin often mixes with English
    let pidgin_text = "Buhari don announce say MTN go pay N330 billion";

    // Should handle ASCII + currency symbol
    assert!(pidgin_text.contains("N330"), "Should have currency amount");

    // Character count should be straightforward (Latin script)
    let char_count = pidgin_text.chars().count();
    assert_eq!(
        char_count,
        pidgin_text.len(),
        "ASCII text should have equal char/byte count"
    );
}

// =============================================================================
// Loader Tests (require network when not mocked)
// =============================================================================

#[test]
fn test_loader_creation() {
    // DatasetLoader should be creatable
    let loader = DatasetLoader::new();
    assert!(loader.is_ok(), "Should create DatasetLoader");
}

#[test]
fn test_cache_filename_mapping() {
    // Each African dataset should have a unique cache filename
    let mut filenames = std::collections::HashSet::new();

    let african_datasets = DatasetId::all_african_languages();
    for dataset in african_datasets {
        let filename = dataset.cache_filename();
        assert!(
            !filename.is_empty(),
            "{:?} should have cache filename",
            dataset
        );
        assert!(
            filenames.insert(filename),
            "{:?} has duplicate cache filename: {}",
            dataset,
            filename
        );
    }
}

// =============================================================================
// Parser Tests
// =============================================================================

#[test]
fn test_afrisenti_parser() {
    // Verify AfriSenti is loadable (parser exists)
    use anno::eval::LoadableDatasetId;
    let result = LoadableDatasetId::try_from(DatasetId::AfriSenti);
    assert!(result.is_ok(), "AfriSenti should be loadable");

    // Actual parsing is tested in loader.rs internal tests
    // This test verifies the dataset has a parser configured
}

#[test]
fn test_masakhanews_parser() {
    // Verify MasakhaNEWS is loadable (parser exists)
    use anno::eval::LoadableDatasetId;
    let result = LoadableDatasetId::try_from(DatasetId::MasakhaNEWS);
    assert!(result.is_ok(), "MasakhaNEWS should be loadable");

    // Actual parsing is tested in loader.rs internal tests
}

#[test]
fn test_conllu_parser() {
    // Verify MasakhaPOS is loadable (parser exists)
    use anno::eval::LoadableDatasetId;
    let result = LoadableDatasetId::try_from(DatasetId::MasakhaPOS);
    assert!(result.is_ok(), "MasakhaPOS should be loadable");

    // Actual parsing is tested in loader.rs internal tests
}

#[test]
fn test_conll_parser_masakhaner() {
    // Verify MasakhaNER is loadable (parser exists)
    use anno::eval::LoadableDatasetId;
    let result = LoadableDatasetId::try_from(DatasetId::MasakhaNER);
    assert!(result.is_ok(), "MasakhaNER should be loadable");

    // Actual parsing is tested in loader.rs internal tests
}

// =============================================================================
// Integration Tests (Require Network)
// =============================================================================

#[test]
#[ignore] // Requires network access
fn test_download_masakhaner_sample() {
    let loader = DatasetLoader::new().unwrap();

    // Try to load MasakhaNER (should download if not cached)
    use anno::eval::LoadableDatasetId;
    let loadable_id = match LoadableDatasetId::try_from(DatasetId::MasakhaNER) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("Skipping: MasakhaNER is not loadable");
            return;
        }
    };
    let result = loader.load_or_download(loadable_id);

    match result {
        Ok(dataset) => {
            println!("MasakhaNER loaded: {} sentences", dataset.sentences.len());
            assert!(!dataset.sentences.is_empty(), "Should have sentences");

            // Check entity distribution
            let mut entity_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for sentence in &dataset.sentences {
                for token in &sentence.tokens {
                    if token.ner_tag != "O" && !token.ner_tag.starts_with("O-") {
                        let tag = token
                            .ner_tag
                            .trim_start_matches("B-")
                            .trim_start_matches("I-");
                        *entity_counts.entry(tag).or_default() += 1;
                    }
                }
            }
            println!("Entity distribution: {:?}", entity_counts);
        }
        Err(e) => {
            // Network errors are acceptable in CI
            println!("Could not download MasakhaNER (network): {}", e);
        }
    }
}

#[test]
#[ignore] // Requires network access
fn test_download_afrisenti_sample() {
    let loader = DatasetLoader::new().unwrap();

    use anno::eval::LoadableDatasetId;
    let loadable_id = match LoadableDatasetId::try_from(DatasetId::AfriSenti) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("Skipping: AfriSenti is not loadable");
            return;
        }
    };
    let result = loader.load_or_download(loadable_id);

    match result {
        Ok(dataset) => {
            println!("AfriSenti loaded: {} samples", dataset.sentences.len());

            // Check sentiment distribution
            let mut sentiment_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for sentence in &dataset.sentences {
                for token in &sentence.tokens {
                    if token.ner_tag.starts_with("B-") {
                        let label = token.ner_tag.trim_start_matches("B-");
                        *sentiment_counts.entry(label).or_default() += 1;
                    }
                }
            }
            println!("Sentiment distribution: {:?}", sentiment_counts);
        }
        Err(e) => {
            println!("Could not download AfriSenti (network): {}", e);
        }
    }
}

// =============================================================================
// Language Family Coverage Tests
// =============================================================================

#[test]
#[ignore] // Requires african_language_codes() method implementation
fn test_language_family_coverage() {
    // TODO: Implement african_language_codes() method to enable this test
    // This test verifies coverage across major African language families
    // For now, we verify that African datasets exist
    let african_datasets = DatasetId::all_african_languages();
    assert!(
        !african_datasets.is_empty(),
        "Should have African language datasets"
    );

    // When african_language_codes() is implemented, uncomment:
    // let mut all_codes: std::collections::HashSet<&str> = std::collections::HashSet::new();
    // for dataset in african_datasets {
    //     for code in dataset.african_language_codes() {
    //         all_codes.insert(code);
    //     }
    // }
    // println!("Total unique African language codes: {}", all_codes.len());
    //
    // // Niger-Congo (Bantu)
    // assert!(all_codes.contains(&"sw"), "Should have Swahili (Bantu)");
    // assert!(all_codes.contains(&"rw"), "Should have Kinyarwanda (Bantu)");
    //
    // // Niger-Congo (non-Bantu)
    // assert!(all_codes.contains(&"yo"), "Should have Yoruba");
    // assert!(all_codes.contains(&"ig"), "Should have Igbo");
    // assert!(all_codes.contains(&"wo"), "Should have Wolof");
    //
    // // Afro-Asiatic
    // assert!(all_codes.contains(&"am"), "Should have Amharic (Semitic)");
    // assert!(all_codes.contains(&"ha"), "Should have Hausa (Chadic)");
    //
    // // Nilo-Saharan
    // assert!(all_codes.contains(&"luo"), "Should have Dholuo");
    //
    // // Pidgins/Creoles
    // assert!(all_codes.contains(&"pcm"), "Should have Nigerian Pidgin");
    //
    // assert!(all_codes.len() >= 20, "Should have at least 20 unique languages, got {}", all_codes.len());
}

#[test]
#[ignore] // Requires african_language_codes() method implementation
fn test_script_diversity() {
    // TODO: Implement african_language_codes() method to enable this test
    // This test verifies coverage of different writing systems (Latin, Ethiopic, Arabic)
    // For now, we verify that the datasets exist and are marked as African language
    assert!(DatasetId::MasakhaNER.is_african_language());
    assert!(DatasetId::AfriSenti.is_african_language());

    // When african_language_codes() is implemented, uncomment:
    // // Latin script (majority)
    // assert!(DatasetId::MasakhaNER.african_language_codes().contains(&"yo")); // Yoruba
    // assert!(DatasetId::MasakhaNER.african_language_codes().contains(&"sw")); // Swahili
    //
    // // Ethiopic/Ge'ez script
    // assert!(DatasetId::MasakhaNER.african_language_codes().contains(&"am")); // Amharic
    //
    // // Verify AfriSenti includes Arabic-script languages
    // let afrisenti_langs = DatasetId::AfriSenti.african_language_codes();
    // // arq = Algerian Arabic, ary = Moroccan Arabic
    // let has_arabic = afrisenti_langs.contains(&"arq") || afrisenti_langs.contains(&"ary");
    // assert!(has_arabic, "AfriSenti should include Arabic varieties");
}
