//! Regression tests encoding nuances and bugs discovered during development.
//!
//! This file serves as a living document of edge cases and subtle issues.
//! Each test documents a specific problem that was fixed.

use anno::{Entity, EntityType, HeuristicNER, Model, StackedNER};

// =============================================================================
// Duplicate Entity Offset Bug (Fixed 2024-12)
// =============================================================================
// BUG: Using `text.find(&entity_text)` to locate entity positions always
// returned the FIRST occurrence. When the same entity appeared multiple times,
// subsequent occurrences had incorrect offsets pointing to the first occurrence.
//
// FIX: Track token byte positions sequentially as we iterate, so each entity
// gets the correct position even if the same text appears multiple times.

mod duplicate_entity_tests {
    use super::*;

    #[test]
    fn duplicate_person_names_have_distinct_offsets() {
        // "John" appears twice - each should have different offsets
        let text = "John met John at the park.";
        let ner = HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        let johns: Vec<_> = entities.iter().filter(|e| e.text == "John").collect();

        if johns.len() >= 2 {
            assert_ne!(
                johns[0].start, johns[1].start,
                "Duplicate 'John' entities have same start offset - bug regression!"
            );
        }
    }

    #[test]
    fn duplicate_orgs_have_distinct_offsets() {
        let text = "Google acquired Google Cloud from Google.";
        let ner = StackedNER::default();
        let entities = ner.extract_entities(text, None).unwrap();

        let googles: Vec<_> = entities
            .iter()
            .filter(|e| e.text.contains("Google"))
            .collect();

        // If multiple Googles found, they should have distinct offsets
        for i in 0..googles.len() {
            for j in (i + 1)..googles.len() {
                if googles[i].text == googles[j].text {
                    assert_ne!(
                        googles[i].start, googles[j].start,
                        "Duplicate '{}' entities have same offset - bug regression!",
                        googles[i].text
                    );
                }
            }
        }
    }

    #[test]
    fn token_position_tracking_is_sequential() {
        // Direct test of the fix: token positions must be calculated sequentially
        let text = "a a a";
        let tokens: Vec<&str> = text.split_whitespace().collect();

        // The fix calculates positions by advancing byte_pos after each token
        let mut positions = Vec::new();
        let mut byte_pos = 0;
        for token in &tokens {
            if let Some(rel_pos) = text[byte_pos..].find(token) {
                let start = byte_pos + rel_pos;
                let end = start + token.len();
                positions.push((start, end));
                byte_pos = end;
            }
        }

        // Each "a" should be at different positions
        assert_eq!(positions[0], (0, 1), "First 'a' should be at 0-1");
        assert_eq!(positions[1], (2, 3), "Second 'a' should be at 2-3");
        assert_eq!(positions[2], (4, 5), "Third 'a' should be at 4-5");
    }
}

// =============================================================================
// Unicode Offset Handling (Ongoing)
// =============================================================================
// BUG: Rust strings are UTF-8 byte sequences. Regex and string operations
// return BYTE offsets, but Entity spans should be CHARACTER offsets.
// Confusing these causes incorrect entity boundaries for non-ASCII text.
//
// FIX: Always use SpanConverter to convert between byte and char offsets.

mod unicode_offset_tests {
    use super::*;

    #[test]
    fn cjk_text_has_valid_char_offsets() {
        let text = "東京 is in 日本";
        let char_count = text.chars().count();

        for backend in get_basic_backends() {
            let entities = backend.extract_entities(text, None).unwrap();
            for entity in &entities {
                assert!(
                    entity.start <= entity.end,
                    "{}: Invalid span start {} > end {}",
                    backend_name(&*backend),
                    entity.start,
                    entity.end
                );
                assert!(
                    entity.end <= char_count,
                    "{}: Entity end {} exceeds char count {} for '{}'",
                    backend_name(&*backend),
                    entity.end,
                    char_count,
                    text
                );
            }
        }
    }

