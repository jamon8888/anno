//! CI-friendly randomized matrix test (muxer-backed).
//!
//! This replaces the older archived test harness with something that:
//! - compiles under `--features eval-advanced` (no `cli` feature required)
//! - uses `muxer` for deterministic, windowed MAB-style backend selection
//! - respects `ANNO_CI_SEED` and `ANNO_SAMPLE_STRATEGY`
//! - only evaluates cached datasets when running in CI (fast + offline)
//!
//! Environment variables:
//! - `ANNO_CI_SEED`: u64 seed (default: 0)
//! - `ANNO_SAMPLE_STRATEGY`: `random` | `ml-only` | `worst-first` (default: `ml-only`)
//! - `ANNO_HISTORY_FILE`: optional JSON path for muxer history
//! - `ANNO_MAX_EXAMPLES`: max examples per dataset (default: 20 in CI, 50 locally)
//! - `ANNO_CACHE_DIR`: cache root (datasets live under `$ANNO_CACHE_DIR/datasets`)
//!
//! Notes:
//! - “worst-first” here means “prioritize historically bad outcomes” where “bad” is
//!   an eval failure or low F1, recorded into a muxer window.

#![cfg(all(test, feature = "eval-advanced"))]

use crate::eval::backend_factory::BackendFactory;
use crate::eval::loader::{DatasetId, DatasetLoader};
use crate::eval::task_evaluator::{TaskEvalConfig, TaskEvaluator};
use crate::eval::task_mapping::{backend_tasks, dataset_tasks, Task};
use muxer::{MabConfig, Outcome, Summary, Window};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
enum SampleStrategy {
    Random,
    MlOnly,
    WorstFirst,
}

impl SampleStrategy {
    fn from_env() -> Self {
        match std::env::var("ANNO_SAMPLE_STRATEGY")
            .ok()
            .unwrap_or_else(|| "ml-only".to_string())
            .to_lowercase()
            .as_str()
        {
            "random" => Self::Random,
            "worst-first" | "worstfirst" => Self::WorstFirst,
            // Historical default in the legacy harness.
            "ml-only" | "mlonly" | "ml" => Self::MlOnly,
            _ => Self::MlOnly,
        }
    }
}

fn in_ci() -> bool {
    std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok()
}

