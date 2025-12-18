# Offset Handling Analysis

Data-driven analysis of byte/char offset handling in anno.

## Summary

**No new dependencies needed.** Current infrastructure is well-designed. Performance
depends on usage pattern - choose the right tool for the job.

## Benchmark Results

Run with: `cargo bench --bench offset_operations -p anno`

### Entity Text Extraction (skip/take vs map)

| Entities | skip().take() | Build Map | Winner |
|----------|---------------|-----------|--------|
| 5 | 206ns | 688ns | **skip/take 3.3x faster** |
| 10 | 365ns | 694ns | **skip/take 1.9x faster** |
| 15 | 551ns | 692ns | **skip/take 1.3x faster** |

**Insight**: Building a char→byte map costs ~600ns. For text extraction, the lazy
iterator `chars().skip(n).take(m).collect()` wins because it only iterates the
needed portion once.

### Byte↔Char Conversion (multi-lookup scenario)

| Entities | Multiple char_indices() | SpanConverter | Winner |
|----------|-------------------------|---------------|--------|
| 1 | 490ns | 5.8µs | **char_indices 12x faster** |
| 5 | 28µs | 4.5µs | **SpanConverter 6x faster** |
| 10 | 68µs | 4.3µs | **SpanConverter 16x faster** |
| 20 | 149µs | 4.8µs | **SpanConverter 31x faster** |
| 50 | 395µs | 5.0µs | **SpanConverter 78x faster** |

**Crossover point: ~2-3 entities per document.**

### Repeated Validation

| Scenario | Current | "Optimized" | Winner |
|----------|---------|-------------|--------|
| Repeated count (10x) | 93ns | 12ns | **Cached 8x faster** |

### Decision Guide

```
Need byte↔char conversion?
├── Single conversion → use char_indices().position()
├── 2+ conversions on same text → use SpanConverter
└── Just extracting text → use chars().skip().take()

Need chars().count()?
├── Called once → just call it
└── Called multiple times → cache it
```

### Why skip/take Wins for Extraction

The lazy iterator pattern `text.chars().skip(start).take(len).collect()` is efficient:
1. No upfront allocation for mapping
2. Only iterates the needed portion (early exit)
3. Single allocation for the final String
4. Compiler can optimize the iterator chain

Building maps has fixed overhead (~4-6µs for 1500 char doc) that only pays off
when you need O(log n) lookups repeatedly.

## Existing Infrastructure

### Core Types (offset.rs)

```rust
// Type-safe newtypes prevent mixing byte/char offsets
pub struct CharOffset(pub usize);
pub struct ByteOffset(pub usize);
pub struct CharRange { start: CharOffset, end: CharOffset }
pub struct ByteRange { start: ByteOffset, end: ByteOffset }

// SpanConverter - For multiple byte↔char conversions on same text
// Use when processing 2+ entities per document
let converter = SpanConverter::new(text);
let char_pos = converter.byte_to_char(byte_pos);  // O(1) lookup
let byte_pos = converter.char_to_byte(char_pos);  // O(1) lookup
```

### Entity Methods (anno-core/src/entity.rs)

```rust
// For single extraction - uses skip/take internally
entity.extract_text(&text)

// When you've already computed chars().count() - avoids recomputation
entity.extract_text_with_len(&text, cached_len)
entity.validate_with_len(&text, cached_len)
```

### HuggingFace tokenizers

```rust
// Tokenizers provides char offsets directly - no conversion needed!
let offsets = encoding.get_offsets();
let (char_start, char_end) = offsets[token_idx];
```

## Code Pattern Analysis

ast-grep found 140 `chars().count()` calls across the codebase.

### Pattern 1: Cache count for validation (RECOMMENDED)

```rust
// GOOD: Cache when checking bounds for multiple entities
let char_count = text.chars().count();  // O(n) once
for entity in &entities {
    if entity.end > char_count { /* ... */ }  // O(1) each
}
```

### Pattern 2: skip/take for extraction (RECOMMENDED)

```rust
// GOOD: Building a map just to extract is slower than skip/take
let entity_text: String = text.chars().skip(start).take(end - start).collect();
```

### Pattern 3: SpanConverter for 2+ byte↔char conversions (RECOMMENDED)

