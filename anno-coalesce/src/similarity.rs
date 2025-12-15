//! Multilingual string similarity for entity resolution.
//!
//! This module provides a proper multilingual string similarity pipeline:
//!
//! 1. **Preprocessing**: Unicode normalization, case folding
//! 2. **Script detection**: Route to appropriate algorithm
//! 3. **Multiple strategies**: Word-based, n-gram, edit distance, Jaro-Winkler
//! 4. **Future**: Byte-level learned embeddings
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    StringSimilarity                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  preprocess(s) -> normalized string                         │
//! │  detect_script(s) -> Script enum                            │
//! │  similarity(a, b) -> f32                                    │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!            ┌─────────────────┼─────────────────┐
//!            ▼                 ▼                 ▼
//!     ┌───────────┐     ┌───────────┐     ┌───────────┐
//!     │ WordBased │     │  NGram    │     │  EditDist │
//!     │ (English) │     │  (CJK)    │     │ (fallback)│
//!     └───────────┘     └───────────┘     └───────────┘
//! ```
//!
//! # Algorithms
//!
//! | Algorithm | Best For | Speed | CJK Support |
//! |-----------|----------|-------|-------------|
//! | Word Jaccard | Space-separated text | Fast | Poor |
//! | N-gram Jaccard | CJK, no word boundaries | Fast | Good |
//! | Levenshtein | Typo detection | Medium | Good |
//! | Jaro-Winkler | Short strings, names | Fast | Good |
//!
//! # Future: Byte-Level Embeddings
//!
//! The architecture is designed to eventually support learned embeddings:
//!
//! - **CANINE**: Tokenizer-free character-level transformer
//! - **ByT5**: Byte-level T5, robust across scripts
//! - **CharFormer**: Parameter-efficient character encoder
//!
//! Training approach:
//! 1. Collect entity pairs (same entity, different mentions)
//! 2. Contrastive loss (InfoNCE, triplet)
//! 3. Bi-encoder for fast inference
//! 4. Quantization for deployment
//!
//! # Example
//!
//! ```rust
//! use anno_coalesce::similarity::{Similarity, Script, multilingual_similarity};
//!
//! let sim = Similarity::new();
//!
//! // English (word-based)
//! let score = sim.compute("Marie Curie", "Curie");
//! assert!(score > 0.0);
//!
//! // CJK (n-gram based)
//! let score = sim.compute("中华人民共和国", "中华民国");
//! assert!(score > 0.0);
//!
//! // Script detection
//! assert_eq!(Script::detect("北京"), Script::Cjk);
//! assert_eq!(Script::detect("Москва"), Script::Cyrillic);
//!
//! // Direct multilingual similarity function
//! let sim = multilingual_similarity("東京", "东京");
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// =============================================================================
// Script Detection
// =============================================================================

/// Unicode script categories for routing similarity algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Script {
    /// Latin script (English, French, German, etc.)
    Latin,
    /// CJK (Chinese, Japanese Kanji, Korean Hanja)
    Cjk,
    /// Japanese Hiragana/Katakana
    Kana,
    /// Korean Hangul
    Hangul,
    /// Arabic script
    Arabic,
    /// Cyrillic script (Russian, etc.)
    Cyrillic,
    /// Devanagari (Hindi, Sanskrit, etc.)
    Devanagari,
    /// Greek script
    Greek,
    /// Hebrew script
    Hebrew,
    /// Thai script
    Thai,
    /// Mixed or unknown
    Mixed,
}