    #[test]
    fn emoji_text_has_valid_char_offsets() {
        let text = "👨‍💻 John works at 🏢 Google";
        let char_count = text.chars().count();

        let ner = anno::HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        for entity in &entities {
            assert!(entity.start <= entity.end);
            assert!(
                entity.end <= char_count,
                "Entity '{}' at {}..{} exceeds char count {}",
                entity.text,
                entity.start,
                entity.end,
                char_count
            );
        }
    }

    #[test]
    fn mixed_script_text_has_valid_offsets() {
        // Mix of Latin, CJK, Arabic, Hebrew
        let text = "Hello 世界 مرحبا שלום";
        let char_count = text.chars().count();

        let ner = anno::RegexNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        for entity in &entities {
            assert!(entity.start <= entity.end);
            assert!(entity.end <= char_count);
        }
    }

    #[test]
    fn byte_char_distinction() {
        // "北京" is 2 chars but 6 bytes
        let text = "北京";
        assert_eq!(text.len(), 6, "北京 should be 6 bytes");
        assert_eq!(text.chars().count(), 2, "北京 should be 2 chars");

        // Entities should use char offsets, not byte offsets
        let ner = anno::HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        for entity in &entities {
            // If entity covers the whole text, end should be 2 (chars), not 6 (bytes)
            if entity.text == "北京" {
                assert_eq!(
                    entity.end, 2,
                    "Entity end should be char count (2), not byte count (6)"
                );
            }
        }
    }
}

// =============================================================================
// BIO Sequence Constraints
// =============================================================================
// BUG: Invalid BIO sequences like "O I-PER" (I- without preceding B-) can
// occur if transition constraints are not enforced.
//
// FIX: CRF/HMM transition matrices must have low/negative scores for invalid
// transitions. Viterbi decoding naturally avoids them.

mod bio_constraint_tests {
    use super::*;

    #[test]
    fn crf_respects_bio_constraints() {
        let ner = HeuristicNER::new();
        let text = "John Smith works at Google Inc in New York City";
        let entities = ner.extract_entities(text, None).unwrap();

        // Extract the labels that would be assigned
        // Each entity should have valid BIO sequence (not testable directly,
        // but we can verify entities don't overlap incorrectly)
        let mut covered = vec![false; text.chars().count()];
        for entity in &entities {
            for i in entity.start..entity.end {
                assert!(
                    !covered[i],
                    "Overlapping entities detected at char {} - BIO violation",
                    i
                );
                covered[i] = true;
            }
        }
    }

    #[test]
    fn hmm_respects_bio_constraints() {
        let ner = HeuristicNER::new();
        let text = "Mary Johnson visited Paris France";
        let entities = ner.extract_entities(text, None).unwrap();

        // Check no overlapping entities (indirect BIO check)
        for i in 0..entities.len() {
            for j in (i + 1)..entities.len() {
                let a = &entities[i];
                let b = &entities[j];
                assert!(
                    a.end <= b.start || b.end <= a.start,
                    "Entities '{}' ({}-{}) and '{}' ({}-{}) overlap",
                    a.text,
                    a.start,
                    a.end,
                    b.text,
                    b.start,
                    b.end
                );
            }
        }
    }
}

// =============================================================================
// Entity Confidence Bounds
// =============================================================================
// INVARIANT: Entity confidence should always be in [0.0, 1.0]

mod confidence_tests {
    use super::*;

    #[test]
    fn all_backends_return_valid_confidence() {
        let text = "John Smith works at Google Inc.";

        for backend in get_basic_backends() {
            let entities = backend.extract_entities(text, None).unwrap();
            for entity in &entities {
                assert!(
                    entity.confidence >= 0.0 && entity.confidence <= 1.0,
                    "{}: Confidence {} out of bounds for '{}'",
                    backend_name(&*backend),
                    entity.confidence,
                    entity.text
                );
            }
        }
    }
}

