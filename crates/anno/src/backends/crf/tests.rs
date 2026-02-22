    use super::*;

    #[test]
    fn test_crf_basic() {
        let ner = CrfNER::new();
        let entities = ner
            .extract_entities("John Smith works at Google in California", None)
            .unwrap();

        // With our default heuristic weights + gazetteers, we should usually get some entities.
        // (Trained weights will do better, but defaults should not be totally dead.)
        assert!(!entities.is_empty(), "Expected some entities, got none");
    }

    #[test]
    fn test_word_shape() {
        assert_eq!(CrfNER::word_shape("John"), "Xx");
        assert_eq!(CrfNER::word_shape("USA"), "X");
        assert_eq!(CrfNER::word_shape("hello"), "x");
        assert_eq!(CrfNER::word_shape("123"), "0");
        assert_eq!(CrfNER::word_shape("Hello123"), "Xx0");
    }

    #[test]
    fn test_tokenize() {
        let tokens = CrfNER::tokenize("Hello world");
        assert_eq!(tokens, vec!["Hello", "world"]);
    }

    #[test]
    fn test_empty_input() {
        let ner = CrfNER::new();
        let entities = ner.extract_entities("", None).unwrap();
        assert!(entities.is_empty());
    }

    #[test]
    fn test_gazetteer_lookup() {
        let ner = CrfNER::new();

        // Gazetteer should contain common entities
        assert!(ner.gazetteers[&EntityType::Person].contains(&"John".to_string()));
        assert!(ner.gazetteers[&EntityType::Location].contains(&"California".to_string()));
        assert!(ner.gazetteers[&EntityType::Organization].contains(&"Google".to_string()));
    }

    #[test]
    fn test_viterbi_returns_valid_labels() {
        let ner = CrfNER::new();
        let tokens = vec!["John", "works", "at", "Google"];
        let labels = ner.viterbi_decode(&tokens);

        assert_eq!(labels.len(), tokens.len());
        for label in &labels {
            assert!(ner.labels.contains(label));
        }
    }

    #[test]
    fn test_common_verbs_not_in_entities() {
        let ner = CrfNER::new();

        // Test that common verbs don't get tagged as part of entities
        let entities = ner
            .extract_entities("John Smith works at Apple", None)
            .unwrap();

        // Should find John Smith and Apple, but NOT "works"
        let entity_texts: Vec<&str> = entities.iter().map(|e| e.text.as_str()).collect();
        for entity_text in &entity_texts {
            assert!(
                !entity_text.contains("works"),
                "Entity '{}' should not contain 'works'",
                entity_text
            );
        }
    }

    #[test]
    fn test_weights_for_common_words() {
        // This test asserts properties of the heuristic weight table, which includes
        // many `word.lower=...` entries. The bundled trained weights are intentionally
        // compact and omit token-identity features to keep the shipped file size small.
        #[cfg(feature = "bundled-crf-weights")]
        {
            return;
        }

        let ner = CrfNER::new();

        // Check that weights exist for common stop words
        assert!(
            ner.weights.contains_key("word.lower=works:O"),
            "Missing weight for word.lower=works:O"
        );
        assert!(
            ner.weights.contains_key("word.lower=works:I-PER"),
            "Missing weight for word.lower=works:I-PER"
        );

        // Check that O weight is positive and I-* weight is negative
        let o_weight = *ner.weights.get("word.lower=works:O").unwrap();
        let i_per_weight = *ner.weights.get("word.lower=works:I-PER").unwrap();
        assert!(
            o_weight > 0.0,
            "O weight should be positive, got {}",
            o_weight
        );
        assert!(
            i_per_weight < 0.0,
            "I-PER weight should be negative, got {}",
            i_per_weight
        );
    }

    #[test]
    fn test_unicode_char_offsets() {
        // Test that entity offsets are character-based, not byte-based
        let ner = CrfNER::new();

        // "北京" is 2 chars, 6 bytes. "Beijing" is 7 chars, 7 bytes.
        // Text "北京 Beijing" is 10 chars, 14 bytes.
        let text = "北京 Beijing";
        assert_eq!(text.len(), 14, "Expected 14 bytes");
        assert_eq!(text.chars().count(), 10, "Expected 10 characters");

        let entities = ner.extract_entities(text, None).unwrap();

        // Regardless of what entities are found, check all offsets are valid char offsets
        let char_count = text.chars().count();
        for entity in &entities {
            assert!(
                entity.start <= entity.end,
                "Invalid span: start {} > end {}",
                entity.start,
                entity.end
            );
            assert!(
                entity.end <= char_count,
                "Entity end {} exceeds char count {} for text {:?}",
                entity.end,
                char_count,
                text
            );

            // Also verify we can extract the text at those offsets
            let extracted: String = text
                .chars()
                .skip(entity.start)
                .take(entity.end - entity.start)
                .collect();
            assert!(
                !extracted.is_empty() || entity.start == entity.end,
                "Empty extraction for entity at {}..{} in {:?}",
                entity.start,
                entity.end,
                text
            );
        }
    }

    #[test]
    fn test_multilingual_inputs_no_panic_and_valid_spans() {
        let ner = CrfNER::new();
        let texts = [
            // Latin
            "Marie Curie discovered radium in Paris.",
            // CJK
            "習近平在北京會見了普京。",
            // Arabic (RTL)
            "التقى محمد بن سلمان بالرئيس في الرياض",
            // Cyrillic
            "Путин встретился с Си Цзиньпином в Москве.",
            // Devanagari
            "प्रधान मंत्री शर्मा दिल्ली में मिले।",
        ];

        for text in texts {
            let entities = ner.extract_entities(text, None).unwrap();
            let char_count = text.chars().count();
            for e in entities {
                assert!(e.start <= e.end);
                assert!(e.end <= char_count);
                let _span: String = text.chars().skip(e.start).take(e.end - e.start).collect();
            }
        }
    }

    /// Test that duplicate entity texts get correct offsets.
    #[test]
    fn test_duplicate_entity_offsets() {
        // Test token position calculation directly
        let text = "Google bought Google for $1 billion.";
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let positions = CrfNER::calculate_token_positions(text, &tokens);

        // First "Google" at byte 0-6
        assert_eq!(
            positions[0],
            (0, 6),
            "First 'Google' should be at bytes 0-6"
        );
        // Second "Google" at byte 14-20
        assert_eq!(
            positions[2],
            (14, 20),
            "Second 'Google' should be at bytes 14-20"
        );
    }

    /// Test token position calculation with Unicode.
    #[test]
    fn test_token_positions_unicode() {
        let text = "東京 Tokyo 東京";
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let positions = CrfNER::calculate_token_positions(text, &tokens);

        // Each 東京 is 6 bytes (2 chars × 3 bytes each)
        assert_eq!(positions[0], (0, 6), "First '東京' at bytes 0-6");
        assert_eq!(positions[1], (7, 12), "Tokyo at bytes 7-12");
        assert_eq!(positions[2], (13, 19), "Second '東京' at bytes 13-19");
    }