impl Script {
    /// Detect the dominant script in a string.
    ///
    /// Returns the script that appears most frequently.
    pub fn detect(s: &str) -> Self {
        let mut counts = [0u32; 11]; // One per Script variant

        for c in s.chars() {
            match c {
                '\u{0000}'..='\u{007F}' => counts[0] += 1, // ASCII/Latin
                '\u{0080}'..='\u{024F}' => counts[0] += 1, // Latin Extended
                '\u{4E00}'..='\u{9FFF}' => counts[1] += 1, // CJK Unified
                '\u{3400}'..='\u{4DBF}' => counts[1] += 1, // CJK Extension A
                '\u{3040}'..='\u{309F}' => counts[2] += 1, // Hiragana
                '\u{30A0}'..='\u{30FF}' => counts[2] += 1, // Katakana
                '\u{AC00}'..='\u{D7AF}' => counts[3] += 1, // Hangul Syllables
                '\u{1100}'..='\u{11FF}' => counts[3] += 1, // Hangul Jamo
                '\u{0600}'..='\u{06FF}' => counts[4] += 1, // Arabic
                '\u{0750}'..='\u{077F}' => counts[4] += 1, // Arabic Supplement
                '\u{0400}'..='\u{04FF}' => counts[5] += 1, // Cyrillic
                '\u{0500}'..='\u{052F}' => counts[5] += 1, // Cyrillic Supplement
                '\u{0900}'..='\u{097F}' => counts[6] += 1, // Devanagari
                '\u{0370}'..='\u{03FF}' => counts[7] += 1, // Greek
                '\u{1F00}'..='\u{1FFF}' => counts[7] += 1, // Greek Extended
                '\u{0590}'..='\u{05FF}' => counts[8] += 1, // Hebrew
                '\u{0E00}'..='\u{0E7F}' => counts[9] += 1, // Thai
                _ => counts[10] += 1,                      // Other
            }
        }

        // Find dominant script (ignoring whitespace/punctuation in ASCII)
        let scripts = [
            Script::Latin,
            Script::Cjk,
            Script::Kana,
            Script::Hangul,
            Script::Arabic,
            Script::Cyrillic,
            Script::Devanagari,
            Script::Greek,
            Script::Hebrew,
            Script::Thai,
            Script::Mixed,
        ];

        let max_idx = counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
            .map(|(i, _)| i)
            .unwrap_or(10);

        scripts[max_idx]
    }

    /// Whether this script uses word boundaries (spaces).
    pub fn has_word_boundaries(&self) -> bool {
        matches!(
            self,
            Script::Latin
                | Script::Cyrillic
                | Script::Greek
                | Script::Arabic
                | Script::Hebrew
                | Script::Devanagari
        )
    }
}

// =============================================================================
// Preprocessing
// =============================================================================

/// Normalize a string for comparison.
///
/// Applies:
/// 1. Case folding (lowercase)
/// 2. Whitespace normalization
/// 3. Basic Unicode normalization
///
/// # Note
///
/// For production use with proper NFKC normalization, consider
/// adding the `unicode-normalization` crate.
pub fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_whitespace() {
                ' '
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// =============================================================================
// Similarity Strategies
// =============================================================================

/// Configuration for multilingual string similarity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityConfig {
    /// Whether to normalize strings before comparison.
    pub normalize: bool,
    /// Minimum string length for n-gram computation.
    pub min_ngram_length: usize,
    /// N-gram size (2 = bigrams, 3 = trigrams).
    pub ngram_size: usize,
}

impl Default for SimilarityConfig {
    fn default() -> Self {
        Self {
            normalize: true,
            min_ngram_length: 2,
            ngram_size: 2, // Bigrams work better for CJK
        }
    }
}

/// Multilingual string similarity calculator.
#[derive(Debug, Clone, Default)]
pub struct Similarity {
    config: SimilarityConfig,
}

impl Similarity {
    /// Create a new similarity calculator with default config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom config.
    pub fn with_config(config: SimilarityConfig) -> Self {
        Self { config }
    }

    /// Compute similarity between two strings.
    ///
    /// Automatically selects the best algorithm based on script detection.
    pub fn compute(&self, a: &str, b: &str) -> f32 {
        // Handle trivial cases
        if a == b {
            return 1.0;
        }
        if a.is_empty() || b.is_empty() {
            return if a.is_empty() && b.is_empty() {
                1.0
            } else {
                0.0
            };
        }

        // Normalize if configured
        let (a_norm, b_norm) = if self.config.normalize {
            (normalize(a), normalize(b))
        } else {
            (a.to_string(), b.to_string())
        };

        // Check again after normalization
        if a_norm == b_norm {
            return 1.0;
        }

        // Detect scripts
        let script_a = Script::detect(&a_norm);
        let script_b = Script::detect(&b_norm);

        // Route to appropriate algorithm
        if script_a.has_word_boundaries() && script_b.has_word_boundaries() {
            // Both have word boundaries: use word-based Jaccard
            self.word_jaccard(&a_norm, &b_norm)
        } else {
            // At least one lacks word boundaries: use n-gram
            self.ngram_jaccard(&a_norm, &b_norm)
        }
    }

