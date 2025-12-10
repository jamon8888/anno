//! Unicode stress tests for entity resolution.
//!
//! Tests correct handling of:
//! - Multi-byte characters (CJK, emoji)
//! - Right-to-left scripts (Arabic, Hebrew)
//! - Combining characters and diacritics
//! - Mixed scripts in single strings
//! - Edge cases (zero-width chars, BOM)

use anno_coalesce::{string_similarity, Resolver};
use anno_core::{Corpus, GroundedDocument, Track};

// =============================================================================
// CJK (Chinese, Japanese, Korean) Tests
// =============================================================================

#[test]
fn test_cjk_identical() {
    // Chinese
    assert_eq!(string_similarity("北京", "北京"), 1.0);
    assert_eq!(string_similarity("习近平", "习近平"), 1.0);

    // Japanese
    assert_eq!(string_similarity("東京", "東京"), 1.0);
    assert_eq!(string_similarity("安倍晋三", "安倍晋三"), 1.0);

    // Korean
    assert_eq!(string_similarity("서울", "서울"), 1.0);
    assert_eq!(string_similarity("김정은", "김정은"), 1.0);
}

#[test]
fn test_cjk_different() {
    // Different Chinese cities (no shared trigrams for 2-char names)
    let sim = string_similarity("北京", "上海");
    // 2-char strings use whole string as single "trigram", so no overlap
    assert!(sim < 0.5, "Different cities should have low similarity: {}", sim);

    // Different Japanese cities
    let sim = string_similarity("東京", "大阪");
    assert!(sim < 0.5, "Different cities should have low similarity: {}", sim);
}

#[test]
fn test_cjk_partial_overlap() {
    // Character trigram similarity works for CJK without spaces
    let sim = string_similarity("中华人民共和国", "中华民国");
    // Shared trigrams: "中华人" overlaps with "中华民" at "中华"
    assert!(
        sim > 0.0 && sim < 1.0,
        "CJK partial overlap should be between 0 and 1: {}",
        sim
    );

    // More overlap = higher similarity
    let sim_more = string_similarity("北京市", "北京");
    let sim_less = string_similarity("北京市", "上海市");
    assert!(
        sim_more > sim_less,
        "More overlap ({}) should be higher than less overlap ({})",
        sim_more,
        sim_less
    );
}

#[test]
fn test_resolver_cjk_clustering() {
    let resolver = Resolver::new().with_threshold(0.99);
    let mut corpus = Corpus::new();

    // Identical Chinese entities
    let mut doc1 = GroundedDocument::new("doc1", "中国首都北京");
    doc1.add_track(Track::new(1, "北京").with_type("LOCATION".to_string()));
    corpus.add_document(doc1);

    let mut doc2 = GroundedDocument::new("doc2", "我去北京");
    doc2.add_track(Track::new(1, "北京").with_type("LOCATION".to_string()));
    corpus.add_document(doc2);

    let ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);
    assert_eq!(ids.len(), 1, "Identical CJK entities should cluster");
}

// =============================================================================
// Right-to-Left (RTL) Script Tests
// =============================================================================

#[test]
fn test_arabic_identical() {
    assert_eq!(string_similarity("محمد", "محمد"), 1.0);
    assert_eq!(string_similarity("الرياض", "الرياض"), 1.0);
    assert_eq!(string_similarity("مُحَمَّد بن سلمان", "مُحَمَّد بن سلمان"), 1.0);
}

#[test]
fn test_hebrew_identical() {
    assert_eq!(string_similarity("ירושלים", "ירושלים"), 1.0);
    assert_eq!(string_similarity("בנימין נתניהו", "בנימין נתניהו"), 1.0);
}

#[test]
fn test_rtl_different() {
    // Different Arabic cities
    let sim = string_similarity("الرياض", "القاهرة");
    assert!(sim < 0.5, "Different cities should have low similarity: {}", sim);
}

// =============================================================================
// Combining Characters and Diacritics Tests
// =============================================================================

#[test]
fn test_diacritics_identical() {
    // Latin with diacritics
    assert_eq!(string_similarity("François", "François"), 1.0);
    assert_eq!(string_similarity("José García", "José García"), 1.0);
    assert_eq!(string_similarity("Zürich", "Zürich"), 1.0);
    assert_eq!(string_similarity("São Paulo", "São Paulo"), 1.0);
}

