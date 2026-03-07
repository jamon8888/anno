//! Chunking helpers for long text.
//!
//! **Status**:
//! - By default, this module provides a lightweight **rule-based** chunker (paragraph boundaries
//!   + size limits + overlap).
//! - With the `semantic-chunking` feature enabled, this module additionally provides a
//!   sentence-level similarity chunker (token-based Jaccard similarity; no embedding model
//!   required).
//!
//! This keeps chunking behavior explicit without implying that embeddings are in use.
//!
//! # Example
//!
//! ```rust,ignore
//! use anno::backends::semantic_chunking::{SemanticChunker, SemanticChunkConfig};
//!
//! let config = SemanticChunkConfig::default();
//! let chunker = anno::backends::semantic_chunking::create_semantic_chunker(config)?;
//! let chunks = chunker.chunk(long_text, Some("en"))?;
//!
//! for chunk in chunks {
//!     println!("Chunk: {} ({} chars)", chunk.text, chunk.text.len());
//! }
//! ```

use crate::{Language, Result};
#[cfg(feature = "semantic-chunking")]
use std::collections::BTreeSet;

/// Configuration for semantic chunking.
#[derive(Debug, Clone)]
pub struct SemanticChunkConfig {
    /// Target chunk size in characters (soft limit)
    pub target_size: usize,
    /// Minimum chunk size in characters (hard limit)
    pub min_size: usize,
    /// Maximum chunk size in characters (hard limit)
    pub max_size: usize,
    /// Similarity threshold for chunk boundaries (0.0-1.0)
    /// Lower = more chunks, Higher = fewer chunks
    pub similarity_threshold: f32,
    /// Overlap between chunks in characters
    pub overlap: usize,
    /// Use sentence boundaries as fallback when similarity is ambiguous
    pub fallback_to_sentences: bool,
}

impl Default for SemanticChunkConfig {
    fn default() -> Self {
        Self {
            target_size: 10_000,
            min_size: 1_000,
            max_size: 20_000,
            similarity_threshold: 0.7,
            overlap: 200,
            fallback_to_sentences: true,
        }
    }
}

impl SemanticChunkConfig {
    /// Create config optimized for long documents.
    pub fn long_document() -> Self {
        Self {
            target_size: 50_000,
            min_size: 5_000,
            max_size: 100_000,
            similarity_threshold: 0.75,
            overlap: 500,
            fallback_to_sentences: true,
        }
    }

    /// Create config for coreference resolution (smaller chunks, higher similarity).
    pub fn coreference() -> Self {
        Self {
            target_size: 5_000,
            min_size: 500,
            max_size: 10_000,
            similarity_threshold: 0.8, // Higher = keep related mentions together
            overlap: 300,
            fallback_to_sentences: true,
        }
    }
}

/// A semantically coherent chunk of text.
#[derive(Debug, Clone)]
pub struct SemanticChunk {
    /// The text content of this chunk
    pub text: String,
    /// Starting character offset in original text
    pub start: usize,
    /// Ending character offset in original text
    pub end: usize,
    /// Optional topic label (if available)
    pub topic: Option<String>,
    /// Semantic similarity score with previous chunk (if available)
    pub similarity_to_prev: Option<f32>,
}

/// Trait for semantic chunking strategies.
pub trait SemanticChunker: Send + Sync {
    /// Chunk text based on semantic similarity.
    ///
    /// Returns chunks sorted by position in the original text.
    fn chunk(&self, text: &str, language: Option<Language>) -> Result<Vec<SemanticChunk>>;
}

/// Simple rule-based semantic chunker (fallback when embeddings unavailable).
///
/// Uses paragraph boundaries and sentence clustering as a lightweight alternative
/// to embedding-based chunking.
#[derive(Debug)]
pub struct RuleBasedSemanticChunker {
    config: SemanticChunkConfig,
}

