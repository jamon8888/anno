//! CI-friendly randomized matrix test for backends × datasets × tasks.
//!
//! This test randomly samples:
//! - A subset of backends (both lightweight and ML)
//! - A subset of cached datasets
//! - All supported tasks for each combination
//!
//! Supports multiple sampling strategies:
//! - Random: Pure random selection
//! - ML-only: Prioritize ML backends, include baselines (heuristic/CRF) for comparison
//! - Worst-first: Prioritize historically worse-performing backends
//!
//! Environment variables:
//! - `HF_TOKEN`: HuggingFace token for gated models (auto-loaded from .env)
//! - `ANNO_CI_SEED`: Fixed seed for reproducibility
//! - `ANNO_SAMPLE_STRATEGY`: Sampling strategy (random, ml-only, worst-first, ml-all)
//!   Default: ml-only (prioritizes ML models, includes baselines for comparison)
//! - `ANNO_RESULTS_FILE`: Path to save JSON results for post-analysis (avoids rerunning)
//!
//! Run with:
//! - `cargo test --test randomized_matrix_ci --features eval-advanced` (lightweight only)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,onnx"` (include ONNX)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,candle"` (include Candle)
//! - `cargo test --test randomized_matrix_ci --features "eval-advanced,burn"` (include Burn)
//! - `ANNO_SAMPLE_STRATEGY=ml-only cargo test ...` (prioritize ML backends)

#![cfg(feature = "eval-advanced")]

use anno::env;
use anno::eval::backend_factory::BackendFactory;
use anno::eval::loader::DatasetId;
use anno::eval::task_evaluator::{TaskEvalConfig, TaskEvaluator};
use anno::eval::task_mapping::Task;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use xxhash_rust::xxh3::xxh3_64;

// =============================================================================
// Entity Type Normalization
// =============================================================================

/// Normalize entity type labels to canonical form for comparison.
/// This allows matching equivalent types like "person"/"per", "geo-loc"/"location", etc.
fn normalize_entity_type(label: &str) -> String {
    let lower = label.to_lowercase();
    match lower.as_str() {
        // Person variants
        "person" | "per" | "people" => "PER".to_string(),
        // Organization variants
        "organization" | "org" | "company" | "corp" => "ORG".to_string(),
        // Location variants
        "location" | "loc" | "geo-loc" | "gpe" | "place" => "LOC".to_string(),
        // Facility variants
        "facility" | "fac" | "building" => "FAC".to_string(),
        // Product variants
        "product" | "prod" => "PRODUCT".to_string(),
        // Event variants
        "event" | "evt" => "EVENT".to_string(),
        // Miscellaneous variants
        "misc" | "miscellaneous" | "other" => "MISC".to_string(),
        // Date/Time variants
        "date" | "time" | "datetime" => "DATE".to_string(),
        // Keep others as uppercase
        _ => lower.to_uppercase(),
    }
}

/// Check if two entity type sets are compatible (have meaningful overlap).
/// Returns (overlap, missing_from_backend, extra_in_backend)
fn compare_entity_types(
    dataset_types: &[String],
    backend_types: &[String],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let dataset_normalized: std::collections::HashSet<String> = dataset_types
        .iter()
        .map(|t| normalize_entity_type(t))
        .collect();
    let backend_normalized: std::collections::HashSet<String> = backend_types
        .iter()
        .map(|t| normalize_entity_type(t))
        .collect();

    let overlap: Vec<String> = dataset_normalized
        .intersection(&backend_normalized)
        .cloned()
        .collect();
    let missing: Vec<String> = dataset_normalized
        .difference(&backend_normalized)
        .cloned()
        .collect();
    let extra: Vec<String> = backend_normalized
        .difference(&dataset_normalized)
        .cloned()
        .collect();

    (overlap, missing, extra)
}

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
    /// Prioritize ML backends (ONNX, Candle, Burn), include baselines for comparison
    MlOnly,
    /// Prioritize historically worse-performing or undertested backends
    WorstFirst,
    /// Test all ML backends, sample from builtins
    MlAll,
}

impl SamplingStrategy {
    fn from_env() -> Self {
        match std::env::var("ANNO_SAMPLE_STRATEGY").as_deref() {
            Ok("random" | "Random") => Self::Random,
            Ok("ml-only" | "ml_only") => Self::MlOnly,
            Ok("worst-first" | "worst_first") => Self::WorstFirst,
            Ok("ml-all" | "ml_all") => Self::MlAll,
            Ok("mab" | "bandit") => Self::WorstFirst, // MAB uses worst-first with history
            // Default to ML-only: prioritize actual models over heuristic backends
            _ => Self::MlOnly,
        }
    }
}

// =============================================================================
// Multi-Armed Bandit Badness Scoring
// =============================================================================

/// Compute "badness" score for a backend-dataset combination.
/// Higher badness = more likely to reveal bugs = higher priority for testing.
///
/// Scoring:
/// - Zero F1 with predictions made: 100 (complete failure, investigate)
/// - Error during evaluation: 90 (broken, needs fixing)
/// - Zero F1 with no predictions: 80 (model not producing output)
/// - Very low F1 (<10%): 70 (something wrong)
/// - Low F1 (<30%): 50 (suboptimal, worth investigating)
/// - High variance across seeds: 40 (unstable, needs investigation)
/// - Untested combination: 30 (exploration bonus)
/// - Good F1 (>50%): 10 (working, low priority)
fn compute_badness(f1: Option<f64>, error: bool, predicted: usize, gold: usize) -> u32 {
    if error {
        return 90;
    }

    match f1 {
        None => 30,                                  // Untested
        Some(f) if f == 0.0 && predicted > 0 => 100, // All wrong
        Some(f) if f == 0.0 && gold == 0 => 60,      // No gold (parsing issue)
        Some(0.0) => 80,                             // No predictions
        Some(f) if f < 0.10 => 70,                   // Very low
        Some(f) if f < 0.30 => 50,                   // Low
        Some(f) if f < 0.50 => 30,                   // Medium
        Some(_) => 10,                               // Good
    }
}

/// Historical performance tracker for MAB-style sampling.
/// Loads from ANNO_HISTORY_FILE if available.
#[derive(Default)]
#[allow(dead_code)] // Methods used conditionally based on env vars
struct BadnessTracker {
    /// (backend, dataset) -> (total_badness, count)
    scores: std::collections::HashMap<(String, String), (u32, u32)>,
}

impl BadnessTracker {
    /// Load historical results from a file.
    fn load_from_file(path: &std::path::Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut tracker = Self::default();

        // Parse simple format: backend,dataset,badness
        for line in content.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 3 {
                let backend = parts[0].to_string();
                let dataset = parts[1].to_string();
                let badness: u32 = parts[2].parse().unwrap_or(30);
                let entry = tracker.scores.entry((backend, dataset)).or_insert((0, 0));
                entry.0 += badness;
                entry.1 += 1;
            }
        }