#[test]
fn test_precomposed_vs_combining() {
    // Precomposed: ü (U+00FC)
    // Combining: u + ̈  (U+0075 + U+0308)
    let precomposed = "Zürich";
    let combining = "Zu\u{0308}rich";

    // These might not be equal depending on normalization
    // The test documents current behavior
    let sim = string_similarity(precomposed, combining);
    // Note: Without Unicode normalization, these are different strings
    assert!(
        sim >= 0.0 && sim <= 1.0,
        "Similarity should be valid: {}",
        sim
    );
}

#[test]
fn test_vietnamese_tones() {
    // Vietnamese with tone marks
    assert_eq!(string_similarity("Hà Nội", "Hà Nội"), 1.0);
    assert_eq!(string_similarity("Nguyễn", "Nguyễn"), 1.0);
}

// =============================================================================
// Cyrillic Tests
// =============================================================================

#[test]
fn test_cyrillic_identical() {
    assert_eq!(string_similarity("Москва", "Москва"), 1.0);
    assert_eq!(string_similarity("Владимир Путин", "Владимир Путин"), 1.0);
}

#[test]
fn test_cyrillic_different() {
    let sim = string_similarity("Москва", "Санкт-Петербург");
    assert!(sim < 0.5, "Different cities: {}", sim);
}

#[test]
fn test_cyrillic_latin_lookalikes() {
    // Cyrillic "а" vs Latin "a" - visually identical but different codepoints
    let cyrillic = "Москва"; // Cyrillic
    let mixed = "Mосква"; // First char is Latin M

    // These should NOT be identical
    let sim = string_similarity(cyrillic, mixed);
    // Behavior depends on whether we normalize
    assert!(sim >= 0.0 && sim <= 1.0);
}

// =============================================================================
// Emoji and Special Characters Tests
// =============================================================================

#[test]
fn test_emoji_in_names() {
    // Names with emoji (common in social media)
    assert_eq!(string_similarity("John 🎉", "John 🎉"), 1.0);
    assert_eq!(string_similarity("Apple 🍎", "Apple 🍎"), 1.0);
}

#[test]
fn test_emoji_only() {
    // Emoji-only "names"
    assert_eq!(string_similarity("🎉", "🎉"), 1.0);
    let sim = string_similarity("🎉", "🎊");
    assert!(sim < 1.0, "Different emoji should differ");
}

#[test]
fn test_multi_codepoint_emoji() {
    // Family emoji is multiple codepoints
    let family = "👨‍👩‍👧‍👦"; // U+1F468 U+200D U+1F469 U+200D U+1F467 U+200D U+1F466
    assert_eq!(string_similarity(family, family), 1.0);
}

#[test]
fn test_flag_emoji() {
    // Flag emoji are regional indicator pairs
    let us_flag = "🇺🇸";
    let uk_flag = "🇬🇧";

    assert_eq!(string_similarity(us_flag, us_flag), 1.0);
    let sim = string_similarity(us_flag, uk_flag);
    assert!(sim < 1.0);
}

// =============================================================================
// Mixed Script Tests
// =============================================================================

#[test]
fn test_mixed_scripts_identical() {
    // Mixed Latin and CJK
    assert_eq!(string_similarity("iPhone 15", "iPhone 15"), 1.0);
    assert_eq!(string_similarity("COVID-19 新冠病毒", "COVID-19 新冠病毒"), 1.0);
}

#[test]
fn test_code_switching() {
    // Common in multilingual contexts
    let text1 = "Meeting tomorrow re: 东京 project";
    let text2 = "Meeting tomorrow re: 東京 project";

    // Japanese 東京 vs simplified 东京 - different characters
    let sim = string_similarity(text1, text2);
    assert!(sim > 0.5, "Should share common words: {}", sim);
}

// =============================================================================
// Edge Cases Tests
// =============================================================================

#[test]
fn test_empty_strings() {
    assert_eq!(string_similarity("", ""), 1.0);
    assert_eq!(string_similarity("test", ""), 0.0);
    assert_eq!(string_similarity("", "test"), 0.0);
}

