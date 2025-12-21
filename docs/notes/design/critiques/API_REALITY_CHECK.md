# API Reality Check: Gaps Between Our Types and Multilingual NLP Reality

This document critically examines our current API against the true complexity of
multilingual NLP tasks.

## The Good News First

**The core ML system is actually well-designed for multilingual use.**

The NER backends (GLiNER, Candle, ONNX) use:
- **Transformer tokenizers** trained on multilingual data
- **Embedding-based matching** that works across languages
- **Zero-shot labels** - you can use `["人物", "地點"]` or `["person", "location"]`

```rust
// This works for ANY language:
let entities = gliner.extract(
    "習近平在北京會見了普京",
    &["人物", "地點", "組織"],  // Chinese labels
    0.5,
)?;
```

The issues below are mostly in **supplementary modules** (keywords, summarization)
that use old-school statistical methods, not the core ML system.

---

## Critical Issues in Supplementary Modules

### 1. Keywords Module: English-Only by Design

**Problem:** Our `keywords.rs` hardcodes English stopwords and assumes whitespace tokenization.

```rust
// Current (broken for non-English):
pub const STOPWORDS: &[&str] = &["a", "about", "above", ...];  // English only!

// Tokenization:
text.split(|c: char| !c.is_alphanumeric())  // Fails for CJK, Arabic
```

**Languages this breaks:**
- **Chinese/Japanese**: No word boundaries, needs segmentation (jieba, MeCab)
- **Korean**: Agglutinative, needs morphological analysis
- **Arabic**: Rich morphology, clitics attached to words
- **Turkish/Finnish**: Extremely agglutinative (one word = one sentence)
- **Thai**: No spaces between words

**What we should have:**

```rust
pub trait Tokenizer: Send + Sync {
    fn tokenize(&self, text: &str) -> Vec<Token>;
    fn is_stopword(&self, token: &Token) -> bool;
}

pub struct Token {
    pub surface: String,      // Raw surface form
    pub lemma: Option<String>, // Normalized form
    pub pos: Option<String>,   // Part of speech
    pub start: usize,
    pub end: usize,
}

// Language-specific implementations
pub struct WhitespaceTokenizer;    // English, etc.
pub struct JiebaTokenizer;         // Chinese
pub struct MecabTokenizer;         // Japanese
pub struct KonlpyTokenizer;        // Korean
pub struct UnicodeSegmenter;       // Fallback using UAX#29
```

**Verdict:** Our keyword extraction is **English-only**. The API doesn't even
accept a language parameter. This is a design flaw, not a missing feature.

---

### 2. Canonical Mention Selection: Western Name Assumptions

**Problem:** Our `MentionType` detection uses capitalization heuristics.

```rust
// Current (broken for most of the world):
pub fn detect_mention_type(text: &str) -> MentionType {
    if text.chars().next().map_or(false, |c| c.is_uppercase()) {
        MentionType::Named  // Assumes capitalization = proper noun
    }
}
```

**Languages this breaks:**
- **Chinese/Japanese/Korean**: No capitalization concept
- **Arabic/Hebrew**: Different orthographic conventions
- **German**: All nouns are capitalized (false positives)

**What we should have:**

```rust
pub trait MentionTypeDetector: Send + Sync {
    fn detect(&self, text: &str, context: &MentionContext) -> MentionType;
}

pub struct MentionContext {
    pub language: Option<LanguageTag>,
    pub sentence_position: usize,  // Is this sentence-initial?
    pub pos_tag: Option<String>,   // Part of speech from tagger
    pub ner_type: Option<EntityType>,  // From NER model
}
```

**Verdict:** We can't reliably distinguish Named/Nominal/Pronominal without
language-specific POS tagging or NER. Our heuristic is **wrong for most languages**.

---

### 3. Entity Salience: Missing Key Features

**Problem:** Our `EntityRanker` trait doesn't capture important salience features:

```rust
// Current API:
fn rank(&self, text: &str, entities: &[Entity]) -> Vec<(Entity, f64)>;
```

**Missing features for real salience:**
- **Grammatical role**: Subject entities are more salient than objects
- **Definiteness**: "The president" vs "a president" (varies by language)
- **Information structure**: Topic vs focus position
- **Discourse relations**: First mention in new discourse segment
- **Zero pronouns**: In pro-drop languages (Spanish, Japanese, Korean), the
  most salient entity is often **not mentioned at all**

