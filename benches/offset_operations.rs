//! Benchmark offset operations to motivate design decisions.
//!
//! Key questions:
//! 1. How expensive is chars().count() on typical NER text?
//! 2. How much does caching char->byte map save?
//! 3. What's the cost of SpanConverter vs ad-hoc conversion?
//!
//! # Usage
//!
//! ```bash
//! cargo bench --bench offset_operations -p anno
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// =============================================================================
// Test Data
// =============================================================================

const ASCII_SHORT: &str = "John Smith works at Apple Inc. in California.";

const ASCII_MEDIUM: &str = "The quick brown fox jumps over the lazy dog. \
    Barack Obama was the 44th President of the United States. \
    He served from 2009 to 2017. Michelle Obama is his wife. \
    They have two daughters, Malia and Sasha. The family lived in Chicago.";

const UNICODE_CJK: &str = "習近平在北京會見了普京。這是一個重要的會議。\
    中國和俄羅斯討論了經濟合作。兩國領導人簽署了多項協議。\
    會議在人民大會堂舉行。雙方對會談結果表示滿意。";

const UNICODE_MIXED: &str = "Tokyo (東京) is the capital of Japan (日本). \
    Москва is the capital of Россия. \
    François Müller met José García in São Paulo. \
    The café serves naïve résumé reviews.";

const UNICODE_EMOJI: &str = "Party 🎉 time! This is fun 😀. Let's celebrate 🎊. \
    The weather is ☀️ sunny. I love 🍕 pizza. \
    Going to the 🏠 house. Family 👨‍👩‍👧 together.";

// =============================================================================
// Benchmarks
// =============================================================================

fn bench_chars_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("chars_count");

    let texts = [
        ("ascii_short", ASCII_SHORT),
        ("ascii_medium", ASCII_MEDIUM),
        ("unicode_cjk", UNICODE_CJK),
        ("unicode_mixed", UNICODE_MIXED),
        ("unicode_emoji", UNICODE_EMOJI),
    ];

    for (name, text) in texts {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::new("chars().count()", name), text, |b, t| {
            b.iter(|| black_box(t.chars().count()))
        });
    }

    group.finish();
}

fn bench_char_indices(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_indices");

    let texts = [
        ("ascii_short", ASCII_SHORT),
        ("ascii_medium", ASCII_MEDIUM),
        ("unicode_cjk", UNICODE_CJK),
        ("unicode_mixed", UNICODE_MIXED),
    ];

    for (name, text) in texts {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("char_indices().count()", name),
            text,
            |b, t| b.iter(|| black_box(t.char_indices().count())),
        );
    }

    group.finish();
}

fn bench_build_char_to_byte_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_map");

    let texts = [
        ("ascii_short", ASCII_SHORT),
        ("ascii_medium", ASCII_MEDIUM),
        ("unicode_cjk", UNICODE_CJK),
        ("unicode_mixed", UNICODE_MIXED),
    ];

    for (name, text) in texts {
        group.throughput(Throughput::Bytes(text.len() as u64));

        // Build char->byte map (what DiscourseScope does)
        group.bench_with_input(BenchmarkId::new("Vec<usize>", name), text, |b, t| {
            b.iter(|| {
                let map: Vec<usize> = t.char_indices().map(|(i, _)| i).collect();
                black_box(map)
            })
        });
    }

    group.finish();
}

fn bench_extract_span_naive_vs_cached(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_span");

    // Simulate extracting 10 entities from text (common NER scenario)
    let texts = [
        ("ascii_medium", ASCII_MEDIUM),
        ("unicode_cjk", UNICODE_CJK),
        ("unicode_mixed", UNICODE_MIXED),
    ];

    for (name, text) in texts {
        let char_count = text.chars().count();
        let num_extractions = 10;

        // Generate extraction positions
        let positions: Vec<(usize, usize)> = (0..num_extractions)
            .map(|i| {
                let start = (i * char_count / (num_extractions + 2)).min(char_count - 1);
                let end = (start + 5).min(char_count);
                (start, end)
            })
            .collect();

        // Naive: recalculate for each extraction
        group.bench_with_input(BenchmarkId::new("naive_10x", name), text, |b, t| {
            b.iter(|| {
                for &(start, end) in &positions {
                    let extracted: String = t.chars().skip(start).take(end - start).collect();
                    black_box(extracted);
                }
            })
        });

        // Cached: build map once, extract many
        group.bench_with_input(BenchmarkId::new("cached_10x", name), text, |b, t| {
            b.iter(|| {
                // Build map once
                let char_to_byte: Vec<usize> = t.char_indices().map(|(i, _)| i).collect();
                let text_len = t.len();

                for &(start, end) in &positions {
                    let byte_start = char_to_byte.get(start).copied().unwrap_or(0);
                    let byte_end = char_to_byte.get(end).copied().unwrap_or(text_len);
                    let extracted = &t[byte_start..byte_end];
                    black_box(extracted);
                }
            })
        });
    }

    group.finish();
}

fn bench_byte_char_ratio_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_char_ratio");

    // Compare texts with different byte/char ratios
    let texts = [
        ("ratio_1.0_ascii", "Hello World Test String Here"),
        ("ratio_1.5_mixed", "Tokyo 東京 Japan 日本"),
        ("ratio_3.0_cjk", "習近平在北京會見了普京"),
        ("ratio_2.0_emoji", "Party 🎉 fun 😀 time 🎊"),
    ];

    for (name, text) in texts {
        let bytes = text.len();
        let chars = text.chars().count();
        let ratio = bytes as f64 / chars as f64;

        // Include ratio in benchmark name for comparison
        let label = format!("{} (ratio={:.1})", name, ratio);

        group.bench_with_input(BenchmarkId::new("chars().count()", label), text, |b, t| {
            b.iter(|| black_box(t.chars().count()))
        });
    }

    group.finish();
}

fn bench_repeated_count_vs_cached(c: &mut Criterion) {
    let mut group = c.benchmark_group("repeated_vs_cached");

    // Simulate a function that checks char count multiple times
    // (common pattern in validation/bounds checking)

    let text = ASCII_MEDIUM;
    let iterations = 10;

    // Repeated: call chars().count() each time
    group.bench_function("repeated_10x", |b| {
        b.iter(|| {
            for _ in 0..iterations {
                let count = text.chars().count();
                black_box(count);
            }
        })
    });

    // Cached: call once, reuse
    group.bench_function("cached_10x", |b| {
        b.iter(|| {
            let count = text.chars().count();
            for _ in 0..iterations {
                black_box(count);
            }
        })
    });

    group.finish();
}

// =============================================================================
// Criterion Setup
// =============================================================================

criterion_group!(
    benches,
    bench_chars_count,
    bench_char_indices,
    bench_build_char_to_byte_map,
    bench_extract_span_naive_vs_cached,
    bench_byte_char_ratio_impact,
    bench_repeated_count_vs_cached,
);

criterion_main!(benches);
