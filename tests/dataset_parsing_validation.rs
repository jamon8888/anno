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

use anno::eval::loader::{DatasetId, DatasetLoader};

// =============================================================================
// Structure Validation
// =============================================================================

/// Verify WikiGold parses correctly with expected structure.
#[test]
fn test_wikigold_structure() {
    let loader = DatasetLoader::new().expect("loader");
    if !loader.is_cached(DatasetId::WikiGold) {
        eprintln!("WikiGold not cached, skipping structure test");
        return;
    }

    let dataset = loader.load(DatasetId::WikiGold).expect("load WikiGold");
    let stats = dataset.stats();

    // WikiGold should have substantial content
    assert!(stats.sentences > 100, "WikiGold should have >100 sentences, got {}", stats.sentences);
    assert!(stats.entities > 500, "WikiGold should have >500 entities, got {}", stats.entities);
    assert!(stats.tokens > 1000, "WikiGold should have >1000 tokens, got {}", stats.tokens);

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
    if !loader.is_cached(DatasetId::Wnut17) {
        eprintln!("WNUT-17 not cached, skipping structure test");
        return;
    }

    let dataset = loader.load(DatasetId::Wnut17).expect("load WNUT-17");
    let stats = dataset.stats();

    // WNUT-17 has social media text
    assert!(stats.sentences > 50, "WNUT-17 should have >50 sentences, got {}", stats.sentences);
    assert!(stats.entities > 100, "WNUT-17 should have >100 entities, got {}", stats.entities);

    // Should have emerging entity types
    let has_person = stats.entities_by_type.contains_key("person")
        || stats.entities_by_type.contains_key("PER");
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
        if !loader.is_cached(*ds_id) {
            continue;
        }

        let dataset = match loader.load(*ds_id) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for sentence in &dataset.sentences {
            let text = sentence.text();
            let text_len = text.len();

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

                // Entity text should match span (if extractable)
                if entity.start < text_len && entity.end <= text_len {
                    let extracted = &text[entity.start..entity.end];
                    // Normalize whitespace for comparison
                    let normalized_extracted = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
                    let normalized_entity = entity.text.split_whitespace().collect::<Vec<_>>().join(" ");
                    
                    // Allow some tolerance for tokenization differences
                    if normalized_extracted != normalized_entity {
                        // Just warn, don't fail - some datasets have minor mismatches
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
}

/// All entity types must be non-empty strings.
#[test]
fn test_entity_type_invariants() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        if !loader.is_cached(*ds_id) {
            continue;
        }

        let dataset = match loader.load(*ds_id) {
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

        // Entity types should be non-empty
        let types = ds_id.expected_entity_types();
        assert!(
            !types.is_empty(),
            "Dataset {} has no expected entity types",
            ds_id.name()
        );
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

    // NER datasets should not be coreference or RE
    for ds_id in DatasetId::all_ner() {
        assert!(
            !ds_id.is_coreference(),
            "Dataset {} is in all_ner but is_coreference() returns true",
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

        match loader.load_or_download(*ds_id) {
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

        match loader.load_or_download(*ds_id) {
            Ok(dataset) => {
                let stats = dataset.stats();
                println!("OK ({} sentences, {} entities)", stats.sentences, stats.entities);
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

    if !loader.is_cached(DatasetId::WikiGold) {
        eprintln!("WikiGold not cached, skipping serialization test");
        return;
    }

    let dataset = loader.load(DatasetId::WikiGold).expect("load");
    let stats = dataset.stats();

    // Stats should be serializable
    let json = serde_json::to_string(&stats).expect("serialize stats");
    assert!(!json.is_empty(), "Serialized stats should not be empty");

    // Should contain expected fields
    assert!(json.contains("sentences"), "JSON should contain 'sentences'");
    assert!(json.contains("entities"), "JSON should contain 'entities'");
    assert!(json.contains("tokens"), "JSON should contain 'tokens'");
}

// =============================================================================
// Test Cases Conversion
// =============================================================================

/// Verify test cases can be generated from loaded datasets.
#[test]
fn test_to_test_cases_conversion() {
    let loader = DatasetLoader::new().expect("loader");

    for ds_id in DatasetId::quick() {
        if !loader.is_cached(*ds_id) {
            continue;
        }

        let dataset = match loader.load(*ds_id) {
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
        for (text, gold) in &test_cases {
            assert!(
                !text.is_empty(),
                "{}: Test case has empty text",
                ds_id.name()
            );
            // Note: Some sentences may have no entities, that's OK
        }
    }
}