impl RuleBasedSemanticChunker {
    /// Create a new rule-based semantic chunker.
    pub fn new(config: SemanticChunkConfig) -> Self {
        Self { config }
    }
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    // Map char offsets (Unicode scalar index) -> byte offsets.
    //
    // We store the byte index of each char boundary, plus a final sentinel at text.len().
    let mut map = Vec::with_capacity(text.chars().count().saturating_add(1));
    for (b, _) in text.char_indices() {
        map.push(b);
    }
    map.push(text.len());
    map
}

fn byte_at_char(char_to_byte: &[usize], char_idx: usize) -> usize {
    // Safe-ish clamp: callers should only provide 0..=len_chars.
    if char_to_byte.is_empty() {
        return 0;
    }
    let i = char_idx.min(char_to_byte.len() - 1);
    char_to_byte[i]
}

fn paragraph_ranges(text: &str) -> Vec<(usize, usize)> {
    // Paragraphs are groups of non-blank lines separated by at least one blank line.
    //
    // This is language-agnostic but formatting-dependent: it assumes newline structure is meaningful.
    // We define "blank" as a line that contains only whitespace (spaces/tabs) plus newline markers.
    //
    // Returned ranges are in character offsets [start, end), preserving original text.
    let mut out = Vec::new();
    let mut para_start: Option<usize> = None;
    let mut line_start = 0usize;
    let mut line_has_non_ws = false;

    let mut i = 0usize;
    for c in text.chars() {
        match c {
            '\n' => {
                // End of line at char offset i (exclusive of '\n'; the newline itself is part of
                // the source text but not part of the line content).
                if line_has_non_ws {
                    if para_start.is_none() {
                        para_start = Some(line_start);
                    }
                } else if let Some(ps) = para_start {
                    // Blank line closes paragraph at the start of this blank line.
                    out.push((ps, line_start));
                    para_start = None;
                }
                i += 1;
                line_start = i;
                line_has_non_ws = false;
            }
            '\r' => {
                // Treat CR as whitespace. If the text uses CRLF, the '\n' branch will close lines.
                i += 1;
            }
            ' ' | '\t' => {
                i += 1;
            }
            _ => {
                line_has_non_ws = true;
                i += 1;
            }
        }
    }

    // Final line: if it had content, ensure paragraph is opened.
    if line_has_non_ws && para_start.is_none() {
        para_start = Some(line_start);
    }
    if let Some(ps) = para_start {
        out.push((ps, i));
    }
    out
}

impl SemanticChunker for RuleBasedSemanticChunker {
    fn chunk(&self, text: &str, language: Option<Language>) -> Result<Vec<SemanticChunk>> {
        let _ = language; // Acknowledge parameter for future use

        if text.is_empty() {
            return Ok(Vec::new());
        }

        let char_to_byte = char_to_byte_map(text);
        let text_len_chars = char_to_byte.len().saturating_sub(1);

        // Paragraph detection is formatting-based; if we can't detect paragraphs, fall back to a
        // single range over the whole document.
        let mut paras = paragraph_ranges(text);
        if paras.is_empty() {
            paras = vec![(0, text_len_chars)];
        }

        // Build chunk ranges in char offsets.
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut cur_start: Option<usize> = None;
        let mut cur_end: usize = 0;

        for (p_start, p_end) in paras {
            if p_end <= p_start {
                continue;
            }
            if cur_start.is_none() {
                cur_start = Some(p_start);
                cur_end = p_start;
            }

            // Would adding this paragraph exceed max_size?
            let next_end = cur_end.max(p_end);
            if let Some(cs) = cur_start {
                let cur_len = next_end.saturating_sub(cs);
                if cur_len > self.config.max_size && cur_end > cs {
                    // Flush current chunk at cur_end.
                    ranges.push((cs, cur_end));
                    // Start new chunk with overlap.
                    let overlap_start = cur_end.saturating_sub(self.config.overlap);
                    cur_start = Some(overlap_start.min(p_start));
                    cur_end = cur_start.unwrap();
                }
            }

            // Extend current chunk to include this paragraph.
            cur_end = cur_end.max(p_end);

            // Soft flush near target size when we're already at/over target and we just ended a paragraph.
            if let Some(cs) = cur_start {
                let cur_len = cur_end.saturating_sub(cs);
                if cur_len >= self.config.target_size && cur_len >= self.config.min_size {
                    ranges.push((cs, cur_end));
                    let overlap_start = cur_end.saturating_sub(self.config.overlap);
                    cur_start = Some(overlap_start.min(cur_end));
                    cur_end = cur_start.unwrap();
                }
            }
        }

        if let Some(cs) = cur_start {
            if cur_end > cs {
                ranges.push((cs, cur_end));
            }
        }

        // Merge too-small chunks into the previous chunk by extending the previous range.
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (s, e) in ranges {
            let len = e.saturating_sub(s);
            if len < self.config.min_size && !merged.is_empty() {
                let last = merged.last_mut().unwrap();
                last.1 = last.1.max(e);
            } else {
                merged.push((s, e));
            }
        }

        // Materialize chunks from original text slices to preserve offsets/text exactly.
        let mut out = Vec::new();
        for (s, e) in merged {
            let sb = byte_at_char(&char_to_byte, s);
            let eb = byte_at_char(&char_to_byte, e);
            if eb <= sb {
                continue;
            }
            let chunk_text = text.get(sb..eb).unwrap_or("").to_string();
            if chunk_text.trim().is_empty() {
                continue;
            }
            out.push(SemanticChunk {
                text: chunk_text,
                start: s,
                end: e,
                topic: None,
                similarity_to_prev: None,
            });
        }

        Ok(out)
    }
}

