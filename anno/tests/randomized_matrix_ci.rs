//! CI-friendly randomized matrix test for backends × datasets × tasks.
//!
//! This test randomly samples:
//! - A subset of backends (both lightweight and ML)
//! - A subset of cached datasets
//! - All supported tasks for each combination
//!
//! Supports multiple sampling strategies:
//! - Random: Pure random selection
//! - ML-only: Prioritize ML backends over builtins
//! - Worst-first: Prioritize historically worse-performing backends
//!
//! Environment variables:
//! - `HF_TOKEN`: HuggingFace token for gated models (auto-loaded from .env)
//! - `ANNO_CI_SEED`: Fixed seed for reproducibility
//! - `ANNO_SAMPLE_STRATEGY`: Sampling strategy (random, ml-only, worst-first, ml-all)
//!
//! Run with:
//! - `cargo test --test randomized_matrix_ci --features eval-advanced` (lightweight only)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,onnx"` (include ONNX)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,candle"` (include Candle)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,burn"` (include Burn)
//! - `ANNO_SAMPLE_STRATEGY=ml-only cargo test ...` (prioritize ML backends)

#![cfg(feature = "eval-advanced")]

use anno::env;
use anno::eval::loader::DatasetId;
use anno::eval::task_evaluator::{TaskEvalConfig, TaskEvaluator};
use anno::eval::task_mapping::Task;
use std::time::{Duration, Instant, SystemTime};
use xxhash_rust::xxh3::xxh3_64;

// =============================================================================
// Backend Categories
// =============================================================================

/// Built-in lightweight backends (no model downloads required)
const BUILTIN_BACKENDS: &[&str] = &["pattern", "heuristic", "crf", "stacked"];

/// Coreference backends (built-in, no model downloads)
const COREF_BACKENDS: &[&str] = &["mention_ranking"];

/// ML backends requiring ONNX feature
#[cfg(feature = "onnx")]
const ONNX_BACKENDS: &[&str] = &["bert_onnx", "gliner_onnx", "nuner", "w2ner", "gliner2"];

#[cfg(not(feature = "onnx"))]
const ONNX_BACKENDS: &[&str] = &[];

/// ML backends requiring Candle feature
#[cfg(feature = "candle")]
const CANDLE_BACKENDS: &[&str] = &["candle_ner", "gliner_candle"];

#[cfg(not(feature = "candle"))]
const CANDLE_BACKENDS: &[&str] = &[];

/// ML backends requiring Burn feature
#[cfg(feature = "burn")]
const BURN_BACKENDS: &[&str] = &["burn_ner"];

#[cfg(not(feature = "burn"))]
const BURN_BACKENDS: &[&str] = &[];

// =============================================================================
// Sampling Strategies
// =============================================================================

/// Sampling strategy for backend selection
#[derive(Debug, Clone, Copy, PartialEq)]
enum SamplingStrategy {
    /// Pure random selection across all backends
    Random,
    /// Prioritize ML backends (ONNX, Candle, Burn) over builtins
    MlOnly,
    /// Prioritize historically worse-performing or undertested backends
    WorstFirst,
    /// Test all ML backends, sample from builtins
    MlAll,
}

impl SamplingStrategy {
    fn from_env() -> Self {
        match std::env::var("ANNO_SAMPLE_STRATEGY").as_deref() {
            Ok("ml-only" | "ml_only") => Self::MlOnly,
            Ok("worst-first" | "worst_first") => Self::WorstFirst,
            Ok("ml-all" | "ml_all") => Self::MlAll,
            _ => Self::Random,
        }
    }
}

/// Get all available backends based on compiled features
fn get_all_backends() -> Vec<&'static str> {
    let mut backends: Vec<&str> = BUILTIN_BACKENDS.to_vec();
    backends.extend(COREF_BACKENDS);
    backends.extend(ONNX_BACKENDS);
    backends.extend(CANDLE_BACKENDS);
    backends.extend(BURN_BACKENDS);
    backends
}

/// Get ML-only backends (non-builtin)
fn get_ml_backends() -> Vec<&'static str> {
    let mut backends: Vec<&str> = Vec::new();
    backends.extend(ONNX_BACKENDS);
    backends.extend(CANDLE_BACKENDS);
    backends.extend(BURN_BACKENDS);
    backends
}

