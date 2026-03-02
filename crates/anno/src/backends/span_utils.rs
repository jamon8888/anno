//! Shared span tensor utilities for GLiNER-family models.
//!
//! GLiNER and NuNER both use span-based architectures where the model scores
//! every possible span (up to `MAX_SPAN_WIDTH` words) against each entity type.
//! This module provides common utilities for generating span tensors and
//! decoding span-based outputs.
//!
//! # Architecture Overview
//!
//! ```text
//! Input Text: "Steve Jobs founded Apple"
//!             [0]    [1]     [2]    [3]
//!
//! Span Grid (MAX_SPAN_WIDTH=3):
//!
//!   Start=0: (0,0) (0,1) (0,2)  → "Steve", "Steve Jobs", "Steve Jobs founded"
//!   Start=1: (1,1) (1,2) (1,3)  → "Jobs", "Jobs founded", "Jobs founded Apple"
//!   Start=2: (2,2) (2,3)        → "founded", "founded Apple"
//!   Start=3: (3,3)              → "Apple"
//!
//! Total spans: sum(min(MAX_WIDTH, remaining_words)) for each start position
//! ```
//!
//! # Span Tensor Format
//!
//! - `span_idx`: `[num_spans, 2]` - (start_word, end_word) indices for each span
//! - `span_mask`: `[num_spans]` - boolean mask indicating valid spans
//!
//! # Output Decoding
//!
//! Model output shape: `[batch, num_words, max_width, num_entity_types]`
//!
//! Each cell `output[0][start][width][type_idx]` contains the score for:
//! - Span starting at word `start`
//! - With width `width + 1` words (i.e., ends at word `start + width`)
//! - Being an entity of type `type_idx`
//!
//! # References
//!
//! - GLiNER paper: "GLiNER: Generalist and Lightweight Model for Named Entity Recognition"
//! - NuNER: Token-based variant using same span representation
//! - Community GLiNER implementations (for span layout conventions)

use anno_core::EntityCategory;
use crate::{Entity, EntityType, Error, Result};

/// Default maximum span width (in words) for GLiNER-family models.
///
/// This matches the training configuration of most GLiNER/NuNER models.
/// Spans longer than this are not considered by the model.
pub const DEFAULT_MAX_SPAN_WIDTH: usize = 12;

/// Configuration for span-based NER decoding.
#[derive(Debug, Clone)]
pub struct SpanConfig {
    /// Maximum span width in words.
    pub max_span_width: usize,
    /// Confidence threshold for entity extraction.
    pub threshold: f32,
}

impl Default for SpanConfig {
    fn default() -> Self {
        Self {
            max_span_width: DEFAULT_MAX_SPAN_WIDTH,
            threshold: 0.5,
        }
    }
}

/// Generate span tensors for ONNX model input.
///
/// Creates the `span_idx` and `span_mask` tensors required by GLiNER-family models.
///
/// # Arguments
///
/// * `num_words` - Number of words in the input text
/// * `max_width` - Maximum span width to consider
///
/// # Returns
///
/// A tuple of:
/// - `span_idx`: Flattened `[num_spans * 2]` array of (start, end) pairs
/// - `span_mask`: `[num_spans]` boolean mask of valid spans
///
/// # Example
///
/// ```rust
/// use anno::backends::span_utils::make_span_tensors;
///
/// let (span_idx, span_mask) = make_span_tensors(4, 3);
///
/// // First span: (0, 0) -> "word 0"
/// assert_eq!(span_idx[0], 0); // start
/// assert_eq!(span_idx[1], 0); // end (exclusive would be 1, but GLiNER uses inclusive)
/// assert!(span_mask[0]);
/// ```
pub fn make_span_tensors(num_words: usize, max_width: usize) -> (Vec<i64>, Vec<bool>) {
    // Calculate total number of spans with overflow protection
    let num_spans = match num_words.checked_mul(max_width) {
        Some(v) => v,
        None => {
            log::warn!(
                "[span_utils] Span count overflow: {} words * {} max_width, returning empty",
                num_words,
                max_width
            );
            return (Vec::new(), Vec::new());
        }
    };

    let span_idx_len = match num_spans.checked_mul(2) {
        Some(v) => v,
        None => {
            log::warn!(
                "[span_utils] Span idx length overflow: {} * 2, returning empty",
                num_spans
            );
            return (Vec::new(), Vec::new());
        }
    };

    let mut span_idx: Vec<i64> = vec![0; span_idx_len];
    let mut span_mask: Vec<bool> = vec![false; num_spans];

    for start in 0..num_words {
        let remaining_width = num_words - start;
        let actual_max_width = max_width.min(remaining_width);

        for width in 0..actual_max_width {
            // Calculate linear index with overflow protection
            let dim = match start.checked_mul(max_width) {
                Some(v) => match v.checked_add(width) {
                    Some(d) => d,
                    None => continue,
                },
                None => continue,
            };

            // Bounds check before array access
            if let Some(dim2) = dim.checked_mul(2) {
                if dim2 + 1 < span_idx_len && dim < num_spans {
                    span_idx[dim2] = start as i64;
                    // End offset: start + width gives the last word index (inclusive)
                    span_idx[dim2 + 1] = (start + width) as i64;
                    span_mask[dim] = true;
                }
            }
        }
    }

    (span_idx, span_mask)
}

