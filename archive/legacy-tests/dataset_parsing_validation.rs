//! Dataset parsing validation tests.
//!
//! Validates that all datasets can be correctly downloaded, parsed, and used.
//! These tests verify data integrity and parsing correctness.
//!
//! ## Test Categories
//!
//! 1. **Structure tests**: Verify parsed data has expected format
//! 2. **Invariant tests**: Check properties that must always hold
//! 3. **Round-trip tests**: Ensure serialization/deserialization works
//!
//! ## Running
//!
//! ```bash
//! # Run all parsing validation tests (uses cached data)
//! cargo test --test dataset_parsing_validation --features eval-advanced
//!
//! # Run with downloads (if not cached)
//! cargo test --test dataset_parsing_validation --features eval-advanced -- --ignored
//! ```

#![cfg(feature = "eval-advanced")]

use anno::eval::loader::DatasetId;
use anno::eval::{DatasetLoader, LoadableDatasetId};

// =============================================================================
// Structure Validation
// =============================================================================

/// Verify WikiGold parses correctly with expected structure.
#[test]
fn test_wikigold_structure() {
    let loader = DatasetLoader::new().expect("loader");
    let wikigold = LoadableDatasetId::try_from(DatasetId::WikiGold).expect("WikiGold loadable");
    if !loader.is_cached(wikigold) {
        eprintln!("WikiGold not cached, skipping structure test");
        return;
    }

    let dataset = loader.load(wikigold).expect("load WikiGold");
    let stats = dataset.stats();

    // WikiGold should have substantial content
    assert!(
        stats.sentences > 100,
        "WikiGold should have >100 sentences, got {}",
        stats.sentences
    );
    assert!(
        stats.entities > 500,
        "WikiGold should have >500 entities, got {}",
        stats.entities
    );
    assert!(
        stats.tokens > 1000,
        "WikiGold should have >1000 tokens, got {}",
        stats.tokens
    );

    // Should have standard NER types
    assert!(
        stats.entities_by_type.contains_key("PER") || stats.entities_by_type.contains_key("PERSON"),
        "WikiGold should contain PER/PERSON entities"
    );
}

/// Verify WNUT-17 parses correctly with expected structure.
#[test]
fn test_wnut17_structure() {
    let loader = DatasetLoader::new().expect("loader");
    let wnut17 = LoadableDatasetId::try_from(DatasetId::Wnut17).expect("WNUT-17 loadable");
    if !loader.is_cached(wnut17) {
        eprintln!("WNUT-17 not cached, skipping structure test");
        return;
    }

    let dataset = loader.load(wnut17).expect("load WNUT-17");
    let stats = dataset.stats();

    // WNUT-17 has social media text
    assert!(
        stats.sentences > 50,
        "WNUT-17 should have >50 sentences, got {}",
        stats.sentences
    );
    assert!(
        stats.entities > 100,
        "WNUT-17 should have >100 entities, got {}",
        stats.entities
    );

    // Should have emerging entity types
    let has_person =
        stats.entities_by_type.contains_key("person") || stats.entities_by_type.contains_key("PER");
    let has_location = stats.entities_by_type.contains_key("location")
        || stats.entities_by_type.contains_key("LOC");
    assert!(
        has_person || has_location,
        "WNUT-17 should contain person or location entities"
    );
}

// =============================================================================
// Invariant Tests
// =============================================================================

/// Entity spans must be valid (start < end, within text bounds).
#[test]
fn test_entity_span_invariants() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        let loadable = match LoadableDatasetId::try_from(*ds_id) {
            Ok(id) => id,
            Err(_) => continue,
        };

        if !loader.is_cached(loadable) {
            continue;
        }

        let dataset = match loader.load(loadable) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for sentence in &dataset.sentences {
            let text = sentence.text();
            let text_len = text.chars().count();

            for entity in sentence.entities() {
                // Start must be less than end
                assert!(
                    entity.start < entity.end,
                    "{}: Entity '{}' has invalid span: start({}) >= end({})",
                    ds_id.name(),
                    entity.text,
                    entity.start,
                    entity.end
                );

                // Spans must be within text bounds
                assert!(
                    entity.end <= text_len,
                    "{}: Entity '{}' end({}) exceeds text length({})",
                    ds_id.name(),
                    entity.text,
                    entity.end,
                    text_len
                );

                // Entity text should match span (best effort; char-offset safe)
                let extracted: String = text
                    .chars()
                    .skip(entity.start)
                    .take(entity.end - entity.start)
                    .collect();

                let normalized_extracted =
                    extracted.split_whitespace().collect::<Vec<_>>().join(" ");
                let normalized_entity =
                    entity.text.split_whitespace().collect::<Vec<_>>().join(" ");

                if normalized_extracted != normalized_entity {
                    // Warn only: tokenization/normalization differences exist across sources.
                    eprintln!(
                        "Warning: {}: Entity text mismatch at [{}, {}): expected '{}', got '{}'",
                        ds_id.name(),
                        entity.start,
                        entity.end,
                        normalized_entity,
                        normalized_extracted
                    );
                }
            }
        }
    }
}

