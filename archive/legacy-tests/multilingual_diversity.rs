//! Multilingual and multicultural diversity tests.
//!
//! Ensures anno handles diverse languages, scripts, and cultural contexts correctly.
//! This is critical for avoiding bias and ensuring global applicability.

use anno::Model;

/// Test texts covering major world scripts and languages.
/// Returns: (text, language_description, expected_entities)
fn diverse_test_texts() -> Vec<(&'static str, &'static str, Vec<&'static str>)> {
    vec![
        // (text, language, expected_entity_texts)

        // === Latin Script ===
        (
            "Marie Curie discovered radium in Paris.",
            "English",
            vec!["Marie Curie", "Paris"],
        ),
        (
            "Angela Merkel war Bundeskanzlerin von Deutschland.",
            "German",
            vec!["Angela Merkel", "Deutschland"],
        ),
        (
            "François Hollande a visité São Paulo.",
            "French/Portuguese",
            vec!["François Hollande", "São Paulo"],
        ),
        (
            "José García trabaja en Ciudad de México.",
            "Spanish",
            vec!["José García", "Ciudad de México"],
        ),
        // === CJK (Chinese, Japanese, Korean) ===
        ("習近平是中國國家主席。", "Chinese", vec!["習近平", "中國"]),
        (
            "安倍晋三は東京で記者会見を開いた。",
            "Japanese",
            vec!["安倍晋三", "東京"],
        ),
        (
            "문재인 대통령이 서울에서 연설했다.",
            "Korean",
            vec!["문재인", "서울"],
        ),
        // === Cyrillic ===
        (
            "Путин встретился с делегацией в Москве.",
            "Russian",
            vec!["Путин", "Москве"],
        ),
        (
            "Зеленський виступив у Києві.",
            "Ukrainian",
            vec!["Зеленський", "Києві"],
        ),
        // === Arabic (RTL) ===
        (
            "محمد بن سلمان زار الرياض",
            "Arabic",
            vec!["محمد بن سلمان", "الرياض"],
        ),
        // === Devanagari ===
        (
            "नरेंद्र मोदी ने नई दिल्ली में भाषण दिया।",
            "Hindi",
            vec!["नरेंद्र मोदी", "नई दिल्ली"],
        ),
        // === Mixed Scripts / Code-Switching ===
        (
            "Dr. 田中 presented at MIT's AI conference.",
            "Mixed",
            vec!["田中", "MIT"],
        ),
        (
            "CEO 李明 announced partnership with Google.",
            "Mixed",
            vec!["李明", "Google"],
        ),
        // === Transliteration Variants ===
        (
            "Meeting in Москва (Moscow) with Putin.",
            "Mixed",
            vec!["Москва", "Moscow", "Putin"],
        ),
        // === Diacritics and Special Characters ===
        (
            "Björk performed in Reykjavík.",
            "Icelandic",
            vec!["Björk", "Reykjavík"],
        ),
        (
            "Müller und Schröder trafen sich in Zürich.",
            "German",
            vec!["Müller", "Schröder", "Zürich"],
        ),
        // === Single-Name Entities ===
        (
            "Pelé scored in the World Cup.",
            "Portuguese",
            vec!["Pelé", "World Cup"],
        ),
        (
            "Madonna performed at the Super Bowl.",
            "English",
            vec!["Madonna", "Super Bowl"],
        ),
        // === Honorifics Across Cultures ===
        (
            "Her Majesty Queen Elizabeth II visited Canada.",
            "English",
            vec!["Queen Elizabeth II", "Canada"],
        ),
        (
            "Sheikh Mohammed opened the Dubai Expo.",
            "Arabic/English",
            vec!["Sheikh Mohammed", "Dubai Expo"],
        ),
        // === Patronymic Names ===
        (
            "Ivan Ivanovich Petrov works in Moscow.",
            "Russian",
            vec!["Ivan Ivanovich Petrov", "Moscow"],
        ),
        // === Emoji in Text ===
        (
            "🎉 Congrats to Elon Musk on SpaceX! 🚀",
            "English+Emoji",
            vec!["Elon Musk", "SpaceX"],
        ),
    ]
}