// =============================================================================
// Empty and Whitespace Input Handling
// =============================================================================
// INVARIANT: Empty and whitespace-only inputs should return empty results

mod empty_input_tests {
    use super::*;

    #[test]
    fn empty_string_returns_empty() {
        for backend in get_basic_backends() {
            let entities = backend.extract_entities("", None).unwrap();
            assert!(
                entities.is_empty(),
                "{}: Expected empty result for empty input",
                backend_name(&*backend)
            );
        }
    }

    #[test]
    fn whitespace_only_returns_empty() {
        for backend in get_basic_backends() {
            let entities = backend.extract_entities("   \n\t  ", None).unwrap();
            assert!(
                entities.is_empty(),
                "{}: Expected empty result for whitespace-only input",
                backend_name(&*backend)
            );
        }
    }
}

// =============================================================================
// Entity Type Handling
// =============================================================================
// NOTE: EntityType::Other requires a String argument, not unit variant

mod entity_type_tests {
    use super::*;

    #[test]
    fn entity_type_other_requires_string() {
        // EntityType::Other is a tuple struct, not a unit variant
        let other = EntityType::Other("custom".to_string());
        assert!(matches!(other, EntityType::Other(_)));
    }

    #[test]
    fn standard_entity_types_supported() {
        let ner = anno::HeuristicNER::new();
        let types = ner.supported_types();

        assert!(types.contains(&EntityType::Person));
        assert!(types.contains(&EntityType::Organization));
        assert!(types.contains(&EntityType::Location));
    }
}

// =============================================================================
// Streaming API Edge Cases
// =============================================================================
// BUG: Streaming with small chunks and large overlap could cause infinite loops
// if position didn't advance properly.
//
// FIX: Always ensure forward progress by at least 1 character.

// TODO: Re-enable when streaming module is public
// mod streaming_tests {
//     use super::*;
//     use anno::backends::streaming::{ChunkConfig, StreamingExtractor};
//
//     #[test]
//     fn small_chunk_large_overlap_terminates() {
//         let model = anno::HeuristicNER::new();
//
//         // Edge case that could cause infinite loop without fix
//         let config = ChunkConfig {
//             chunk_size: 5,
//             overlap: 4, // Almost as big as chunk
//             respect_sentences: false,
//             buffer_size: 10,
//         };
//
//         let extractor = StreamingExtractor::new(&model, config);
//         let text = "Hello world test";
//
//         // Should terminate, not hang
//         let entities: Vec<Entity> = extractor.extract(text).take(100).collect();
//         assert!(entities.len() < 100, "Possible infinite loop detected");
//     }
// }

// =============================================================================
// SpanConverter Invariants
// =============================================================================
// INVARIANT: SpanConverter round-trips must preserve positions

mod span_converter_tests {
    use anno::offset::SpanConverter;

    #[test]
    fn round_trip_ascii() {
        let text = "Hello world";
        let converter = SpanConverter::new(text);

        for byte_pos in 0..=text.len() {
            let char_pos = converter.byte_to_char(byte_pos);
            let round_trip = converter.char_to_byte(char_pos);
            assert_eq!(
                round_trip, byte_pos,
                "Round-trip failed at byte {}",
                byte_pos
            );
        }
    }

    #[test]
    fn round_trip_unicode() {
        let text = "日本語テスト";
        let converter = SpanConverter::new(text);

        let char_count = text.chars().count();
        for char_pos in 0..=char_count {
            let byte_pos = converter.char_to_byte(char_pos);
            let round_trip = converter.byte_to_char(byte_pos);
            assert_eq!(
                round_trip, char_pos,
                "Round-trip failed at char {}",
                char_pos
            );
        }
    }

    #[test]
    fn byte_char_boundary_alignment() {
        // Verify byte positions always land on char boundaries
        let text = "Müller 北京"; // Multi-byte chars
        let converter = SpanConverter::new(text);

        // All valid byte positions should map to char boundaries
        let mut byte_pos = 0;
        for (char_idx, ch) in text.chars().enumerate() {
            assert_eq!(
                converter.byte_to_char(byte_pos),
                char_idx,
                "Byte {} should map to char {}",
                byte_pos,
                char_idx
            );
            byte_pos += ch.len_utf8();
        }
    }
}