/// Calculate word positions (byte offsets) in the original text.
///
/// Maps word indices to their (start, end) byte positions in the source text.
///
/// # Arguments
///
/// * `text` - The original text
/// * `words` - Whitespace-split words
///
/// # Returns
///
/// A vector of (start_byte, end_byte) positions for each word.
///
/// # Errors
///
/// Returns an error if any word cannot be found at the expected position.
pub fn calculate_word_positions(text: &str, words: &[&str]) -> Result<Vec<(usize, usize)>> {
    let mut positions = Vec::with_capacity(words.len());
    let mut pos = 0;

    for (idx, word) in words.iter().enumerate() {
        // Find word starting from current position
        if let Some(rel_start) = text[pos..].find(word) {
            let abs_start = pos + rel_start;
            let abs_end = abs_start + word.len();

            // Validate: words should be in order
            if !positions.is_empty() {
                let (_prev_start, prev_end) = positions[positions.len() - 1];
                if abs_start < prev_end {
                    log::warn!(
                        "[span_utils] Word '{}' at {} overlaps with previous word ending at {}",
                        word,
                        abs_start,
                        prev_end
                    );
                }
            }

            positions.push((abs_start, abs_end));
            pos = abs_end;
        } else {
            return Err(Error::Parse(format!(
                "Word '{}' (index {}) not found in text starting at position {}",
                word, idx, pos
            )));
        }
    }

    Ok(positions)
}

/// Extract entity span from text given word positions.
///
/// # Arguments
///
/// * `text` - The original text
/// * `word_positions` - Byte positions of each word
/// * `start_word` - Starting word index (inclusive)
/// * `end_word` - Ending word index (inclusive)
///
/// # Returns
///
/// The text span and its byte range, or None if indices are invalid.
pub fn extract_span<'a>(
    text: &'a str,
    word_positions: &[(usize, usize)],
    start_word: usize,
    end_word: usize,
) -> Option<(&'a str, usize, usize)> {
    let start_pos = word_positions.get(start_word)?.0;
    let end_pos = word_positions.get(end_word)?.1;

    if start_pos > end_pos || end_pos > text.len() {
        return None;
    }

    Some((&text[start_pos..end_pos], start_pos, end_pos))
}