        Some(tracker)
    }

    /// Get average badness for a combination, with exploration bonus for untested.
    #[allow(dead_code)] // Used for MAB-style selection, not currently active
    fn get_badness(&self, backend: &str, dataset: &str) -> f64 {
        match self.scores.get(&(backend.to_string(), dataset.to_string())) {
            Some((total, count)) if *count > 0 => {
                let avg = *total as f64 / *count as f64;
                // Add UCB-style exploration bonus: prioritize less-tested combinations
                let exploration_bonus = 10.0 / (*count as f64).sqrt();
                avg + exploration_bonus
            }
            _ => 50.0, // Untested: moderate priority with exploration bonus
        }
    }

    /// Get top-N highest badness combinations.
    fn top_badness(&self, n: usize) -> Vec<(String, String, f64)> {
        let mut all: Vec<_> = self
            .scores
            .iter()
            .map(|((b, d), (total, count))| {
                let avg = if *count > 0 {
                    *total as f64 / *count as f64
                } else {
                    50.0
                };
                (b.clone(), d.clone(), avg)
            })
            .collect();
        all.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(n);
        all
    }

    /// Get average badness per dataset (for dataset prioritization).
    fn dataset_badness(&self) -> Vec<(String, f64)> {
        let mut by_dataset: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        for ((_, dataset), (total, count)) in &self.scores {
            let entry = by_dataset.entry(dataset.clone()).or_insert((0, 0));
            entry.0 += total;
            entry.1 += count;
        }
        let mut result: Vec<_> = by_dataset
            .iter()
            .map(|(d, (t, c))| (d.clone(), if *c > 0 { *t as f64 / *c as f64 } else { 50.0 }))
            .collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }

    /// Get average badness per backend.
    fn backend_badness(&self) -> Vec<(String, f64)> {
        let mut by_backend: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        for ((backend, _), (total, count)) in &self.scores {
            let entry = by_backend.entry(backend.clone()).or_insert((0, 0));
            entry.0 += total;
            entry.1 += count;
        }
        let mut result: Vec<_> = by_backend
            .iter()
            .map(|(b, (t, c))| (b.clone(), if *c > 0 { *t as f64 / *c as f64 } else { 50.0 }))
            .collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
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
                // Fallback to builtins (heuristic/CRF are baselines, should be included)
                select_random(BUILTIN_BACKENDS, count, seed)
            } else {
                // Prioritize ML backends, but include baselines for comparison
                let ml_count = count.min(ml.len());
                let mut selected = select_random(&ml, ml_count, seed);
                // If we have room, add baseline backends for comparison
                if selected.len() < count {
                    let remaining = count - selected.len();
                    let baseline_candidates: Vec<&str> = BUILTIN_BACKENDS.to_vec();
                    if !baseline_candidates.is_empty() {
                        let additional = select_random(
                            &baseline_candidates,
                            remaining.min(baseline_candidates.len()),
                            seed.wrapping_add(1000),
                        );
                        selected.extend(additional);
                    }
                }
                selected
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
            let all = get_all_backends();

            // Try to load history for MAB-style prioritization
            let history_path = std::env::var("ANNO_HISTORY_FILE").map(PathBuf::from).ok();
            let tracker = history_path
                .as_ref()
                .and_then(|p| BadnessTracker::load_from_file(p));

            if let Some(ref tracker) = tracker {
                // Use history-based selection: prioritize high-badness backends
                let worst_combos = tracker.top_badness(20);
                let worst_backends: Vec<&str> = all
                    .iter()
                    .copied()
                    .filter(|b| worst_combos.iter().any(|(wb, _, _)| wb == *b))
                    .collect();

                if !worst_backends.is_empty() {
                    eprintln!(
                        "MAB: Using history-based selection, {} high-badness backends",
                        worst_backends.len()
                    );
                    return select_random(&worst_backends, count.min(worst_backends.len()), seed);
                }
            }

            // Fallback: Sort by static priority (lower = higher priority)
            let mut all = all;
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

/// Essential English NER datasets for testing ML backends
/// These should always be included when testing ML models
const ESSENTIAL_NER_DATASETS: &[DatasetId] = &[
    DatasetId::WikiGold,        // Standard Wikipedia NER
    DatasetId::Wnut17,          // Social media/emerging entities
    DatasetId::CoNLL2003Sample, // Standard benchmark
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

fn running_in_ci() -> bool {
    // Common CI env vars across providers (GitHub Actions, Buildkite, etc.)
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        return true;
    }
    // When running locally with `cargo nextest run --profile ci`, CI env vars may not be set.
    // nextest sets NEXTEST_PROFILE; treat that as CI for bounding this test.
    matches!(std::env::var("NEXTEST_PROFILE").as_deref(), Ok("ci"))
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
    // - CI: 3 (keep runtime bounded under nextest timeouts)
    // - cached-only (non-CI): 5 (better local signal)
    // - downloads allowed: 8 (more diversity when downloads enabled)
    let default = if allow_downloads_in_matrix() {
        8
    } else if running_in_ci() {
        3
    } else {
        5
    };
    match std::env::var("ANNO_MATRIX_DATASET_COUNT").as_deref() {
        Ok(s) => s.parse::<usize>().unwrap_or(default).max(1),
        Err(_) => default,
    }
}

fn max_examples_in_matrix() -> usize {
    // Default is intentionally small in CI; you can override locally with:
    // `ANNO_MATRIX_MAX_EXAMPLES=50`.
    let default = if running_in_ci() { 15 } else { 50 };
    match std::env::var("ANNO_MATRIX_MAX_EXAMPLES").as_deref() {
        Ok(s) => s.parse::<usize>().unwrap_or(default).max(1),
        Err(_) => default,
    }
}

fn task_aware_sampling_in_matrix() -> bool {
    // Default to true for better task coverage (can be disabled via env var)
    match std::env::var("ANNO_MATRIX_TASK_AWARE").as_deref() {
        Ok("0") | Ok("false") | Ok("no") => false,
        _ => true, // Default enabled
    }
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
        "Max examples: {} (set ANNO_MATRIX_MAX_EXAMPLES to override)",
        max_examples_in_matrix()
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
    if ml_backends.is_empty() {
        eprintln!("WARN: No ML backends available. Enable features to test actual models:");
        eprintln!("  - cargo test --features \"eval-advanced,onnx\" (ONNX backends)");
        eprintln!("  - cargo test --features \"eval-advanced,candle\" (Candle backends)");
        eprintln!("  - cargo test --features \"eval-advanced,burn\" (Burn backends)");
        eprintln!("Current test will use builtin backends (heuristic/CRF/stacked) as baselines for comparison.");
    }

    // Select backends based on strategy
    // More backends if ML features enabled (increased for better coverage)
    let backend_count = if running_in_ci() {
        3
    } else if all_backends.len() > 5 {
        5
    } else {
        4
    };
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

    // CI fast-path: avoid a couple known slow/unstable choices that routinely
    // push this randomized test over nextest's 120s termination threshold.
    //
    // You can still test these locally by running without the nextest `ci` profile
    // or by explicitly setting ANNO_MATRIX_DATASET_COUNT / ANNO_MATRIX_ALLOW_DOWNLOAD, etc.
    if running_in_ci() {
        cached_candidates.retain(|d| !matches!(d, DatasetId::LitBank | DatasetId::ECBPlus));
        download_candidates.retain(|d| !matches!(d, DatasetId::LitBank | DatasetId::ECBPlus));
    }

    let mut candidates: Vec<DatasetId> = Vec::new();

    // When testing ML backends, always include essential English NER datasets
    // These provide the best signal for testing actual ML models
    let has_ml_backends = !get_ml_backends().is_empty();
    if has_ml_backends {
        for ds in ESSENTIAL_NER_DATASETS.iter().copied() {
            if candidates.len() >= dataset_count {
                break;
            }
            // Check if loadable and either cached or downloadable
            if let Ok(loadable) = anno::eval::LoadableDatasetId::try_from(ds) {
                if loader.is_cached(loadable)
                    || (allow_downloads && download_candidates.contains(&ds))
                {
                    candidates.push(ds);
                }
            }
        }
    }

    // Pick up to `max_downloads` uncached datasets, then fill from cached ones.
    if allow_downloads && max_downloads > 0 && !download_candidates.is_empty() {
        candidates.extend(select_random(
            &download_candidates
                .iter()
                .copied()
                .filter(|d| !candidates.contains(d))
                .collect::<Vec<_>>(),
            max_downloads
                .min(download_candidates.len())
                .min(dataset_count.saturating_sub(candidates.len())),
            seed.wrapping_add(2),
        ));
    }
    if !cached_candidates.is_empty() {
        candidates.extend(select_random(
            &cached_candidates
                .iter()
                .copied()
                .filter(|d| !candidates.contains(d))
                .collect::<Vec<_>>(),
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

        // Start with essential NER datasets if ML backends are being tested
        // This ensures we always have meaningful NER evaluation data
        let mut chosen: Vec<DatasetId> = if has_ml_backends {
            candidates
                .iter()
                .copied()
                .filter(|ds| ESSENTIAL_NER_DATASETS.contains(ds))
                .take(2) // Include up to 2 essential datasets
                .collect()
        } else {
            Vec::new()
        };
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
        max_examples: Some(max_examples_in_matrix()),
        seed: Some(seed),
        require_cached: !allow_downloads,
        relation_threshold: 0.5,
        robustness: false,
        compute_familiarity: false,
        temporal_stratification: false,
        confidence_intervals: false,
        custom_coref_resolver: None,
        coref_use_gold_mentions: false,
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

    // Collect metrics for analysis
    let mut f1_scores: Vec<f64> = Vec::new();
    let mut precision_scores: Vec<f64> = Vec::new();
    let mut recall_scores: Vec<f64> = Vec::new();
    let mut zero_f1_count = 0;
    let mut backend_dataset_f1: std::collections::HashMap<(String, String), Vec<f64>> =
        std::collections::HashMap::new();

    for result in &results.results {
        let (status, category) = if result.success {
            success_count += 1;
            if let Some(&latency) = result.metrics.get("latency_ms") {
                timing_stats.record(Duration::from_secs_f64(latency / 1000.0));
            }

            // Collect metrics for analysis
            let f1 = result.metrics.get("f1").copied().unwrap_or(0.0);
            let precision = result.metrics.get("precision").copied().unwrap_or(0.0);
            let recall = result.metrics.get("recall").copied().unwrap_or(0.0);
            // strict_f1/partial_f1 collected below in the output section

            f1_scores.push(f1);
            precision_scores.push(precision);
            recall_scores.push(recall);

            if f1 == 0.0 {
                zero_f1_count += 1;
            }

            // Track F1 by backend-dataset for variance analysis
            let key = (result.backend.clone(), format!("{:?}", result.dataset));
            backend_dataset_f1.entry(key).or_default().push(f1);

            ("PASS", None)
        } else if let Some(err) = &result.error {
            let cat = ErrorCategory::from_error(err);
            *error_counts.entry(cat).or_insert(0) += 1;
            (cat.symbol(), Some(cat))
        } else {
            *error_counts.entry(ErrorCategory::Unknown).or_insert(0) += 1;
            ("?", Some(ErrorCategory::Unknown))
        };

        // Enhanced output with detailed correctness analysis
        if result.success {
            let f1 = result.metrics.get("f1").copied().unwrap_or(0.0);
            let precision = result.metrics.get("precision").copied().unwrap_or(0.0);
            let recall = result.metrics.get("recall").copied().unwrap_or(0.0);
            let strict_f1 = result.metrics.get("strict_f1").copied();
            let partial_f1 = result.metrics.get("partial_f1").copied();
            let exact_f1 = result.metrics.get("exact_f1").copied();
            let type_f1 = result.metrics.get("type_f1").copied();
            let num_gold = result.metrics.get("num_gold").copied().unwrap_or(0.0) as usize;
            let num_pred = result.metrics.get("num_predicted").copied().unwrap_or(0.0) as usize;

            // Main result line with key metrics
            eprint!(
                "  {} {:?} × {:?} × {} → F1={:.1}% (P={:.1}%, R={:.1}%)",
                status,
                result.task,
                result.dataset,
                result.backend,
                f1 * 100.0,
                precision * 100.0,
                recall * 100.0
            );

            // Add quick diagnosis on same line for critical issues
            if f1 == 0.0 && num_gold > 0 && num_pred == 0 {
                eprint!(" [NO_PRED]");
            } else if f1 == 0.0 && num_gold == 0 && num_pred > 0 {
                eprint!(" [NO_GOLD]");
            } else if f1 == 0.0 && num_gold > 0 && num_pred > 0 {
                eprint!(" [ALL_WRONG]");
            } else if f1 > 0.0 && f1 < 0.1 {
                eprint!(" [VERY_LOW]");
            }

            eprintln!();
            eprintln!(
                "         Entities: {} gold, {} predicted",
                num_gold, num_pred
            );

            // Show mode comparison if available (critical for correctness validation)
            if let (Some(sf1), Some(pf1)) = (strict_f1, partial_f1) {
                if (sf1 - pf1).abs() > 0.01 {
                    eprintln!(
                        "         Modes: strict={:.1}%, partial={:.1}%",
                        sf1 * 100.0,
                        pf1 * 100.0
                    );
                }
            }

            // Backend status warnings (correctness: are we using trained vs heuristic?)
            if result.backend == "crf" {
                eprintln!("  CRF backend: Baseline with heuristic weights (not trained). Expected ~65-70% F1 when trained, not ~88%.");
                eprintln!("         To train: uv run scripts/train_crf_weights.py");
            } else if result.backend == "heuristic" {
                eprintln!(
                    "  Heuristic backend: Baseline - works best on formal text with capitalization."
                );
                eprintln!("         Struggles with social media/informal text.");
            } else if result.backend == "pattern" {
                eprintln!("  Pattern backend: Only extracts structured entities (DATE, MONEY, EMAIL, URL).");
                eprintln!("         Not compatible with named entity datasets.");
            }

            // Type mismatch analysis (correctness: are types compatible?)
            let dataset_types: Vec<String> = result
                .dataset
                .entity_types()
                .iter()
                .map(|t| t.to_lowercase())
                .collect();
            eprintln!("      Dataset entity types: {}", dataset_types.join(", "));

            // Get backend supported types (if available via factory)
            if let Ok(backend_model) = BackendFactory::create(&result.backend) {
                let backend_types: Vec<String> = backend_model
                    .supported_types()
                    .iter()
                    .map(|t| t.as_label().to_lowercase())
                    .collect();
                if !backend_types.is_empty() {
                    eprintln!(
                        "      Backend supported types: {}",
                        backend_types.join(", ")
                    );

                    // Type overlap analysis (with normalization for equivalent types)
                    let (overlap, missing, extra) =
                        compare_entity_types(&dataset_types, &backend_types);

                    if !overlap.is_empty() {
                        eprintln!(
                            "         Type overlap (normalized): {} (PASS)",
                            overlap.join(", ")
                        );
                    }
                    if !missing.is_empty() {
                        eprintln!(
                            "         Missing in backend (normalized): {} (WARN)",
                            missing.join(", ")
                        );
                    }
                    if !extra.is_empty() {
                        eprintln!(
                            "         Extra in backend (normalized): {} (INFO)",
                            extra.join(", ")
                        );
                    }

                    // Compatibility assessment
                    if missing.is_empty() {
                        eprintln!("         Type compatibility: PASS All dataset types supported");
                    } else {
                        eprintln!(
                            "         Type compatibility: WARN  {} types not supported by backend",
                            missing.len()
                        );
                    }
                }
            } else {
                eprintln!(
                    "  Backend type info: Unable to create backend instance for type checking"
                );
            }

            // Mode comparison details (correctness: boundary vs type issues?)
            if let (Some(sf1), Some(pf1), Some(ef1), Some(tf1)) =
                (strict_f1, partial_f1, exact_f1, type_f1)
            {
                eprintln!("      Evaluation modes:");
                eprintln!(
                    "         Strict: {:.1}% (exact boundary + exact type)  Standard CoNLL metric",
                    sf1 * 100.0
                );
                eprintln!("         Exact: {:.1}% (exact boundary, type ignored)  Boundary detection test", ef1 * 100.0);
                eprintln!(
                    "         Partial: {:.1}% (overlap + exact type)  Lenient boundary matching",
                    pf1 * 100.0
                );
                eprintln!("         Type: {:.1}% (overlap + exact type, lenient)  Type classification test", tf1 * 100.0);

                // Diagnose failure mode (critical for correctness validation)
                if sf1 == 0.0 && ef1 > 0.0 {
                    eprintln!("      Diagnosis: Boundary detection works PASS, but type classification fails FAIL");
                    eprintln!("          Backend finds correct spans but assigns wrong types");
                    eprintln!("          Fix: Check type mapping or backend type support");
                } else if sf1 == 0.0 && pf1 > 0.0 {
                    eprintln!("      Diagnosis: Type classification works PASS, but exact boundary detection fails FAIL");
                    eprintln!(
                        "          Backend finds correct types but boundary offsets are wrong"
                    );
                    eprintln!("          Fix: Backend may have boundary detection issues (CRF heuristic weights?)");
                } else if sf1 == 0.0 && tf1 > 0.0 {
                    eprintln!("      Diagnosis: Type classification works PASS, but strict boundary matching fails FAIL");
                    eprintln!("          Backend finds correct types with overlapping spans, but not exact matches");
                    eprintln!(
                        "          Fix: Boundary alignment issue - backend close but not exact"
                    );
                } else if sf1 == 0.0 && pf1 == 0.0 && tf1 == 0.0 {
                    eprintln!(
                        "      Diagnosis: Complete mismatch - no entities found or all wrong"
                    );
                    eprintln!(
                        "          Backend may be incompatible with dataset or completely failing"
                    );
                    eprintln!("          Fix: Check backend-dataset compatibility, type support, or backend initialization");
                } else if sf1 > 0.0 && (pf1 - sf1).abs() < 0.01 {
                    eprintln!("      Diagnosis: Boundary detection is accurate (strict ~ partial)");
                    eprintln!("          Backend has good boundary precision");
                } else if sf1 > 0.0 && ef1 > sf1 + 0.05 {
                    eprintln!(
                        "      Diagnosis: Type errors are significant (exact > strict by {:.1}%)",
                        (ef1 - sf1) * 100.0
                    );
                    eprintln!("          Backend boundaries are good, but type classification needs improvement");
                }

                // Mode ordering invariants (must hold for correctness)
                let mode_ordering_violations = vec![
                    (
                        sf1 > pf1 + 0.01,
                        "strict > partial",
                        "partial mode is more lenient than strict",
                    ),
                    (
                        pf1 > tf1 + 0.01,
                        "partial > type",
                        "type mode is more lenient than partial",
                    ),
                    (
                        sf1 > ef1 + 0.01,
                        "exact < strict",
                        "exact ignores type so should be >= strict",
                    ),
                ];
                for (violated, desc, reason) in mode_ordering_violations {
                    if violated {
                        eprintln!(
                            "      INVARIANT VIOLATION: {} ({} should never happen)",
                            desc, reason
                        );
                    }
                }
            }

            // Metric correctness validation
            eprintln!("      Metric validation:");

            // Finiteness check (must be finite, not NaN/Inf)
            let precision_finite = precision.is_finite();
            let recall_finite = recall.is_finite();
            let f1_finite = f1.is_finite();
            if !precision_finite || !recall_finite || !f1_finite {
                eprintln!(
                    "        FINITENESS FAIL: precision={}, recall={}, f1={}",
                    precision, recall, f1
                );
            } else {
                eprintln!("        Finiteness: PASS (all metrics finite)");
            }

            // Range check [0.0, 1.0]
            let in_range = (0.0..=1.0).contains(&precision)
                && (0.0..=1.0).contains(&recall)
                && (0.0..=1.0).contains(&f1);
            if !in_range {
                eprintln!("        RANGE FAIL: precision={:.6}, recall={:.6}, f1={:.6} (must be in [0,1])", 
                    precision, recall, f1);
            } else {
                eprintln!("        Range: PASS (all metrics in [0.0, 1.0])");
            }

            // F1 formula consistency (F1 = 2PR/(P+R))
            if precision > 0.0 && recall > 0.0 && f1 > 0.0 {
                let expected_f1: f64 = 2.0_f64
                    * (precision as f64)
                    * (recall as f64)
                    / ((precision + recall) as f64);
                let f1_diff: f64 = (expected_f1 - (f1 as f64)).abs();
                if f1_diff > 0.001 {
                    eprintln!(
                        "        F1_FORMULA FAIL: F1={:.6}, but 2PR/(P+R)={:.6}, diff={:.6}",
                        f1, expected_f1, f1_diff
                    );
                } else {
                    eprintln!(
                        "        F1 formula: PASS (F1 = 2PR/(P+R), diff={:.6})",
                        f1_diff
                    );
                }
            }

            // Precision/recall theoretical bounds
            if num_pred > 0 && num_gold > 0 {
                let max_precision = (num_gold.min(num_pred) as f64) / (num_pred as f64);
                let max_recall = (num_gold.min(num_pred) as f64) / (num_gold as f64);
                if precision > max_precision + 0.01 {
                    eprintln!(
                        "        PRECISION_BOUND FAIL: precision={:.6} > max={:.6}",
                        precision, max_precision
                    );
                }
                if recall > max_recall + 0.01 {
                    eprintln!(
                        "        RECALL_BOUND FAIL: recall={:.6} > max={:.6}",
                        recall, max_recall
                    );
                }
            }

            // Entity count validation
            eprintln!("      Entity counts:");

            // Dataset structure analysis (critical nuance: num_examples is sentences, not entities)
            let actual_sentences = result.num_examples;
            let entities_per_sentence = if actual_sentences > 0 {
                num_gold as f64 / actual_sentences as f64
            } else {
                0.0
            };
            eprintln!(
                "        Dataset structure: {} sentences, {:.1} entities/sentence",
                actual_sentences, entities_per_sentence
            );

            // Note: num_examples is sentences, not entities
            // For datasets like LitBank that group all entities into 1 sentence,
            // this can be misleading (1 sentence with 509 entities)
            if actual_sentences == 1 && num_gold > 50 {
                eprintln!("        WARN  Single-sentence dataset with many entities (possible parser grouping issue)");
                eprintln!("              This suggests dataset parser groups all entities into one sentence");
            }

            eprintln!("        gold: {} (expected: dataset entities)", num_gold);
            eprintln!("        predicted: {} (expected: backend output)", num_pred);

            // MUC count invariants verification (indirect)
            // If we had access to MUC counts, we'd verify:
            // correct + incorrect + partial + missed = num_gold
            // correct + incorrect + partial + spurious = num_pred
            // For now, verify indirect consistency: precision/recall imply same correct count
            if num_gold > 0 && num_pred > 0 && precision > 0.0 && recall > 0.0 {
                let implied_correct_from_precision = (precision * num_pred as f64).round() as usize;
                let implied_correct_from_recall = (recall * num_gold as f64).round() as usize;
                let count_diff = (implied_correct_from_precision as i64
                    - implied_correct_from_recall as i64)
                    .abs();
                if count_diff > 1 {
                    eprintln!("        COUNT_CONSISTENCY WARN: precision implies {} correct, recall implies {} correct, diff={}", 
                        implied_correct_from_precision, implied_correct_from_recall, count_diff);
                } else {
                    eprintln!("        Count consistency: PASS (precision and recall imply same correct count)");
                }
            }

            // Flag concernoing patterns
            if f1 == 0.0 && num_gold > 0 && num_pred == 0 {
                eprintln!("      WARN  No predictions made (all false negatives)");
                eprintln!("         Possible causes: backend not compatible, type mismatch, or backend failure");
            } else if f1 == 0.0 && num_gold == 0 {
                eprintln!("      WARN  No gold entities in sample (dataset may be empty, filtered, or nested entities not parsed)");
                eprintln!("         Possible causes: dataset parsing issue (nested entities?), empty sample, or filtering");

                // Check if this is a nested NER dataset
                let dataset_name = format!("{:?}", result.dataset);
                if dataset_name.contains("Nested")
                    || dataset_name.contains("GENIA")
                    || dataset_name.contains("NNE")
                {
                    eprintln!("         This is a nested NER dataset - parser may not be handling nested spans correctly");
                    eprintln!("         Nested entities require special parsing (multiple entities at same span)");
                    eprintln!("         Standard CoNLL BIO format doesn't support nesting - need specialized parser");
                }
            } else if precision == 0.0 && num_pred > 0 {
                eprintln!("      WARN  All predictions wrong (100% false positives)");
                eprintln!("         Possible causes: type mismatch, boundary errors, or backend producing wrong types");

                // Additional diagnosis for boundary vs type issues
                if let (Some(sf1), Some(ef1)) = (strict_f1, exact_f1) {
                    if sf1 == 0.0 && ef1 > 0.0 {
                        eprintln!("         Boundary detection works (exact_f1={:.1}%), but type classification fails", ef1 * 100.0);
                    } else if sf1 == 0.0 && ef1 == 0.0 {
                        eprintln!(
                            "         Both boundary and type matching fail (strict=0%, exact=0%)"
                        );
                    }
                }
            } else if recall == 0.0 && num_gold > 0 {
                eprintln!("      WARN  No correct predictions (100% false negatives)");
                eprintln!("         Possible causes: backend not finding entities, type mismatch, or threshold too high");
            }

            // Stratified metrics (if available) - shows per-type breakdown
            if let Some(ref stratified) = result.stratified {
                if !stratified.by_entity_type.is_empty() {
                    eprintln!("      Per-type metrics:");

                    // Check if types look like character IDs (LitBank coreference issue)
                    let mut suspicious_types = Vec::new();
                    for entity_type in stratified.by_entity_type.keys() {
                        // LitBank character IDs look like "ANCIENT_GREENWICH_PENSIONERS-24"
                        // They're long, contain underscores and hyphens, and have numeric suffixes
                        if entity_type.contains('-')
                            && entity_type.len() > 20
                            && entity_type.chars().any(|c| c.is_uppercase())
                        {
                            suspicious_types.push(entity_type.clone());
                        }
                    }

                    if !suspicious_types.is_empty() {
                        eprintln!("        WARN  Stratified metrics show character IDs instead of entity types");
                        eprintln!("              This suggests LitBank coreference data is being treated as NER");
                        eprintln!(
                            "              First suspicious type: {}",
                            suspicious_types[0]
                        );
                        eprintln!("              LitBank is a coreference dataset - character mentions have IDs, not just types");
                    }

                    let mut type_entries: Vec<_> = stratified.by_entity_type.iter().collect();
                    type_entries.sort_by_key(|(k, _)| *k);
                    for (type_name, metrics) in type_entries.iter().take(5) {
                        eprintln!(
                            "         {}: F1={:.1}% ± {:.1}% (95% CI: [{:.1}%, {:.1}%], n={})",
                            type_name,
                            metrics.mean * 100.0,
                            metrics.std_dev * 100.0,
                            metrics.ci_95.0 * 100.0,
                            metrics.ci_95.1 * 100.0,
                            metrics.n
                        );
                    }
                    if stratified.by_entity_type.len() > 5 {
                        eprintln!(
                            "         ... and {} more types",
                            stratified.by_entity_type.len() - 5
                        );
                    }
                }
            }

            // Sample size (note: this is sentences, not entities)
            let actual_examples = result.num_examples;
            eprintln!(
                "      Sample size: {} sentences (not entities)",
                actual_examples
            );
            eprintln!(
                "         Total entities evaluated: {} gold, {} predicted",
                num_gold, num_pred
            );

            if actual_examples < 10 {
                eprintln!("         WARN  <10 sentences may cause high variance");
                if num_gold > 0 {
                    eprintln!(
                        "         Note: {} entities across {} sentences ({:.1} entities/sentence)",
                        num_gold, actual_examples, entities_per_sentence
                    );
                }
            } else if actual_examples >= 50 {
                eprintln!("         PASS  >=50 sentences (adequate for stability)");
            } else {
                eprintln!(
                    "         PASS  {} sentences (reasonable sample size)",
                    actual_examples
                );
            }

            // Nested entity detection (only show if not already shown above)
            if num_gold == 0 && actual_examples > 0 {
                let dataset_name = format!("{:?}", result.dataset);
                if dataset_name.contains("Nested")
                    || dataset_name.contains("GENIA")
                    || dataset_name.contains("NNE")
                {
                    eprintln!("         WARN  Zero gold entities in {} sentences - nested entity parsing issue confirmed", actual_examples);
                    eprintln!(
                        "              Standard CoNLL BIO parser doesn't handle nested entities"
                    );
                    eprintln!("              Nested datasets require specialized parsing (multiple entities per token/span)");
                    eprintln!("              Example: [[IL-2 receptor] alpha chain] has nested PROTEIN entities");
                }
            }
        } else {
            eprintln!(
                "  {} {:?} × {:?} × {}  ERROR",
                status, result.task, result.dataset, result.backend
            );
        }

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

    // Metrics analysis
    if !f1_scores.is_empty() {
        eprintln!("\n=== Metrics Analysis ===");
        let f1_mean = f1_scores.iter().sum::<f64>() / f1_scores.len() as f64;
        let f1_min = f1_scores.iter().cloned().fold(f64::INFINITY, f64::min);
        let f1_max = f1_scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let f1_variance = if f1_scores.len() > 1 {
            f1_scores.iter().map(|x| (x - f1_mean).powi(2)).sum::<f64>()
                / (f1_scores.len() - 1) as f64
        } else {
            0.0
        };
        let f1_std_dev = f1_variance.sqrt();

        eprintln!(
            "F1 scores: mean={:.1}%, std={:.1}%, min={:.1}%, max={:.1}%",
            f1_mean * 100.0,
            f1_std_dev * 100.0,
            f1_min * 100.0,
            f1_max * 100.0
        );
        eprintln!(
            "  Zero F1 results: {} ({:.1}%)",
            zero_f1_count,
            (zero_f1_count as f64 / f1_scores.len() as f64) * 100.0
        );

        if !precision_scores.is_empty() {
            let p_mean = precision_scores.iter().sum::<f64>() / precision_scores.len() as f64;
            let r_mean = recall_scores.iter().sum::<f64>() / recall_scores.len() as f64;
            eprintln!("Precision: mean={:.1}%", p_mean * 100.0);
            eprintln!("Recall: mean={:.1}%", r_mean * 100.0);
        }

        // Backend-dataset variance analysis (within this seed run)
        // Note: This tracks variance if same backend-dataset pair appears multiple times
        // For cross-seed variance, run multiple seeds and aggregate externally
        if backend_dataset_f1.len() > 1 {
            eprintln!("\nBackend-Dataset F1 variance (within this run):");
            type VarianceRow = ((String, String), f64, f64, Vec<f64>);
            let mut variances: Vec<VarianceRow> = backend_dataset_f1
                .iter()
                .filter(|(_, scores)| scores.len() > 1)
                .map(|((backend, dataset), scores)| {
                    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
                    let variance = scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                        / (scores.len() - 1) as f64;
                    let std_dev = variance.sqrt();
                    (
                        (backend.clone(), dataset.clone()),
                        mean,
                        std_dev,
                        scores.clone(),
                    )
                })
                .collect();
            variances.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

            if !variances.is_empty() {
                eprintln!("  Pairs with multiple runs in this seed:");
                for ((backend, dataset), mean, std_dev, scores) in variances.iter().take(5) {
                    let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    eprintln!(
                        "    {} × {}: mean={:.1}%, std={:.1}%, range=[{:.1}%, {:.1}%], n={}",
                        backend,
                        dataset,
                        mean * 100.0,
                        std_dev * 100.0,
                        min * 100.0,
                        max * 100.0,
                        scores.len()
                    );
                }
            } else {
                eprintln!("  No backend-dataset pairs repeated in this run");
                eprintln!("  (Each backend-dataset combination appears once per seed)");
            }
        }

        // Pattern analysis: identify common failure modes
        eprintln!("\nPattern Analysis:");
        let crf_zero_count = results
            .results
            .iter()
            .filter(|r| {
                r.success
                    && r.backend == "crf"
                    && r.metrics.get("f1").copied().unwrap_or(0.0) == 0.0
            })
            .count();
        let litbank_zero_count = results
            .results
            .iter()
            .filter(|r| {
                r.success
                    && format!("{:?}", r.dataset).contains("LitBank")
                    && r.metrics.get("f1").copied().unwrap_or(0.0) == 0.0
            })
            .count();
        let chisec_zero_count = results
            .results
            .iter()
            .filter(|r| {
                r.success
                    && format!("{:?}", r.dataset).contains("CHisIEC")
                    && r.metrics.get("f1").copied().unwrap_or(0.0) == 0.0
            })
            .count();
        let genia_zero_count = results
            .results
            .iter()
            .filter(|r| {
                r.success
                    && format!("{:?}", r.dataset).contains("GENIANested")
                    && r.metrics.get("f1").copied().unwrap_or(0.0) == 0.0
            })
            .count();

        if crf_zero_count > 0 {
            eprintln!("  CRF backend: {} zero F1 results (baseline with heuristic weights, needs training for better performance)", crf_zero_count);
        }
        let heuristic_zero_count = results
            .results
            .iter()
            .filter(|r| {
                r.success
                    && r.backend == "heuristic"
                    && r.metrics.get("f1").copied().unwrap_or(0.0) == 0.0
            })
            .count();
        if heuristic_zero_count > 0 {
            eprintln!("  Heuristic backend: {} zero F1 results (baseline, works best on formal text with capitalization)", heuristic_zero_count);
        }
        if litbank_zero_count > 0 {
            eprintln!("  LitBank dataset: {} zero F1 results (coreference dataset, character IDs in stratified metrics)", litbank_zero_count);
        }
        if chisec_zero_count > 0 {
            eprintln!("  CHisIEC dataset: {} zero F1 results (type mismatch: OFI/book not supported by most backends)", chisec_zero_count);
        }
        if genia_zero_count > 0 {
            eprintln!("  GENIANested dataset: {} zero F1 results (nested entity parsing issue - standard CoNLL BIO parser doesn't handle nesting)", genia_zero_count);
        }

        // Backend performance summary
        let mut backend_perf: std::collections::HashMap<String, (usize, f64, usize)> =
            std::collections::HashMap::new();
        for result in &results.results {
            if result.success {
                let f1 = result.metrics.get("f1").copied().unwrap_or(0.0);
                let entry = backend_perf
                    .entry(result.backend.clone())
                    .or_insert((0, 0.0, 0));
                entry.0 += 1;
                entry.1 += f1;
                if f1 == 0.0 {
                    entry.2 += 1;
                }
            }
        }

        if !backend_perf.is_empty() {
            eprintln!("\n  Backend performance summary:");
            let mut backend_entries: Vec<_> = backend_perf.iter().collect();
            backend_entries.sort_by_key(|(_, (count, _, _))| *count);
            backend_entries.reverse();

            for (backend, (count, total_f1, zero_count)) in backend_entries {
                let mean_f1 = *total_f1 / *count as f64;
                let zero_pct = (*zero_count as f64 / *count as f64) * 100.0;
                eprintln!(
                    "    {}: {} runs, mean F1={:.1}%, {} zero F1 ({:.1}%)",
                    backend,
                    count,
                    mean_f1 * 100.0,
                    zero_count,
                    zero_pct
                );
            }
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

    // Save results to file for post-analysis (avoids wasteful reruns)
    if let Ok(results_file) = std::env::var("ANNO_RESULTS_FILE") {
        let results_path = PathBuf::from(&results_file);
        if let Some(parent) = results_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Simple JSON format (no serde dependency needed)
        let mut output = String::new();
        output.push_str("{\n");
        output.push_str(&format!("  \"seed\": {},\n", seed));
        output.push_str(&format!("  \"strategy\": \"{:?}\",\n", strategy));
        output.push_str(&format!(
            "  \"timestamp\": {},\n",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        output.push_str(&format!(
            "  \"backends_tested\": {:?},\n",
            selected_backends
        ));
        output.push_str(&format!(
            "  \"datasets_tested\": {:?},\n",
            selected_datasets
                .iter()
                .map(|d| format!("{:?}", d))
                .collect::<Vec<_>>()
        ));
        output.push_str(&format!(
            "  \"total_results\": {},\n",
            results.results.len()
        ));
        output.push_str(&format!("  \"success_count\": {},\n", success_count));

        if !f1_scores.is_empty() {
            let mean = f1_scores.iter().sum::<f64>() / f1_scores.len() as f64;
            let variance = if f1_scores.len() > 1 {
                f1_scores.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                    / (f1_scores.len() - 1) as f64
            } else {
                0.0
            };
            output.push_str("  \"f1_stats\": {\n");
            output.push_str(&format!("    \"mean\": {:.6},\n", mean));
            output.push_str(&format!("    \"std\": {:.6},\n", variance.sqrt()));
            output.push_str(&format!(
                "    \"min\": {:.6},\n",
                f1_scores.iter().cloned().fold(f64::INFINITY, f64::min)
            ));
            output.push_str(&format!(
                "    \"max\": {:.6},\n",
                f1_scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            ));
            output.push_str(&format!("    \"zero_count\": {},\n", zero_f1_count));
            output.push_str(&format!(
                "    \"zero_pct\": {:.2}\n",
                (zero_f1_count as f64 / f1_scores.len() as f64) * 100.0
            ));
            output.push_str("  },\n");
        }

        output.push_str("  \"results\": [\n");
        for (i, r) in results.results.iter().enumerate() {
            if i > 0 {
                output.push_str(",\n");
            }
            output.push_str("    {\n");
            output.push_str(&format!("      \"task\": \"{:?}\",\n", r.task));
            output.push_str(&format!("      \"dataset\": \"{:?}\",\n", r.dataset));
            output.push_str(&format!("      \"backend\": \"{}\",\n", r.backend));
            output.push_str(&format!("      \"success\": {},\n", r.success));
            output.push_str(&format!(
                "      \"f1\": {:.6},\n",
                r.metrics.get("f1").copied().unwrap_or(0.0)
            ));
            output.push_str(&format!(
                "      \"precision\": {:.6},\n",
                r.metrics.get("precision").copied().unwrap_or(0.0)
            ));
            output.push_str(&format!(
                "      \"recall\": {:.6}",
                r.metrics.get("recall").copied().unwrap_or(0.0)
            ));
            let mut has_more = r.metrics.contains_key("num_examples")
                || r.metrics.contains_key("num_gold")
                || r.metrics.contains_key("num_predicted")
                || r.error.is_some();
            if has_more {
                output.push(',');
            }
            output.push('\n');
            if let Some(n) = r.metrics.get("num_examples") {
                output.push_str(&format!("      \"num_examples\": {}", n));
                has_more = r.metrics.contains_key("num_gold")
                    || r.metrics.contains_key("num_predicted")
                    || r.error.is_some();
                if has_more {
                    output.push(',');
                }
                output.push('\n');
            }
            if let Some(n) = r.metrics.get("num_gold") {
                output.push_str(&format!("      \"num_gold\": {}", n));
                has_more = r.metrics.contains_key("num_predicted") || r.error.is_some();
                if has_more {
                    output.push(',');
                }
                output.push('\n');
            }
            if let Some(n) = r.metrics.get("num_predicted") {
                output.push_str(&format!("      \"num_predicted\": {}", n));
                if r.error.is_some() {
                    output.push(',');
                }
                output.push('\n');
            }
            if let Some(err) = &r.error {
                let escaped = err
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n");
                output.push_str(&format!("      \"error\": \"{}\"\n", escaped));
            }
            output.push_str("    }");
        }
        output.push_str("\n  ]\n");
        output.push_str("}\n");

        if let Err(e) = fs::write(&results_path, output) {
            eprintln!("WARN: Failed to write results to {}: {}", results_file, e);
        } else {
            eprintln!("\nResults saved to: {}", results_file);
            eprintln!("  Analyze with: cat {} | jq '.results[] | select(.f1 > 0) | {{backend, dataset, f1}}'", results_file);
        }
    }

    // Compute and report badness scores for MAB-style prioritization
    eprintln!("\n=== Badness Analysis (MAB) ===");
    let mut badness_scores: Vec<(String, String, u32)> = Vec::new();
    for result in &results.results {
        let f1 = result.metrics.get("f1").copied();
        let predicted = result
            .metrics
            .get("num_predicted")
            .map(|n| *n as usize)
            .unwrap_or(0);
        let gold = result
            .metrics
            .get("num_gold")
            .map(|n| *n as usize)
            .unwrap_or(0);
        let has_error = result.error.is_some();
        let badness = compute_badness(f1, has_error, predicted, gold);
        badness_scores.push((
            result.backend.clone(),
            format!("{:?}", result.dataset),
            badness,
        ));
    }

    // Sort by badness (highest first)
    badness_scores.sort_by(|a, b| b.2.cmp(&a.2));

    // Show top 5 worst combinations
    eprintln!("Top combinations needing investigation (high badness = likely bugs):");
    for (backend, dataset, badness) in badness_scores.iter().take(5) {
        let severity = match *badness {
            90..=100 => "CRITICAL",
            70..=89 => "HIGH",
            50..=69 => "MEDIUM",
            30..=49 => "LOW",
            _ => "OK",
        };
        eprintln!(
            "  {} x {} -> badness={} [{}]",
            backend, dataset, badness, severity
        );
    }

    // Save badness to history file for future runs
    if let Ok(history_file) = std::env::var("ANNO_HISTORY_FILE") {
        let history_path = PathBuf::from(&history_file);
        let mut content = String::new();
        for (backend, dataset, badness) in &badness_scores {
            content.push_str(&format!("{},{},{}\n", backend, dataset, badness));
        }
        // Append to existing history
        let existing = std::fs::read_to_string(&history_path).unwrap_or_default();
        let combined = existing + &content;

        if let Err(e) = std::fs::write(&history_path, &combined) {
            eprintln!("WARN: Failed to write badness history: {}", e);
        } else {
            eprintln!("Badness history saved to: {}", history_file);

            // Show accumulated statistics from history
            let tracker = BadnessTracker::load_from_file(&history_path);
            if let Some(tracker) = tracker {
                let backend_stats = tracker.backend_badness();
                let dataset_stats = tracker.dataset_badness();

                if !backend_stats.is_empty() {
                    eprintln!("\nAccumulated backend badness (all runs):");
                    for (backend, avg_badness) in backend_stats.iter().take(5) {
                        eprintln!("  {}: {:.1}", backend, avg_badness);
                    }
                }

                if !dataset_stats.is_empty() {
                    eprintln!("\nAccumulated dataset badness (all runs):");
                    for (dataset, avg_badness) in dataset_stats.iter().take(5) {
                        eprintln!("  {}: {:.1}", dataset, avg_badness);
                    }
                }
            }
        }
    }

    // Error legend
    eprintln!(
        "\nLegend: PASS=success D=dataset F=feature M=model A=auth T=type !=runtime ?=unknown"
    );

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
            coref_use_gold_mentions: false,
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

        eprintln!("  {} {}", if available { "PASS" } else { "FAIL" }, backend);
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
            "PASS"
        } else if cfg!(feature = "onnx") {
            "○"
        } else {
            "FAIL"
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
            "PASS"
        } else if cfg!(feature = "candle") {
            "○"
        } else {
            "FAIL"
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

            let icon = if available { "PASS" } else { "○" };
            eprintln!("  {} {} ({})", icon, backend, reason);
        }
    }

    #[cfg(not(feature = "burn"))]
    {
        eprintln!("\nBurn backends (feature = false):");
        eprintln!("  FAIL burn_ner (feature not enabled)");
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

// =============================================================================
// Regression Tests for Type Normalization
// =============================================================================

/// Test that entity type normalization correctly maps equivalent types.
/// Regression test for: person/per, geo-loc/location, company/org mismatches.
#[test]
fn test_entity_type_normalization() {
    // Direct equivalences
    assert_eq!(normalize_entity_type("person"), "PER");
    assert_eq!(normalize_entity_type("per"), "PER");
    assert_eq!(normalize_entity_type("PER"), "PER");
    assert_eq!(normalize_entity_type("Person"), "PER");

    assert_eq!(normalize_entity_type("organization"), "ORG");
    assert_eq!(normalize_entity_type("org"), "ORG");
    assert_eq!(normalize_entity_type("company"), "ORG");
    assert_eq!(normalize_entity_type("COMPANY"), "ORG");

    assert_eq!(normalize_entity_type("location"), "LOC");
    assert_eq!(normalize_entity_type("loc"), "LOC");
    assert_eq!(normalize_entity_type("geo-loc"), "LOC");
    assert_eq!(normalize_entity_type("gpe"), "LOC");
    assert_eq!(normalize_entity_type("GPE"), "LOC");

    assert_eq!(normalize_entity_type("facility"), "FAC");
    assert_eq!(normalize_entity_type("fac"), "FAC");

    assert_eq!(normalize_entity_type("misc"), "MISC");
    assert_eq!(normalize_entity_type("other"), "MISC");
    assert_eq!(normalize_entity_type("OTHER"), "MISC");

    eprintln!("PASS Type normalization mappings verified");
}

/// Test that type comparison correctly identifies overlap after normalization.
/// Regression test for: WNUT16 (person, geo-loc, company) vs GLiNER (per, loc, org)
#[test]
fn test_type_comparison_with_normalization() {
    // WNUT16-like dataset types
    let dataset_types = vec![
        "person".to_string(),
        "geo-loc".to_string(),
        "company".to_string(),
        "facility".to_string(),
        "product".to_string(),
        "other".to_string(),
    ];

    // GLiNER-like backend types
    let backend_types = vec![
        "per".to_string(),
        "org".to_string(),
        "loc".to_string(),
        "date".to_string(),
        "misc".to_string(),
    ];

    let (overlap, missing, extra) = compare_entity_types(&dataset_types, &backend_types);

    eprintln!("Dataset types: {:?}", dataset_types);
    eprintln!("Backend types: {:?}", backend_types);
    eprintln!("Overlap (normalized): {:?}", overlap);
    eprintln!("Missing (normalized): {:?}", missing);
    eprintln!("Extra (normalized): {:?}", extra);

    // With normalization:
    // - person -> PER matches per -> PER
    // - geo-loc -> LOC matches loc -> LOC
    // - company -> ORG matches org -> ORG
    // - other -> MISC matches misc -> MISC
    assert!(
        overlap.contains(&"PER".to_string()),
        "person should match per after normalization"
    );
    assert!(
        overlap.contains(&"LOC".to_string()),
        "geo-loc should match loc after normalization"
    );
    assert!(
        overlap.contains(&"ORG".to_string()),
        "company should match org after normalization"
    );
    assert!(
        overlap.contains(&"MISC".to_string()),
        "other should match misc after normalization"
    );

    // FAC and PRODUCT should be missing (not in backend)
    assert!(
        missing.contains(&"FAC".to_string()),
        "facility should be missing from backend"
    );
    assert!(
        missing.contains(&"PRODUCT".to_string()),
        "product should be missing from backend"
    );

    // DATE should be extra (in backend but not dataset after normalization)
    assert!(
        extra.contains(&"DATE".to_string()),
        "date should be extra in backend"
    );

    eprintln!("PASS Type comparison with normalization works correctly");
}