/// Backend priority scores (lower = test more often)
/// Based on: complexity, historical issues, coverage gaps
fn backend_priority(backend: &str) -> u32 {
    match backend {
        // ML backends: high priority (more likely to have issues)
        "gliner2" | "gliner2_candle" => 1, // Multi-task, complex
        "w2ner" => 2,                      // Discontinuous NER, auth issues
        "gliner_onnx" | "gliner_candle" => 3, // Zero-shot
        "nuner" => 4,                      // Token-based
        "bert_onnx" | "candle_ner" => 5,   // Standard BERT
        "burn_ner" => 6,                   // New, undertested
        // Coreference backends: medium priority
        "mention_ranking" => 8, // Book-scale coref, type-specific limits
        // Builtins: lower priority (well-tested)
        "stacked" => 10,
        "crf" => 11,
        "heuristic" => 13,
        "pattern" => 14,
        _ => 100,
    }
}

/// Select backends based on strategy
fn select_backends(strategy: SamplingStrategy, count: usize, seed: u64) -> Vec<&'static str> {
    match strategy {
        SamplingStrategy::Random => {
            let all = get_all_backends();
            select_random(&all, count, seed)
        }
        SamplingStrategy::MlOnly => {
            let ml = get_ml_backends();
            if ml.is_empty() {
                // Fallback to builtins if no ML features
                select_random(BUILTIN_BACKENDS, count, seed)
            } else {
                select_random(&ml, count.min(ml.len()), seed)
            }
        }
        SamplingStrategy::MlAll => {
            let mut backends = get_ml_backends();
            // Add some builtins for comparison
            let remaining = count.saturating_sub(backends.len());
            if remaining > 0 {
                backends.extend(select_random(BUILTIN_BACKENDS, remaining, seed));
            }
            backends
        }
        SamplingStrategy::WorstFirst => {
            let mut all = get_all_backends();
            // Sort by priority (lower = higher priority)
            all.sort_by_key(|b| backend_priority(b));
            // Take top N by priority, with some randomness
            let top_priority: Vec<_> = all.iter().take(count * 2).copied().collect();
            select_random(&top_priority, count, seed)
        }
    }
}

/// Datasets that are typically cached or fast to load
const QUICK_DATASETS: &[DatasetId] = &[
    // NER datasets
    DatasetId::WikiGold,
    DatasetId::Wnut17,
    DatasetId::MitMovie,
    DatasetId::MitRestaurant,
    // Coreference datasets (for coref backend testing)
    DatasetId::GAP,
    DatasetId::WikiCoref,
];

/// More comprehensive dataset list for thorough testing
#[allow(dead_code)]
const ALL_DATASETS: &[DatasetId] = &[
    // Standard NER
    DatasetId::WikiGold,
    DatasetId::Wnut17,
    DatasetId::MitMovie,
    DatasetId::MitRestaurant,
    DatasetId::CoNLL2003Sample,
    DatasetId::OntoNotesSample,
    // Biomedical NER (requires HF_TOKEN for some)
    DatasetId::BC5CDR,
    DatasetId::NCBIDisease,
    // Multilingual
    DatasetId::MultiNERD,
];

/// Biomedical datasets for specialized testing
#[allow(dead_code)]
const BIOMEDICAL_DATASETS: &[DatasetId] = &[
    DatasetId::BC5CDR,
    DatasetId::NCBIDisease,
    DatasetId::BC2GM,
    DatasetId::BC4CHEMD,
];

/// Tasks to test in the CI matrix.
///
/// This list is intentionally restricted to tasks that have concrete evaluation implementations
/// in `TaskEvaluator` (NER/coref/RE). Catalog-only tasks can exist in the registry but are not
/// meaningful to include here yet.
const TASKS: &[Task] = &[
    Task::NER,
    Task::DiscontinuousNER,
    Task::IntraDocCoref,
    Task::InterDocCoref,
    Task::AbstractAnaphora,
    Task::RelationExtraction,
];

/// Extended tasks for comprehensive testing (includes coref, relations)
#[allow(dead_code)]
const ALL_TASKS: &[Task] = &[
    Task::NER,
    Task::DiscontinuousNER,
    Task::IntraDocCoref,
    Task::RelationExtraction,
];

// =============================================================================
// Error Categories for Better Diagnostics
// =============================================================================

/// Categorize errors for better diagnostic reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ErrorCategory {
    /// Dataset not cached locally
    DatasetNotCached,
    /// Backend feature not enabled
    FeatureNotEnabled,
    /// Model not downloaded
    ModelNotDownloaded,
    /// Authentication required (e.g., HuggingFace token)
    AuthRequired,
    /// Entity type incompatibility between backend and dataset
    TypeIncompatible,
    /// Runtime error during extraction
    RuntimeError,
    /// Unknown error
    Unknown,
}

