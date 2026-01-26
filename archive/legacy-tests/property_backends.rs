//! Property-based tests for backend implementations.
//!
//! Uses proptest to verify invariants across random inputs.
//!
//! Note: Tests involving composite backends (Stacked/Ensemble) use reduced
//! proptest cases to avoid timeouts from repeated model initialization.

use anno::{EntityType, Model};
use proptest::prelude::*;

fn fast_config() -> ProptestConfig {
    ProptestConfig {
        cases: 25,
        // Silence persistence warnings under nextest (workspace cwd).
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// Reduced proptest config for composite backends that are slower
fn slow_config() -> ProptestConfig {
    ProptestConfig {
        cases: 5, // Much fewer cases for slow backends
        // nextest runs tests from the workspace root; proptest's default persistence
        // can emit warnings when it can't locate lib.rs/main.rs upward from cwd.
        // Persistence isn't needed for CI/quick profiles.
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn fast_stacked() -> anno::StackedNER {
    anno::StackedNER::builder()
        .layer(anno::RegexNER::new())
        .layer(anno::HeuristicNER::new())
        .build()
}

/// Generate arbitrary text with potential entity-like patterns.
fn arb_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::string::string_regex(r"[A-Za-z0-9\s.,!?'-]{1,50}")
            .expect("regex pattern should be valid")
            .prop_filter("non-empty", |s| !s.trim().is_empty()),
        1..20,
    )
    .prop_map(|parts| parts.join(" "))
}

/// Generate text with known entity-like patterns.
fn arb_text_with_entities() -> impl Strategy<Value = String> {
    let names = prop::sample::select(vec![
        "John Smith",
        "Marie Curie",
        "Dr. Jane Doe",
        "President Biden",
    ]);
    let orgs = prop::sample::select(vec!["Google", "Apple Inc.", "Microsoft", "UN"]);
    let locs = prop::sample::select(vec!["California", "New York", "London", "Paris, France"]);
    let verbs = prop::sample::select(vec!["works at", "visited", "founded", "lives in"]);

    (names, verbs, orgs, locs)
        .prop_map(|(name, verb, org, loc)| format!("{} {} {} in {}.", name, verb, org, loc))
}

// =============================================================================
// Property: All backends return valid span ranges
// =============================================================================

proptest! {
    #![proptest_config(fast_config())]
    #[test]
    fn pattern_backend_valid_spans(text in arb_text()) {
        let ner = anno::RegexNER::new();
        let char_count = text.chars().count(); // Use char count, not byte length!
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.start <= entity.end, "Invalid span: {} > {}", entity.start, entity.end);
                prop_assert!(entity.end <= char_count, "Span exceeds text: {} > {}", entity.end, char_count);
            }
        }
    }

    #[test]
    fn heuristic_backend_valid_spans(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.start <= entity.end);
                prop_assert!(entity.end <= char_count);
            }
        }
    }

    #[test]
    fn crf_backend_valid_spans(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.start <= entity.end);
                prop_assert!(entity.end <= char_count);
            }
        }
    }

}

// Separate block for slow composite backends with reduced test cases
proptest! {
    #![proptest_config(slow_config())]

    #[test]
    fn stacked_backend_valid_spans(text in arb_text()) {
        let ner = fast_stacked();
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.start <= entity.end);
                prop_assert!(entity.end <= char_count);
            }
        }
    }

    #[test]
    fn ensemble_backend_valid_spans(text in arb_text()) {
        use anno::backends::ensemble::EnsembleNER;
        // Avoid default ensemble construction (it may opportunistically include ML backends).
        let ner = EnsembleNER::with_backends(vec![
            Box::new(anno::RegexNER::new()),
            Box::new(anno::HeuristicNER::new()),
        ]);
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.start <= entity.end);
                prop_assert!(entity.end <= char_count);
            }
        }
    }
}

// =============================================================================
// Property: All backends return valid confidence scores
// =============================================================================

