use anno::backends::span_utils::{decode_span_output, map_label_to_entity_type, SpanConfig};

#[test]
fn decode_supports_fine_grained_labels() {
    // Text with two words; we want to tag "Dalmatian" as ANIMAL using fine label.
    let text = "Dalmatian runs";
    let text_words: Vec<&str> = text.split_whitespace().collect();
    let entity_types = ["animal"];

    // Shape: [batch=1, num_words=2, max_width=2, num_classes=1]
    // Order: start=0,width=0 -> score 0.9 (Dalmatian)
    // Others are low scores.
    let output_data = vec![
        0.9, // start 0, width 0
        0.1, // start 0, width 1
        0.1, // start 1, width 0
        0.1, // start 1, width 1
    ];
    let shape = [1, 2, 2, 1];
    let config = SpanConfig {
        max_span_width: 2,
        threshold: 0.5,
    };

    let entities =
        decode_span_output(&output_data, &shape, text, &text_words, &entity_types, &config)
            .expect("decode should succeed");
    assert_eq!(entities.len(), 1);
    let e = &entities[0];
    assert_eq!(e.text, "Dalmatian");
    if let anno_core::EntityType::Other(tag) = &e.entity_type {
        assert_eq!(tag, "ANIMAL");
    } else {
        panic!("Expected EntityType::Other(ANIMAL), got {:?}", e.entity_type);
    }
}

#[test]
fn map_label_to_entity_type_covers_cner_tags() {
    assert!(matches!(
        map_label_to_entity_type("substance"),
        anno_core::EntityType::Other(ref s) if s == "SUBSTANCE"
    ));
    assert!(matches!(
        map_label_to_entity_type("supernatural"),
        anno_core::EntityType::Other(ref s) if s == "SUPER"
    ));
    assert!(matches!(
        map_label_to_entity_type("celestial"),
        anno_core::EntityType::Other(ref s) if s == "CELESTIAL"
    ));
}

#[test]
fn overlapping_spans_keep_highest_confidence() {
    // Text has three words; we will score two overlapping spans:
    // span (0,1) with higher score, span (0,0) with lower score.
    let text = "Steve Jobs founded";
    let text_words: Vec<&str> = text.split_whitespace().collect();
    let entity_types = ["person"];

    // shape: [batch=1, num_words=3, max_width=2, num_classes=1]
    // Layout: start * max_width * num_classes + width * num_classes + class
    // Scores:
    // (0,0): 0.6
    // (0,1): 0.9 (overlaps and should be kept)
    // others: low
    let output_data = vec![
        0.6, 0.9, // start 0 width 0/1
        0.1, 0.1, // start 1 width 0/1
        0.1, 0.1, // start 2 width 0/1
    ];
    let shape = [1, 3, 2, 1];
    let config = SpanConfig {
        max_span_width: 2,
        threshold: 0.5,
    };

    let entities =
        decode_span_output(&output_data, &shape, text, &text_words, &entity_types, &config)
            .expect("decode should succeed");
    assert_eq!(entities.len(), 1);
    let e = &entities[0];
    assert_eq!(e.text, "Steve Jobs");
    assert!(
        e.confidence > 0.8,
        "expected higher-confidence overlapping span to survive"
    );
}

#[test]
fn decode_span_output_uses_character_offsets_for_unicode() {
    // `decode_span_output` internally finds word spans using byte offsets, but `Entity` requires
    // character offsets. This test ensures we convert correctly across diverse scripts.
    let cases = [
        // CJK + Latin
        ("北京", "Beijing"),
        // Cyrillic + Latin
        ("Москва", "Moscow"),
        // Arabic (RTL)
        ("محمد", "الرياض"),
        // Devanagari
        ("शर्मा", "दिल्ली"),
    ];

    for (w1, w2) in cases {
        let text = format!("{w1} {w2}");
        let text_words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(text_words.len(), 2);

        // Shape: [batch=1, num_words=2, max_width=1, num_classes=1]
        // We only allow width=0 (single-word spans). Make word 2 the only entity.
        let output_data = vec![
            0.0, // start 0, width 0
            0.9, // start 1, width 0  -> selects w2
        ];
        let shape = [1, 2, 1, 1];
        let entity_types = ["loc"];
        let config = SpanConfig {
            max_span_width: 1,
            threshold: 0.5,
        };

        let entities =
            decode_span_output(&output_data, &shape, &text, &text_words, &entity_types, &config)
                .expect("decode should succeed");

        assert_eq!(entities.len(), 1);
        let e = &entities[0];
        assert_eq!(e.text, w2);

        let expected_start = w1.chars().count() + 1; // one ASCII space
        let expected_end = expected_start + w2.chars().count();
        assert_eq!(
            (e.start, e.end),
            (expected_start, expected_end),
            "unexpected offsets for text={:?} entity={:?}",
            text,
            e.text
        );
    }
}