    /// Word-based Jaccard similarity.
    fn word_jaccard(&self, a: &str, b: &str) -> f32 {
        let words_a: HashSet<&str> = a.split_whitespace().collect();
        let words_b: HashSet<&str> = b.split_whitespace().collect();

        if words_a.is_empty() && words_b.is_empty() {
            return 1.0;
        }
        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Character n-gram Jaccard similarity.
    fn ngram_jaccard(&self, a: &str, b: &str) -> f32 {
        let ngrams_a = self.char_ngrams(a);
        let ngrams_b = self.char_ngrams(b);

        if ngrams_a.is_empty() && ngrams_b.is_empty() {
            return 1.0;
        }
        if ngrams_a.is_empty() || ngrams_b.is_empty() {
            return 0.0;
        }

        let intersection = ngrams_a.intersection(&ngrams_b).count();
        let union = ngrams_a.union(&ngrams_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// Extract character n-grams.
    fn char_ngrams(&self, s: &str) -> HashSet<String> {
        let chars: Vec<char> = s.chars().collect();
        let n = self.config.ngram_size;

        if chars.len() < n {
            // For very short strings, use the whole string
            let mut set = HashSet::new();
            if !chars.is_empty() {
                set.insert(s.to_string());
            }
            return set;
        }

        chars
            .windows(n)
            .map(|w| w.iter().collect::<String>())
            .collect()
    }
}

// =============================================================================
// Edit Distance
// =============================================================================

/// Levenshtein edit distance.
///
/// Computes the minimum number of single-character edits (insertions,
/// deletions, substitutions) needed to transform one string into another.
///
/// # Complexity
///
/// Time: O(m * n), Space: O(n) where m, n are string lengths.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use two-row optimization for space efficiency
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Normalized Levenshtein similarity (0.0 to 1.0).
///
/// Returns 1.0 for identical strings, 0.0 for completely different strings.
pub fn levenshtein_similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }

    let distance = levenshtein_distance(a, b);
    1.0 - (distance as f32 / max_len as f32)
}

// =============================================================================
// Jaro-Winkler
// =============================================================================

/// Jaro similarity between two strings.
///
/// Good for short strings like names. Returns 0.0 to 1.0.
pub fn jaro_similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 || b_len == 0 {
        return 0.0;
    }

    // Match window
    let match_distance = (a_len.max(b_len) / 2).saturating_sub(1);

    let mut a_matches = vec![false; a_len];
    let mut b_matches = vec![false; b_len];

    let mut matches = 0;
    let mut transpositions = 0;

