//! Benchmark offset operations to motivate design decisions.
//!
//! Based on ast-grep analysis of actual patterns in the codebase:
//! - 140 matches for `.chars().count()` across the codebase
//! - 17 matches for `.chars().skip().take().collect()` (expensive!)
//! - Heavy use in: onnx.rs, crf.rs, crossdoc.rs, explain.rs
//!
//! Key questions these benchmarks answer:
//! 1. How expensive is chars().count() on typical NER text?
//! 2. How much does caching char->byte map save?
//! 3. What's the cost of the crossdoc.rs pattern (8+ char_indices() on same doc)?
//!
//! # Usage
//!
//! ```bash
//! cargo bench --bench offset_operations -p anno
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// =============================================================================
// Test Data - Representative of real NER workloads
// =============================================================================

/// Short text - typical single sentence NER
const TEXT_SHORT: &str = "John Smith works at Apple Inc. in California.";

/// Medium text - typical paragraph with multiple entities (like onnx.rs processes)
const TEXT_MEDIUM: &str = "Meeting scheduled for January 15, 2025 at $500 per hour. \
    Apple Inc. announced new products in Cupertino, California. \
    CEO Tim Cook presented the iPhone 16 and MacBook Pro. \
    The event was held at the Steve Jobs Theater.";

/// CJK text - tests Unicode overhead (from multilingual.mdc guidelines)
const TEXT_CJK: &str = "習近平在北京會見了普京。這是一個重要的會議。\
    中國和俄羅斯討論了經濟合作。兩國領導人簽署了多項協議。";

/// Mixed text - real-world multilingual (from e2e_unicode tests)
const TEXT_MIXED: &str = "Tokyo (東京) is the capital of Japan (日本). \
    Москва is the capital of Россия. \
    François Müller met José García in São Paulo.";

/// Long document - like crossdoc.rs processes
const TEXT_LONG: &str = "The European Union announced today that new regulations will come into effect. \
    Commission President Ursula von der Leyen stated that the Digital Markets Act represents a landmark achievement. \
    The legislation targets major technology companies including Apple, Google, Amazon, and Meta. \
    Brussels-based officials expect implementation to begin in January 2025. \
    Industry representatives from Silicon Valley expressed concerns during hearings in Strasbourg. \
    The regulation affects companies with market capitalization exceeding €75 billion. \
    Similar measures are being considered in Washington, Tokyo, and Beijing. \
    Critics argue the rules may stifle innovation, while supporters cite consumer protection benefits. \
    The vote passed with 588 members in favor, 11 against, and 31 abstentions.";

// =============================================================================
// Benchmark: chars().count() - the most common pattern (140 matches)
// =============================================================================

fn bench_chars_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("chars_count");

    let texts = [
        ("short_45b", TEXT_SHORT),
        ("medium_250b", TEXT_MEDIUM),
        ("cjk_160b", TEXT_CJK),
        ("mixed_180b", TEXT_MIXED),
        ("long_900b", TEXT_LONG),
    ];

    for (name, text) in texts {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), text, |b, t| {
            b.iter(|| black_box(t.chars().count()))
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark: Real pattern from onnx.rs - extract N entities from text
// Pattern: cache chars().count(), but still do skip().take() per entity
// =============================================================================

fn bench_onnx_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("onnx_entity_extraction");

    // Simulate extracting 5-15 entities (typical for NER on medium text)
    let entity_counts = [5, 10, 15];

    for num_entities in entity_counts {
        let text = TEXT_MEDIUM;
        let char_count = text.chars().count();

        // Generate realistic entity positions
        let positions: Vec<(usize, usize)> = (0..num_entities)
            .map(|i| {
                let start = (i * char_count / (num_entities + 2)).min(char_count - 1);
                let end = (start + 8).min(char_count); // ~8 chars per entity
                (start, end)
            })
            .collect();

        // Current onnx.rs pattern: cache count, skip/take each entity
        group.bench_with_input(
            BenchmarkId::new("current_skip_take", num_entities),
            &positions,
            |b, pos| {
                b.iter(|| {
                    let _char_count = text.chars().count(); // cached
                    for &(start, end) in pos {
                        let extracted: String =
                            text.chars().skip(start).take(end - start).collect();
                        black_box(extracted);
                    }
                })
            },
        );

        // Improved: build char->byte map once, slice directly
        group.bench_with_input(
            BenchmarkId::new("cached_byte_map", num_entities),
            &positions,
            |b, pos| {
                b.iter(|| {
                    // Build map once
                    let char_to_byte: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
                    let text_len = text.len();

                    for &(start, end) in pos {
                        let byte_start = char_to_byte.get(start).copied().unwrap_or(0);
                        let byte_end = char_to_byte.get(end).copied().unwrap_or(text_len);
                        let extracted = &text[byte_start..byte_end];
                        black_box(extracted);
                    }
                })
            },
        );
    }

    group.finish();
}

// =============================================================================
// Benchmark: Real pattern from crossdoc.rs - TERRIBLE pattern
// Does char_indices() 8+ times on same document!
// =============================================================================