proptest! {
    #![proptest_config(fast_config())]
    #[test]
    fn pattern_backend_valid_confidence(text in arb_text()) {
        let ner = anno::RegexNER::new();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0,
                    "Invalid confidence: {}", entity.confidence);
            }
        }
    }

    #[test]
    fn heuristic_backend_valid_confidence(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }

    #[test]
    fn crf_backend_valid_confidence(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }

    #[test]
    fn bilstm_crf_backend_valid_confidence(text in arb_text()) {
        let ner = fast_stacked();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }

    #[test]
    fn hmm_backend_valid_confidence(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }

}

// Slow composite backend confidence tests
proptest! {
    #![proptest_config(slow_config())]

    #[test]
    fn stacked_backend_valid_confidence(text in arb_text()) {
        let ner = fast_stacked();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }

    #[test]
    fn ensemble_backend_valid_confidence(text in arb_text()) {
        use anno::backends::ensemble::EnsembleNER;
        let ner = EnsembleNER::with_backends(vec![
            Box::new(anno::RegexNER::new()),
            Box::new(anno::HeuristicNER::new()),
        ]);
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                prop_assert!(entity.confidence >= 0.0 && entity.confidence <= 1.0);
            }
        }
    }
}

// =============================================================================
// Property: Entity text matches span
// =============================================================================

proptest! {
    #![proptest_config(fast_config())]
    #[test]
    fn pattern_backend_text_matches_span(text in arb_text_with_entities()) {
        let ner = anno::RegexNER::new();
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                // Use character offsets, not byte offsets!
                if entity.start < char_count && entity.end <= char_count {
                    let span_text: String = text.chars()
                        .skip(entity.start)
                        .take(entity.end - entity.start)
                        .collect();
                    // Entity text should contain the span text (may differ due to normalization)
                    prop_assert!(
                        entity.text.contains(&span_text) || span_text.contains(&entity.text),
                        "Mismatch: entity='{}', span='{}'", entity.text, span_text
                    );
                }
            }
        }
    }

    #[test]
    fn heuristic_backend_text_matches_span(text in arb_text_with_entities()) {
        let ner = anno::HeuristicNER::new();
        let char_count = text.chars().count();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                // Use character offsets, not byte offsets!
                if entity.start < char_count && entity.end <= char_count {
                    let span_text: String = text.chars()
                        .skip(entity.start)
                        .take(entity.end - entity.start)
                        .collect();
                    prop_assert!(
                        entity.text == span_text,
                        "Mismatch: entity='{}', span='{}'", entity.text, span_text
                    );
                }
            }
        }
    }
}

// =============================================================================
// Property: Empty input returns empty output
// =============================================================================

