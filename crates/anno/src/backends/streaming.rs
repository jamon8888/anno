//! Streaming NER API for incremental entity extraction.
//!
//! Provides iterator-based entity extraction for large documents,
//! real-time text streams, or memory-constrained environments.
//!
//! # Overview
//!
//! Standard NER processes entire documents at once, which can be slow and
//! memory-intensive for large texts. The streaming API offers:
//!
//! - **Chunked processing**: Split text into manageable chunks
//! - **Iterator interface**: Lazily yield entities as they're found
//! - **Backpressure**: Consumer controls the pace of extraction
//! - **Stateful context**: Maintain context across chunk boundaries
//!
//! # Example
//!
//! ```rust,ignore
//! use anno::backends::streaming::{StreamingExtractor, ChunkConfig};
//! use anno::StackedNER;
//!
//! let backend = StackedNER::default();
//! let config = ChunkConfig::default();
//!
//! // Process large text in chunks
//! let extractor = StreamingExtractor::new(&backend, config);
//! for entity in extractor.extract("Very long text...") {
//!     println!("Found: {} at {}-{}", entity.text, entity.start, entity.end);
//! }
//! ```
//!
//! # Pipeline Integration
//!
//! The streaming API integrates with async pipelines:
//!
//! ```rust,ignore
//! use futures::StreamExt;
//!
//! let stream = extractor.extract_stream(text);
//! while let Some(entity) = stream.next().await {
//!     process(entity);
//! }
//! ```

use crate::{Entity, Model, Result};

// Semantic chunking integration pending
// #[cfg(feature = "semantic-chunking")]
// use crate::backends::semantic_chunking::{SemanticChunkConfig, SemanticChunker};

/// Configuration for chunked text processing.
#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Target chunk size in characters (actual may vary to avoid splitting words)
    pub chunk_size: usize,
    /// Overlap between chunks (characters) to catch entities at boundaries
    pub overlap: usize,
    /// Sentence boundary detection (if true, chunks end at sentence boundaries)
    pub respect_sentences: bool,
    /// Maximum entities to buffer before yielding
    pub buffer_size: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            chunk_size: 10_000,
            overlap: 100,
            respect_sentences: true,
            buffer_size: 1000,
        }
    }
}

impl ChunkConfig {
    /// Create a config for small documents (no chunking).
    pub fn no_chunking() -> Self {
        Self {
            chunk_size: usize::MAX,
            overlap: 0,
            respect_sentences: false,
            buffer_size: usize::MAX,
        }
    }

    /// Create a config optimized for long documents.
    pub fn long_document() -> Self {
        Self {
            chunk_size: 50_000,
            overlap: 200,
            respect_sentences: true,
            buffer_size: 5000,
        }
    }

    /// Create a config for real-time/streaming input.
    pub fn realtime() -> Self {
        Self {
            chunk_size: 1000,
            overlap: 50,
            respect_sentences: false,
            buffer_size: 100,
        }
    }
}

/// A streaming entity extractor that processes text in chunks.
#[derive(Debug)]
pub struct StreamingExtractor<'m, M: Model> {
    model: &'m M,
    config: ChunkConfig,
}

impl<'m, M: Model> StreamingExtractor<'m, M> {
    /// Create a new streaming extractor with the given model and config.
    pub fn new(model: &'m M, config: ChunkConfig) -> Self {
        Self { model, config }
    }

    /// Create with default config.
    pub fn with_model(model: &'m M) -> Self {
        Self::new(model, ChunkConfig::default())
    }

    /// Extract entities from text, yielding them as an iterator.
    pub fn extract<'t>(&'m self, text: &'t str) -> EntityIterator<'m, 't, M> {
        EntityIterator::new(self, text)
    }

    /// Process a single chunk and return entities with adjusted offsets.
    fn process_chunk(&self, chunk: &str, offset: usize) -> Result<Vec<Entity>> {
        let entities = self.model.extract_entities(chunk, None)?;

        // Adjust offsets to be relative to original text
        Ok(entities
            .into_iter()
            .map(|mut e| {
                e.start += offset;
                e.end += offset;
                e
            })
            .collect())
    }
}