/// Sentence-similarity chunker (feature = `semantic-chunking`).
///
/// Uses sentence-level similarity to identify coarse boundaries.
///
/// Despite the name, the current implementation does **not** use embeddings: it uses a
/// sentence-level token Jaccard similarity to decide boundaries. This keeps the feature gate and
/// config surface stable while avoiding heavyweight dependencies.
#[cfg(feature = "semantic-chunking")]
#[derive(Debug)]
pub struct EmbeddingSemanticChunker {
    config: SemanticChunkConfig,
    // TODO: Add embedding model when available
    // embedding_model: Box<dyn EmbeddingModel>,
}

#[cfg(feature = "semantic-chunking")]
impl EmbeddingSemanticChunker {
    /// Create a new embedding-based semantic chunker.
    pub fn new(config: SemanticChunkConfig) -> Result<Self> {
        Ok(Self { config })
    }

    fn tokenize_for_similarity(s: &str) -> BTreeSet<String> {
        // Keep this intentionally simple and dependency-light.
        //
        // - lowercase (ASCII)
        // - scrub non-alphanumeric to spaces
        // - split on whitespace
        // - drop very short tokens (noise)
        let mut t = String::with_capacity(s.len());
        for c in s.chars() {
            if c.is_alphanumeric() {
                t.push(c.to_ascii_lowercase());
            } else {
                t.push(' ');
            }
        }
        t.split_whitespace()
            .filter(|w| w.chars().count() > 2)
            .map(|w| w.to_string())
            .collect()
    }

    fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
        if a.is_empty() && b.is_empty() {
            return 1.0;
        }
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        let inter = a.intersection(b).count() as f32;
        let uni = a.union(b).count() as f32;
        if uni <= 0.0 {
            0.0
        } else {
            inter / uni
        }
    }

    fn char_to_byte_map(text: &str) -> Vec<usize> {
        super::semantic_chunking::char_to_byte_map(text)
    }

    fn byte_at_char(map: &[usize], char_idx: usize) -> usize {
        super::semantic_chunking::byte_at_char(map, char_idx)
    }

    fn split_sentences_spans(text: &str) -> Vec<(usize, usize)> {
        // Return (start_char, end_char) spans for coarse sentence segments.
        let terminators = [
            '.', '!', '?', // Latin
            '。', '！', '？', // CJK
            '؟', '۔', // Arabic/Urdu
            '।', // Devanagari
        ];
        let mut out = Vec::new();
        let mut start = 0usize;
        let mut i = 0usize;
        for c in text.chars() {
            i += 1;
            if terminators.contains(&c) {
                if i > start {
                    out.push((start, i));
                }
                start = i;
            }
        }
        if i > start {
            out.push((start, i));
        }
        out
    }
}