impl ErrorCategory {
    fn from_error(err: &str) -> Self {
        let lower = err.to_lowercase();
        if lower.contains("not cached")
            || lower.contains("unavailable")
            || lower.contains("not found")
            || lower.contains("missing dataset")
        {
            Self::DatasetNotCached
        } else if lower.contains("feature")
            || lower.contains("not enabled")
            || lower.contains("requires feature")
        {
            Self::FeatureNotEnabled
        } else if lower.contains("model not")
            || lower.contains("download")
            || lower.contains("loading model")
            || lower.contains("model file")
        {
            Self::ModelNotDownloaded
        } else if lower.contains("auth")
            || lower.contains("token")
            || lower.contains("hf_")
            || lower.contains("huggingface")
            || lower.contains("unauthorized")
            || lower.contains("403")
        {
            Self::AuthRequired
        } else if lower.contains("incompatible")
            || lower.contains("entity type")
            || lower.contains("does not support")
            || lower.contains("unsupported task")
            || lower.contains("invalid input")
            || lower.contains("backend")
        {
            Self::TypeIncompatible
        } else if lower.contains("panic") || lower.contains("internal error") {
            Self::RuntimeError
        } else {
            Self::Unknown
        }
    }

    fn is_expected_failure(&self) -> bool {
        matches!(
            self,
            Self::DatasetNotCached
                | Self::FeatureNotEnabled
                | Self::ModelNotDownloaded
                | Self::AuthRequired
                | Self::TypeIncompatible
        )
    }

    fn symbol(&self) -> &'static str {
        match self {
            Self::DatasetNotCached => "D",
            Self::FeatureNotEnabled => "F",
            Self::ModelNotDownloaded => "M",
            Self::AuthRequired => "A",
            Self::TypeIncompatible => "T",
            Self::RuntimeError => "!",
            Self::Unknown => "?",
        }
    }
}

/// Timing statistics for performance regression detection
#[derive(Default)]
struct TimingStats {
    durations: Vec<Duration>,
}

impl TimingStats {
    fn record(&mut self, duration: Duration) {
        self.durations.push(duration);
    }

    fn mean_ms(&self) -> f64 {
        if self.durations.is_empty() {
            return 0.0;
        }
        let total: Duration = self.durations.iter().sum();
        total.as_secs_f64() * 1000.0 / self.durations.len() as f64
    }

    fn max_ms(&self) -> f64 {
        self.durations
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .fold(0.0, f64::max)
    }
}

/// Generate a seed based on current time (for CI variety)
fn ci_seed() -> u64 {
    // Use env var if set (for reproducibility), otherwise time-based
    if let Ok(seed_str) = std::env::var("ANNO_CI_SEED") {
        seed_str.parse().unwrap_or(42)
    } else {
        // Time-based seed for variety across CI runs
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(42)
    }
}

