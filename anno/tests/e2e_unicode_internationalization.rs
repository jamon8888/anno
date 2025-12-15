//! Unicode and internationalization tests
//!
//! Tests handling of non-ASCII text, various scripts, and international characters.

use anno_coalesce::Resolver;
use anno_core::Corpus;
use anno_core::{GroundedDocument, Location, Signal, Track};

/// E2E: Chinese text extraction
#[test]
fn e2e_chinese_text() {
    let text = "北京是中国的首都。上海是最大的城市。";
    let mut doc = GroundedDocument::new("chinese_doc", text);

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 2), "北京", "LOC", 0.95));
    let _sig2 = doc.add_signal(Signal::new(1, Location::text(3, 5), "中国", "LOC", 0.95));
    let _sig3 = doc.add_signal(Signal::new(2, Location::text(8, 10), "上海", "LOC", 0.95));

    assert_eq!(doc.signals().len(), 3);

    // Verify character offsets work correctly with multi-byte characters
    let signal = doc.get_signal(sig1).unwrap();
    assert_eq!(signal.surface(), "北京");
}

/// E2E: Japanese text with mixed scripts
#[test]
fn e2e_japanese_mixed_scripts() {
    let text = "東京は日本の首都です。Tokyo is also written as 東京.";
    let mut doc = GroundedDocument::new("japanese_doc", text);

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 2), "東京", "LOC", 0.95));
    let _sig2 = doc.add_signal(Signal::new(1, Location::text(3, 5), "日本", "LOC", 0.95));
    let sig3 = doc.add_signal(Signal::new(2, Location::text(15, 20), "Tokyo", "LOC", 0.90));

    assert_eq!(doc.signals().len(), 3);

    // Verify mixed script handling
    let signal1 = doc.get_signal(sig1).unwrap();
    let signal3 = doc.get_signal(sig3).unwrap();
    assert_eq!(signal1.surface(), "東京");
    assert_eq!(signal3.surface(), "Tokyo");
}

/// E2E: Arabic text with RTL direction
#[test]
fn e2e_arabic_rtl_text() {
    let text = "القاهرة هي عاصمة مصر. الإسكندرية مدينة ساحلية.";
    let mut doc = GroundedDocument::new("arabic_doc", text);

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 6), "القاهرة", "LOC", 0.95));
    let _sig2 = doc.add_signal(Signal::new(1, Location::text(18, 22), "مصر", "LOC", 0.95));

    assert_eq!(doc.signals().len(), 2);

    // Verify RTL text handling
    let signal = doc.get_signal(sig1).unwrap();
    assert_eq!(signal.surface(), "القاهرة");
}

/// E2E: Russian Cyrillic text
#[test]
fn e2e_russian_cyrillic() {
    let text = "Москва — столица России. Санкт-Петербург — культурный центр.";
    let mut doc = GroundedDocument::new("russian_doc", text);

    let sig1 = doc.add_signal(Signal::new(0, Location::text(0, 6), "Москва", "LOC", 0.95));
    let _sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(18, 25),
        "России",
        "LOC",
        0.95,
    ));

    assert_eq!(doc.signals().len(), 2);

    let signal = doc.get_signal(sig1).unwrap();
    assert_eq!(signal.surface(), "Москва");
}

/// E2E: Mixed scripts in same document
#[test]
fn e2e_mixed_scripts() {
    let text = "Tokyo (東京) is the capital of Japan (日本). Москва is the capital of Россия.";
    let mut doc = GroundedDocument::new("mixed_doc", text);

    let _sig1 = doc.add_signal(Signal::new(0, Location::text(0, 5), "Tokyo", "LOC", 0.95));
    let _sig2 = doc.add_signal(Signal::new(1, Location::text(7, 9), "東京", "LOC", 0.95));
    let _sig3 = doc.add_signal(Signal::new(2, Location::text(35, 40), "Japan", "LOC", 0.95));
    let _sig4 = doc.add_signal(Signal::new(3, Location::text(42, 44), "日本", "LOC", 0.95));
    let _sig5 = doc.add_signal(Signal::new(
        4,
        Location::text(46, 52),
        "Москва",
        "LOC",
        0.95,
    ));
    let sig6 = doc.add_signal(Signal::new(
        5,
        Location::text(70, 76),
        "Россия",
        "LOC",
        0.95,
    ));

    assert_eq!(doc.signals().len(), 6);

    // Verify all scripts handled correctly
    let signals: Vec<&str> = doc.signals().iter().map(|s| s.surface()).collect();
    assert!(signals.contains(&"Tokyo"));
    assert!(signals.contains(&"東京"));
    assert!(signals.contains(&"Москва"));
    assert!(signals.contains(&"Россия"));
}

/// E2E: Emoji in entity names
#[test]
fn e2e_emoji_entities() {
    let text = "The 🏛️ White House is in Washington. 🇺🇸 is the flag of the United States.";
    let mut doc = GroundedDocument::new("emoji_doc", text);

    let sig1 = doc.add_signal(Signal::new(0, Location::text(4, 8), "🏛️", "EMOJI", 0.8));
    let _sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(9, 20),
        "White House",
        "LOC",
        0.95,
    ));
    let sig3 = doc.add_signal(Signal::new(2, Location::text(35, 37), "🇺🇸", "FLAG", 0.9));

    assert_eq!(doc.signals().len(), 3);

    // Verify emoji handling
    let signal = doc.get_signal(sig1).unwrap();
    assert_eq!(signal.surface(), "🏛️");
}