#[cfg(feature = "semantic-chunking")]
impl SemanticChunker for EmbeddingSemanticChunker {
    fn chunk(&self, text: &str, language: Option<Language>) -> Result<Vec<SemanticChunk>> {
        let _ = language;
        let t = text.trim();
        if t.is_empty() {
            return Ok(vec![]);
        }

        let spans = Self::split_sentences_spans(text);
        if spans.is_empty() {
            let fallback = RuleBasedSemanticChunker::new(self.config.clone());
            return fallback.chunk(text, None);
        }

        let char_to_byte = Self::char_to_byte_map(text);

        let mut chunks: Vec<SemanticChunk> = Vec::new();
        let mut chunk_start_char = spans[0].0;
        let mut chunk_end_char = spans[0].1;
        let mut prev_sentence_tokens: Option<BTreeSet<String>> = None;
        let mut prev_chunk_similarity: Option<f32> = None;

        for (idx, (s0, s1)) in spans.iter().copied().enumerate() {
            let sent_start = s0;
            let sent_end = s1;

            let sent_bytes_start = Self::byte_at_char(&char_to_byte, sent_start);
            let sent_bytes_end = Self::byte_at_char(&char_to_byte, sent_end);
            let sent_text = text
                .get(sent_bytes_start..sent_bytes_end)
                .unwrap_or("")
                .trim();

            if sent_text.is_empty() {
                continue;
            }

            let tokens = Self::tokenize_for_similarity(sent_text);
            let sim_to_prev_sentence = prev_sentence_tokens
                .as_ref()
                .map(|p| Self::jaccard(p, &tokens));

            // Decide whether to cut before this sentence.
            if idx > 0 {
                let cur_len = chunk_end_char.saturating_sub(chunk_start_char);
                let would_len = sent_end.saturating_sub(chunk_start_char);
                let similarity_break = sim_to_prev_sentence
                    .map(|s| s < self.config.similarity_threshold)
                    .unwrap_or(false);
                let would_exceed =
                    would_len > self.config.max_size && cur_len >= self.config.min_size;

                if (similarity_break && cur_len >= self.config.min_size) || would_exceed {
                    let start_b = Self::byte_at_char(&char_to_byte, chunk_start_char);
                    let end_b = Self::byte_at_char(&char_to_byte, chunk_end_char);
                    let chunk_text = text.get(start_b..end_b).unwrap_or("").trim().to_string();
                    if !chunk_text.is_empty() {
                        chunks.push(SemanticChunk {
                            text: chunk_text,
                            start: chunk_start_char,
                            end: chunk_end_char,
                            topic: None,
                            similarity_to_prev: prev_chunk_similarity,
                        });
                    }

                    // Start new chunk, with optional overlap.
                    let overlap_start_char = chunk_end_char
                        .saturating_sub(self.config.overlap)
                        .min(sent_start);
                    chunk_start_char = overlap_start_char;
                    prev_chunk_similarity = sim_to_prev_sentence;
                }
            }

            // Extend chunk end to cover this sentence.
            chunk_end_char = sent_end;
            prev_sentence_tokens = Some(tokens);
        }

        // Final chunk.
        if chunk_end_char > chunk_start_char {
            let start_b = Self::byte_at_char(&char_to_byte, chunk_start_char);
            let end_b = Self::byte_at_char(&char_to_byte, chunk_end_char);
            let chunk_text = text.get(start_b..end_b).unwrap_or("").trim().to_string();
            if !chunk_text.is_empty() {
                chunks.push(SemanticChunk {
                    text: chunk_text,
                    start: chunk_start_char,
                    end: chunk_end_char,
                    topic: None,
                    similarity_to_prev: prev_chunk_similarity,
                });
            }
        }

        if chunks.is_empty() {
            let fallback = RuleBasedSemanticChunker::new(self.config.clone());
            return fallback.chunk(text, None);
        }

        Ok(chunks)
    }
}