/// Iterator over entities extracted from text.
pub struct EntityIterator<'m, 't, M: Model> {
    extractor: &'m StreamingExtractor<'m, M>,
    text: &'t str,
    /// Current position in text (character offset)
    position: usize,
    /// Buffer of entities from current chunk
    buffer: Vec<Entity>,
    /// Index into buffer
    buffer_idx: usize,
    /// Set of (start, end) pairs already yielded (for deduplication)
    seen: std::collections::HashSet<(usize, usize)>,
    /// Whether we've finished processing
    done: bool,
}

impl<'m, 't, M: Model> EntityIterator<'m, 't, M> {
    fn new(extractor: &'m StreamingExtractor<'m, M>, text: &'t str) -> Self {
        Self {
            extractor,
            text,
            position: 0,
            buffer: Vec::new(),
            buffer_idx: 0,
            seen: std::collections::HashSet::new(),
            done: false,
        }
    }

    /// Fill the buffer with entities from the next chunk.
    fn fill_buffer(&mut self) -> Result<()> {
        if self.done {
            return Ok(());
        }

        let text_chars: Vec<char> = self.text.chars().collect();
        let text_len = text_chars.len();

        if self.position >= text_len {
            self.done = true;
            return Ok(());
        }

        // Calculate chunk boundaries
        let chunk_end = (self.position + self.extractor.config.chunk_size).min(text_len);

        // Find a good break point (sentence boundary or word boundary)
        let actual_end = if self.extractor.config.respect_sentences {
            find_sentence_boundary(&text_chars, self.position, chunk_end)
        } else {
            find_word_boundary(&text_chars, chunk_end)
        };

        // Extract the chunk
        let chunk: String = text_chars[self.position..actual_end].iter().collect();

        // Process chunk
        let entities = self.extractor.process_chunk(&chunk, self.position)?;

        // Filter out entities we've already seen (from overlap regions)
        self.buffer = entities
            .into_iter()
            .filter(|e| !self.seen.contains(&(e.start, e.end)))
            .collect();

        // Mark these entities as seen
        for e in &self.buffer {
            self.seen.insert((e.start, e.end));
        }

        self.buffer_idx = 0;

        // Move position forward (with overlap for next chunk)
        // CRITICAL: Always ensure we make forward progress to avoid infinite loops
        let overlap = self.extractor.config.overlap;
        let new_position = if actual_end >= text_len {
            text_len
        } else {
            // Ensure we always advance by at least 1 character
            let overlap_position = actual_end.saturating_sub(overlap);
            // If overlap would cause us to not advance, force forward progress
            if overlap_position <= self.position {
                self.position + 1
            } else {
                overlap_position
            }
        };

        self.position = new_position;

        if actual_end >= text_len || self.position >= text_len {
            self.done = true;
        }

        Ok(())
    }
}

impl<'m, 't, M: Model> Iterator for EntityIterator<'m, 't, M> {
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Return from buffer if available
            if self.buffer_idx < self.buffer.len() {
                let entity = self.buffer[self.buffer_idx].clone();
                self.buffer_idx += 1;
                return Some(entity);
            }

            // Buffer empty, try to fill it
            if self.done {
                return None;
            }

            if self.fill_buffer().is_err() {
                self.done = true;
                return None;
            }

            // If buffer is still empty after fill, we're done
            if self.buffer.is_empty() && self.done {
                return None;
            }
        }
    }
}

/// A text chunk with its character offset in the original text.
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// The chunk text.
    pub text: String,
    /// Character offset of this chunk's start in the original text.
    pub char_offset: usize,
}