fn bench_crossdoc_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossdoc_context_extraction");

    let text = TEXT_LONG;

    // Simulate extracting context for one entity mention (like crossdoc.rs does)
    // Entity at byte position ~100-110
    let entity_byte_start = 100;
    let entity_byte_end = 115;
    let context_window = 50;

    // Current crossdoc.rs pattern: 8+ char_indices() calls!
    group.bench_function("current_8x_char_indices", |b| {
        b.iter(|| {
            // 1. Find entity_start_char
            let entity_start_char = text
                .char_indices()
                .position(|(byte_idx, _)| byte_idx >= entity_byte_start)
                .unwrap_or(0);

            // 2. Find entity_end_char
            let entity_end_char = text
                .char_indices()
                .position(|(byte_idx, _)| byte_idx >= entity_byte_end)
                .unwrap_or(text.chars().count()); // 3. chars().count()

            // 4. Calculate context range
            let context_start_char = entity_start_char.saturating_sub(context_window);
            let context_end_char = (entity_end_char + context_window).min(text.chars().count()); // 5. chars().count() again!

            // 6. Convert back to bytes - safe_start
            let safe_start = text
                .char_indices()
                .nth(context_start_char)
                .map(|(byte_idx, _)| byte_idx)
                .unwrap_or(0);

            // 7. Convert back to bytes - safe_end
            let safe_end = text
                .char_indices()
                .nth(context_end_char)
                .map(|(byte_idx, _)| byte_idx)
                .unwrap_or(text.len());

            let context = &text[safe_start..safe_end];

            // 8. More char_indices for entity boundaries...
            let _entity_start_byte = text
                .char_indices()
                .find(|&(byte_idx, _)| byte_idx >= entity_byte_start)
                .map(|(byte_idx, _)| byte_idx)
                .unwrap_or(entity_byte_start);

            black_box(context)
        })
    });

    // Improved: build mapping once
    group.bench_function("improved_single_map", |b| {
        b.iter(|| {
            // Build both mappings once
            let char_to_byte: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            let byte_to_char: Vec<usize> = {
                let mut map = vec![0; text.len() + 1];
                for (char_idx, (byte_idx, _)) in text.char_indices().enumerate() {
                    map[byte_idx] = char_idx;
                }
                // Fill in gaps for multi-byte chars
                let mut last = 0;
                for (i, val) in map.iter_mut().enumerate() {
                    if i < text.len() && *val == 0 && i > 0 {
                        *val = last;
                    } else {
                        last = *val;
                    }
                }
                map[text.len()] = text.chars().count();
                map
            };
            let char_count = char_to_byte.len();
            let text_len = text.len();

            // Now all lookups are O(1)
            let entity_start_char = byte_to_char
                .get(entity_byte_start.min(text_len))
                .copied()
                .unwrap_or(0);
            let entity_end_char = byte_to_char
                .get(entity_byte_end.min(text_len))
                .copied()
                .unwrap_or(char_count);

            let context_start_char = entity_start_char.saturating_sub(context_window);
            let context_end_char = (entity_end_char + context_window).min(char_count);

            let safe_start = char_to_byte.get(context_start_char).copied().unwrap_or(0);
            let safe_end = char_to_byte
                .get(context_end_char)
                .copied()
                .unwrap_or(text_len);

            let context = &text[safe_start..safe_end];
            black_box(context)
        })
    });

    group.finish();
}

// =============================================================================
// Benchmark: Repeated count vs cached (validation patterns)
// =============================================================================

fn bench_repeated_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("repeated_validation");

    let text = TEXT_MEDIUM;

    // Pattern seen in lib.rs validate_entities: check bounds for each entity
    let num_entities = 10;

    group.bench_function("repeated_count_10x", |b| {
        b.iter(|| {
            for _ in 0..num_entities {
                let count = text.chars().count();
                black_box(count);
            }
        })
    });

    group.bench_function("cached_count_10x", |b| {
        b.iter(|| {
            let count = text.chars().count();
            for _ in 0..num_entities {
                black_box(count);
            }
        })
    });

    group.finish();
}

// =============================================================================
// Benchmark: Byte/char ratio impact on performance
// =============================================================================

fn bench_unicode_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("unicode_overhead");

    // Compare same logical operations on different scripts
    let texts = [
        ("ascii_1.0", "Hello World Test"),      // ratio 1.0
        ("latin_ext_1.1", "Naïve café résumé"), // ratio ~1.1
        ("cyrillic_2.0", "Москва столица"),     // ratio 2.0
        ("cjk_3.0", "習近平在北京"),            // ratio 3.0
        ("emoji_4.0", "Party 🎉🎊😀"),          // ratio ~2.5
    ];

    for (name, text) in texts {
        let bytes = text.len();
        let chars = text.chars().count();
        let ratio = bytes as f64 / chars as f64;

        group.throughput(Throughput::Elements(chars as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("chars_count_{:.1}", ratio), name),
            text,
            |b, t| b.iter(|| black_box(t.chars().count())),
        );
    }

    group.finish();
}

// =============================================================================
// Criterion Setup
// =============================================================================

/// Benchmark group for offset operations.
criterion_group!(
    benches,
    bench_chars_count,
    bench_onnx_pattern,
    bench_crossdoc_pattern,
    bench_repeated_validation,
    bench_unicode_overhead,
);

criterion_main!(benches);