```rust
// GOOD: When processing multiple entities from same document
let converter = SpanConverter::new(&doc.text);
for entity in &entities {
    let char_start = converter.byte_to_char(entity.start);  // O(1)
    let char_end = converter.byte_to_char(entity.end);      // O(1)
    // ... use char offsets
}
```

### Pattern 4: Single char_indices() call (OK for 1 lookup)

```rust
// OK for single lookup, but use SpanConverter if you need 2+ lookups
let entity_start_char = text
    .char_indices()
    .position(|(byte_idx, _)| byte_idx >= entity.start)
    .unwrap_or(0);
```

### Crossdoc.rs Verbose Mode

The verbose display in `crossdoc.rs` (lines 911-995) uses ~10 `char_indices()` calls
per entity. This is suboptimal for documents with many mentions, but:
- It's only in verbose mode (not hot path)
- The display loop already limits to 3 mentions unless `--verbose`
- Could use SpanConverter for speedup if needed

**Potential optimization (not urgent):**
```rust
// Before loop over mentions in a document:
let converter = SpanConverter::new(&doc.text);
// Then use converter.byte_to_char() instead of char_indices().position()
```

## Unicode Coverage

### Performance by Script (chars().count() throughput)

| Script | Bytes/Char | Time per 16 chars | Throughput |
|--------|------------|-------------------|------------|
| ASCII | 1 | 2.9ns | 8.7 GiB/s |
| Latin Extended | ~1.1 | 4.6ns | 5.7 GiB/s |
| Cyrillic | 2 | 5.0ns | 5.3 GiB/s |
| CJK | 3 | 3.2ns | 8.3 GiB/s |
| Emoji | 4 | 3.6ns | 7.3 GiB/s |

**Insight**: CJK is surprisingly fast (per character) because UTF-8 decoding is
branchless for 3-byte sequences. The throughput difference is mostly due to
bytes/char ratio, not decoding overhead.

### Already tested

- Multi-byte UTF-8 (CJK, Cyrillic, Arabic)
- Emoji sequences (👨‍👩‍👧, 🇺🇸)
- Combining characters (café vs café)
- Mixed scripts (Tokyo 東京)

See: `tests/die_hard.rs`, `tests/unicode_edge_cases.rs`, `anno/tests/discourse_proptest.rs`

### Grapheme vs Char

| Text | Bytes | Chars | Graphemes |
|------|-------|-------|-----------|
| "Hello" | 5 | 5 | 5 |
| "café" (precomposed) | 5 | 4 | 4 |
| "café" (decomposed) | 6 | 5 | 4 |
| "👨‍👩‍👧" | 18 | 5 | 1 |

**Current decision**: Use char offsets (Unicode scalar values). This matches:
- HuggingFace tokenizers output
- Standard NER evaluation tools
- Most downstream consumers

Grapheme segmentation would only matter if anno needed to split text ON grapheme
boundaries (e.g., for chunking). For span extraction, char offsets work correctly.

## Dependency Recommendations

| Dependency | Add? | Reason |
|------------|------|--------|
| `unicode-segmentation` | No | Not needed for span extraction |
| `bstr` | No | UTF-8 sources already validated |
| `unicode-normalization` | Maybe later | Only if datasets have mixed NFC/NFD |

## When to Revisit

Consider adding dependencies if:
1. Need to split text ON grapheme boundaries (chunking)
2. Ingesting raw files with unknown/invalid UTF-8
3. Performance profiling shows offset operations as bottleneck (unlikely based on benchmarks)
4. Downstream systems require grapheme-based offsets

## Files Reference

- `anno/src/offset.rs` - Core offset types and SpanConverter
- `anno-core/src/entity.rs` - Entity extraction methods  
- `anno/benches/offset_operations.rs` - Benchmarks proving these claims

## Profiling Commands

```bash
# Run offset benchmarks
cargo bench --bench offset_operations -p anno --features eval

# Quick adhoc profiling (if benchmarks don't compile due to broken imports)
rustc -O /tmp/bench.rs -o /tmp/bench && /tmp/bench

# Profile specific code patterns with ast-grep
ast-grep --pattern '$_.chars().count()' --lang rust anno/src/
```

## Known Issues

- Some files (`edit_distance.rs`, `eval/cluster_encoder.rs`) are orphaned (not in mod.rs)
- This can cause compile errors when features pull in modules that import them
- Benchmark runs fine with correct feature combinations