/// Split text into chunks suitable for independent NER processing.
///
/// Each chunk respects sentence/word boundaries and includes overlap
/// for entity recovery at boundaries. Returns chunks with their
/// character offsets so entities can be mapped back to the original text.
///
/// This is the shared chunking primitive used by both `StreamingExtractor`
/// and LLM backends like UniversalNER.
pub fn chunk_text(text: &str, config: &ChunkConfig) -> Vec<TextChunk> {
    let chars: Vec<char> = text.chars().collect();
    let text_len = chars.len();

    if text_len == 0 {
        return Vec::new();
    }

    // If text fits in one chunk, return it directly.
    if text_len <= config.chunk_size {
        return vec![TextChunk {
            text: text.to_string(),
            char_offset: 0,
        }];
    }

    let mut chunks = Vec::new();
    let mut position = 0;

    while position < text_len {
        let chunk_end = (position + config.chunk_size).min(text_len);

        let actual_end = if chunk_end >= text_len {
            text_len
        } else if config.respect_sentences {
            find_sentence_boundary(&chars, position, chunk_end)
        } else {
            find_word_boundary(&chars, chunk_end)
        };

        let chunk_str: String = chars[position..actual_end].iter().collect();
        chunks.push(TextChunk {
            text: chunk_str,
            char_offset: position,
        });

        if actual_end >= text_len {
            break;
        }

        // Advance with overlap, ensuring forward progress.
        let overlap_position = actual_end.saturating_sub(config.overlap);
        position = if overlap_position <= position {
            position + 1
        } else {
            overlap_position
        };
    }

    chunks
}

/// Find a sentence boundary near the target position.
pub fn find_sentence_boundary(chars: &[char], start: usize, target: usize) -> usize {
    // Look backwards from target for sentence-ending punctuation
    let search_start = target.saturating_sub(200);
    for i in (search_start..target).rev() {
        if i >= chars.len() {
            continue;
        }
        let c = chars[i];
        // Sentence boundaries: . ! ? followed by whitespace or end
        if (c == '.' || c == '!' || c == '?' || c == '。' || c == '！' || c == '？')
            && (i + 1 >= chars.len() || chars[i + 1].is_whitespace())
        {
            // Return position after the punctuation and whitespace
            let mut end = i + 1;
            while end < chars.len() && chars[end].is_whitespace() {
                end += 1;
            }
            if end > start {
                return end;
            }
        }
    }
    // No sentence boundary found, fall back to word boundary
    find_word_boundary(chars, target)
}

/// Find a word boundary near the target position.
pub fn find_word_boundary(chars: &[char], target: usize) -> usize {
    let target = target.min(chars.len());

    // If we're already at end, return it
    if target >= chars.len() {
        return chars.len();
    }

    // Look backwards for whitespace
    for i in (0..target).rev() {
        if chars[i].is_whitespace() {
            return i + 1;
        }
    }
    target
}

// =============================================================================
// Async Stream Support (requires tokio/async-std)
// =============================================================================

/// Async streaming adapters for `StreamingExtractor`.
#[cfg(feature = "production")]
pub mod async_stream {
    use super::*;
    use futures::stream::{self, Stream};

    impl<'m, M: Model + Sync> StreamingExtractor<'m, M> {
        /// Create an async stream of entities.
        pub fn extract_stream<'t>(&'m self, text: &'t str) -> impl Stream<Item = Entity> + 'm
        where
            't: 'm,
        {
            let iter = self.extract(text);
            stream::iter(iter)
        }
    }
}

// =============================================================================
// Pipeline Integration Hooks
// =============================================================================

/// A processing stage in an NER pipeline.
pub trait PipelineStage: Send + Sync {
    /// Process entities before they're returned.
    fn process(&self, entities: Vec<Entity>, text: &str) -> Vec<Entity>;

    /// Name of this stage (for debugging/logging).
    fn name(&self) -> &'static str;
}

/// A complete NER pipeline with preprocessing and postprocessing stages.
pub struct Pipeline<M: Model> {
    model: M,
    /// Stages that run after entity extraction
    post_stages: Vec<Box<dyn PipelineStage>>,
    /// Chunk configuration for streaming
    chunk_config: ChunkConfig,
}

impl<M: Model> Pipeline<M> {
    /// Create a new pipeline with the given model.
    pub fn new(model: M) -> Self {
        Self {
            model,
            post_stages: Vec::new(),
            chunk_config: ChunkConfig::default(),
        }
    }

    /// Add a post-processing stage.
    pub fn add_stage(mut self, stage: Box<dyn PipelineStage>) -> Self {
        self.post_stages.push(stage);
        self
    }

    /// Set chunk configuration.
    pub fn with_chunk_config(mut self, config: ChunkConfig) -> Self {
        self.chunk_config = config;
        self
    }

    /// Extract entities with all pipeline stages applied.
    pub fn extract(&self, text: &str) -> Result<Vec<Entity>> {
        let mut entities = self.model.extract_entities(text, None)?;

        for stage in &self.post_stages {
            entities = stage.process(entities, text);
        }

        Ok(entities)
    }