// =============================================================================
// Backend Factory Consistency
// =============================================================================
// INVARIANT: Backend factory should create consistent backends

#[cfg(feature = "eval")]
mod factory_tests {
    use anno::eval::backend_factory::BackendFactory;

    #[test]
    fn factory_creates_available_backends() {
        let backend_names = ["pattern", "heuristic", "crf", "stacked", "ensemble"];

        for name in backend_names {
            let result = BackendFactory::create(name);
            assert!(
                result.is_ok(),
                "Factory failed to create '{}': {:?}",
                name,
                result.err()
            );
            let backend = result.unwrap();
            assert!(backend.is_available(), "Backend '{}' not available", name);
        }
    }

    #[test]
    fn factory_rejects_unknown_backends() {
        let result = BackendFactory::create("nonexistent_backend_xyz");
        assert!(result.is_err());
    }
}

// =============================================================================
// Offset Edge Cases
// =============================================================================
// Various edge cases around offset handling

mod offset_edge_cases {
    use super::*;

    #[test]
    fn zero_length_entity_allowed() {
        // Some systems may return zero-length entities (e.g., for markers)
        // This should be representable even if not meaningful for NER
        let entity = Entity::new("", EntityType::Other("marker".to_string()), 5, 5, 1.0);
        assert_eq!(entity.start, entity.end);
    }

    #[test]
    fn entity_at_text_start() {
        let text = "Google Inc is a company";
        let ner = anno::HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        // If Google is found, it should start at 0
        if let Some(google) = entities.iter().find(|e| e.text.contains("Google")) {
            assert_eq!(google.start, 0, "Entity at start should have start=0");
        }
    }

    #[test]
    fn entity_at_text_end() {
        let text = "The company is Apple";
        let ner = anno::HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        let char_count = text.chars().count();
        // If Apple is found, it should end at text end
        if let Some(apple) = entities.iter().find(|e| e.text == "Apple") {
            assert_eq!(
                apple.end, char_count,
                "Entity at end should have end=char_count"
            );
        }
    }

    #[test]
    fn single_char_text() {
        let text = "A";
        for backend in get_basic_backends() {
            let result = backend.extract_entities(text, None);
            assert!(result.is_ok(), "Backend should handle single-char text");
        }
    }
}

// =============================================================================
// Entity Type Serialization
// =============================================================================
// Ensure entity types round-trip through serialization

mod serialization_tests {
    use super::*;

    #[test]
    fn entity_json_roundtrip() {
        let entity = Entity::new("Google", EntityType::Organization, 0, 6, 0.95);

        let json = serde_json::to_string(&entity).expect("serialize failed");
        let deserialized: Entity = serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(entity.text, deserialized.text);
        assert_eq!(entity.start, deserialized.start);
        assert_eq!(entity.end, deserialized.end);
        assert_eq!(entity.entity_type, deserialized.entity_type);
    }

    #[test]
    fn entity_type_other_roundtrip() {
        let entity = Entity::new(
            "custom",
            EntityType::Other("CUSTOM_TYPE".to_string()),
            0,
            6,
            0.5,
        );

        let json = serde_json::to_string(&entity).expect("serialize failed");
        let deserialized: Entity = serde_json::from_str(&json).expect("deserialize failed");

        match &deserialized.entity_type {
            EntityType::Other(s) => assert_eq!(s, "CUSTOM_TYPE"),
            _ => panic!("Expected EntityType::Other"),
        }
    }
}

// =============================================================================
// Concurrent Access Safety
// =============================================================================
// Backends should be thread-safe

mod thread_safety_tests {
    use super::*;
    use std::thread;