**What we should have:**

```rust
pub trait EntityRanker {
    fn rank(&self, doc: &AnnotatedDocument, entities: &[Entity]) -> Vec<ScoredEntity>;
}

pub struct AnnotatedDocument {
    pub text: String,
    pub language: LanguageTag,
    pub sentences: Vec<Sentence>,
    pub discourse_segments: Vec<DiscourseSegment>,
    pub coreference_chains: Vec<CorefChain>,
}

pub struct ScoredEntity {
    pub entity: Entity,
    pub salience: f64,
    pub features: SalienceFeatures,
}

pub struct SalienceFeatures {
    pub grammatical_role: GrammaticalRole,  // Subject, Object, Oblique
    pub information_status: InfoStatus,      // Given, New, Accessible
    pub discourse_prominence: f64,           // Position in discourse structure
    pub frequency: usize,
    pub first_mention_position: usize,
}
```

**Verdict:** Our salience ranking works for English news text but misses
**grammatical and discourse features** that are critical for other genres
and languages.

---

### 4. Coreference: Zero Pronouns Are Invisible

**Problem:** Our coreference model assumes all mentions have surface forms.

```rust
// Current (broken for pro-drop):
pub struct MentionFeatures {
    pub surface: String,  // What if there's no surface form?
    pub position: usize,
    pub mention_type: MentionType,
}
```

**Languages this breaks:**
- **Spanish**: "Vino a casa" (He/She came home) - subject is zero
- **Japanese**: Subject and object routinely omitted
- **Korean**: Very pro-drop, relies on context
- **Chinese**: Topic-drop language
- **Italian**: Subject pro-drop
- **Arabic**: Subject pro-drop PLUS morphological complexity (see below)

In these languages, the most salient entity often has **no mention in the text**
at a given point. Our API can't represent this.

**Arabic-specific challenge**: Arabic is pro-drop AND agglutinative. Pronominal
clitics attach to words, so a single token may contain multiple mentions:

```
وأبوه = و + أب + ه = "and" + "father" + "his"
       ^   ^     ^
       |   |     └── possessive pronoun (mention!)
       |   └── noun
       └── conjunction
```

Standard tokenization misses clitics. Morpheme-aware tokenization (ATB/Farasa)
is required for correct mention detection. OntoNotes Arabic uses morpheme
boundaries; ACE uses different conventions.

**What we should have:**

```rust
pub enum Mention {
    Overt(OvertMention),   // Has surface form
    Zero(ZeroMention),     // No surface form (pro-drop)
}

pub struct OvertMention {
    pub surface: String,
    pub span: (usize, usize),
    pub mention_type: MentionType,
}

pub struct ZeroMention {
    pub position: usize,           // Where it "would be"
    pub grammatical_role: Role,    // What role it fills
    pub recovered_entity: Option<EntityRef>,  // What entity it refers to
}
```

**Update (2025):** Zero pronoun support has been implemented:

```rust
use anno::eval::coref::{Mention, GrammaticalRole};
use anno_core::{MentionType, PhiFeatures, Person, Number, Gender};

// Create a zero mention for Arabic pro-drop
let zero = Mention::zero(
    0, // anchor position (where the zero "would be")
    PhiFeatures::new(Person::Third, Number::Singular, Gender::Masculine),
    GrammaticalRole::Subject,
);
assert!(zero.is_zero());
assert_eq!(zero.mention_type, Some(MentionType::Zero));
```

**Verdict:** Zero pronoun support is now available via `MentionType::Zero` and
`Mention::zero()`. This enables representation of pro-drop for Arabic, Spanish,
Japanese, Korean, Chinese, and other languages.

---

### 5. Summarization: Position Bias is Cultural

**Problem:** Our `PositionSummarizer` assumes "inverted pyramid" structure.

```rust
// Current assumption:
// "Earlier sentences are more important" - only true for news articles!
fn summarize(&self, text: &str, num_sentences: usize) -> Vec<String> {
    split_sentences(text).into_iter().take(num_sentences).collect()
}
```

