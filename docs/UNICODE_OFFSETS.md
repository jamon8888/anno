# Unicode Offset Handling

Entity spans in `anno` use **character offsets**, not byte offsets. This document explains the distinction and how the codebase handles Unicode text correctly.

## Type-Safe Offsets (Recommended)

**New in v0.3:** Use type-safe wrappers to prevent byte/char confusion at compile time:

```rust
use anno::offset::{ByteOffset, CharOffset, ByteSpan, CharSpan};

// Regex returns byte offsets
let byte_start = ByteOffset::new(regex_match.start());
let byte_end = ByteOffset::new(regex_match.end());
let byte_span = ByteSpan::new(byte_start, byte_end);

// Convert to character offsets for Entity
let char_span: CharSpan = byte_span.to_char_span(text);

// Use in Entity construction
Entity::new(
    matched_text,
    entity_type,
    char_span.start_usize(),  // Correct character offset
    char_span.end_usize(),    // Correct character offset
    confidence,
)
```

This prevents the most common bugs:
- **Compile-time safety**: Can't accidentally pass `ByteOffset` where `CharOffset` is expected
- **Explicit conversion**: Must call `.to_char()` or `.to_char_span()` with the text
- **Display hints**: `ByteOffset(42)` prints as `"42b"`, `CharOffset(10)` prints as `"10c"`

## The Problem

In Rust, `String::len()` returns **bytes**, but NER tools need **character** positions:

```text
Text: "北京 Beijing"

Byte view:  [北: 0-2][京: 3-5][ : 6][B: 7][e: 8][i: 9][j: 10][i: 11][n: 12][g: 13]
            ↑ 3 bytes  ↑ 3 bytes   ↑ 1 byte   ... 7 ASCII bytes ...
            Total: 14 bytes

Char view:  [北: 0][京: 1][ : 2][B: 3][e: 4][i: 5][j: 6][i: 7][n: 8][g: 9]
            Total: 10 characters
```

If "Beijing" is found at byte offset 7 with byte length 7, that gives end=14. But the text only has 10 characters! Without proper conversion, evaluations and downstream tools break.

## Solution: `SpanConverter`

The `anno::offset::SpanConverter` provides efficient byte↔char conversion:

```rust
use anno::offset::SpanConverter;

let text = "北京 Beijing";
let converter = SpanConverter::new(text);

// Regex finds "Beijing" at byte offset 7
let byte_start = 7;
let byte_end = 14; // 7 + "Beijing".len()

// Convert to character offsets
let char_start = converter.byte_to_char(byte_start); // → 3
let char_end = converter.byte_to_char(byte_end);     // → 10

// Now: Entity { start: 3, end: 10 } is valid character span
```

## Performance

`SpanConverter` pre-computes lookup tables, making each conversion O(1):

| Operation | Without converter | With converter |
|-----------|------------------|----------------|
| First conversion | O(n) | O(n) to build |
| Each additional | O(n) | O(1) lookup |
| 10 conversions | O(10n) | O(n + 10) |

For ASCII text, the converter detects this and skips table-building entirely.

## Backend Usage

All backends that return byte offsets must convert them:

### ✅ Correct (RegexNER, CrfNER, HeuristicNER)

```rust
use crate::offset::SpanConverter;

fn extract_entities(&self, text: &str, ...) -> Result<Vec<Entity>> {
    let converter = SpanConverter::new(text);
    
    for match_ in regex.find_iter(text) {
        let char_start = converter.byte_to_char(match_.start());
        let char_end = converter.byte_to_char(match_.end());
        
        entities.push(Entity::new(
            match_.as_str(),
            entity_type,
            char_start,  // ← Character offset
            char_end,    // ← Character offset
            confidence,
        ));
    }
}
```

### ❌ Incorrect (Common Bug)

```rust
// BUG: Uses byte offsets directly!
for match_ in regex.find_iter(text) {
    entities.push(Entity::new(
        match_.as_str(),
        entity_type,
        match_.start(),  // ← Byte offset (WRONG)
        match_.end(),    // ← Byte offset (WRONG)
        confidence,
    ));
}
```

## Entity Contract

The `Entity` struct documents the offset semantics:

```rust
/// Entity extracted from text.
pub struct Entity {
    /// Entity text content
    pub text: String,
    /// Start position (CHARACTER offset, inclusive)
    pub start: usize,
    /// End position (CHARACTER offset, exclusive)
    pub end: usize,
    // ...
}
```

## Testing Unicode Handling

Property tests verify all backends return valid character offsets:

```rust
#[test]
fn backends_valid_char_offsets(text in arb_unicode_text()) {
    let entities = backend.extract_entities(&text, None)?;
    let char_count = text.chars().count();
    
    for entity in &entities {
        assert!(entity.start <= entity.end);
        assert!(entity.end <= char_count);
    }
}
```

Test cases include:
- CJK: `"北京 Beijing"` (mixed multi-byte + ASCII)
- Emoji: `"Hello 👋 World"` (4-byte codepoints)
- Currency: `"€50 £25"` (2-3 byte symbols)
- Combining chars: `"café"` vs `"café"` (composed vs decomposed)

## Common Pitfalls

### 1. Using `String::len()` for span length

```rust
// WRONG: len() returns bytes
let end = start + entity_text.len();

// CORRECT: count characters
let end = start + entity_text.chars().count();
```

### 2. Using `text.find()` without conversion

```rust
// WRONG: find() returns byte offset
if let Some(pos) = text.find(&entity_text) {
    Entity::new(entity_text, typ, pos, pos + entity_text.len(), conf)
}

// CORRECT: convert byte→char
let converter = SpanConverter::new(text);
if let Some(byte_pos) = text.find(&entity_text) {
    let char_start = converter.byte_to_char(byte_pos);
    let char_end = converter.byte_to_char(byte_pos + entity_text.len());
    Entity::new(entity_text, typ, char_start, char_end, conf)
}
```

### 3. Creating SpanConverter inside loops

```rust
// WRONG: O(n * m) where m = number of patterns
for pattern in patterns {
    let converter = SpanConverter::new(text);  // Rebuilds every iteration!
    // ...
}

// CORRECT: O(n + m)
let converter = SpanConverter::new(text);  // Build once
for pattern in patterns {
    // Use existing converter
}
```

## Related Modules

- `anno::offset` - Core offset conversion utilities
- `anno::backends::span_utils` - Span tensor generation for ML backends
- `anno_core::Entity` - Entity struct with offset contract

## References

- [The Rust Book: Strings](https://doc.rust-lang.org/book/ch08-02-strings.html)
- [UTF-8 Encoding](https://en.wikipedia.org/wiki/UTF-8)