    // Find matches
    for i in 0..a_len {
        let start = i.saturating_sub(match_distance);
        let end = (i + match_distance + 1).min(b_len);

        for j in start..end {
            if b_matches[j] || a_chars[i] != b_chars[j] {
                continue;
            }
            a_matches[i] = true;
            b_matches[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    // Count transpositions
    let mut k = 0;
    for i in 0..a_len {
        if !a_matches[i] {
            continue;
        }
        while !b_matches[k] {
            k += 1;
        }
        if a_chars[i] != b_chars[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let m = matches as f32;
    let t = (transpositions / 2) as f32;

    (m / a_len as f32 + m / b_len as f32 + (m - t) / m) / 3.0
}

/// Jaro-Winkler similarity (boosts common prefix).
///
/// Extends Jaro with a prefix bonus. Good for names that start similarly.
pub fn jaro_winkler_similarity(a: &str, b: &str) -> f32 {
    let jaro = jaro_similarity(a, b);

    // Calculate common prefix length (up to 4 chars)
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let prefix_len = a_chars
        .iter()
        .zip(b_chars.iter())
        .take(4)
        .take_while(|(a, b)| a == b)
        .count();

    // Winkler modification: boost for common prefix
    let p = 0.1; // Standard prefix scale
    jaro + (prefix_len as f32 * p * (1.0 - jaro))
}

// =============================================================================
// Public API
// =============================================================================

/// Compute multilingual string similarity using a hybrid approach.
///
/// Automatically selects algorithm based on script:
/// - Space-separated text (English, etc.): Word-based Jaccard
/// - CJK and other scripts without word boundaries: Character n-gram Jaccard
///
/// # Example
///
/// ```rust
/// use anno_coalesce::similarity::multilingual_similarity;
///
/// // English
/// let sim = multilingual_similarity("Marie Curie", "Curie");
/// assert!(sim > 0.0);
///
/// // CJK
/// let sim = multilingual_similarity("中华人民共和国", "中华民国");
/// assert!(sim > 0.0);
/// ```
pub fn multilingual_similarity(a: &str, b: &str) -> f32 {
    Similarity::new().compute(a, b)
}

// =============================================================================
// Acronym Detection
// =============================================================================

/// Check if one string is an acronym of another.
///
/// This is a language-agnostic algorithm that checks if the "short" form
/// consists of the first letters of words in the "long" form.
///
/// # Algorithm
///
/// 1. Identify which string is shorter (candidate acronym)
/// 2. Verify the short form looks like an acronym (mostly uppercase, 2-10 chars)
/// 3. Extract initials from the long form by splitting on whitespace/hyphens
/// 4. Compare initials to the short form (case-insensitive)
///
/// # Examples
///
/// ```rust
/// use anno_coalesce::similarity::is_acronym_match;
///
/// assert!(is_acronym_match("WHO", "World Health Organization"));
/// assert!(is_acronym_match("MRSA", "Methicillin-resistant Staphylococcus aureus"));
/// assert!(is_acronym_match("IBM", "International Business Machines"));
///
/// // Also works with reversed argument order
/// assert!(is_acronym_match("World Health Organization", "WHO"));
///
/// // Negative cases
/// assert!(!is_acronym_match("IBM", "Apple"));
/// assert!(!is_acronym_match("USA", "Canada"));
/// ```
///
/// # Language Agnosticism
///
/// This works for any language that uses spaces or hyphens to separate words:
/// - English: "WHO" ↔ "World Health Organization"
/// - German: "DDR" ↔ "Deutsche Demokratische Republik"
/// - Spanish: "ONU" ↔ "Organización de las Naciones Unidas"
///
/// For languages without word boundaries (CJK), this will not produce matches,
/// which is the correct behavior since CJK acronyms work differently.
pub fn is_acronym_match(a: &str, b: &str) -> bool {
    // Determine which is the potential acronym (shorter one)
    let (short, long) = if a.chars().count() < b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };

    // Acronym should be reasonably short (2-10 chars)
    let short_len = short.chars().count();
    if !(2..=10).contains(&short_len) {
        return false;
    }

    // Check if short form looks like an acronym (mostly uppercase letters)
    let upper_count = short.chars().filter(|c| c.is_uppercase()).count();
    let alpha_count = short.chars().filter(|c| c.is_alphabetic()).count();

    // Need at least half uppercase, and mostly alphabetic
    if upper_count < short_len / 2 || alpha_count < short_len / 2 {
        return false;
    }

    // Extract initials from long form
    let initials: String = long
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphabetic())
        .collect();

    // Compare case-insensitively
    initials.eq_ignore_ascii_case(short)
}

// =============================================================================
// Synonym Infrastructure
// =============================================================================

/// Source for synonym relationships.
///
/// Implement this trait to provide synonym lookups from various sources:
/// - UMLS MRCONSO table (medical)
/// - WordNet (general English)
/// - Wikidata aliases (multilingual)
/// - Custom domain-specific tables
///
/// The trait is designed to be composable: you can chain multiple sources.
///
/// # Example
///
/// ```rust,ignore
/// use anno_coalesce::similarity::{SynonymSource, SynonymMatch};
///
/// struct UmlsSynonyms { /* UMLS connection */ }
///
/// impl SynonymSource for UmlsSynonyms {
///     fn lookup(&self, term: &str) -> Option<SynonymMatch> {
///         // Query UMLS MRCONSO for the term
///         // Return canonical CUI and confidence
///         None
///     }
/// }
/// ```
pub trait SynonymSource: Send + Sync {
    /// Look up a term and return synonym information if found.
    ///
    /// Returns `None` if the term is not in this source.
    fn lookup(&self, term: &str) -> Option<SynonymMatch>;

    /// Check if two terms are synonyms according to this source.
    ///
    /// Default implementation looks up both terms and checks if they
    /// share a canonical ID.
    fn are_synonyms(&self, a: &str, b: &str) -> Option<SynonymMatch> {
        let match_a = self.lookup(a)?;
        let match_b = self.lookup(b)?;

        // If they share a canonical ID, they're synonyms
        if match_a.canonical_id == match_b.canonical_id {
            Some(SynonymMatch {
                canonical_id: match_a.canonical_id,
                confidence: (match_a.confidence + match_b.confidence) / 2.0,
                source: match_a.source,
            })
        } else {
            None
        }
    }

    /// Name of this synonym source for provenance tracking.
    fn source_name(&self) -> &str;
}

/// Result of a synonym lookup.
#[derive(Debug, Clone)]
pub struct SynonymMatch {
    /// Canonical identifier (e.g., UMLS CUI, WordNet synset ID)
    pub canonical_id: String,
    /// Confidence in the match [0, 1]
    pub confidence: f32,
    /// Name of the source that produced this match
    pub source: String,
}

/// Empty synonym source that never matches.
///
/// Use this as the default when no synonym sources are configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSynonyms;

impl SynonymSource for NoSynonyms {
    fn lookup(&self, _term: &str) -> Option<SynonymMatch> {
        None
    }

    fn source_name(&self) -> &str {
        "none"
    }
}

/// Chained synonym sources that try each source in order.
#[derive(Default)]
pub struct ChainedSynonyms {
    sources: Vec<Box<dyn SynonymSource>>,
}

impl ChainedSynonyms {
    /// Create a new empty chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a synonym source to the chain.
    pub fn with_source<S: SynonymSource + 'static>(mut self, source: S) -> Self {
        self.sources.push(Box::new(source));
        self
    }
}

impl SynonymSource for ChainedSynonyms {
    fn lookup(&self, term: &str) -> Option<SynonymMatch> {
        for source in &self.sources {
            if let Some(m) = source.lookup(term) {
                return Some(m);
            }
        }
        None
    }

    fn source_name(&self) -> &str {
        "chained"
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_detection_latin() {
        assert_eq!(Script::detect("Hello World"), Script::Latin);
        assert_eq!(Script::detect("Marie Curie"), Script::Latin);
    }

    #[test]
    fn test_script_detection_cjk() {
        assert_eq!(Script::detect("北京"), Script::Cjk);
        assert_eq!(Script::detect("中华人民共和国"), Script::Cjk);
    }

    #[test]
    fn test_script_detection_kana() {
        assert_eq!(Script::detect("ひらがな"), Script::Kana);
        assert_eq!(Script::detect("カタカナ"), Script::Kana);
    }

    #[test]
    fn test_script_detection_hangul() {
        assert_eq!(Script::detect("서울"), Script::Hangul);
    }

    #[test]
    fn test_script_detection_cyrillic() {
        assert_eq!(Script::detect("Москва"), Script::Cyrillic);
    }

    #[test]
    fn test_script_detection_arabic() {
        assert_eq!(Script::detect("الرياض"), Script::Arabic);
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("  Hello   World  "), "hello world");
        assert_eq!(normalize("UPPERCASE"), "uppercase");
    }

    #[test]
    fn test_similarity_identical() {
        let sim = Similarity::new();
        assert_eq!(sim.compute("test", "test"), 1.0);
        assert_eq!(sim.compute("北京", "北京"), 1.0);
    }

    #[test]
    fn test_similarity_cjk_partial() {
        let sim = Similarity::new();
        let score = sim.compute("中华人民共和国", "中华民国");
        assert!(score > 0.0 && score < 1.0, "CJK partial: {}", score);
    }

    #[test]
    fn test_similarity_english_words() {
        let sim = Similarity::new();
        let score = sim.compute("Marie Curie", "Curie");
        assert!(score > 0.0 && score < 1.0, "English partial: {}", score);
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "test"), 4);
        assert_eq!(levenshtein_distance("same", "same"), 0);
    }

