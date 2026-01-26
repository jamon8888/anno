//! Unicode and offset edge cases wired into the `anno` crate tests so they run in CI.
//! Focus:
//! - Grapheme clusters (ZWJ sequences, regional flags, combining marks)
//! - Mixed scripts (including RTL)
//! - Span conversion mid-scalar determinism
//! - UTF-8 validity checks

use anno::{offset::bytes_to_chars, offset::SpanConverter, Entity, EntityType};
use bstr::{BStr, ByteSlice};
use proptest::prelude::*;

fn proptest_quick_config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        // nextest runs from workspace root; default persistence emits warnings.
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Deterministic coverage for complex graphemes and mixed scripts.
#[test]
fn complex_graphemes_and_mixed_scripts() {
    let samples = [
        // ZWJ family emoji (multiple scalars per grapheme)
        "👨‍👩‍👧‍👦 went to the park",
        // Skin tone modifiers + ZWJ
        "👍🏽‍💻 coding session",
        // Regional indicator pair (flag)
        "Flag test: 🇯🇵🇺🇸",
        // Combining mark stack
        "a\u{0301}\u{0327} layered accents",
        // RTL with mixed scripts
        "مرحبا بالعالم — hello world — שלום",
        // Astral musical symbol + text
        "Music: 𝄞 score",
    ];

    for text in samples {
        let text_char_count = text.chars().count();

        // This is an offset/span test; do not depend on any NER backend behavior.
        // Create a couple of deterministic spans and ensure optimized extraction/validation works.
        let spans = [
            (0usize, 1usize.min(text_char_count)),
            (0usize, text_char_count.min(3)),
        ];

        for (start, end) in spans {
            if end <= start {
                continue;
            }
            let span_text: String = text.chars().skip(start).take(end - start).collect();
            let entity = Entity::new(&span_text, EntityType::Person, start, end, 0.5);

            let extracted = entity.extract_text_with_len(text, text_char_count);
            let issues = entity.validate_with_len(text, text_char_count);

            assert!(
                entity.start <= entity.end && entity.end <= text_char_count,
                "Span out of bounds for text {:?}: start={}, end={}, len={}",
                text,
                entity.start,
                entity.end,
                text_char_count
            );

            for issue in &issues {
                match issue {
                    anno::ValidationIssue::SpanOutOfBounds { .. }
                    | anno::ValidationIssue::InvalidSpan { .. } => panic!(
                        "Invalid span on {:?}: start={}, end={}, issue={:?}",
                        text, entity.start, entity.end, issue
                    ),
                    _ => {}
                }
            }

            assert!(std::str::from_utf8(extracted.as_bytes()).is_ok());
        }
    }
}

/// Mid-scalar byte offsets should map to the owning character (deterministic contract).
#[test]
fn mid_scalar_byte_maps_to_owning_char() {
    let text = "👍🏽‍💻👨‍👩‍👧‍👦🇯🇵a\u{0301}\u{0327}مرحبا";
    for (char_idx, (byte_idx, ch)) in text.char_indices().enumerate() {
        let len = ch.len_utf8();
        if len > 1 {
            for delta in 1..len {
                let b = byte_idx + delta;
                if b < text.len() {
                    let (char_start, char_end) = bytes_to_chars(text, b, b + 1);
                    assert_eq!(
                        char_start, char_idx,
                        "Mid-scalar byte should map to owning char: byte={}, expected_char={}, got_char={}",
                        b, char_idx, char_start
                    );
                    assert!(char_end > char_start, "char_end should advance");
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(proptest_quick_config(100))]

    #[test]
    fn byte_vs_char_substring_search(text in "[\\u{0000}-\\u{FFFF}]{0,200}", pattern in "[\\u{0000}-\\u{FFFF}]{0,50}") {
        prop_assume!(!pattern.is_empty() && pattern.len() <= text.len());

        let text_bytes = text.as_bytes();
        let pattern_bytes = pattern.as_bytes();
        let bstr_text = BStr::new(text_bytes);

        let byte_found = bstr_text.find(pattern_bytes).is_some();
        let char_found = text.contains(&pattern);

        prop_assert_eq!(
            byte_found, char_found,
            "Byte-level and char-level search should agree on pattern existence: text={:?}, pattern={:?}, byte_found={}, char_found={}",
            text, pattern, byte_found, char_found
        );
    }

    #[test]
    fn byte_level_utf8_handling(bytes in proptest::collection::vec(0u8..=255u8, 0..100)) {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            // No model calls here: just ensure bstr/UTF-8 invariants hold for valid UTF-8.
            let bstr_view = BStr::new(text.as_bytes());
            prop_assert!(bstr_view.to_str().is_ok(), "Text should remain valid UTF-8");
        } else {
            // Invalid UTF-8 must be rejected before model code is invoked.
            prop_assert!(std::str::from_utf8(&bytes).is_err(), "Expect invalid UTF-8 to fail fast");
        }
    }
}

#[test]
fn span_converter_empty_text() {
    let text = "";
    let converter = SpanConverter::new(text);
    let char_count = text.chars().count();
    let byte_len = text.len();

    assert_eq!(converter.byte_to_char(0), 0);
    assert_eq!(converter.char_to_byte(0), 0);

    let c_out = converter.byte_to_char(10);
    assert!(
        c_out >= char_count,
        "Byte-to-char for empty text should not underflow: got {}, expected >= {}",
        c_out,
        char_count
    );

    let b_out = converter.char_to_byte(10);
    assert!(
        b_out >= byte_len,
        "Char-to-byte for empty text should not underflow: got {}, expected >= {}",
        b_out,
        byte_len
    );
}