/// Factory function to create appropriate chunker based on available features.
pub fn create_semantic_chunker(config: SemanticChunkConfig) -> Result<Box<dyn SemanticChunker>> {
    #[cfg(feature = "semantic-chunking")]
    {
        // Try embedding-based chunker first
        match EmbeddingSemanticChunker::new(config.clone()) {
            Ok(chunker) => Ok(Box::new(chunker)),
            Err(_) => Ok(Box::new(RuleBasedSemanticChunker::new(config))),
        }
    }

    #[cfg(not(feature = "semantic-chunking"))]
    {
        // Fall back to rule-based
        Ok(Box::new(RuleBasedSemanticChunker::new(config)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_based_chunker() {
        let config = SemanticChunkConfig {
            target_size: 100,
            min_size: 50,
            max_size: 200,
            similarity_threshold: 0.7,
            overlap: 20,
            fallback_to_sentences: true,
        };

        let chunker = RuleBasedSemanticChunker::new(config);
        let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
        let chunks = chunker.chunk(text, None).unwrap();

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].start, 0);
    }

    #[test]
    fn test_chunker_respects_min_size() {
        let config = SemanticChunkConfig {
            target_size: 1000,
            min_size: 100,
            max_size: 2000,
            similarity_threshold: 0.7,
            overlap: 50,
            fallback_to_sentences: true,
        };

        let chunker = RuleBasedSemanticChunker::new(config);
        let text = "Short.\n\nAlso short.";
        let chunks = chunker.chunk(text, None).unwrap();

        // Small chunks should be merged
        assert!(chunks.len() <= 1 || chunks[0].text.chars().count() >= 100);
    }

    // ── char_to_byte_map / byte_at_char ──────────────────────────────────

    #[test]
    fn char_to_byte_map_ascii() {
        let text = "hello";
        let map = super::char_to_byte_map(text);
        // 5 chars + sentinel = 6 entries; all byte==char for ASCII.
        assert_eq!(map, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn char_to_byte_map_multibyte() {
        // 'e', U+0301 COMBINING ACUTE (2 bytes), ' ', 'a' => 4 chars, 5 bytes total.
        let text = "e\u{0301} a";
        let map = super::char_to_byte_map(text);
        assert_eq!(map.len(), text.chars().count() + 1);
        // Sentinel must equal byte length.
        assert_eq!(*map.last().unwrap(), text.len());
    }

    #[test]
    fn char_to_byte_map_emoji() {
        // Flag emoji U+1F1FA U+1F1F8 is two chars (4 bytes each).
        let text = "\u{1F1FA}\u{1F1F8}";
        let map = super::char_to_byte_map(text);
        assert_eq!(map.len(), 3); // 2 chars + sentinel
        assert_eq!(map[0], 0);
        assert_eq!(map[1], 4);
        assert_eq!(map[2], 8);
    }

    #[test]
    fn char_to_byte_map_empty() {
        let map = super::char_to_byte_map("");
        // Just the sentinel.
        assert_eq!(map, vec![0]);
    }

    #[test]
    fn byte_at_char_clamping() {
        let map = super::char_to_byte_map("abc"); // [0,1,2,3]
                                                  // Within bounds.
        assert_eq!(super::byte_at_char(&map, 0), 0);
        assert_eq!(super::byte_at_char(&map, 3), 3); // sentinel
                                                     // Out of bounds clamps to last entry.
        assert_eq!(super::byte_at_char(&map, 100), 3);
        // Empty map returns 0.
        assert_eq!(super::byte_at_char(&[], 5), 0);
    }

    // ── paragraph_ranges ─────────────────────────────────────────────────

    #[test]
    fn paragraph_ranges_basic() {
        let text = "Hello world.\n\nSecond paragraph.\n\nThird.";
        let ranges = super::paragraph_ranges(text);
        assert_eq!(ranges.len(), 3);
        // First paragraph starts at 0.
        assert_eq!(ranges[0].0, 0);
        // The end offset is the char offset of the blank line's start (i.e. includes the
        // trailing '\n' of the content line). Verify the text round-trips correctly.
        let chars: Vec<char> = text.chars().collect();
        let first: String = chars[ranges[0].0..ranges[0].1].iter().collect();
        // "Hello world." plus trailing '\n' (the blank line closes the paragraph at line_start).
        assert_eq!(first, "Hello world.\n");
        // Third paragraph has no trailing newline.
        let third: String = chars[ranges[2].0..ranges[2].1].iter().collect();
        assert_eq!(third, "Third.");
    }

    #[test]
    fn paragraph_ranges_no_blank_lines() {
        // No blank lines => entire text is one paragraph.
        let text = "line one\nline two\nline three";
        let ranges = super::paragraph_ranges(text);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], (0, text.chars().count()));
    }

    #[test]
    fn paragraph_ranges_all_blank() {
        let text = "\n\n\n";
        let ranges = super::paragraph_ranges(text);
        assert!(ranges.is_empty());
    }

    #[test]
    fn paragraph_ranges_crlf() {
        let text = "Para one.\r\n\r\nPara two.";
        let ranges = super::paragraph_ranges(text);
        assert_eq!(ranges.len(), 2);
    }

    // ── RuleBasedSemanticChunker ──────────────────────────────────────────

    #[test]
    fn rule_chunker_empty_input() {
        let chunker = RuleBasedSemanticChunker::new(SemanticChunkConfig::default());
        let chunks = chunker.chunk("", None).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn rule_chunker_single_paragraph_under_target() {
        let config = SemanticChunkConfig {
            target_size: 1000,
            min_size: 10,
            max_size: 2000,
            overlap: 0,
            ..Default::default()
        };
        let chunker = RuleBasedSemanticChunker::new(config);
        let text = "A single paragraph that fits easily.";
        let chunks = chunker.chunk(text, None).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].end, text.chars().count());
    }

    #[test]
    fn rule_chunker_splits_at_target_with_overlap() {
        // Build text with two paragraphs, each ~60 chars, and set target_size=60 so a split occurs.
        let p1 = "a".repeat(60);
        let p2 = "b".repeat(60);
        let text = format!("{p1}\n\n{p2}");
        let config = SemanticChunkConfig {
            target_size: 60,
            min_size: 10,
            max_size: 200,
            overlap: 10,
            ..Default::default()
        };
        let chunker = RuleBasedSemanticChunker::new(config);
        let chunks = chunker.chunk(&text, None).unwrap();
        assert!(
            chunks.len() >= 2,
            "expected at least 2 chunks, got {}",
            chunks.len()
        );
        // Second chunk should start before the second paragraph due to overlap.
        assert!(
            chunks[1].start < p1.chars().count() + 2, // +2 for the two newlines
            "second chunk start {} should be < {}",
            chunks[1].start,
            p1.chars().count() + 2
        );
    }

    #[test]
    fn rule_chunker_merges_tiny_trailing_chunk() {
        // Three paragraphs: first two are big enough, third is tiny.
        // The tiny third should be merged into the second chunk.
        let big = "x".repeat(80);
        let tiny = "y".repeat(5);
        let text = format!("{big}\n\n{big}\n\n{tiny}");
        let config = SemanticChunkConfig {
            target_size: 80,
            min_size: 50,
            max_size: 300,
            overlap: 0,
            ..Default::default()
        };
        let chunker = RuleBasedSemanticChunker::new(config);
        let chunks = chunker.chunk(&text, None).unwrap();
        // The tiny trailing paragraph should be merged into the previous chunk.
        for chunk in &chunks {
            assert!(
                chunk.text.chars().count() >= 50 || chunks.len() == 1,
                "chunk below min_size: {} chars",
                chunk.text.chars().count()
            );
        }
    }

    #[test]
    fn rule_chunker_unicode_offsets_round_trip() {
        // Verify that chunk start/end are char offsets that recover the correct text.
        let text = "Caf\u{00e9} au lait.\n\nR\u{00e9}sum\u{00e9} section.\n\nNa\u{00ef}ve end.";
        let config = SemanticChunkConfig {
            target_size: 20,
            min_size: 5,
            max_size: 40,
            overlap: 0,
            ..Default::default()
        };
        let chunker = RuleBasedSemanticChunker::new(config);
        let chunks = chunker.chunk(text, None).unwrap();
        let chars: Vec<char> = text.chars().collect();
        for chunk in &chunks {
            let reconstructed: String = chars[chunk.start..chunk.end].iter().collect();
            assert_eq!(
                chunk.text, reconstructed,
                "offset round-trip mismatch: text={:?}, reconstructed={:?}",
                chunk.text, reconstructed
            );
        }
    }

    // ── Config presets ───────────────────────────────────────────────────

    #[test]
    fn config_presets_are_valid() {
        let d = SemanticChunkConfig::default();
        assert!(d.min_size < d.target_size);
        assert!(d.target_size < d.max_size);

        let ld = SemanticChunkConfig::long_document();
        assert!(ld.min_size < ld.target_size);
        assert!(ld.target_size < ld.max_size);

        let coref = SemanticChunkConfig::coreference();
        assert!(coref.min_size < coref.target_size);
        assert!(coref.target_size < coref.max_size);
        // Coreference should use a higher similarity threshold to keep mentions together.
        assert!(coref.similarity_threshold > d.similarity_threshold);
    }

    // ── EmbeddingSemanticChunker helpers (semantic-chunking feature) ──────

    #[cfg(feature = "semantic-chunking")]
    mod semantic_chunking_feature {
        use super::super::EmbeddingSemanticChunker;
        use std::collections::BTreeSet;

        #[test]
        fn tokenize_filters_short_tokens() {
            let tokens = EmbeddingSemanticChunker::tokenize_for_similarity("I am a big cat");
            // "I", "am", "a" are <= 2 chars and should be filtered out.
            assert!(!tokens.contains("i"));
            assert!(!tokens.contains("am"));
            assert!(!tokens.contains("a"));
            assert!(tokens.contains("big"));
            assert!(tokens.contains("cat"));
        }

        #[test]
        fn jaccard_identical_sets() {
            let a: BTreeSet<String> = ["foo", "bar"].iter().map(|s| s.to_string()).collect();
            let sim = EmbeddingSemanticChunker::jaccard(&a, &a);
            assert!((sim - 1.0).abs() < f32::EPSILON);
        }

        #[test]
        fn jaccard_disjoint_sets() {
            let a: BTreeSet<String> = ["foo"].iter().map(|s| s.to_string()).collect();
            let b: BTreeSet<String> = ["bar"].iter().map(|s| s.to_string()).collect();
            let sim = EmbeddingSemanticChunker::jaccard(&a, &b);
            assert!((sim - 0.0).abs() < f32::EPSILON);
        }

        #[test]
        fn jaccard_both_empty() {
            let empty: BTreeSet<String> = BTreeSet::new();
            assert!((EmbeddingSemanticChunker::jaccard(&empty, &empty) - 1.0).abs() < f32::EPSILON);
        }

        #[test]
        fn split_sentences_spans_basic() {
            let text = "Hello world. How are you? Fine.";
            let spans = EmbeddingSemanticChunker::split_sentences_spans(text);
            assert_eq!(spans.len(), 3);
            // Verify spans cover the entire text.
            assert_eq!(spans[0].0, 0);
            assert_eq!(spans.last().unwrap().1, text.chars().count());
        }

        #[test]
        fn split_sentences_spans_cjk_terminators() {
            let text = "first sentence\u{3002}second\u{FF01}";
            let spans = EmbeddingSemanticChunker::split_sentences_spans(text);
            assert_eq!(spans.len(), 2);
        }
    }
}