    #[test]
    fn test_levenshtein_similarity() {
        assert_eq!(levenshtein_similarity("same", "same"), 1.0);
        let sim = levenshtein_similarity("kitten", "sitting");
        assert!(sim > 0.5 && sim < 1.0);
    }

    #[test]
    fn test_jaro_winkler() {
        let sim = jaro_winkler_similarity("MARTHA", "MARHTA");
        assert!(sim > 0.9, "Jaro-Winkler for similar strings: {}", sim);

        let sim = jaro_winkler_similarity("DWAYNE", "DUANE");
        assert!(sim > 0.8, "Jaro-Winkler: {}", sim);
    }

    #[test]
    fn test_multilingual_api() {
        // Verify the public API works
        let sim = multilingual_similarity("test", "test");
        assert_eq!(sim, 1.0);
    }
}

// =============================================================================
// Property Tests
// =============================================================================

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Similarity is symmetric
        #[test]
        fn similarity_symmetric(a in "\\PC{1,30}", b in "\\PC{1,30}") {
            let sim = Similarity::new();
            let ab = sim.compute(&a, &b);
            let ba = sim.compute(&b, &a);
            prop_assert!((ab - ba).abs() < 0.001,
                "Symmetry: {} vs {}", ab, ba);
        }

        /// Similarity is bounded [0, 1]
        #[test]
        fn similarity_bounded(a in "\\PC{0,50}", b in "\\PC{0,50}") {
            let sim = Similarity::new();
            let score = sim.compute(&a, &b);
            prop_assert!((0.0..=1.0).contains(&score),
                "Bounds: {}", score);
        }

        /// Identical strings have similarity 1.0
        #[test]
        fn similarity_identity(s in "\\PC{0,50}") {
            let sim = Similarity::new();
            let score = sim.compute(&s, &s);
            prop_assert!((score - 1.0).abs() < 0.001,
                "Identity: {}", score);
        }

        /// Levenshtein distance is non-negative
        #[test]
        fn levenshtein_non_negative(a in "\\PC{0,30}", b in "\\PC{0,30}") {
            let dist = levenshtein_distance(&a, &b);
            prop_assert!(dist <= a.chars().count() + b.chars().count());
        }

        /// Jaro-Winkler is bounded [0, 1]
        #[test]
        fn jaro_winkler_bounded(a in "\\PC{1,30}", b in "\\PC{1,30}") {
            let sim = jaro_winkler_similarity(&a, &b);
            prop_assert!((0.0..=1.0).contains(&sim),
                "Jaro-Winkler bounds: {}", sim);
        }
    }

    // =========================================================================
    // Acronym matching tests
    // =========================================================================

    #[test]
    fn test_acronym_who() {
        assert!(is_acronym_match("WHO", "World Health Organization"));
        assert!(is_acronym_match("World Health Organization", "WHO"));
    }

    #[test]
    fn test_acronym_mrsa() {
        assert!(is_acronym_match(
            "MRSA",
            "Methicillin-resistant Staphylococcus aureus"
        ));
    }

    #[test]
    fn test_acronym_ibm() {
        assert!(is_acronym_match("IBM", "International Business Machines"));
    }

    #[test]
    fn test_acronym_german() {
        // German acronyms work too (DDR = Deutsche Demokratische Republik)
        assert!(is_acronym_match("DDR", "Deutsche Demokratische Republik"));
        // EU works in German too
        assert!(is_acronym_match("EU", "Europäische Union"));
    }

    #[test]
    fn test_acronym_negative() {
        assert!(!is_acronym_match("IBM", "Apple"));
        assert!(!is_acronym_match("WHO", "United Nations"));
        assert!(!is_acronym_match("USA", "Canada"));
    }

    #[test]
    fn test_acronym_too_short() {
        // Single letter is not an acronym
        assert!(!is_acronym_match("A", "Apple"));
    }

    #[test]
    fn test_acronym_not_mostly_uppercase() {
        // Lowercase doesn't look like an acronym
        assert!(!is_acronym_match("who", "World Health Organization"));
    }

    // =========================================================================
    // Synonym infrastructure tests
    // =========================================================================

    #[test]
    fn test_no_synonyms_returns_none() {
        let source = NoSynonyms;
        assert!(source.lookup("test").is_none());
        assert!(source.are_synonyms("a", "b").is_none());
    }

    #[test]
    fn test_chained_synonyms_empty() {
        let chain = ChainedSynonyms::new();
        assert!(chain.lookup("test").is_none());
    }
}