    /// Get a reference to the underlying model.
    pub fn model(&self) -> &M {
        &self.model
    }
}

// =============================================================================
// Common Pipeline Stages
// =============================================================================

/// Filter entities by confidence threshold.
pub struct ConfidenceFilter {
    threshold: f64,
}

impl ConfidenceFilter {
    /// Create a new confidence filter with the given threshold.
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl PipelineStage for ConfidenceFilter {
    fn process(&self, entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
        entities
            .into_iter()
            .filter(|e| e.confidence >= self.threshold)
            .collect()
    }

    fn name(&self) -> &'static str {
        "ConfidenceFilter"
    }
}

/// Deduplicate overlapping entities, keeping highest confidence.
pub struct DeduplicateOverlapping;

impl PipelineStage for DeduplicateOverlapping {
    fn process(&self, mut entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
        // Sort by start, then by confidence (desc)
        entities.sort_by(|a, b| {
            a.start.cmp(&b.start).then(
                b.confidence
                    .partial_cmp(&a.confidence)
                    .expect("confidence values should be comparable"),
            )
        });

        let mut result = Vec::new();
        let mut last_end = 0;

        for entity in entities {
            if entity.start >= last_end {
                last_end = entity.end;
                result.push(entity);
            }
            // Skip overlapping entities (we already have a higher-confidence one)
        }

        result
    }

    fn name(&self) -> &'static str {
        "DeduplicateOverlapping"
    }
}

/// Deduplicate overlapping entities of the same type, keeping the longer span.
///
/// Unlike `DeduplicateOverlapping` (which drops any overlapping entity regardless
/// of type), this stage only merges when two entities overlap AND share the same
/// `entity_type`. Different-type overlaps are preserved (Union behavior).
///
/// Consistent with `ConflictStrategy::LongestSpan` in stacked NER.
/// Primary use case: coalescing entities from overlapping chunks where the same
/// entity gets extracted with slightly different boundaries.
pub struct DeduplicateOverlappingSameType;

impl PipelineStage for DeduplicateOverlappingSameType {
    fn process(&self, mut entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
        if entities.len() <= 1 {
            return entities;
        }

        entities.sort_by_key(|e| (e.start, e.end));

        let mut result: Vec<Entity> = Vec::with_capacity(entities.len());

        for entity in entities {
            let dominated = result.last().map_or(false, |prev: &Entity| {
                entity.start < prev.end
                    && prev.start < entity.end
                    && prev.entity_type == entity.entity_type
            });

            if dominated {
                let prev = result.last().unwrap();
                let prev_len = prev.end - prev.start;
                let cand_len = entity.end - entity.start;
                if cand_len > prev_len {
                    *result.last_mut().unwrap() = entity;
                }
            } else {
                result.push(entity);
            }
        }

        result
    }

    fn name(&self) -> &'static str {
        "DeduplicateOverlappingSameType"
    }
}