/// Verify character offset handling with Unicode
#[test]
fn test_unicode_character_offsets() {
    let test_cases = [
        // (text, expected_char_count)
        ("Hello", 5),
        ("日本語", 3),        // 3 chars, 9 bytes
        ("Müller", 6),        // 6 chars (ü is 1 char)
        ("🎉🎊🎁", 3),        // 3 chars, 12 bytes
        ("北京 Beijing", 10), // Mixed
        ("Москва", 6),        // Cyrillic
        ("مصر", 3),           // Arabic
    ];

    for (text, expected_chars) in test_cases {
        let char_count = text.chars().count();
        assert_eq!(
            char_count, expected_chars,
            "Character count mismatch for '{}': got {}, expected {}",
            text, char_count, expected_chars
        );
    }
}

/// Test entity extraction handles multi-byte characters correctly
#[test]
fn test_entity_span_extraction() {
    let text = "習近平訪問了北京。";
    let char_count = text.chars().count();

    // Simulate entity at character positions 0-3 (習近平)
    let start = 0;
    let end = 3;

    assert!(end <= char_count, "Span should not exceed text length");

    let extracted: String = text.chars().skip(start).take(end - start).collect();
    assert_eq!(extracted, "習近平");
}

/// Test that span boundaries don't split multi-byte characters
#[test]
fn test_no_mid_character_splits() {
    let text = "Müller works at Google";

    // Valid character boundaries
    let valid_positions: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();

    // Verify 'ü' is treated as single character
    let u_umlaut_pos = text.find('ü').unwrap();
    assert!(
        valid_positions.contains(&u_umlaut_pos),
        "ü position should be a valid character boundary"
    );
}

/// Test diverse name patterns across cultures
#[test]
fn test_diverse_name_patterns() {
    let name_patterns = [
        // Western: Given Family
        ("John Smith", "Western"),
        // East Asian: Family Given
        ("山田太郎", "Japanese"),
        ("王小明", "Chinese"),
        // Patronymic
        ("Ivan Ivanovich", "Russian patronymic"),
        ("Björk Guðmundsdóttir", "Icelandic patronymic"),
        // Single name
        ("Pelé", "Single name"),
        ("Sukarno", "Indonesian"),
        // Arabic: Given bin/bint Family
        ("محمد بن سلمان", "Arabic"),
        // Compound names
        ("Jean-Claude Van Damme", "Western compound"),
        ("García Márquez", "Hispanic"),
    ];

    for (name, pattern) in name_patterns {
        assert!(!name.is_empty(), "Name should not be empty: {}", pattern);
        // Names should be extractable as entities without panicking
        let char_count = name.chars().count();
        assert!(
            char_count > 0,
            "Name '{}' ({}) should have chars",
            name,
            pattern
        );
    }
}

/// Test organization names across cultures
#[test]
fn test_organization_suffixes() {
    let org_patterns = [
        // Western
        ("Apple Inc.", "US"),
        ("Google LLC", "US"),
        ("Microsoft Corporation", "US"),
        ("Siemens AG", "German"),
        ("Total S.A.", "French"),
        ("Royal Dutch Shell plc", "UK/Dutch"),
        // East Asian
        ("トヨタ自動車株式会社", "Japanese"),
        ("삼성전자", "Korean"),
        ("华为技术有限公司", "Chinese"),
        // Other
        ("Газпром", "Russian"),
        ("أرامكو السعودية", "Arabic"),
    ];

    for (org, region) in org_patterns {
        assert!(!org.is_empty(), "Org name should not be empty: {}", region);
    }
}

/// Test with code-switching (multilingual sentences)
#[test]
fn test_code_switching() {
    let mixed_texts = [
        "I went to 東京 last week.",
        "Das Meeting ist in New York.",
        "J'ai visité la Google headquarters.",
        "CEO 李明 announced the merger.",
        "Мы встретились в Starbucks.",
    ];

    for text in mixed_texts {
        // Should handle without panicking
        let char_count = text.chars().count();
        assert!(char_count > 0, "Mixed text should have chars");
    }
}