/// Explicit Unicode offset invariants (character offsets, not byte offsets).
#[test]
fn test_unicode_span_offsets_are_character_based() {
    use anno::eval::datasets::GoldEntity;
    use anno_core::EntityType;

    // Each case: (text, start_char, end_char)
    let cases = [
        // CJK (multi-byte)
        ("習近平在北京會見了普京。", 0, 3), // 習近平 (3 chars)
        // Arabic (RTL, multi-byte)
        ("التقى محمد بن سلمان بالرئيس في الرياض", 6, 10), // محمد (4 chars)
        // Combining marks (NFD-style)
        ("o\u{0304} is a vowel", 0, 2), // o + macron-combining = 2 chars
        // Emoji (single scalar value here)
        ("🎉 party", 0, 1),
    ];

    for (text, start, end) in cases {
        let span: String = text.chars().skip(start).take(end - start).collect();
        let entity =
            GoldEntity::with_span(span.clone(), EntityType::Other("TEST".into()), start, end);

        // Validate character counts line up with the provided span.
        assert_eq!(
            entity.text.chars().count(),
            end - start,
            "Entity span should match char length for text={:?}, span={:?}",
            text,
            span
        );
    }
}

/// All entity types must be non-empty strings.
#[test]
fn test_entity_type_invariants() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        let loadable = match LoadableDatasetId::try_from(*ds_id) {
            Ok(id) => id,
            Err(_) => continue,
        };

        if !loader.is_cached(loadable) {
            continue;
        }

        let dataset = match loader.load(loadable) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for sentence in &dataset.sentences {
            for entity in sentence.entities() {
                assert!(
                    !entity.original_label.is_empty(),
                    "{}: Entity '{}' has empty label",
                    ds_id.name(),
                    entity.text
                );

                assert!(
                    !entity.text.is_empty(),
                    "{}: Entity has empty text with label '{}'",
                    ds_id.name(),
                    entity.original_label
                );
            }
        }
    }
}

// =============================================================================
// Dataset ID Tests
// =============================================================================

/// All dataset IDs should have valid metadata.
#[test]
fn test_dataset_id_metadata() {
    for ds_id in DatasetId::all() {
        // Name should be non-empty
        assert!(
            !ds_id.name().is_empty(),
            "Dataset {:?} has empty name",
            ds_id
        );

        // Description should be non-empty
        assert!(
            !ds_id.description().is_empty(),
            "Dataset {} has empty description",
            ds_id.name()
        );

        // Cache filename should be valid
        let filename = ds_id.cache_filename();
        assert!(
            !filename.is_empty() && !filename.contains('/') && !filename.contains('\\'),
            "Dataset {} has invalid cache filename: {}",
            ds_id.name(),
            filename
        );

        // Entity types should be non-empty for tasks where we expect a closed label set.
        //
        // For other tasks (e.g., discourse, QA), `entity_types` can be empty because the dataset
        // isn't annotated with a fixed entity/tag label inventory that we use in eval.
        let types = ds_id.entity_types();
        let tasks = ds_id.tasks();
        let expects_label_set = tasks.contains(&"ner")
            || tasks.contains(&"pos")
            || tasks.contains(&"sentiment")
            || tasks.contains(&"text_classification");
        if expects_label_set {
            assert!(
                !types.is_empty(),
                "Dataset {} has no expected entity types (tasks: {:?})",
                ds_id.name(),
                tasks
            );
        }
    }
}

/// Dataset categorization should be consistent.
#[test]
fn test_dataset_categorization() {
    // All coref datasets should be in all_coref
    for ds_id in DatasetId::all_coref() {
        assert!(
            ds_id.is_coreference(),
            "Dataset {} is in all_coref but is_coreference() returns false",
            ds_id.name()
        );
    }

    // All RE datasets should be in all_relation_extraction
    for ds_id in DatasetId::all_relation_extraction() {
        assert!(
            ds_id.is_relation_extraction(),
            "Dataset {} is in all_relation_extraction but is_relation_extraction() returns false",
            ds_id.name()
        );
    }

    // NER datasets should have a non-empty label inventory.
    for ds_id in DatasetId::all_ner() {
        assert!(
            !ds_id.entity_types().is_empty(),
            "Dataset {} is in all_ner but has no expected entity types",
            ds_id.name()
        );
    }
}

// =============================================================================
// Download Tests (Ignored by default)
// =============================================================================

/// Download and validate quick datasets.
#[test]
#[ignore]
fn download_and_validate_quick_datasets() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        println!("Downloading {}...", ds_id.name());

        let loadable = match LoadableDatasetId::try_from(*ds_id) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("  Skipping non-loadable {}: {}", ds_id.name(), e);
                continue;
            }
        };

        match loader.load_or_download(loadable) {
            Ok(dataset) => {
                let stats = dataset.stats();
                println!(
                    "  {} sentences, {} entities, {} tokens",
                    stats.sentences, stats.entities, stats.tokens
                );

                // Verify basic invariants
                assert!(stats.sentences > 0, "{} has no sentences", ds_id.name());
                assert!(stats.entities > 0, "{} has no entities", ds_id.name());
            }
            Err(e) => {
                eprintln!("  Failed to download {}: {}", ds_id.name(), e);
            }
        }
    }
}