fn ci_seed() -> u64 {
    std::env::var("ANNO_CI_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn max_examples_per_dataset() -> usize {
    // Keep this intentionally small: this test runs inside `cargo test` and must not time out.
    let default = if in_ci() { 10 } else { 25 };
    std::env::var("ANNO_MAX_EXAMPLES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone, Copy)]
enum MatrixPerspective {
    Ner,
    Coref,
    Coalesce,
}

impl MatrixPerspective {
    fn from_env() -> Self {
        match std::env::var("ANNO_MATRIX_PERSPECTIVE")
            .ok()
            .unwrap_or_else(|| "ner".to_string())
            .to_lowercase()
            .as_str()
        {
            "coref" | "intra-coref" | "intracoref" => Self::Coref,
            "coalesce" | "inter-coref" | "intercoref" | "cdcr" => Self::Coalesce,
            _ => Self::Ner,
        }
    }

    fn tasks(&self) -> Vec<Task> {
        match self {
            Self::Ner => vec![Task::NER],
            Self::Coref => vec![Task::IntraDocCoref],
            Self::Coalesce => vec![Task::InterDocCoref],
        }
    }

    fn preferred_datasets(&self) -> &'static [DatasetId] {
        match self {
            Self::Ner => &[
                DatasetId::WikiGold,
                DatasetId::Wnut17,
                DatasetId::MitRestaurant,
                DatasetId::MitMovie,
                DatasetId::CoNLL2003Sample,
            ],
            // Prefer smaller/common coref datasets that are likely to be cached.
            Self::Coref => &[
                DatasetId::GAP,
                DatasetId::PreCo,
                DatasetId::OntoNotesCoref,
                DatasetId::LitBank,
            ],
            // Cross-doc / CDCR style.
            Self::Coalesce => &[DatasetId::ECBPlus, DatasetId::WikiCoref],
        }
    }
}

fn history_path() -> PathBuf {
    if let Ok(p) = std::env::var("ANNO_HISTORY_FILE") {
        return PathBuf::from(p);
    }

    // Prefer ANNO_CACHE_DIR so it matches CI caching.
    if let Ok(dir) = std::env::var("ANNO_CACHE_DIR") {
        return PathBuf::from(dir).join("muxer_history.json");
    }

    // Fallback: place next to default dataset cache root.
    // DatasetLoader::new() uses platform cache: ~/.cache/anno/datasets (linux).
    // We'll store history alongside `.../anno/`.
    #[cfg(feature = "eval")]
    {
        if let Some(base) = dirs::cache_dir() {
            return base.join("anno").join("muxer_history.json");
        }
    }
    PathBuf::from(".").join("muxer_history.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BackendHistory {
    #[serde(default)]
    version: u32,
    window_cap: usize,
    windows: BTreeMap<String, Window>,
}

impl BackendHistory {
    fn load(path: &PathBuf, window_cap: usize) -> Self {
        let bytes = fs::read(path).ok();
        if let Some(bytes) = bytes {
            if let Ok(h) = serde_json::from_slice::<BackendHistory>(&bytes) {
                // v2+ format (explicit version field).
                if h.version >= 2 {
                    return h;
                }
                // Otherwise, fall through to legacy migration path below.
            }

            // Legacy format (no version field, Window serialized as {cap, buf}).
            #[derive(Debug, Clone, serde::Deserialize)]
            struct WindowSerde {
                cap: usize,
                buf: VecDeque<Outcome>,
            }
            #[derive(Debug, Clone, serde::Deserialize)]
            struct BackendHistoryLegacy {
                window_cap: usize,
                windows: BTreeMap<String, WindowSerde>,
            }

            if let Ok(legacy) = serde_json::from_slice::<BackendHistoryLegacy>(&bytes) {
                // Keep these fields to document/validate the legacy shape.
                let _ = legacy.window_cap;
                let mut windows: BTreeMap<String, Window> = BTreeMap::new();
                for (backend, w) in legacy.windows {
                    let _ = w.cap;
                    let mut out = Window::new(window_cap.max(1));
                    for mut o in w.buf {
                        // v1 semantics stored ok := bad. Recover success from hard_junk.
                        o.ok = !o.hard_junk;
                        out.push(o);
                    }
                    windows.insert(backend, out);
                }
                return Self {
                    version: 2,
                    window_cap: window_cap.max(1),
                    windows,
                };
            }
        }
        Self {
            version: 2,
            window_cap: window_cap.max(1),
            windows: BTreeMap::new(),
        }
    }

    fn save(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path, bytes);
        }
    }

    fn window_mut(&mut self, backend: &str) -> &mut Window {
        self.windows
            .entry(backend.to_string())
            .or_insert_with(|| Window::new(self.window_cap))
    }

    fn summaries_for(&self, arms: &[String]) -> BTreeMap<String, Summary> {
        let mut out = BTreeMap::new();
        for a in arms {
            let s = self.windows.get(a).map(|w| w.summary()).unwrap_or_default();
            out.insert(a.clone(), s);
        }
        out
    }
}

fn stable_hash64(seed: u64, s: &str) -> u64 {
    // Deterministic (not crypto): stable per process and platform.
    // FNV-1a 64-bit with seed mixing.
    let mut h: u64 = 14695981039346656037u64 ^ seed;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211u64);
    }
    h
}

fn pick_random_subset(seed: u64, items: &[String], k: usize) -> Vec<String> {
    // Deterministic sampling without external RNG dependencies.
    let mut scored: Vec<(u64, &String)> =
        items.iter().map(|s| (stable_hash64(seed, s), s)).collect();
    scored.sort_by_key(|(h, s)| (*h, (*s).as_str()));
    scored
        .into_iter()
        .take(k.min(items.len()))
        .map(|(_, s)| s.clone())
        .collect()
}