**Where this fails:**
- **Academic papers**: Important info is in abstract AND conclusion
- **Narrative text**: Climax is in the middle/end
- **Japanese business letters**: Key information comes last (cultural norm)
- **Some Arabic genres**: Rhetorical structure differs

**What we should have:**

```rust
pub struct SummarizationConfig {
    pub language: LanguageTag,
    pub genre: Genre,  // News, Academic, Narrative, Business, etc.
    pub position_model: PositionModel,  // InvertedPyramid, Climactic, etc.
}

pub enum Genre {
    News,
    Academic,
    Narrative,
    Business,
    Legal,
    Social,  // Social media posts
    Technical,
}
```

**Verdict:** Position-based summarization is **genre and culture dependent**.
One algorithm doesn't fit all.

---

### 6. Sentence Splitting: Utterly Naive

**Problem:** We split on `.!?` which is English-centric.

```rust
// Current (broken):
text.split(|c| c == '.' || c == '!' || c == '?')
```

**Languages this breaks:**
- **Chinese**: Uses `。` (full stop), `？` (question mark), `！` (exclamation)
- **Thai**: Uses `ฯ` and spaces differently
- **Japanese**: Uses `。` and can have `・` for lists
- **Hindi/Arabic**: Different punctuation marks
- **Abbreviations**: "Dr. Smith" becomes two sentences

**What we should have:**

```rust
pub trait SentenceSegmenter: Send + Sync {
    fn segment(&self, text: &str) -> Vec<Sentence>;
}

pub struct Sentence {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub is_complete: bool,  // vs fragment
}

// Use ICU BreakIterator or unicode-segmentation crate
pub struct IcuSentenceSegmenter {
    locale: icu::locid::Locale,
}
```

**Verdict:** Our sentence splitting is **broken for non-Latin scripts**.

---

## What's Actually Good

Despite the above, we did some things right:

### 1. Entity struct has `kb_id` for linking
```rust
pub kb_id: Option<String>,  // External KB link - good!
```

### 2. Character offsets, not byte offsets
```rust
/// Start position (character offset, NOT byte offset).
pub start: usize,
```

### 3. Discontinuous span support
```rust
pub discontinuous_span: Option<DiscontinuousSpan>,  // For "New ... York"
```

### 4. Ontology normalization exists
```rust
pub use ontology::{normalize, is_known, CoreType, ...};
```

### 5. LanguageTag is in the codebase
```rust
// In various places, language is parameterized
pub language: Option<LanguageTag>,
```

---

## Recommendations

### Short Term (Pragmatic)

1. **Add `language` parameter to all trait methods**
   ```rust
   fn extract(&self, text: &str, language: &str, max_keywords: usize) -> Vec<(String, f64)>;
   ```

2. **Make stopwords configurable**
   ```rust
   pub fn with_stopwords(mut self, stopwords: HashSet<String>) -> Self
   ```
   This already exists but isn't used by default.

3. **Document English-only limitations clearly**
   ```rust
   //! # Warning: English Only
   //!
   //! This implementation uses English stopwords and whitespace tokenization.
   //! For other languages, provide custom stopwords via `with_stopwords()`.
   ```

4. **Use `unicode-segmentation` for sentence splitting**
   ```rust
   use unicode_segmentation::UnicodeSegmentation;
   text.split_sentence_bounds()
   ```

### Medium Term (Proper Multilingual)

1. **Define `Tokenizer` trait hierarchy**
2. **Integrate language detection**
3. **Use ICU for sentence segmentation**
4. **Add zero-mention support to coreference**

### Long Term (Research-Aligned)

1. **Discourse-aware salience with RST/SDRT**
2. **Cross-lingual entity alignment**
3. **Culture-aware summarization models**
4. **Typological features database** (WALS-informed)

---

## Conclusion

Our APIs work for **English news/technical text**. They will produce wrong results
for:
- Non-whitespace-delimited languages (Chinese, Japanese, Thai)
- Pro-drop languages (Spanish, Japanese, Korean)
- Languages with different punctuation (Chinese, Arabic)
- Genres other than news/technical (narrative, social media)

The fix isn't just "add more stopword lists" — it requires making tokenization,
segmentation, and mention representation **language-parameterized at the type level**.

**This is honest assessment, not criticism of the work done.** We built useful
English NLP tools. Making them truly multilingual requires design changes.