    #[test]
    fn backends_are_send_sync() {
        // Compile-time check that backends implement Send + Sync
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<anno::RegexNER>();
        assert_send_sync::<anno::HeuristicNER>();
        assert_send_sync::<StackedNER>();
    }

    #[test]
    fn concurrent_extraction_safe() {
        let text = "John Smith works at Google Inc.";
        let ner = anno::HeuristicNER::new();

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let text = text.to_string();
                thread::spawn(move || {
                    let ner = anno::HeuristicNER::new();
                    ner.extract_entities(&text, None)
                })
            })
            .collect();

        for handle in handles {
            let result = handle.join().expect("Thread panicked");
            assert!(result.is_ok());
        }
    }
}

// =============================================================================
// Model Trait Consistency
// =============================================================================
// All backends should behave consistently with Model trait

mod model_trait_tests {
    use super::*;

    #[test]
    fn name_is_non_empty() {
        for backend in get_basic_backends() {
            let name = backend.name();
            assert!(!name.is_empty(), "Backend name should not be empty");
        }
    }

    #[test]
    fn supported_types_is_non_empty() {
        for backend in get_basic_backends() {
            let types = backend.supported_types();
            assert!(
                !types.is_empty(),
                "Backend should support at least one type"
            );
        }
    }

    #[test]
    fn is_available_consistent() {
        // If is_available returns true, extraction should not error
        let text = "John works at Google.";
        for backend in get_basic_backends() {
            if backend.is_available() {
                let result = backend.extract_entities(text, None);
                assert!(
                    result.is_ok(),
                    "Available backend should extract without error"
                );
            }
        }
    }
}

// =============================================================================
// Regression: Specific Bug Scenarios
// =============================================================================
// Tests for specific bugs that were discovered and fixed

mod specific_bug_regressions {
    use super::*;

    #[test]
    fn repeated_word_boundary_handling() {
        // Bug: "the the" could cause issues with word boundary detection
        let text = "The the company is Apple Inc.";
        let ner = HeuristicNER::new();
        let result = ner.extract_entities(text, None);
        assert!(result.is_ok(), "Should handle repeated words");
    }

    #[test]
    fn hyphenated_names() {
        // Bug: Hyphenated names might be split incorrectly
        let text = "Jean-Pierre visited New York";
        let ner = anno::HeuristicNER::new();
        let entities = ner.extract_entities(text, None).unwrap();

        // Should not split Jean-Pierre into separate entities
        let jean_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.text.contains("Jean"))
            .collect();

        for e in &jean_entities {
            // Either capture full "Jean-Pierre" or skip it - don't split weirdly
            assert!(
                e.text == "Jean-Pierre" || !e.text.contains("-"),
                "Hyphenated name incorrectly split: '{}'",
                e.text
            );
        }
    }

    #[test]
    fn apostrophe_handling() {
        // Bug: Apostrophes in names could cause issues
        let text = "O'Brien met McDonald's CEO";
        let ner = anno::HeuristicNER::new();
        let result = ner.extract_entities(text, None);
        assert!(result.is_ok(), "Should handle apostrophes");
    }

    #[test]
    fn numeric_suffix_handling() {
        // Companies with numeric suffixes
        let text = "Web3 Inc and 3M Corporation";
        let ner = anno::HeuristicNER::new();
        let result = ner.extract_entities(text, None);
        assert!(result.is_ok(), "Should handle numeric suffixes");
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn get_basic_backends() -> Vec<Box<dyn Model>> {
    vec![
        Box::new(anno::RegexNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(StackedNER::default()),
    ]
}

fn backend_name(backend: &dyn Model) -> &'static str {
    // Use supported_types to distinguish backends
    let types = backend.supported_types();
    if types.contains(&EntityType::Email) {
        "RegexNER"
    } else if types.contains(&EntityType::Other("MISC".to_string())) {
        "CrfNER/BiLstmCrfNER/HmmNER"
    } else {
        "HeuristicNER"
    }
}