fn select_backends(
    strategy: SampleStrategy,
    seed: u64,
    history: &BackendHistory,
    candidates_in_order: &[String],
    k: usize,
) -> Vec<String> {
    if candidates_in_order.is_empty() || k == 0 {
        return Vec::new();
    }

    match strategy {
        SampleStrategy::Random => pick_random_subset(seed, candidates_in_order, k),
        SampleStrategy::MlOnly => {
            // MAB selection (muxer): pick historically "best" arms (high ok_rate, low junk),
            // with exploration to avoid fixating too early.
            let cfg = MabConfig {
                exploration_c: 0.8,
                cost_weight: 0.0,
                latency_weight: 0.0,
                junk_weight: 0.0,
                hard_junk_weight: 0.0,
                ..MabConfig::default()
            };

            let mut remaining: Vec<String> = candidates_in_order.to_vec();
            let mut chosen: Vec<String> = Vec::new();
            for _ in 0..k.min(remaining.len()) {
                let summaries = history.summaries_for(&remaining);
                let sel = muxer::select_mab(&remaining, &summaries, cfg);
                let pick = sel.chosen;
                remaining.retain(|b| b != &pick);
                chosen.push(pick);
            }
            chosen
        }
        SampleStrategy::WorstFirst => {
            // Worst-first selection is intentionally *not* muxer::select_mab:
            // muxer is designed to pick "good" providers. Here we want to bias toward
            // historically bad/flaky arms (to find regressions), while still exploring.
            //
            // Score: higher means "worse", with an exploration term favoring under-sampled arms.
            let mut remaining: Vec<String> = candidates_in_order.to_vec();
            let mut chosen: Vec<String> = Vec::new();

            for round in 0..k.min(remaining.len()) {
                let summaries = history.summaries_for(&remaining);
                let total_calls: f64 = summaries
                    .values()
                    .map(|s| s.calls as f64)
                    .sum::<f64>()
                    .max(1.0);

                let mut best: Option<(String, f64)> = None;
                for b in &remaining {
                    let s = summaries.get(b).copied().unwrap_or_default();
                    let calls = (s.calls as f64).max(1.0);
                    let ok_rate = s.ok_rate();
                    let hard_junk = s.hard_junk_rate();
                    let soft_junk = s.soft_junk_rate();

                    let bad_rate = 1.0 - ok_rate;
                    let exploration = 0.8 * ((total_calls.ln() / calls).sqrt());
                    let score = bad_rate + 0.7 * hard_junk + 0.3 * soft_junk + exploration;

                    match best {
                        None => best = Some((b.clone(), score)),
                        Some((ref best_name, best_score)) => {
                            if score > best_score
                                || ((score - best_score).abs() <= 1e-12 && b < best_name)
                            {
                                best = Some((b.clone(), score));
                            }
                        }
                    }
                }

                let pick = best
                    .map(|(b, _)| b)
                    .unwrap_or_else(|| remaining[round].clone());
                remaining.retain(|b| b != &pick);
                chosen.push(pick);
            }

            chosen
        }
    }
}

fn backend_candidates(strategy: SampleStrategy, tasks: &[Task]) -> Vec<String> {
    // Start from what is actually feature-enabled in this build.
    let available: BTreeSet<String> = BackendFactory::available_backends()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    // Keep the CI matrix focused: always include fast baselines; include ML only when asked.
    // Note: BackendFactory names are stable across the codebase.
    let mut out: Vec<String> = Vec::new();
    let want = |name: &str| available.contains(name);

    // Baselines.
    for b in ["pattern", "heuristic", "stacked", "crf", "hmm", "ensemble"] {
        if want(b) {
            out.push(b.to_string());
        }
    }

    // Coreference resolvers do not implement `Model`, so they are not returned by
    // `BackendFactory::available_backends()`. They are created via
    // `eval::backend_factory::create_coref_resolver()` and are still valid backend
    // “arms” for coref-family tasks.
    if tasks.iter().any(|t| t.is_coref_family()) {
        out.extend(
            ["coref_resolver", "mention_ranking", "box"]
                .into_iter()
                .map(|s| s.to_string()),
        );
    }

    match strategy {
        SampleStrategy::Random => {
            // Include the full available set, but keep a stable order.
            let mut all: Vec<String> = available.into_iter().collect();
            all.sort();
            // Also include coref resolvers when coref tasks are requested.
            if tasks.iter().any(|t| t.is_coref_family()) {
                for b in ["coref_resolver", "mention_ranking", "box"] {
                    if !all.iter().any(|x| x == b) {
                        all.push(b.to_string());
                    }
                }
                all.sort();
                all.dedup();
            }
            all
        }
        SampleStrategy::MlOnly | SampleStrategy::WorstFirst => {
            // CI default: baselines only.
            //
            // Rationale: the default feature set enables `onnx`, so ML backends are *available*
            // but can be expensive and flaky if model caches are cold. Make ML opt-in via env:
            // `ANNO_ML_IN_MATRIX=1`.
            let allow_ml = std::env::var("ANNO_ML_IN_MATRIX")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
            if allow_ml {
                for b in [
                    "gliner",
                    "gliner_onnx",
                    "nuner",
                    "w2ner",
                    "gliner2",
                    "bert_onnx",
                    "deberta_v3",
                    "albert",
                    "candle_ner",
                    "gliner_candle",
                    "burn",
                ] {
                    if want(b) {
                        out.push(b.to_string());
                    }
                }
            }
            out.sort();
            out.dedup();
            out
        }
    }
}

fn cached_dataset_candidates(loader: &DatasetLoader) -> Vec<DatasetId> {
    // Prefer cached datasets only; TaskEvaluator can filter further by task support.
    let mut out = Vec::new();
    for &ds in DatasetId::all() {
        let Ok(loadable) = crate::eval::LoadableDatasetId::try_from(ds) else {
            continue;
        };
        if loader.is_cached(loadable) {
            out.push(ds);
        }
    }
    out
}