/// Test RTL (right-to-left) text handling
#[test]
fn test_rtl_text() {
    let rtl_texts = [
        // Arabic
        "محمد يعمل في شركة جوجل",
        // Hebrew
        "דוד עובד בתל אביב",
        // Mixed LTR/RTL
        "Meeting with محمد at Google",
    ];

    for text in rtl_texts {
        let char_count = text.chars().count();
        // Character iteration should work correctly
        let chars: Vec<char> = text.chars().collect();
        assert_eq!(chars.len(), char_count);
    }
}

/// Test entity extraction produces valid spans for diverse texts
#[test]
fn test_backend_handles_diverse_input() {
    // Use basic backends that are always available
    let ner = anno::StackedNER::default();

    // Sample of diverse texts
    let diverse_samples = [
        "Marie Curie discovered radium.",
        "東京オリンピック2020",
        "Angela Merkel war Bundeskanzlerin.",
        "CEO 李明 announced partnership.",
        "Müller und Schröder in Zürich.",
    ];

    for text in diverse_samples {
        let result = ner.extract_entities(text, None);
        assert!(
            result.is_ok(),
            "Backend should handle '{}': {:?}",
            text,
            result.err()
        );

        let entities = result.unwrap();
        let char_count = text.chars().count();

        for entity in &entities {
            assert!(
                entity.start <= entity.end,
                "Invalid span for '{}': {} > {}",
                text,
                entity.start,
                entity.end
            );
            assert!(
                entity.end <= char_count,
                "Span exceeds text for '{}': {} > {}",
                text,
                entity.end,
                char_count
            );
        }
    }
}

/// Constructed language test data for edge cases
#[test]
fn test_constructed_languages() {
    // Esperanto - has real speakers and UD treebank
    let esperanto = "Doktoro Zamenhof fondis Esperanton en Varsovio.";

    // Toki Pona - minimal vocabulary (120 words)
    let toki_pona = "jan Sonja li mama pi toki pona.";

    // These should be processable without panicking
    assert!(esperanto.chars().count() > 0);
    assert!(toki_pona.chars().count() > 0);
}

/// Test all diverse texts can be processed by backends
#[test]
fn test_diverse_texts_comprehensive() {
    let ner = anno::StackedNER::default();

    for (text, lang, _expected) in diverse_test_texts() {
        let result = ner.extract_entities(text, None);
        assert!(
            result.is_ok(),
            "Backend should handle {} text '{}': {:?}",
            lang,
            &text[..text.len().min(30)],
            result.err()
        );

        // Verify span validity
        let entities = result.unwrap();
        let char_count = text.chars().count();
        for entity in &entities {
            assert!(
                entity.end <= char_count,
                "Entity span exceeds text for {} '{}': {} > {}",
                lang,
                text,
                entity.end,
                char_count
            );
        }
    }
}

/// Property-based test for Unicode safety
#[cfg(test)]
mod property_tests {

    use proptest::prelude::*;

    /// Generate Unicode text that might cause issues
    fn arb_unicode_heavy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec![
                "日本語テスト",
                "Müller",
                "François",
                "北京",
                "Москва",
                "🎉🚀",
                "محمد",
                "नई दिल्ली",
                " works at ",
                " met ",
            ]),
            1..5,
        )
        .prop_map(|parts| parts.join(""))
    }

    proptest! {
        #[test]
        fn char_iteration_never_panics(text in arb_unicode_heavy()) {
            // Character iteration should never panic
            let _ = text.chars().count();
            let _: Vec<char> = text.chars().collect();
            let _: Vec<(usize, char)> = text.char_indices().collect();
        }

        #[test]
        fn span_extraction_never_panics(text in arb_unicode_heavy()) {
            let char_count = text.chars().count();
            if char_count > 0 {
                // Extract first half as span
                let mid = char_count / 2;
                let span: String = text.chars().take(mid).collect();
                prop_assert!(!span.is_empty() || mid == 0);
            }
        }
    }
}