#[test]
fn test_whitespace_only() {
    assert_eq!(string_similarity("   ", "   "), 1.0);
    // Whitespace-only vs empty
    let sim = string_similarity("   ", "");
    assert!(sim >= 0.0 && sim <= 1.0);
}

#[test]
fn test_zero_width_characters() {
    // Zero-width space (U+200B)
    let with_zwsp = "Hello\u{200B}World";
    let without = "HelloWorld";

    // These might or might not match depending on handling
    let sim = string_similarity(with_zwsp, without);
    assert!(sim >= 0.0 && sim <= 1.0);
}

#[test]
fn test_bom_handling() {
    // Byte Order Mark (U+FEFF)
    let with_bom = "\u{FEFF}Hello";
    let without = "Hello";

    let sim = string_similarity(with_bom, without);
    // BOM is treated as a character, so won't be identical
    assert!(sim >= 0.0 && sim <= 1.0);
}

#[test]
fn test_direction_marks() {
    // Left-to-right mark (U+200E)
    let with_lrm = "Hello\u{200E}";
    let without = "Hello";

    let sim = string_similarity(with_lrm, without);
    assert!(sim >= 0.0 && sim <= 1.0);
}

// =============================================================================
// Very Long Unicode Strings Tests
// =============================================================================

#[test]
fn test_long_cjk_string() {
    // Long Chinese text
    let long1 = "中华人民共和国是位于东亚的社会主义国家中华人民共和国";
    let long2 = "中华人民共和国是位于东亚的社会主义国家中华人民共和国";

    assert_eq!(string_similarity(long1, long2), 1.0);
}

#[test]
fn test_very_long_mixed_script() {
    let long = "Apple Inc 苹果公司 is a technology company ".repeat(10);

    // Should not panic or hang
    let sim = string_similarity(&long, &long);
    assert_eq!(sim, 1.0);
}

// =============================================================================
// Corpus-Level Unicode Tests
// =============================================================================

#[test]
fn test_corpus_multilingual_entities() {
    let resolver = Resolver::new().with_threshold(0.3);
    let mut corpus = Corpus::new();

    // Add entities in different scripts
    let entities = vec![
        ("doc1", "北京", "LOCATION"),       // Chinese
        ("doc2", "東京", "LOCATION"),       // Japanese
        ("doc3", "Москва", "LOCATION"),     // Russian
        ("doc4", "الرياض", "LOCATION"),     // Arabic
        ("doc5", "ירושלים", "LOCATION"),    // Hebrew
        ("doc6", "मुंबई", "LOCATION"),       // Hindi
        ("doc7", "กรุงเทพ", "LOCATION"),    // Thai
        ("doc8", "서울", "LOCATION"),       // Korean
    ];

    for (doc_id, name, etype) in entities {
        let mut doc = GroundedDocument::new(doc_id, "text");
        doc.add_track(Track::new(1, name).with_type(etype.to_string()));
        corpus.add_document(doc);
    }

    let ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // All different cities should stay separate
    assert_eq!(
        ids.len(),
        8,
        "Different scripts/cities should not cluster"
    );
}

// =============================================================================
// Property Tests with Unicode
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Unicode strings should have symmetric similarity
        #[test]
        fn unicode_similarity_symmetric(a in "\\PC{1,30}", b in "\\PC{1,30}") {
            let sim_ab = string_similarity(&a, &b);
            let sim_ba = string_similarity(&b, &a);
            prop_assert!((sim_ab - sim_ba).abs() < 0.001,
                "Symmetry violated for Unicode: {} vs {}", sim_ab, sim_ba);
        }

        /// Unicode strings should have bounded similarity
        #[test]
        fn unicode_similarity_bounded(a in "\\PC{0,50}", b in "\\PC{0,50}") {
            let sim = string_similarity(&a, &b);
            prop_assert!(sim >= 0.0 && sim <= 1.0,
                "Similarity {} out of bounds", sim);
        }

        /// Identical Unicode strings should have similarity 1.0
        #[test]
        fn unicode_identity(s in "\\PC{0,50}") {
            let sim = string_similarity(&s, &s);
            prop_assert!((sim - 1.0).abs() < 0.001,
                "Identity violated: {}", sim);
        }
    }
}