/// Decode span-based model output into entities.
///
/// This is the core decoding function for GLiNER-family models.
///
/// # Arguments
///
/// * `output_data` - Flattened model output tensor
/// * `shape` - Output shape `[batch, num_words, max_width, num_classes]`
/// * `text` - Original input text
/// * `text_words` - Whitespace-split words
/// * `entity_types` - Entity type labels
/// * `config` - Decoding configuration
///
/// # Returns
///
/// A vector of extracted entities.
///
/// # Output Format
///
/// The model output has shape `[batch, num_words, max_width, num_classes]`:
/// - `batch`: Always 1 for single-text inference
/// - `num_words`: Number of words in the input
/// - `max_width`: Maximum span width (DEFAULT_MAX_SPAN_WIDTH)
/// - `num_classes`: Number of entity types
///
/// Each cell contains the score for a (start, width, type) triple.
pub fn decode_span_output(
    output_data: &[f32],
    shape: &[i64],
    text: &str,
    text_words: &[&str],
    entity_types: &[&str],
    config: &SpanConfig,
) -> Result<Vec<Entity>> {
    // Validate shape
    if shape.len() < 3 {
        return Err(Error::Parse(format!(
            "Expected at least 3D output, got shape {:?}",
            shape
        )));
    }

    // Parse shape dimensions
    let (out_num_words, out_max_width, num_classes) = if shape.len() == 4 {
        // Standard GLiNER format: [batch, num_words, max_width, num_classes]
        (shape[1] as usize, shape[2] as usize, shape[3] as usize)
    } else if shape.len() == 3 {
        // Squeezed batch dimension: [num_words, max_width, num_classes]
        (shape[0] as usize, shape[1] as usize, shape[2] as usize)
    } else {
        return Err(Error::Parse(format!(
            "Unexpected output shape: {:?}",
            shape
        )));
    };

    log::debug!(
        "[span_utils] Decoding: words={}, max_width={}, classes={}, data_len={}",
        out_num_words,
        out_max_width,
        num_classes,
        output_data.len()
    );

    // Calculate word positions
    let word_positions = calculate_word_positions(text, text_words)?;
    // `calculate_word_positions` (and most tokenizer/regex style tooling) yields byte offsets.
    // `Entity` offsets are defined as character offsets, so convert at construction time.
    let span_converter = crate::offset::SpanConverter::new(text);

    // Validate dimensions match
    let num_text_words = text_words.len();
    if out_num_words < num_text_words {
        log::warn!(
            "[span_utils] Output has fewer words ({}) than input ({})",
            out_num_words,
            num_text_words
        );
    }

    let mut entities = Vec::with_capacity(32);

    // Iterate over all valid spans
    for start in 0..num_text_words.min(out_num_words) {
        for width in 0..config.max_span_width.min(out_max_width) {
            let end = start + width;
            if end >= num_text_words {
                break;
            }

            // Find best entity type for this span
            let base_idx = (start * out_max_width * num_classes) + (width * num_classes);

            let mut best_score = config.threshold;
            let mut best_type_idx = None;

            for type_idx in 0..num_classes.min(entity_types.len()) {
                let score = output_data.get(base_idx + type_idx).copied().unwrap_or(0.0);

                if score > best_score {
                    best_score = score;
                    best_type_idx = Some(type_idx);
                }
            }

            // Create entity if score exceeds threshold
            if let Some(type_idx) = best_type_idx {
                if let Some((span_text, start_byte, end_byte)) =
                    extract_span(text, &word_positions, start, end)
                {
                    let entity_type = map_label_to_entity_type(entity_types[type_idx]);
                    let mut entity = Entity::new(
                        span_text,
                        entity_type,
                        span_converter.byte_to_char(start_byte),
                        span_converter.byte_to_char(end_byte),
                        best_score as f64,
                    );
                    entity.provenance =
                        Some(crate::Provenance::ml("span-decoder", best_score as f64));
                    entities.push(entity);
                }
            }
        }
    }

    // Sort by position and remove overlaps (keep highest confidence)
    entities.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            // Use total ordering to avoid `partial_cmp` returning None on NaN.
            .then_with(|| b.confidence.total_cmp(&a.confidence))
    });

    // Remove overlapping entities (keep first = highest confidence due to sort)
    let mut filtered = Vec::with_capacity(entities.len());
    for entity in entities {
        let overlaps = filtered
            .iter()
            .any(|e: &Entity| ranges_overlap(e.start, e.end, entity.start, entity.end));
        if !overlaps {
            filtered.push(entity);
        }
    }

    Ok(filtered)
}

/// Check if two ranges overlap.
#[inline]
fn ranges_overlap(start1: usize, end1: usize, start2: usize, end2: usize) -> bool {
    start1 < end2 && start2 < end1
}