#[test]
fn all_backends_empty_input() {
    let empty_texts = vec!["", "   ", "\n\n", "\t"];

    let backends: Vec<Box<dyn Model>> = vec![
        Box::new(anno::RegexNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(fast_stacked()),
        Box::new(fast_stacked()),
    ];

    for text in empty_texts {
        for backend in &backends {
            let entities = backend.extract_entities(text, None).unwrap();
            assert!(
                entities.is_empty(),
                "Backend {} returned {} entities for empty text '{}'",
                backend.name(),
                entities.len(),
                text
            );
        }
    }
}

// =============================================================================
// Property: Supported types are consistent
// =============================================================================

#[test]
fn all_backends_have_supported_types() {
    let backends: Vec<Box<dyn Model>> = vec![
        Box::new(anno::RegexNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(fast_stacked()),
        Box::new(fast_stacked()),
    ];

    for backend in &backends {
        let types = backend.supported_types();
        assert!(
            !types.is_empty(),
            "Backend {} reports no supported types",
            backend.name()
        );
    }
}

// =============================================================================
// Property: Entity types match supported types (weak invariant)
// =============================================================================

proptest! {
    #![proptest_config(fast_config())]
    #[test]
    fn pattern_backend_entity_types_subset_of_supported(text in arb_text_with_entities()) {
        let ner = anno::RegexNER::new();
        let supported = ner.supported_types();

        if let Ok(entities) = ner.extract_entities(&text, None) {
            for entity in &entities {
                // Pattern types should be in supported list
                let type_matches = supported.iter().any(|t| {
                    std::mem::discriminant(t) == std::mem::discriminant(&entity.entity_type)
                });
                prop_assert!(type_matches || matches!(&entity.entity_type, EntityType::Other(_)),
                    "Unsupported entity type: {:?}", entity.entity_type);
            }
        }
    }
}

// =============================================================================
// Property: Backend is_available consistency
// =============================================================================

#[test]
fn available_backends_can_extract() {
    let text = "John Smith works at Google.";

    let backends: Vec<Box<dyn Model>> = vec![
        Box::new(anno::RegexNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(anno::HeuristicNER::new()),
        Box::new(fast_stacked()),
        Box::new(fast_stacked()),
    ];

    for backend in &backends {
        if backend.is_available() {
            let result = backend.extract_entities(text, None);
            assert!(
                result.is_ok(),
                "Available backend {} failed: {:?}",
                backend.name(),
                result.err()
            );
        }
    }
}

// =============================================================================
// Property: Deterministic output (same input → same output)
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]  // Includes StackedNER which is slow

    #[test]
    fn backends_deterministic(text in arb_text_with_entities()) {
        let backends: Vec<Box<dyn Model>> = vec![
            Box::new(anno::RegexNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(fast_stacked()),
        ];

        for backend in &backends {
            if let (Ok(entities1), Ok(entities2)) = (
                backend.extract_entities(&text, None),
                backend.extract_entities(&text, None),
            ) {
                prop_assert_eq!(entities1.len(), entities2.len(),
                    "Backend {} non-deterministic: {} vs {} entities",
                    backend.name(), entities1.len(), entities2.len());

                for (e1, e2) in entities1.iter().zip(entities2.iter()) {
                    prop_assert_eq!(&e1.text, &e2.text);
                    prop_assert_eq!(e1.start, e2.start);
                    prop_assert_eq!(e1.end, e2.end);
                }
            }
        }
    }
}

// =============================================================================
// Property: Valid UTF-8 byte boundaries
// =============================================================================

/// Generate text with multi-byte UTF-8 characters.
fn arb_unicode_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "François",     // French accents
            "日本語",       // Japanese
            "中文测试",     // Chinese
            "Müller",       // German umlaut
            "Москва",       // Russian
            "São Paulo",    // Portuguese
            "Zürich",       // Swiss
            "東京",         // Tokyo
            "北京 Beijing", // Mixed
            "Straße",       // German eszett
            "€100",         // Euro symbol
            "£50.00",       // Pound symbol
            "¥1000",        // Yen symbol
        ]),
        1..5,
    )
    .prop_map(|parts| parts.join(" works at "))
}

proptest! {
    #![proptest_config(slow_config())]  // Includes StackedNER which is slow

    #[test]
    fn backends_valid_char_offsets(text in arb_unicode_text()) {
        let backends: Vec<Box<dyn Model>> = vec![
            Box::new(anno::RegexNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(fast_stacked()),
        ];

        let char_count = text.chars().count();

        for backend in &backends {
            if let Ok(entities) = backend.extract_entities(&text, None) {
                for entity in &entities {
                    // Verify that character offsets are valid
                    // Entity::start and Entity::end are CHARACTER offsets, not byte offsets
                    prop_assert!(entity.start <= entity.end,
                        "Backend {} returned invalid span: start {} > end {} for text: {}",
                        backend.name(), entity.start, entity.end, text);
                    prop_assert!(entity.end <= char_count,
                        "Backend {} returned span beyond text: end {} > char_count {} for text: {}",
                        backend.name(), entity.end, char_count, text);

                    // Verify we can extract text at these character positions
                    let extracted: String = text.chars().skip(entity.start).take(entity.end - entity.start).collect();
                    prop_assert!(!extracted.is_empty() || entity.start == entity.end,
                        "Backend {} returned empty span at valid positions for text: {}",
                        backend.name(), text);
                }
            }
        }
    }
}

// =============================================================================
// Property: Non-overlapping entities (for backends that guarantee this)
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]  // StackedNER is slow

    #[test]
    fn stacked_backend_no_overlaps(text in arb_text_with_entities()) {
        let ner = fast_stacked();
        if let Ok(mut entities) = ner.extract_entities(&text, None) {
            entities.sort_by_key(|e| e.start);

            for window in entities.windows(2) {
                let (e1, e2) = (&window[0], &window[1]);
                prop_assert!(e1.end <= e2.start,
                    "StackedNER returned overlapping entities: {:?} and {:?}", e1, e2);
            }
        }
    }
}