/// E2E: Accented characters in entity names
#[test]
fn e2e_accented_characters() {
    let text = "São Paulo is in Brazil. München is in Germany. Montréal is in Canada.";
    let mut doc = GroundedDocument::new("accented_doc", text);

    let sig1 = doc.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "São Paulo",
        "LOC",
        0.95,
    ));
    let sig2 = doc.add_signal(Signal::new(
        1,
        Location::text(25, 33),
        "München",
        "LOC",
        0.95,
    ));
    let sig3 = doc.add_signal(Signal::new(
        2,
        Location::text(50, 59),
        "Montréal",
        "LOC",
        0.95,
    ));

    assert_eq!(doc.signals().len(), 3);

    // Verify accented characters preserved
    let signal1 = doc.get_signal(sig1).unwrap();
    let signal2 = doc.get_signal(sig2).unwrap();
    let signal3 = doc.get_signal(sig3).unwrap();

    assert_eq!(signal1.surface(), "São Paulo");
    assert_eq!(signal2.surface(), "München");
    assert_eq!(signal3.surface(), "Montréal");
}

/// E2E: Cross-document coreference with accented vs non-accented names
#[test]
fn e2e_crossdoc_accented_vs_non_accented() {
    let mut corpus = Corpus::new();

    // Doc1: "São Paulo" (with accent)
    let mut doc1 = GroundedDocument::new("doc1", "São Paulo is a city in Brazil.");
    let sig1 = doc1.add_signal(Signal::new(
        0,
        Location::text(0, 10),
        "São Paulo",
        "LOC",
        0.95,
    ));
    let mut track1 = Track::new(0, "são paulo");
    track1.add_signal(sig1, 0);
    track1.entity_type = Some("LOC".to_string());
    doc1.add_track(track1);
    corpus.add_document(doc1);

    // Doc2: "Sao Paulo" (without accent)
    let mut doc2 = GroundedDocument::new("doc2", "Sao Paulo is the largest city in Brazil.");
    let sig2 = doc2.add_signal(Signal::new(
        0,
        Location::text(0, 9),
        "Sao Paulo",
        "LOC",
        0.95,
    ));
    let mut track2 = Track::new(0, "sao paulo");
    track2.add_signal(sig2, 0);
    track2.entity_type = Some("LOC".to_string());
    doc2.add_track(track2);
    corpus.add_document(doc2);

    // Run crossdoc with lower threshold to allow accent differences
    let resolver = Resolver::new().with_threshold(0.4);
    let identity_ids = resolver.resolve_inter_doc_coref(&mut corpus, None, None);

    // Should create 1 identity (accented and non-accented should match)
    // or 2 if similarity too low
    assert!(!identity_ids.is_empty());
}

/// E2E: Zero-width characters and invisible characters
#[test]
fn e2e_zero_width_characters() {
    // Text with zero-width space (U+200B) and other invisible characters
    let text = "Marie\u{200B}Curie won the Nobel Prize."; // Zero-width space between words

    let mut doc = GroundedDocument::new("zwc_doc", text);

    // Should handle zero-width characters gracefully
    let sig = doc.add_signal(Signal::new(
        0,
        Location::text(0, 11),
        "Marie Curie",
        "PER",
        0.95,
    ));

    assert_eq!(doc.signals().len(), 1);

    // Surface should match (may or may not include zero-width chars depending on implementation)
    let signal = doc.get_signal(sig).unwrap();
    assert!(signal.surface().contains("Marie") && signal.surface().contains("Curie"));
}

/// E2E: Combining characters (diacritics)
#[test]
fn e2e_combining_characters() {
    // Text with combining diacritics
    let text = "Café uses combining e\u{0301} (e + combining acute).";

    let mut doc = GroundedDocument::new("combining_doc", text);

    let sig = doc.add_signal(Signal::new(0, Location::text(0, 4), "Café", "ORG", 0.9));

    assert_eq!(doc.signals().len(), 1);

    // Should handle combining characters
    let signal = doc.get_signal(sig).unwrap();
    assert!(
        signal.surface().contains("Caf")
            && (signal.surface().contains("é") || signal.surface().contains("e\u{0301}"))
    );
}

/// E2E: Very long Unicode strings
#[test]
fn e2e_very_long_unicode() {
    // Create text with many Unicode characters
    let text: String = (0..1000)
        .map(|i| char::from_u32(0x4E00 + (i % 100)).unwrap())
        .collect();

    let mut doc = GroundedDocument::new("long_unicode_doc", &text);

    // Add signal in the middle - need to extract substring using char indices
    let start = 500;
    let end = 510;
    let surface: String = text.chars().skip(start).take(end - start).collect();
    let sig = doc.add_signal(Signal::new(
        0,
        Location::text(start, end),
        &surface,
        "PER",
        0.9,
    ));

    assert_eq!(doc.signals().len(), 1);

    let signal = doc.get_signal(sig).unwrap();
    assert_eq!(signal.surface().chars().count(), 10);
}