/// Extract entities from a large document using parallel chunked processing.
///
/// Splits `text` into overlapping chunks via `chunk_text()`, processes each
/// chunk through `extract_fn` in parallel threads, and coalesces results with
/// exact-span + same-type overlap dedup.
///
/// This is the generic parallel chunking primitive. Backend-specific wrappers
/// (e.g., UniversalNER) call this with their own extraction closure.
///
/// Returns entities sorted by position with no duplicate spans.
pub fn extract_chunked_parallel<F>(
    text: &str,
    config: &ChunkConfig,
    extract_fn: F,
) -> Result<Vec<Entity>>
where
    F: Fn(&str, usize) -> Result<Vec<Entity>> + Send + Sync,
{
    let chunks = chunk_text(text, config);

    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    if chunks.len() == 1 {
        return extract_fn(&chunks[0].text, chunks[0].char_offset);
    }

    // Process all chunks in parallel.
    let results: Vec<Result<Vec<Entity>>> = std::thread::scope(|s| {
        let extract_fn = &extract_fn;
        let handles: Vec<_> = chunks
            .iter()
            .map(|chunk| s.spawn(move || extract_fn(&chunk.text, chunk.char_offset)))
            .collect();

        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Coalesce: exact-span dedup + same-type overlap dedup.
    let mut seen = std::collections::HashSet::new();
    let mut all_entities = Vec::new();

    for result in results {
        let entities = result?;
        for entity in entities {
            if seen.insert((entity.start, entity.end)) {
                all_entities.push(entity);
            }
        }
    }

    all_entities.sort_by_key(|e| (e.start, e.end));

    let dedup = DeduplicateOverlappingSameType;
    Ok(dedup.process(all_entities, text))
}

/// Normalize entity text (trim whitespace, normalize case, etc.).
pub struct NormalizeText {
    lowercase: bool,
}

impl NormalizeText {
    /// Create a new text normalizer with optional lowercasing.
    pub fn new(lowercase: bool) -> Self {
        Self { lowercase }
    }
}

impl PipelineStage for NormalizeText {
    fn process(&self, entities: Vec<Entity>, _text: &str) -> Vec<Entity> {
        entities
            .into_iter()
            .map(|mut e| {
                e.text = e.text.trim().to_string();
                if self.lowercase {
                    e.text = e.text.to_lowercase();
                }
                e
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "NormalizeText"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityType, HeuristicNER};

    #[test]
    fn test_streaming_basic() {
        let model = HeuristicNER::new();
        let extractor = StreamingExtractor::with_model(&model);

        let text = "John Smith works at Google Inc. in New York.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        assert!(!entities.is_empty());
    }

    #[test]
    fn test_streaming_long_text() {
        let model = HeuristicNER::new();
        let config = ChunkConfig {
            chunk_size: 50,
            overlap: 10,
            respect_sentences: false,
            buffer_size: 100,
        };
        let extractor = StreamingExtractor::new(&model, config);

        // Create a longer text
        let text =
            "John Smith works at Google. Mary Johnson is at Apple. Bob Williams joined Microsoft.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        // Should find entities across chunks
        assert!(!entities.is_empty());
    }

    #[test]
    fn test_pipeline() {
        let model = HeuristicNER::new();
        let pipeline = Pipeline::new(model)
            .add_stage(Box::new(ConfidenceFilter::new(0.5)))
            .add_stage(Box::new(DeduplicateOverlapping));

        let text = "John Smith works at Google Inc.";
        let entities = pipeline.extract(text).unwrap();

        // All entities should have confidence >= 0.5
        for entity in &entities {
            assert!(entity.confidence >= 0.5);
        }
    }

    #[test]
    fn test_chunk_config_presets() {
        let _no_chunk = ChunkConfig::no_chunking();
        let _long = ChunkConfig::long_document();
        let _realtime = ChunkConfig::realtime();
    }

    #[test]
    fn test_find_sentence_boundary() {
        let text: Vec<char> = "Hello world. This is a test.".chars().collect();
        let boundary = find_sentence_boundary(&text, 0, 20);
        // Should find boundary after "Hello world. "
        assert!(boundary > 0);
        assert!(boundary <= 20);
    }

    #[test]
    fn test_entity_deduplication_across_chunks() {
        // When an entity appears in the overlap region between chunks,
        // it should be deduplicated (seen set should prevent duplicates)
        let model = HeuristicNER::new();

        // Use reasonable chunks with small overlap (avoid infinite loop edge cases)
        let config = ChunkConfig {
            chunk_size: 100,
            overlap: 20,
            respect_sentences: false,
            buffer_size: 100,
        };
        let extractor = StreamingExtractor::new(&model, config);

        let text = "I work at Google Inc in California. Then I visited Google headquarters.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        // Should find entities without infinite loops
        // (the fix ensures forward progress)
        assert!(
            entities.len() < 100,
            "Possible infinite loop: too many entities"
        );
    }

    #[test]
    fn test_empty_text_streaming() {
        let model = HeuristicNER::new();
        let extractor = StreamingExtractor::with_model(&model);

        let entities: Vec<Entity> = extractor.extract("").collect();
        assert!(entities.is_empty());
    }

    #[test]
    fn test_unicode_text_streaming() {
        let model = HeuristicNER::new();
        let extractor = StreamingExtractor::with_model(&model);

        let text = "東京 is the capital of 日本. Paris is in France.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        // Character offsets should be valid
        let char_count = text.chars().count();
        for entity in &entities {
            assert!(entity.start <= entity.end, "Invalid span");
            assert!(entity.end <= char_count, "Offset exceeds text length");
        }
    }

    #[test]
    fn test_forward_progress_guaranteed() {
        // Test that streaming always makes forward progress even with small chunks
        let model = HeuristicNER::new();

        let config = ChunkConfig {
            chunk_size: 5, // Very small chunks
            overlap: 3,    // Large overlap relative to chunk
            respect_sentences: false,
            buffer_size: 10,
        };
        let extractor = StreamingExtractor::new(&model, config);

        // Short text that could cause infinite loop without the fix
        let text = "abc def";

        // Should complete without hanging (the fix ensures forward progress)
        let entities: Vec<Entity> = extractor.extract(text).collect();
        // We don't care about the results, just that it terminates
        let _ = entities;
    }

    // =========================================================================
    // Chunk boundary helpers
    // =========================================================================

    #[test]
    fn test_find_word_boundary_at_end() {
        let chars: Vec<char> = "hello world".chars().collect();
        assert_eq!(find_word_boundary(&chars, 100), chars.len());
        assert_eq!(find_word_boundary(&chars, chars.len()), chars.len());
    }

    #[test]
    fn test_find_word_boundary_mid_word() {
        let chars: Vec<char> = "hello world foo".chars().collect();
        // target=14 is inside "foo" -> backtrack to space at 11 -> return 12
        let boundary = find_word_boundary(&chars, 14);
        assert_eq!(boundary, 12, "should break before 'foo'");
    }

    #[test]
    fn test_find_word_boundary_no_whitespace() {
        let chars: Vec<char> = "abcdefghij".chars().collect();
        assert_eq!(find_word_boundary(&chars, 5), 5);
    }

    #[test]
    fn test_find_sentence_boundary_cjk_punctuation() {
        let text: Vec<char> = "这是测试。下一句话开始了".chars().collect();
        let boundary = find_sentence_boundary(&text, 0, text.len());
        assert!(boundary <= text.len());
        // '。' is at index 4; next char (no whitespace) is index 5
        assert!(boundary >= 5, "should be at or after the CJK period");
    }

    #[test]
    fn test_find_sentence_boundary_no_punctuation() {
        let chars: Vec<char> = "no punctuation here at all".chars().collect();
        let boundary = find_sentence_boundary(&chars, 0, 20);
        // Falls through to find_word_boundary
        assert!(boundary > 0 && boundary <= 20);
    }

    #[test]
    fn test_find_sentence_boundary_exclamation_and_question() {
        let chars: Vec<char> = "Wow! Really? Yes indeed.".chars().collect();
        // Target 10 => backwards from 10: '!' at 3 followed by ' ' at 4 -> boundary = 5
        let boundary = find_sentence_boundary(&chars, 0, 10);
        assert_eq!(boundary, 5, "should split after 'Wow! '");
    }

    // =========================================================================
    // Offset adjustment across chunks
    // =========================================================================

    #[test]
    fn test_offset_adjustment_multi_chunk() {
        // Entities in later chunks must have offsets relative to the full text.
        let model = HeuristicNER::new();
        let config = ChunkConfig {
            chunk_size: 30,
            overlap: 5,
            respect_sentences: false,
            buffer_size: 100,
        };
        let extractor = StreamingExtractor::new(&model, config);

        let text = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx Google Inc is here.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        let text_chars: Vec<char> = text.chars().collect();
        for e in &entities {
            let span: String = text_chars[e.start..e.end].iter().collect();
            assert_eq!(
                span, e.text,
                "offset-adjusted text should match entity text"
            );
        }
    }

    // =========================================================================
    // Entity dedup at chunk boundaries
    // =========================================================================

    #[test]
    fn test_no_duplicate_entities_from_overlap() {
        let model = HeuristicNER::new();
        let config = ChunkConfig {
            chunk_size: 40,
            overlap: 20,
            respect_sentences: false,
            buffer_size: 100,
        };
        let extractor = StreamingExtractor::new(&model, config);

        let text = "Dr. John Smith is a researcher at Google Inc in New York City area.";
        let entities: Vec<Entity> = extractor.extract(text).collect();

        let mut spans: Vec<(usize, usize)> = entities.iter().map(|e| (e.start, e.end)).collect();
        let before = spans.len();
        spans.sort();
        spans.dedup();
        assert_eq!(before, spans.len(), "duplicate entities found in output");
    }

    // =========================================================================
    // Empty / single-token chunks
    // =========================================================================

    #[test]
    fn test_whitespace_only_text() {
        let model = HeuristicNER::new();
        let extractor = StreamingExtractor::with_model(&model);

        let entities: Vec<Entity> = extractor.extract("   \n\t  ").collect();
        assert!(
            entities.is_empty(),
            "whitespace-only text should yield no entities"
        );
    }

    #[test]
    fn test_single_character_text() {
        let model = HeuristicNER::new();
        let config = ChunkConfig {
            chunk_size: 1,
            overlap: 0,
            respect_sentences: false,
            buffer_size: 10,
        };
        let extractor = StreamingExtractor::new(&model, config);

        let entities: Vec<Entity> = extractor.extract("A").collect();
        let _ = entities;
    }

    #[test]
    fn test_single_token_chunks() {
        // Very small chunk_size with space-separated tokens.
        let model = HeuristicNER::new();
        let config = ChunkConfig {
            chunk_size: 2,
            overlap: 0,
            respect_sentences: false,
            buffer_size: 100,
        };
        let extractor = StreamingExtractor::new(&model, config);

        let text = "A B C D E";
        let entities: Vec<Entity> = extractor.extract(text).collect();
        let char_count = text.chars().count();
        for e in &entities {
            assert!(e.end <= char_count, "offset exceeds text length");
        }
    }

    // =========================================================================
    // Config / builder patterns
    // =========================================================================

    #[test]
    fn test_chunk_config_no_chunking_values() {
        let cfg = ChunkConfig::no_chunking();
        assert_eq!(cfg.chunk_size, usize::MAX);
        assert_eq!(cfg.overlap, 0);
        assert!(!cfg.respect_sentences);
        assert_eq!(cfg.buffer_size, usize::MAX);
    }

    #[test]
    fn test_chunk_config_long_document_values() {
        let cfg = ChunkConfig::long_document();
        assert_eq!(cfg.chunk_size, 50_000);
        assert_eq!(cfg.overlap, 200);
        assert!(cfg.respect_sentences);
        assert_eq!(cfg.buffer_size, 5000);
    }

    #[test]
    fn test_chunk_config_realtime_values() {
        let cfg = ChunkConfig::realtime();
        assert_eq!(cfg.chunk_size, 1000);
        assert_eq!(cfg.overlap, 50);
        assert!(!cfg.respect_sentences);
        assert_eq!(cfg.buffer_size, 100);
    }

    #[test]
    fn test_chunk_config_default_values() {
        let cfg = ChunkConfig::default();
        assert_eq!(cfg.chunk_size, 10_000);
        assert_eq!(cfg.overlap, 100);
        assert!(cfg.respect_sentences);
        assert_eq!(cfg.buffer_size, 1000);
    }

    // =========================================================================
    // Pipeline stages
    // =========================================================================

    #[test]
    fn test_confidence_filter_removes_low_confidence() {
        let filter = ConfidenceFilter::new(0.8);
        let entities = vec![
            Entity::new("A", EntityType::Person, 0, 1, 0.9),
            Entity::new("B", EntityType::Person, 2, 3, 0.3),
        ];
        let result = filter.process(entities, "");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "A");
    }

    #[test]
    fn test_deduplicate_overlapping_keeps_higher_confidence() {
        let dedup = DeduplicateOverlapping;
        let entities = vec![
            Entity::new("New York", EntityType::Location, 0, 8, 0.7),
            Entity::new("New York City", EntityType::Location, 0, 13, 0.9),
        ];
        let result = dedup.process(entities, "");
        assert_eq!(result.len(), 1);
        assert!(
            result[0].confidence > 0.8,
            "should keep the higher-confidence entity"
        );
    }

    #[test]
    fn test_normalize_text_trims_and_lowercases() {
        let normalizer = NormalizeText::new(true);
        let entities = vec![Entity::new(
            "  John Smith  ",
            EntityType::Person,
            0,
            10,
            0.9,
        )];
        let result = normalizer.process(entities, "");
        assert_eq!(result[0].text, "john smith");
    }

    #[test]
    fn test_normalize_text_no_lowercase() {
        let normalizer = NormalizeText::new(false);
        let entities = vec![Entity::new(
            "  GOOGLE  ",
            EntityType::Organization,
            0,
            6,
            0.9,
        )];
        let result = normalizer.process(entities, "");
        assert_eq!(result[0].text, "GOOGLE");
    }

    #[test]
    fn test_pipeline_stage_names() {
        assert_eq!(ConfidenceFilter::new(0.5).name(), "ConfidenceFilter");
        assert_eq!(DeduplicateOverlapping.name(), "DeduplicateOverlapping");
        assert_eq!(NormalizeText::new(false).name(), "NormalizeText");
    }

    // =========================================================================
    // chunk_text() shared utility
    // =========================================================================

    #[test]
    fn test_chunk_text_small_text_single_chunk() {
        let config = ChunkConfig::default(); // 10k chars
        let chunks = chunk_text("Hello world.", &config);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello world.");
        assert_eq!(chunks[0].char_offset, 0);
    }

    #[test]
    fn test_chunk_text_empty() {
        let config = ChunkConfig::default();
        let chunks = chunk_text("", &config);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_splits_large_text() {
        let config = ChunkConfig {
            chunk_size: 20,
            overlap: 5,
            respect_sentences: false,
            buffer_size: 100,
        };
        let text = "Alice met Bob in Paris. Charlie visited London yesterday. Dave works in Tokyo today.";
        let chunks = chunk_text(text, &config);
        assert!(chunks.len() > 1, "should split into multiple chunks");

        // Verify offsets are monotonically increasing
        for i in 1..chunks.len() {
            assert!(
                chunks[i].char_offset > chunks[i - 1].char_offset,
                "chunk offsets must increase"
            );
        }

        // Verify all text is covered (last chunk offset + last chunk len >= original len)
        let last = &chunks[chunks.len() - 1];
        let total_covered = last.char_offset + last.text.chars().count();
        assert!(
            total_covered >= text.chars().count(),
            "chunks must cover all text"
        );
    }

    #[test]
    fn test_chunk_text_respects_sentences() {
        let config = ChunkConfig {
            chunk_size: 30,
            overlap: 5,
            respect_sentences: true,
            buffer_size: 100,
        };
        let text = "First sentence here. Second sentence here. Third sentence here.";
        let chunks = chunk_text(text, &config);
        assert!(chunks.len() >= 2);
        // First chunk should end at a sentence boundary
        assert!(
            chunks[0].text.ends_with(". ") || chunks[0].text.ends_with('.'),
            "first chunk should end near sentence boundary: {:?}",
            chunks[0].text
        );
    }

    #[test]
    fn test_chunk_text_overlap_creates_redundancy() {
        let config = ChunkConfig {
            chunk_size: 20,
            overlap: 10,
            respect_sentences: false,
            buffer_size: 100,
        };
        let text = "0123456789 abcdefghij klmnopqrst uvwxyz";
        let chunks = chunk_text(text, &config);
        assert!(chunks.len() >= 2);

        // With overlap, chunk N+1's start should be before chunk N's end
        if chunks.len() >= 2 {
            let c0_end = chunks[0].char_offset + chunks[0].text.chars().count();
            let c1_start = chunks[1].char_offset;
            assert!(
                c1_start < c0_end,
                "overlap should cause chunk start ({}) < prev chunk end ({})",
                c1_start,
                c0_end
            );
        }
    }

    #[test]
    fn test_chunk_text_unicode() {
        let config = ChunkConfig {
            chunk_size: 10,
            overlap: 3,
            respect_sentences: false,
            buffer_size: 100,
        };
        let text = "東京は日本の首都です。パリはフランスの首都です。";
        let chunks = chunk_text(text, &config);
        assert!(chunks.len() >= 2);

        // Verify char offsets are correct
        let chars: Vec<char> = text.chars().collect();
        for chunk in &chunks {
            let expected: String = chars
                [chunk.char_offset..chunk.char_offset + chunk.text.chars().count()]
                .iter()
                .collect();
            assert_eq!(chunk.text, expected, "chunk text must match offset slice");
        }
    }
}