#[test]
fn test_randomized_matrix_sample() {
    crate::env::load_dotenv();

    let seed = ci_seed();
    let strategy = SampleStrategy::from_env();
    let perspective = MatrixPerspective::from_env();
    let hist_path = history_path();
    let window_cap = 50;

    let loader = DatasetLoader::new().expect("DatasetLoader::new");
    let cached_datasets = cached_dataset_candidates(&loader);

    // If CI cache is cold, do not fail the whole build; this job is intended to be best-effort.
    // (Other jobs provide strict compile/test guarantees.)
    if cached_datasets.is_empty() {
        eprintln!(
            "matrix-muxer: no cached datasets found; set ANNO_CACHE_DIR and cache it in CI (path={:?})",
            loader.cache_dir()
        );
        return;
    }

    let mut history = BackendHistory::load(&hist_path, window_cap);

    let tasks = perspective.tasks();

    // Backends: choose a *tiny* subset per run (keep this test fast).
    let mut candidates = backend_candidates(strategy, &tasks);
    if candidates.is_empty() {
        eprintln!("matrix-muxer: no backend candidates available for this feature set");
        return;
    }

    // Filter backend candidates down to those that can actually serve the chosen tasks.
    candidates.retain(|b| {
        let ts = backend_tasks(b);
        tasks.iter().any(|t| ts.contains(t))
    });
    if candidates.is_empty() {
        eprintln!(
            "matrix-muxer: no backend candidates support tasks={:?} for this feature set",
            tasks
        );
        return;
    }

    let chosen_backends = select_backends(strategy, seed, &history, &candidates, 2);

    // Datasets: choose 2 cached datasets, biased toward small/common ones.
    // Keep this deterministic and stable across runs.
    let eligible_cached: Vec<DatasetId> = cached_datasets
        .iter()
        .copied()
        .filter(|d| {
            let ts = dataset_tasks(*d);
            tasks.iter().any(|t| ts.contains(t))
        })
        .collect();
    if eligible_cached.is_empty() {
        eprintln!(
            "matrix-muxer: no cached datasets support tasks={:?} (cache_dir={:?})",
            tasks,
            loader.cache_dir()
        );
        return;
    }

    let mut chosen_datasets: Vec<DatasetId> = perspective
        .preferred_datasets()
        .iter()
        .copied()
        .filter(|d| eligible_cached.contains(d))
        .take(2)
        .collect();
    if chosen_datasets.len() < 2 {
        // Fallback: deterministic sample of whatever is cached.
        let ds_strings: Vec<String> = eligible_cached.iter().map(|d| format!("{:?}", d)).collect();
        let chosen_ds_strings = pick_random_subset(seed ^ 0xDADA_BEEF, &ds_strings, 2);
        chosen_datasets.extend(
            eligible_cached
                .into_iter()
                .filter(|d| chosen_ds_strings.contains(&format!("{:?}", d))),
        );
        chosen_datasets.sort_by_key(|d| format!("{:?}", d));
        chosen_datasets.dedup();
        chosen_datasets.truncate(2);
    }

    let eval = TaskEvaluator::new().expect("TaskEvaluator::new");

    let config = TaskEvalConfig {
        tasks,
        datasets: chosen_datasets,
        backends: chosen_backends.clone(),
        max_examples: Some(max_examples_per_dataset()),
        seed: Some(seed),
        require_cached: true,
        relation_threshold: 0.5,
        robustness: false,
        compute_familiarity: false,
        temporal_stratification: false,
        confidence_intervals: false,
        custom_coref_resolver: None,
        coref_use_gold_mentions: std::env::var("ANNO_COREF_GOLD")
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
    };

    let results = match eval.evaluate_all(config) {
        Ok(r) => r,
        Err(e) => {
            // If the requested slice has no valid combinations, treat as non-fatal.
            eprintln!("matrix-muxer: evaluation returned error: {}", e);
            return;
        }
    };

    // Update per-backend muxer windows.
    //
    // Record outcomes into per-backend windows.
    //
    // Muxer semantics:
    // - Outcome.ok: "request succeeded" (here: evaluation succeeded)
    // - Outcome.junk: low-signal output (here: very low F1)
    // - Outcome.hard_junk: hard failure (here: evaluation failed)
    for r in &results.results {
        let f1 = r.metrics.get("f1").copied().unwrap_or(0.0);
        let dur_ms = r.duration_ms.unwrap_or(0.0).max(0.0);

        let o = Outcome {
            ok: r.success,
            http_429: false,
            junk: f1 < 0.30,
            hard_junk: !r.success,
            cost_units: 1,
            elapsed_ms: dur_ms as u64,
        };
        history.window_mut(&r.backend).push(o);
    }

    // Persist history best-effort for future runs.
    history.save(&hist_path);

    // If we got here, the harness executed. Failures are recorded, not fatal.
    // (This job is intended to find regressions over time, not block all merges on flaky data.)
}