/// Map entity type label string to EntityType enum.
///
/// Handles common label variations (case-insensitive).
pub fn map_label_to_entity_type(label: &str) -> EntityType {
    match label.to_lowercase().as_str() {
        "person" | "per" => EntityType::Person,
        "organization" | "org" | "company" | "corp" => EntityType::Organization,
        "location" | "loc" | "place" | "gpe" => EntityType::Location,
        "date" => EntityType::Date,
        "datetime" => EntityType::Date,
        "time" => EntityType::Time,
        "money" | "currency" => EntityType::Money,
        "monetary" => EntityType::Money,
        "percent" | "percentage" => EntityType::Percent,
        "email" => EntityType::Email,
        "phone" => EntityType::Phone,
        "url" => EntityType::Url,
        "quantity" => EntityType::Quantity,
        "measure" => EntityType::Quantity,
        "cardinal" => EntityType::Cardinal,
        "number" | "num" => EntityType::Cardinal,
        "ordinal" => EntityType::Ordinal,
        "event" => EntityType::Custom { name: "EVENT".to_string(), category: EntityCategory::Creative },
        "product" | "prod" => EntityType::Custom { name: "PRODUCT".to_string(), category: EntityCategory::Creative },
        "work_of_art" | "work" => EntityType::Custom { name: "WORK_OF_ART".to_string(), category: EntityCategory::Creative },
        "law" | "legal" => EntityType::Custom { name: "LAW".to_string(), category: EntityCategory::Creative },
        "language" | "lang" => EntityType::Custom { name: "LANGUAGE".to_string(), category: EntityCategory::Creative },
        "norp" => EntityType::Custom { name: "NORP".to_string(), category: EntityCategory::Agent }, // Nationalities, religions, political groups
        "fac" | "facility" => EntityType::Custom { name: "FACILITY".to_string(), category: EntityCategory::Organization },
        // Fine-grained / CNER-inspired labels
        "animal" => EntityType::Custom { name: "ANIMAL".to_string(), category: EntityCategory::Misc },
        "biology" => EntityType::Custom { name: "BIOLOGY".to_string(), category: EntityCategory::Misc },
        "celestial" => EntityType::Custom { name: "CELESTIAL".to_string(), category: EntityCategory::Place },
        "culture" => EntityType::Custom { name: "CULTURE".to_string(), category: EntityCategory::Creative },
        "discipline" => EntityType::Custom { name: "DISCIPLINE".to_string(), category: EntityCategory::Creative },
        "disease" => EntityType::Custom { name: "DISEASE".to_string(), category: EntityCategory::Misc },
        "feeling" => EntityType::Custom { name: "FEELING".to_string(), category: EntityCategory::Misc },
        "food" => EntityType::Custom { name: "FOOD".to_string(), category: EntityCategory::Misc },
        "group" => EntityType::Custom { name: "GROUP".to_string(), category: EntityCategory::Agent },
        "instrument" => EntityType::Custom { name: "INSTRUMENT".to_string(), category: EntityCategory::Misc },
        "media" => EntityType::Custom { name: "MEDIA".to_string(), category: EntityCategory::Creative },
        "asset" => EntityType::Custom { name: "ASSET".to_string(), category: EntityCategory::Misc },
        "artifact" => EntityType::Custom { name: "ARTIFACT".to_string(), category: EntityCategory::Misc },
        "part" => EntityType::Custom { name: "PART".to_string(), category: EntityCategory::Misc },
        "physical_phenomenon" | "physical" => EntityType::Custom { name: "PHYSICAL_PHENOMENON".to_string(), category: EntityCategory::Misc },
        "plant" => EntityType::Custom { name: "PLANT".to_string(), category: EntityCategory::Misc },
        "property" => EntityType::Custom { name: "PROPERTY".to_string(), category: EntityCategory::Misc },
        "psych" => EntityType::Custom { name: "PSYCH".to_string(), category: EntityCategory::Misc },
        "relation" => EntityType::Custom { name: "RELATION".to_string(), category: EntityCategory::Relation },
        "struct" => EntityType::Custom { name: "STRUCT".to_string(), category: EntityCategory::Misc },
        "substance" => EntityType::Custom { name: "SUBSTANCE".to_string(), category: EntityCategory::Misc },
        "super" | "supernatural" => EntityType::Custom { name: "SUPER".to_string(), category: EntityCategory::Misc },
        "vehicle" | "vehi" => EntityType::Custom { name: "VEHICLE".to_string(), category: EntityCategory::Misc },
        _ => EntityType::Custom { name: label.to_uppercase(), category: EntityCategory::Misc },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_span_tensors_basic() {
        let (span_idx, span_mask) = make_span_tensors(3, 2);

        // 3 words * 2 max_width = 6 spans
        assert_eq!(span_mask.len(), 6);
        assert_eq!(span_idx.len(), 12);

        // First span: word 0, width 0 → (0, 0)
        assert!(span_mask[0]);
        assert_eq!(span_idx[0], 0);
        assert_eq!(span_idx[1], 0);

        // Second span: word 0, width 1 → (0, 1)
        assert!(span_mask[1]);
        assert_eq!(span_idx[2], 0);
        assert_eq!(span_idx[3], 1);
    }

    #[test]
    fn test_make_span_tensors_overflow_protection() {
        // Very large input shouldn't panic
        let (span_idx, span_mask) = make_span_tensors(usize::MAX / 2, DEFAULT_MAX_SPAN_WIDTH);
        // Should return empty due to overflow
        assert!(span_idx.is_empty());
        assert!(span_mask.is_empty());
    }

    #[test]
    fn test_calculate_word_positions() {
        let text = "Steve Jobs founded Apple";
        let words: Vec<&str> = text.split_whitespace().collect();

        let positions = calculate_word_positions(text, &words).unwrap();

        assert_eq!(positions.len(), 4);
        assert_eq!(positions[0], (0, 5)); // "Steve"
        assert_eq!(positions[1], (6, 10)); // "Jobs"
        assert_eq!(positions[2], (11, 18)); // "founded"
        assert_eq!(positions[3], (19, 24)); // "Apple"
    }

    #[test]
    fn test_extract_span() {
        let text = "Steve Jobs founded Apple";
        let positions = vec![(0, 5), (6, 10), (11, 18), (19, 24)];

        // Single word span
        let (span, start, end) = extract_span(text, &positions, 0, 0).unwrap();
        assert_eq!(span, "Steve");
        assert_eq!((start, end), (0, 5));

        // Two-word span
        let (span, start, end) = extract_span(text, &positions, 0, 1).unwrap();
        assert_eq!(span, "Steve Jobs");
        assert_eq!((start, end), (0, 10));

        // Three-word span
        let (span, start, end) = extract_span(text, &positions, 1, 3).unwrap();
        assert_eq!(span, "Jobs founded Apple");
        assert_eq!((start, end), (6, 24));
    }

    #[test]
    fn test_map_label_to_entity_type() {
        assert_eq!(map_label_to_entity_type("person"), EntityType::Person);
        assert_eq!(map_label_to_entity_type("PER"), EntityType::Person);
        assert_eq!(
            map_label_to_entity_type("organization"),
            EntityType::Organization
        );
        assert_eq!(map_label_to_entity_type("ORG"), EntityType::Organization);
        assert_eq!(map_label_to_entity_type("location"), EntityType::Location);
        assert_eq!(map_label_to_entity_type("GPE"), EntityType::Location);
        assert_eq!(
            map_label_to_entity_type("custom_type"),
            EntityType::Custom { name: "CUSTOM_TYPE".to_string(), category: EntityCategory::Misc }
        );
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap(0, 10, 5, 15)); // Partial overlap
        assert!(ranges_overlap(0, 10, 0, 5)); // Contained
        assert!(ranges_overlap(5, 15, 0, 10)); // Partial overlap (reversed)
        assert!(!ranges_overlap(0, 5, 10, 15)); // No overlap
        assert!(!ranges_overlap(0, 5, 5, 10)); // Adjacent (not overlapping)
    }

    // ---------------------------------------------------------------
    // Additional tests
    // ---------------------------------------------------------------

    #[test]
    fn test_make_span_tensors_zero_words() {
        let (span_idx, span_mask) = make_span_tensors(0, 5);
        assert!(span_idx.is_empty());
        assert!(span_mask.is_empty());
    }

    #[test]
    fn test_make_span_tensors_zero_width() {
        let (span_idx, span_mask) = make_span_tensors(4, 0);
        assert!(span_idx.is_empty());
        assert!(span_mask.is_empty());
    }

    #[test]
    fn test_make_span_tensors_single_word() {
        let (span_idx, span_mask) = make_span_tensors(1, 3);
        // 1 word * 3 max_width = 3 slots, but only 1 valid span (0,0)
        assert_eq!(span_mask.len(), 3);
        assert_eq!(span_idx.len(), 6);

        // Only the first span (word 0, width 0) is valid
        assert!(span_mask[0]);
        assert_eq!(span_idx[0], 0);
        assert_eq!(span_idx[1], 0);

        // Remaining slots are invalid (width exceeds word count)
        assert!(!span_mask[1]);
        assert!(!span_mask[2]);
    }

    #[test]
    fn test_make_span_tensors_width_exceeds_words() {
        // 2 words, max_width 5: only 3 valid spans (0,0), (0,1), (1,1)
        let (span_idx, span_mask) = make_span_tensors(2, 5);
        assert_eq!(span_mask.len(), 10); // 2 * 5

        // Word 0: width 0 -> (0,0), width 1 -> (0,1)
        assert!(span_mask[0]);
        assert_eq!((span_idx[0], span_idx[1]), (0, 0));
        assert!(span_mask[1]);
        assert_eq!((span_idx[2], span_idx[3]), (0, 1));
        // Word 0: widths 2..4 invalid
        assert!(!span_mask[2]);
        assert!(!span_mask[3]);
        assert!(!span_mask[4]);

        // Word 1: width 0 -> (1,1)
        assert!(span_mask[5]);
        assert_eq!((span_idx[10], span_idx[11]), (1, 1));
        // Word 1: widths 1..4 invalid
        assert!(!span_mask[6]);
    }

    #[test]
    fn test_make_span_tensors_valid_span_count() {
        // For n words and max_width w, the number of valid spans is
        // sum_{i=0}^{n-1} min(w, n-i).
        let (_, mask) = make_span_tensors(4, 3);
        let valid = mask.iter().filter(|&&v| v).count();
        // start=0: min(3,4)=3, start=1: min(3,3)=3, start=2: min(3,2)=2, start=3: min(3,1)=1
        assert_eq!(valid, 3 + 3 + 2 + 1);
    }

    #[test]
    fn test_calculate_word_positions_unicode() {
        let text = "le cafe\u{0301} cou\u{0302}te cher";
        // "le café coûte cher" with combining marks
        let words: Vec<&str> = text.split_whitespace().collect();
        let positions = calculate_word_positions(text, &words).unwrap();

        assert_eq!(positions.len(), 4);
        // "le" at bytes 0..2
        assert_eq!(positions[0], (0, 2));
        // Verify each word round-trips
        for (i, word) in words.iter().enumerate() {
            let (s, e) = positions[i];
            assert_eq!(&text[s..e], *word);
        }
    }

    #[test]
    fn test_calculate_word_positions_multiple_spaces() {
        let text = "hello   world";
        let words: Vec<&str> = text.split_whitespace().collect();
        let positions = calculate_word_positions(text, &words).unwrap();

        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0], (0, 5)); // "hello"
        assert_eq!(positions[1], (8, 13)); // "world"
    }

    #[test]
    fn test_calculate_word_positions_missing_word() {
        let text = "hello world";
        let words = vec!["hello", "missing"];
        let result = calculate_word_positions(text, &words);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_span_out_of_bounds() {
        let positions = vec![(0, 5), (6, 10)];
        let text = "hello world";

        // start_word beyond positions length
        assert!(extract_span(text, &positions, 5, 5).is_none());

        // end_word beyond positions length
        assert!(extract_span(text, &positions, 0, 5).is_none());
    }

    #[test]
    fn test_extract_span_last_word() {
        let text = "one two three";
        let positions = vec![(0, 3), (4, 7), (8, 13)];

        let (span, start, end) = extract_span(text, &positions, 2, 2).unwrap();
        assert_eq!(span, "three");
        assert_eq!((start, end), (8, 13));
    }

    #[test]
    fn test_ranges_overlap_identical() {
        assert!(ranges_overlap(3, 7, 3, 7));
    }

    #[test]
    fn test_ranges_overlap_fully_contained() {
        // Inner range fully inside outer range
        assert!(ranges_overlap(0, 20, 5, 10));
        assert!(ranges_overlap(5, 10, 0, 20));
    }

    #[test]
    fn test_ranges_overlap_empty_and_adjacent() {
        // Empty range (start == end): start1 < end2 && start2 < end1
        // (5,5) vs (5,5): 5 < 5 is false => no overlap
        assert!(!ranges_overlap(5, 5, 5, 5));
        // (5,5) vs (5,10): 5 < 10 && 5 < 5 => false (second condition)
        assert!(!ranges_overlap(5, 5, 5, 10));
        // (5,10) vs (5,5): 5 < 5 && 5 < 10 => false (first condition)
        assert!(!ranges_overlap(5, 10, 5, 5));
        // (0,10) vs (3,3): 0 < 3 && 3 < 10 => true (zero-width point inside range)
        assert!(ranges_overlap(0, 10, 3, 3));

        // Adjacent ranges (half-open convention: [0,5) and [5,10))
        assert!(!ranges_overlap(0, 5, 5, 10));
        assert!(!ranges_overlap(5, 10, 0, 5));
    }

    #[test]
    fn test_map_label_to_entity_type_case_insensitivity() {
        assert_eq!(map_label_to_entity_type("Person"), EntityType::Person);
        assert_eq!(map_label_to_entity_type("PERSON"), EntityType::Person);
        assert_eq!(map_label_to_entity_type("PerSoN"), EntityType::Person);
        assert_eq!(
            map_label_to_entity_type("ORGANIZATION"),
            EntityType::Organization
        );
        assert_eq!(map_label_to_entity_type("Loc"), EntityType::Location);
    }

    #[test]
    fn test_map_label_to_entity_type_extended_labels() {
        assert_eq!(map_label_to_entity_type("date"), EntityType::Date);
        assert_eq!(map_label_to_entity_type("datetime"), EntityType::Date);
        assert_eq!(map_label_to_entity_type("time"), EntityType::Time);
        assert_eq!(map_label_to_entity_type("money"), EntityType::Money);
        assert_eq!(map_label_to_entity_type("currency"), EntityType::Money);
        assert_eq!(map_label_to_entity_type("monetary"), EntityType::Money);
        assert_eq!(map_label_to_entity_type("percent"), EntityType::Percent);
        assert_eq!(map_label_to_entity_type("percentage"), EntityType::Percent);
        assert_eq!(map_label_to_entity_type("email"), EntityType::Email);
        assert_eq!(map_label_to_entity_type("phone"), EntityType::Phone);
        assert_eq!(map_label_to_entity_type("url"), EntityType::Url);
        assert_eq!(map_label_to_entity_type("quantity"), EntityType::Quantity);
        assert_eq!(map_label_to_entity_type("cardinal"), EntityType::Cardinal);
        assert_eq!(map_label_to_entity_type("number"), EntityType::Cardinal);
        assert_eq!(map_label_to_entity_type("ordinal"), EntityType::Ordinal);
        assert_eq!(
            map_label_to_entity_type("event"),
            EntityType::Custom { name: "EVENT".to_string(), category: EntityCategory::Creative }
        );
        assert_eq!(
            map_label_to_entity_type("product"),
            EntityType::Custom { name: "PRODUCT".to_string(), category: EntityCategory::Creative }
        );
        assert_eq!(
            map_label_to_entity_type("norp"),
            EntityType::Custom { name: "NORP".to_string(), category: EntityCategory::Agent }
        );
        assert_eq!(
            map_label_to_entity_type("facility"),
            EntityType::Custom { name: "FACILITY".to_string(), category: EntityCategory::Organization }
        );
    }

    #[test]
    fn test_extract_span_unicode_multibyte() {
        // Text with multi-byte characters: each CJK char is 3 bytes in UTF-8
        let text = "\u{6771}\u{4eac} \u{304f} \u{91ce}";
        // "東京 く 野" -- 3 words
        let words: Vec<&str> = text.split_whitespace().collect();
        let positions = calculate_word_positions(text, &words).unwrap();

        // Verify byte positions account for multi-byte chars
        // "東京" = 6 bytes (2 chars * 3 bytes each)
        assert_eq!(positions[0], (0, 6));

        // Full span from first to last word
        let (span, s, e) = extract_span(text, &positions, 0, 2).unwrap();
        assert_eq!(span, text);
        assert_eq!(s, 0);
        assert_eq!(e, text.len());
    }

    #[test]
    fn test_span_config_default() {
        let config = SpanConfig::default();
        assert_eq!(config.max_span_width, DEFAULT_MAX_SPAN_WIDTH);
        assert!((config.threshold - 0.5).abs() < f32::EPSILON);
    }
}