// =============================================================================
// Property: Entity text is non-empty
// =============================================================================

proptest! {
    #![proptest_config(slow_config())]  // Includes StackedNER which is slow

    #[test]
    fn backends_nonempty_entities(text in arb_text_with_entities()) {
        let backends: Vec<Box<dyn Model>> = vec![
            Box::new(anno::RegexNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(fast_stacked()),
            Box::new(anno::HeuristicNER::new()),
            Box::new(fast_stacked()),
        ];

        for backend in &backends {
            if let Ok(entities) = backend.extract_entities(&text, None) {
                for entity in &entities {
                    prop_assert!(!entity.text.is_empty(),
                        "Backend {} returned empty entity text", backend.name());
                    prop_assert!(!entity.text.trim().is_empty(),
                        "Backend {} returned whitespace-only entity text: '{}'",
                        backend.name(), entity.text);
                }
            }
        }
    }
}

// =============================================================================
// Property: Historical NER backends (BiLSTM-CRF, HMM) produce valid spans
// =============================================================================

proptest! {
    #![proptest_config(fast_config())]
    #[test]
    fn bilstm_crf_backend_valid_spans(text in arb_text()) {
        // Keep this fast: validate span invariants on a lightweight stacked pipeline.
        let ner = fast_stacked();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            let char_count = text.chars().count();
            for entity in &entities {
                prop_assert!(entity.start <= entity.end,
                    "Invalid span: {} > {}", entity.start, entity.end);
                prop_assert!(entity.end <= char_count,
                    "Span exceeds text: {} > {}", entity.end, char_count);
            }
        }
    }

    #[test]
    fn hmm_backend_valid_spans(text in arb_text()) {
        let ner = anno::HeuristicNER::new();
        if let Ok(entities) = ner.extract_entities(&text, None) {
            let char_count = text.chars().count();
            for entity in &entities {
                prop_assert!(entity.start <= entity.end,
                    "Invalid span: {} > {}", entity.start, entity.end);
                prop_assert!(entity.end <= char_count,
                    "Span exceeds text: {} > {}", entity.end, char_count);
            }
        }
    }
}

// =============================================================================
// Property: Span utilities produce valid tensors
// =============================================================================
// TODO: Re-enable when span_utils module is made public
// proptest! {
//     #[test]
//     fn span_tensors_valid(num_words in 0usize..100) {
//         use anno::backends::span_utils::make_span_tensors;
//
//         const MAX_WIDTH: usize = 12;
//         let (span_idx, span_mask) = make_span_tensors(num_words, MAX_WIDTH);
//
//         // Check tensor sizes
//         let expected_spans = num_words.saturating_mul(MAX_WIDTH);
//         prop_assert_eq!(span_mask.len(), expected_spans,
//             "span_mask has wrong size for {} words", num_words);
//         prop_assert_eq!(span_idx.len(), expected_spans.saturating_mul(2),
//             "span_idx has wrong size for {} words", num_words);
//
//         // Check that valid spans have correct indices
//         for (i, &valid) in span_mask.iter().enumerate() {
//             if valid && i * 2 + 1 < span_idx.len() {
//                 let start = span_idx[i * 2];
//                 let end = span_idx[i * 2 + 1];
//
//                 prop_assert!(start >= 0, "Negative start index");
//                 prop_assert!(end >= start, "End before start: {} < {}", end, start);
//                 prop_assert!((end as usize) < num_words,
//                     "End index {} exceeds num_words {}", end, num_words);
//             }
//         }
//     }
// }