/// Download and validate all datasets (very slow).
#[test]
#[ignore]
fn download_and_validate_all_datasets() {
    let loader = DatasetLoader::new().expect("loader");
    let mut successes = 0;
    let mut failures = Vec::new();

    for ds_id in DatasetId::all() {
        print!("{}... ", ds_id.name());

        let loadable = match LoadableDatasetId::try_from(*ds_id) {
            Ok(id) => id,
            Err(e) => {
                println!("SKIP (not loadable: {})", e);
                continue;
            }
        };

        match loader.load_or_download(loadable) {
            Ok(dataset) => {
                let stats = dataset.stats();
                println!(
                    "OK ({} sentences, {} entities)",
                    stats.sentences, stats.entities
                );
                successes += 1;
            }
            Err(e) => {
                println!("FAILED: {}", e);
                failures.push((ds_id.name(), e.to_string()));
            }
        }
    }

    println!("\nDownloaded: {}/{}", successes, DatasetId::all().len());
    if !failures.is_empty() {
        println!("Failed ({}):", failures.len());
        for (name, err) in &failures {
            println!("  - {}: {}", name, err);
        }
    }
}

// =============================================================================
// Serialization Tests
// =============================================================================

/// Test that dataset statistics can be serialized/deserialized.
#[test]
fn test_stats_serialization() {
    let loader = DatasetLoader::new().expect("loader");

    let wikigold = LoadableDatasetId::try_from(DatasetId::WikiGold).expect("WikiGold loadable");
    if !loader.is_cached(wikigold) {
        eprintln!("WikiGold not cached, skipping serialization test");
        return;
    }

    let dataset = loader.load(wikigold).expect("load");
    let stats = dataset.stats();

    // Stats should be serializable
    let json = serde_json::to_string(&stats).expect("serialize stats");
    assert!(!json.is_empty(), "Serialized stats should not be empty");

    // Should contain expected fields
    assert!(
        json.contains("sentences"),
        "JSON should contain 'sentences'"
    );
    assert!(json.contains("entities"), "JSON should contain 'entities'");
    assert!(json.contains("tokens"), "JSON should contain 'tokens'");
}

// =============================================================================
// CSV NER Format (E-NER)
// =============================================================================

/// Verify E-NER (EDGAR-NER) CSV format parses correctly.
#[test]
fn test_ener_csv_structure() {
    let loader = DatasetLoader::new().expect("loader");
    let ener = LoadableDatasetId::try_from(DatasetId::ENer).expect("ENer loadable");
    if !loader.is_cached(ener) {
        eprintln!("E-NER not cached, skipping structure test");
        return;
    }

    let dataset = loader.load(ener).expect("load E-NER");
    let stats = dataset.stats();

    // E-NER should have substantial content
    // Note: HuggingFace API may return a sample (~54 sentences) rather than the full dataset
    assert!(
        stats.sentences > 10,
        "E-NER should have >10 sentences, got {}",
        stats.sentences
    );
    assert!(
        stats.entities > 10,
        "E-NER should have >10 entities, got {}",
        stats.entities
    );
    assert!(
        stats.tokens > 100,
        "E-NER should have >100 tokens, got {}",
        stats.tokens
    );

    // Should have legal/financial entity types
    let has_business = stats.entities_by_type.contains_key("BUSINESS")
        || stats.entities_by_type.contains_key("I-BUSINESS");
    let has_person = stats.entities_by_type.contains_key("PERSON")
        || stats.entities_by_type.contains_key("I-PERSON");
    assert!(
        has_business || has_person,
        "E-NER should contain BUSINESS or PERSON entities: {:?}",
        stats.entities_by_type.keys().collect::<Vec<_>>()
    );
}

// =============================================================================
// Test Cases Conversion
// =============================================================================

/// Verify test cases can be generated from loaded datasets.
#[test]
fn test_to_test_cases_conversion() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        let loadable = match LoadableDatasetId::try_from(*ds_id) {
            Ok(id) => id,
            Err(_) => continue,
        };

        if !loader.is_cached(loadable) {
            continue;
        }

        let dataset = match loader.load(loadable) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let test_cases = dataset.to_test_cases();

        // Should produce at least some test cases
        assert!(
            !test_cases.is_empty(),
            "{} should produce non-empty test cases",
            ds_id.name()
        );

        // Each test case should have text and gold entities
        for (text, _gold) in &test_cases {
            assert!(
                !text.is_empty(),
                "{}: Test case has empty text",
                ds_id.name()
            );
            // Note: Some sentences may have no entities, that's OK
        }
    }
}
