//! Convert GLiNER character offsets to byte offsets safely for
//! multi-byte (UTF-8) text such as French legal documents.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

/// Convert a `[start_char, end_char)` character-index span into a
/// `[start_byte, end_byte)` byte span for `text`.
///
/// Returns `None` when the span is empty, inverted, or out of bounds.
pub fn char_span_to_byte_span(text: &str, start_char: usize, end_char: usize) -> Option<ByteSpan> {
    if start_char >= end_char {
        return None;
    }
    // Build a char-index → byte-offset map. The sentinel at the end is
    // `text.len()` so `map[end_char]` works when `end_char` is one past
    // the last character (i.e. the entity runs to the end of the string).
    let mut map: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    map.push(text.len());

    if start_char >= map.len() || end_char > map.len() {
        return None;
    }
    Some(ByteSpan { start: map[start_char], end: map[end_char] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_char_offsets_to_byte_offsets_for_ascii() {
        let text = "hello world";
        let span = char_span_to_byte_span(text, 6, 11).unwrap();
        assert_eq!(&text[span.start..span.end], "world");
    }

    #[test]
    fn converts_char_offsets_to_byte_offsets_for_french_text() {
        // '€' is 3 bytes in UTF-8
        let text = "Loyer annuel de 12 000 € payé à échéance.";
        let start_char = text.chars().position(|c| c == '1').unwrap();
        let end_char = start_char + "12 000 €".chars().count();

        let span = char_span_to_byte_span(text, start_char, end_char).expect("span");
        assert_eq!(&text[span.start..span.end], "12 000 €");
    }

    #[test]
    fn returns_none_for_empty_span() {
        assert!(char_span_to_byte_span("hello", 3, 3).is_none());
    }

    #[test]
    fn returns_none_for_inverted_span() {
        assert!(char_span_to_byte_span("hello", 4, 2).is_none());
    }

    #[test]
    fn returns_none_for_out_of_bounds() {
        assert!(char_span_to_byte_span("hi", 0, 10).is_none());
    }

    #[test]
    fn span_to_end_of_string() {
        let text = "abc";
        let span = char_span_to_byte_span(text, 1, 3).unwrap();
        assert_eq!(&text[span.start..span.end], "bc");
    }
}