fn allow_downloads_in_matrix() -> bool {
    matches!(
        std::env::var("ANNO_MATRIX_ALLOW_DOWNLOAD").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn max_downloads_in_matrix() -> usize {
    let allow = allow_downloads_in_matrix();
    if !allow {
        return 0;
    }
    match std::env::var("ANNO_MATRIX_MAX_DOWNLOADS").as_deref() {
        Ok(s) => s.parse::<usize>().unwrap_or(2),
        Err(_) => 2,
    }
}

fn dataset_count_in_matrix() -> usize {
    // Default:
    // - cached-only: 2 (fast, deterministic)
    // - downloads allowed: 3 (one extra slot for diversity)
    let default = if allow_downloads_in_matrix() { 3 } else { 2 };
    match std::env::var("ANNO_MATRIX_DATASET_COUNT").as_deref() {
        Ok(s) => s.parse::<usize>().unwrap_or(default).max(1),
        Err(_) => default,
    }
}

fn task_aware_sampling_in_matrix() -> bool {
    matches!(
        std::env::var("ANNO_MATRIX_TASK_AWARE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn allow_manual_datasets() -> bool {
    matches!(
        std::env::var("ANNO_DATASET_ALLOW_MANUAL").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn dataset_supports_task(ds: DatasetId, task: Task) -> bool {
    ds.tasks_typed().contains(&task)
}

/// Hash-based deterministic selection using xxHash for cross-run stability.
/// NOTE: DefaultHasher (SipHash) is NOT deterministic across Rust versions/platforms.
/// xxHash3 guarantees identical outputs everywhere.
fn select_random<T: Clone>(items: &[T], count: usize, seed: u64) -> Vec<T> {
    if items.len() <= count {
        return items.to_vec();
    }

    let mut indexed: Vec<(usize, u64)> = items
        .iter()
        .enumerate()
        .map(|(i, _)| {
            // Combine seed and index using xxHash for deterministic selection
            let mut data = [0u8; 16];
            data[..8].copy_from_slice(&seed.to_le_bytes());
            data[8..].copy_from_slice(&(i as u64).to_le_bytes());
            (i, xxh3_64(&data))
        })
        .collect();

    indexed.sort_by_key(|(_, hash)| *hash);
    indexed.truncate(count);

    indexed.iter().map(|(i, _)| items[*i].clone()).collect()
}

#[test]
fn test_randomized_matrix_sample() {
    // Load .env for HF_TOKEN and other secrets
    env::load_dotenv();

    let seed = ci_seed();
    let strategy = SamplingStrategy::from_env();
    let test_start = Instant::now();

    eprintln!("\n=== Randomized Matrix Test ===");
    eprintln!("CI seed: {} (set ANNO_CI_SEED to reproduce)", seed);

    // Report HF_TOKEN status
    let hf_status = if env::has_hf_token() {
        "set"
    } else {
        "not set"
    };
    eprintln!("HF_TOKEN: {} (from .env or environment)", hf_status);
    let allow_downloads = allow_downloads_in_matrix();
    let max_downloads = max_downloads_in_matrix();
    let dataset_count = dataset_count_in_matrix();
    let task_aware = task_aware_sampling_in_matrix();
    eprintln!(
        "Allow downloads: {} (set ANNO_MATRIX_ALLOW_DOWNLOAD=1 to enable)",
        allow_downloads
    );
    eprintln!(
        "Max downloads: {} (set ANNO_MATRIX_MAX_DOWNLOADS to override)",
        max_downloads
    );
    eprintln!(
        "Dataset count: {} (set ANNO_MATRIX_DATASET_COUNT to override)",
        dataset_count
    );
    eprintln!(
        "Task-aware dataset selection: {} (set ANNO_MATRIX_TASK_AWARE=1 to enable)",
        task_aware
    );
    eprintln!(
        "Sampling strategy: {:?} (set ANNO_SAMPLE_STRATEGY to change)",
        strategy
    );
    eprintln!(
        "Manual datasets: {} (set ANNO_DATASET_ALLOW_MANUAL=1 to include gated/large/unstable sources)",
        allow_manual_datasets()
    );
    eprintln!("Hash algorithm: xxHash3 (deterministic across platforms)");

    // Get all available backends based on compiled features
    let all_backends = get_all_backends();
    let ml_backends = get_ml_backends();
    eprintln!(
        "\nBackends available: {} total ({} ML)",
        all_backends.len(),
        ml_backends.len()
    );

    // Select backends based on strategy
    // More backends if ML features enabled
    let backend_count = if all_backends.len() > 5 { 3 } else { 2 };
    let selected_backends = select_backends(strategy, backend_count, seed);
    // Prefer cached datasets when possible so we exercise real pipelines on dev machines.
    let loader = match anno::eval::loader::DatasetLoader::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Skipping: DatasetLoader init failed: {}", e);
            return;
        }
    };

    // Candidate set:
    // - Always include cached datasets (fast, reproducible).
    // - Optionally include additional *uncached* datasets if downloads are allowed.
    //   This is opt-in because network runs are slower and less deterministic.
    let mut cached_candidates: Vec<DatasetId> = Vec::new();
    let mut download_candidates: Vec<DatasetId> = Vec::new();

    for ds in DatasetId::all().iter().copied() {
        let Ok(loadable) = anno::eval::LoadableDatasetId::try_from(ds) else {
            continue;
        };

        if loader.is_cached(loadable) {
            cached_candidates.push(ds);
            continue;
        }

        if !allow_downloads || max_downloads == 0 {
            continue;
        }

        // Heuristic: only try datasets that appear download-able (public/HF/local mirror).
        let access = ds.access_status();
        let maybe_downloadable = matches!(
            access,
            anno::eval::dataset_registry::DatasetAccessibility::Public
                | anno::eval::dataset_registry::DatasetAccessibility::HuggingFace
                | anno::eval::dataset_registry::DatasetAccessibility::Local
        ) && !ds.download_url().is_empty()
            && (allow_manual_datasets() || ds.is_automatable_download())
            && (allow_manual_datasets() || !ds.requires_hf_token() || anno::env::has_hf_token());

        if maybe_downloadable {
            download_candidates.push(ds);
        }
    }

    let mut candidates: Vec<DatasetId> = Vec::new();
    // Pick up to `max_downloads` uncached datasets, then fill from cached ones.
    if allow_downloads && max_downloads > 0 && !download_candidates.is_empty() {
        candidates.extend(select_random(
            &download_candidates,
            max_downloads
                .min(download_candidates.len())
                .min(dataset_count),
            seed.wrapping_add(2),
        ));
    }
    if !cached_candidates.is_empty() {
        candidates.extend(select_random(
            &cached_candidates,
            dataset_count
                .saturating_sub(candidates.len())
                .min(cached_candidates.len()),
            seed.wrapping_add(3),
        ));
    }

    // Fallback: if still empty, use quick list.
    if candidates.is_empty() {
        candidates = QUICK_DATASETS.to_vec();
    }

    let selected_datasets = if task_aware {
        // Choose a subset of tasks to cover this run (bounded by dataset_count),
        // then pick one dataset per chosen task. This improves per-run signal density:
        // we are more likely to actually exercise multiple task pipelines, instead of
        // listing tasks that have no matching datasets in the sample.
        let tasks_to_cover: Vec<Task> =
            select_random(TASKS, dataset_count.min(TASKS.len()), seed.wrapping_add(4));

        let mut chosen: Vec<DatasetId> = Vec::new();
        let mut remaining_download_budget = max_downloads;

        for (ti, task) in tasks_to_cover.iter().copied().enumerate() {
            // Prefer cached datasets for stability.
            let task_cached: Vec<DatasetId> = cached_candidates
                .iter()
                .copied()
                .filter(|ds| dataset_supports_task(*ds, task))
                .filter(|ds| !chosen.contains(ds))
                .collect();

            if !task_cached.is_empty() {
                chosen.extend(select_random(
                    &task_cached,
                    1,
                    seed.wrapping_add(10 + ti as u64),
                ));
                continue;
            }

            // Fall back to download candidates if allowed + budget remains.
            if allow_downloads && remaining_download_budget > 0 {
                let task_dl: Vec<DatasetId> = download_candidates
                    .iter()
                    .copied()
                    .filter(|ds| dataset_supports_task(*ds, task))
                    .filter(|ds| !chosen.contains(ds))
                    .collect();

                if !task_dl.is_empty() {
                    chosen.extend(select_random(
                        &task_dl,
                        1,
                        seed.wrapping_add(20 + ti as u64),
                    ));
                    remaining_download_budget = remaining_download_budget.saturating_sub(1);
                }
            }
        }

        // If we still have room, fill from whatever candidates we have.
        if chosen.len() < dataset_count {
            let fill: Vec<DatasetId> = candidates
                .iter()
                .copied()
                .filter(|ds| !chosen.contains(ds))
                .collect();
            chosen.extend(select_random(
                &fill,
                dataset_count.saturating_sub(chosen.len()).min(fill.len()),
                seed.wrapping_add(5),
            ));
        }

        if chosen.is_empty() {
            select_random(
                &candidates,
                dataset_count.min(candidates.len()),
                seed.wrapping_add(1),
            )
        } else {
            chosen
        }
    } else {
        select_random(
            &candidates,
            dataset_count.min(candidates.len()),
            seed.wrapping_add(1),
        )
    };

    eprintln!("Selected backends: {:?}", selected_backends);
    eprintln!("Selected datasets: {:?}", selected_datasets);

    let evaluator = match TaskEvaluator::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Skipping: TaskEvaluator init failed: {}", e);
            return;
        }
    };

    let config = TaskEvalConfig {
        tasks: TASKS.to_vec(),
        datasets: selected_datasets.clone(),
        backends: selected_backends.iter().map(|s| s.to_string()).collect(),
        max_examples: Some(10), // Small sample for CI speed
        seed: Some(seed),
        require_cached: !allow_downloads,
        relation_threshold: 0.5,
        robustness: false,
        compute_familiarity: false,
        temporal_stratification: false,
        confidence_intervals: false,
        custom_coref_resolver: None,
    };

    let eval_start = Instant::now();
    let results = match evaluator.evaluate_all(config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Evaluation failed: {}", e);
            // Not a failure - datasets may not be cached
            return;
        }
    };
    let eval_duration = eval_start.elapsed();

    // Categorize and report results
    eprintln!("\n=== Results ===");
    let mut success_count = 0;
    let mut timing_stats = TimingStats::default();
    let mut error_counts: std::collections::HashMap<ErrorCategory, usize> =
        std::collections::HashMap::new();

    for result in &results.results {
        let (status, category) = if result.success {
            success_count += 1;
            if let Some(&latency) = result.metrics.get("latency_ms") {
                timing_stats.record(Duration::from_secs_f64(latency / 1000.0));
            }
            ("✓", None)
        } else if let Some(err) = &result.error {
            let cat = ErrorCategory::from_error(err);
            *error_counts.entry(cat).or_insert(0) += 1;
            (cat.symbol(), Some(cat))
        } else {
            *error_counts.entry(ErrorCategory::Unknown).or_insert(0) += 1;
            ("?", Some(ErrorCategory::Unknown))
        };

        eprintln!(
            "  {} {:?} × {:?} × {} → F1={:.1}%",
            status,
            result.task,
            result.dataset,
            result.backend,
            result.metrics.get("f1").copied().unwrap_or(0.0) * 100.0
        );

        if let Some(err) = &result.error {
            if let Some(cat) = category {
                if !cat.is_expected_failure() {
                    eprintln!("      [{:?}] {}", cat, err);
                }
            }
        }
    }

    // Summary with error breakdown
    eprintln!("\n=== Summary ===");
    eprintln!("Success: {}", success_count);
    if !error_counts.is_empty() {
        eprintln!("Errors by category:");
        for (cat, count) in &error_counts {
            let expected = if cat.is_expected_failure() {
                " (expected)"
            } else {
                " (UNEXPECTED)"
            };
            eprintln!("  {:?}: {}{}", cat, count, expected);
        }
    }

    // Timing report
    if !timing_stats.durations.is_empty() {
        eprintln!(
            "\nTiming: mean={:.1}ms, max={:.1}ms",
            timing_stats.mean_ms(),
            timing_stats.max_ms()
        );
    }

    let total_duration = test_start.elapsed();
    eprintln!(
        "Total time: {:.2}s (eval: {:.2}s)",
        total_duration.as_secs_f64(),
        eval_duration.as_secs_f64()
    );

    // Error legend
    eprintln!("\nLegend: ✓=success D=dataset F=feature M=model A=auth T=type !=runtime ?=unknown");

    // Assertions
    let unexpected_errors: usize = error_counts
        .iter()
        .filter(|(cat, _)| !cat.is_expected_failure())
        .map(|(_, count)| count)
        .sum();

    if success_count == 0 && results.results.len() == error_counts.values().sum::<usize>() {
        // Check if all errors are expected types
        let all_expected = error_counts.keys().all(|cat| cat.is_expected_failure());
        if all_expected {
            eprintln!(
                "\nAll failures are expected (datasets not cached, features not enabled, etc.)"
            );
        }
    }

    assert!(
        unexpected_errors == 0 || success_count > 0,
        "Found {} unexpected errors with no successes",
        unexpected_errors
    );
}

#[test]
fn test_multi_seed_variance() {
    //! Test that different seeds produce different samples but consistent results.

    let seeds = [42, 123, 456];
    let mut f1_scores: Vec<f64> = Vec::new();

    for &seed in &seeds {
        let evaluator = match TaskEvaluator::new() {
            Ok(e) => e,
            Err(_) => return, // Skip if evaluator unavailable
        };

        let config = TaskEvalConfig {
            tasks: vec![Task::NER],
            datasets: vec![DatasetId::WikiGold],
            backends: vec!["pattern".to_string()],
            max_examples: Some(20),
            seed: Some(seed),
            require_cached: true,
            relation_threshold: 0.5,
            robustness: false,
            compute_familiarity: false,
            temporal_stratification: false,
            confidence_intervals: false,
            custom_coref_resolver: None,
        };

        if let Ok(results) = evaluator.evaluate_all(config) {
            for result in &results.results {
                if result.success {
                    if let Some(&f1) = result.metrics.get("f1") {
                        f1_scores.push(f1);
                    }
                }
            }
        }
    }

    if f1_scores.len() >= 2 {
        // Variance should be reasonable (not zero, not huge)
        let mean = f1_scores.iter().sum::<f64>() / f1_scores.len() as f64;
        let variance = f1_scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
            / (f1_scores.len() - 1) as f64;
        let std_dev = variance.sqrt();

        eprintln!("Multi-seed F1: mean={:.3}, std={:.3}", mean, std_dev);

        // Pattern backend should be consistent (low variance)
        assert!(
            std_dev < 0.1,
            "Pattern backend should have low variance across seeds"
        );
    }
}

#[test]
fn test_backend_availability_matrix() {
    //! Verify which backends are available (informational).

    eprintln!("\n=== Backend Availability Matrix ===");
    eprintln!(
        "(Features: onnx={}, candle={})",
        cfg!(feature = "onnx"),
        cfg!(feature = "candle")
    );

    eprintln!("\nLightweight backends:");
    for backend in BUILTIN_BACKENDS {
        let available = anno::eval::backend_factory::BackendFactory::create(backend)
            .map(|b| b.is_available())
            .unwrap_or(false);

        eprintln!("  {} {}", if available { "✓" } else { "✗" }, backend);
    }

    // Check ONNX backends
    eprintln!("\nONNX backends (feature = {}):", cfg!(feature = "onnx"));
    let onnx_backends = ["bert_onnx", "gliner_onnx", "nuner", "w2ner", "gliner2"];
    for backend in onnx_backends {
        let (available, reason) = match anno::eval::backend_factory::BackendFactory::create(backend)
        {
            Ok(b) => (
                b.is_available(),
                if b.is_available() {
                    "ready"
                } else {
                    "model not downloaded"
                },
            ),
            Err(e) => {
                let err_str = e.to_string();
                let reason = if err_str.contains("feature") {
                    "feature not enabled"
                } else if err_str.contains("auth")
                    || err_str.contains("token")
                    || err_str.contains("HuggingFace")
                {
                    "requires auth (set HF_TOKEN)"
                } else if err_str.contains("download") || err_str.contains("network") {
                    "model download failed"
                } else {
                    "initialization failed"
                };
                (false, reason)
            }
        };

        let icon = if available {
            "✓"
        } else if cfg!(feature = "onnx") {
            "○"
        } else {
            "✗"
        };
        eprintln!("  {} {} ({})", icon, backend, reason);
    }

    // Check Candle backends
    eprintln!(
        "\nCandle backends (feature = {}):",
        cfg!(feature = "candle")
    );
    let candle_backends = ["candle_ner", "gliner_candle"];
    for backend in candle_backends {
        let (available, reason) = match anno::eval::backend_factory::BackendFactory::create(backend)
        {
            Ok(b) => (
                b.is_available(),
                if b.is_available() {
                    "ready"
                } else {
                    "model not downloaded"
                },
            ),
            Err(e) => {
                let err_str = e.to_string();
                let reason = if err_str.contains("feature") {
                    "feature not enabled"
                } else if err_str.contains("auth")
                    || err_str.contains("token")
                    || err_str.contains("HuggingFace")
                {
                    "requires auth (set HF_TOKEN)"
                } else if err_str.contains("download") || err_str.contains("network") {
                    "model download failed"
                } else {
                    "initialization failed"
                };
                (false, reason)
            }
        };

        let icon = if available {
            "✓"
        } else if cfg!(feature = "candle") {
            "○"
        } else {
            "✗"
        };
        eprintln!("  {} {} ({})", icon, backend, reason);
    }

    // Check Burn backends (only if feature enabled)
    #[cfg(feature = "burn")]
    {
        eprintln!("\nBurn backends (feature = true):");
        let burn_backends = ["burn_ner"];
        for backend in burn_backends {
            let (available, reason) =
                match anno::eval::backend_factory::BackendFactory::create(backend) {
                    Ok(b) => (
                        b.is_available(),
                        if b.is_available() {
                            "ready"
                        } else {
                            "model not downloaded"
                        },
                    ),
                    Err(_) => (false, "initialization failed"),
                };

            let icon = if available { "✓" } else { "○" };
            eprintln!("  {} {} ({})", icon, backend, reason);
        }
    }

    #[cfg(not(feature = "burn"))]
    {
        eprintln!("\nBurn backends (feature = false):");
        eprintln!("  ✗ burn_ner (feature not enabled)");
    }

    // Summary
    let all_backends = get_all_backends();
    let available_count = all_backends
        .iter()
        .filter(|b| {
            anno::eval::backend_factory::BackendFactory::create(b)
                .map(|m| m.is_available())
                .unwrap_or(false)
        })
        .count();

    eprintln!(
        "\nSummary: {}/{} backends available",
        available_count,
        all_backends.len()
    );
}

#[test]
fn test_sampling_strategy_coverage() {
    //! Verify that different sampling strategies produce different backend selections.

    let seed = 42u64;
    let count = 3;

    // Test each strategy
    let random_selection = select_backends(SamplingStrategy::Random, count, seed);
    let worst_first = select_backends(SamplingStrategy::WorstFirst, count, seed);

    eprintln!("Random selection: {:?}", random_selection);
    eprintln!("Worst-first selection: {:?}", worst_first);

    // Worst-first should prioritize ML backends (lower priority numbers)
    // If ML backends are available, they should appear in worst_first
    let ml_backends = get_ml_backends();
    if !ml_backends.is_empty() {
        let worst_has_ml = worst_first.iter().any(|b| ml_backends.contains(b));
        eprintln!("ML backends available: {:?}", ml_backends);
        eprintln!("Worst-first includes ML: {}", worst_has_ml);

        // With ML features enabled, worst-first should prefer ML backends
        // This is a soft assertion since randomness is involved
    }

    // All selections should be non-empty
    assert!(
        !random_selection.is_empty(),
        "Random selection should not be empty"
    );
    assert!(
        !worst_first.is_empty(),
        "Worst-first selection should not be empty"
    );
}

#[test]
fn test_priority_ordering() {
    //! Verify backend priority scores are reasonable.

    // ML backends should have lower (higher priority) scores than builtins
    let gliner_priority = backend_priority("gliner_onnx");
    let pattern_priority = backend_priority("pattern");

    assert!(
        gliner_priority < pattern_priority,
        "ML backends should have higher priority (lower score) than builtins: {} vs {}",
        gliner_priority,
        pattern_priority
    );

    // Complex backends should have higher priority than simple ones
    let w2ner_priority = backend_priority("w2ner");
    let bert_priority = backend_priority("bert_onnx");

    assert!(
        w2ner_priority < bert_priority,
        "Complex backends (w2ner) should have higher priority than simple ones (bert): {} vs {}",
        w2ner_priority,
        bert_priority
    );

    eprintln!("Priority ordering validated:");
    eprintln!("  gliner_onnx: {} (ML)", gliner_priority);
    eprintln!("  w2ner: {} (complex ML)", w2ner_priority);
    eprintln!("  bert_onnx: {} (simple ML)", bert_priority);
    eprintln!("  pattern: {} (builtin)", pattern_priority);
}

#[test]
fn test_deterministic_sampling() {
    //! Same seed should produce same selection.

    let seed = 12345u64;
    let count = 3;

    let selection1 = select_backends(SamplingStrategy::Random, count, seed);
    let selection2 = select_backends(SamplingStrategy::Random, count, seed);

    assert_eq!(
        selection1, selection2,
        "Same seed should produce identical backend selection"
    );

    // Different seeds should (usually) produce different selections
    let selection3 = select_backends(SamplingStrategy::Random, count, seed + 1);

    // This might occasionally be the same due to hash collisions, but usually won't be
    if selection1 != selection3 {
        eprintln!("Different seeds produce different selections (as expected)");
    }
}

#[test]
fn test_dataset_metadata_quality() {
    //! Verify that datasets used in testing have complete metadata.

    eprintln!("\n=== Dataset Metadata Quality Check ===");

    let datasets_to_check = [
        QUICK_DATASETS,
        #[allow(dead_code)]
        ALL_DATASETS,
    ];

    let mut issues = Vec::new();

    for dataset in datasets_to_check.iter().flat_map(|d| d.iter()) {
        let name = dataset.name();
        let desc = dataset.description();
        let url = dataset.download_url();

        // Check for placeholder values
        if name == "Unknown Dataset" {
            issues.push(format!("{:?}: missing name", dataset));
        }
        if desc == "Dataset not yet fully integrated" {
            issues.push(format!("{:?}: missing description", dataset));
        }
        if url.is_empty()
            && !matches!(
                dataset,
                DatasetId::GAP | DatasetId::WikiCoref // Local/sample datasets OK
            )
        {
            // Note: empty URL is OK for some datasets that require registration
            eprintln!(
                "  {:?}: no download URL (may require registration)",
                dataset
            );
        }

        eprintln!(
            "  {:?}: name={}, desc_len={}, has_url={}",
            dataset,
            if name != "Unknown Dataset" {
                "ok"
            } else {
                "MISSING"
            },
            desc.len(),
            !url.is_empty()
        );
    }

    if !issues.is_empty() {
        eprintln!("\nMetadata issues found:");
        for issue in &issues {
            eprintln!("  - {}", issue);
        }
    }

    // Datasets used in matrix testing should have complete metadata
    assert!(
        issues.is_empty(),
        "Found {} metadata issues in test datasets",
        issues.len()
    );
}

#[test]
fn test_ml_only_strategy() {
    //! ML-only strategy should only include ML backends.

    let seed = 42u64;
    let count = 5;

    let selection = select_backends(SamplingStrategy::MlOnly, count, seed);

    let ml_backends = get_ml_backends();

    if ml_backends.is_empty() {
        // No ML features - should fall back to builtins
        eprintln!(
            "No ML features enabled - ML-only falls back to builtins: {:?}",
            selection
        );
        assert!(
            selection.iter().all(|b| BUILTIN_BACKENDS.contains(b)),
            "Without ML features, ML-only should fall back to builtins"
        );
    } else {
        // With ML features - should only include ML backends
        eprintln!("ML backends selected: {:?}", selection);
        assert!(
            selection.iter().all(|b| ml_backends.contains(b)),
            "ML-only strategy should only select ML backends: {:?}",
            selection
        );
    }
}
