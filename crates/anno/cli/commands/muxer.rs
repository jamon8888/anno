//! Muxer command - Inspect the muxer-backed CI matrix history.
//!
//! This is a DX tool: it helps answer questions like:
//! - Have we "seen" each arm at least once?
//! - Which arms look flaky / low-signal recently (windowed)?
//! - Are we mixing signals across tasks/datasets (weight bleed)?

use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[cfg(feature = "eval-advanced")]
use crate::eval::backend_factory::BackendFactory;
#[cfg(feature = "eval-advanced")]
use crate::eval::task_mapping::{backend_tasks, get_task_backends, Task};

// Note: we intentionally do NOT depend on the `muxer` crate here.
// In this repo, `muxer` is a dev-dependency used by the matrix test harness. The CLI should
// still compile without pulling dev-deps into normal builds.

/// Inspect muxer history from the randomized matrix harness.
#[derive(Parser, Debug)]
#[command(about = "Inspect muxer history from the randomized matrix harness")]
pub struct MuxerArgs {
    /// Override history file path (otherwise uses env/defaults).
    #[arg(long)]
    pub history_file: Option<PathBuf>,

    /// Perspective to inspect (controls tasks + preferred datasets in the matrix harness).
    #[arg(long, value_enum)]
    pub perspective: Option<MuxerPerspective>,

    /// Sampling strategy (as used by the matrix harness).
    #[arg(long, value_enum)]
    pub strategy: Option<MuxerStrategy>,

    /// Include ML backends in candidate set (same intent as `ANNO_ML_IN_MATRIX=1`).
    #[arg(long, default_value_t = false)]
    pub include_ml: bool,

    /// Action to run.
    #[command(subcommand)]
    pub action: MuxerAction,
}

/// Subcommands for `anno muxer`.
#[derive(Subcommand, Debug)]
pub enum MuxerAction {
    /// Print coverage + per-arm window stats.
    Stats,
}

/// Which slice of the matrix harness to inspect.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MuxerPerspective {
    /// Named entity recognition slice.
    Ner,
    /// Within-document coreference slice.
    Coref,
    /// Cross-document clustering slice.
    Coalesce,
    /// Relation extraction slice.
    Relation,
}

impl MuxerPerspective {
    fn tag(&self) -> &'static str {
        match self {
            Self::Ner => "ner",
            Self::Coref => "coref",
            Self::Coalesce => "coalesce",
            Self::Relation => "relation",
        }
    }

    fn tasks(&self) -> Vec<Task> {
        match self {
            Self::Ner => vec![Task::NER],
            Self::Coref => vec![Task::IntraDocCoref],
            Self::Coalesce => vec![Task::InterDocCoref],
            Self::Relation => vec![Task::RelationExtraction],
        }
    }
}

/// Which sampling strategy to assume when interpreting candidate sets.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MuxerStrategy {
    /// Deterministic random subset.
    Random,
    /// MAB-driven selection (prefers higher ok-rate / lower junk).
    MlOnly,
    /// Regression-hunting selection (bias toward historically bad/flaky arms).
    WorstFirst,
}

impl MuxerStrategy {
    fn to_env_str(&self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::MlOnly => "ml-only",
            Self::WorstFirst => "worst-first",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BackendHistory {
    #[serde(default)]
    version: u32,
    window_cap: usize,
    windows: BTreeMap<String, WindowSerde>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
struct OutcomeSerde {
    ok: bool,
    #[serde(default)]
    http_429: bool,
    #[serde(default)]
    junk: bool,
    #[serde(default)]
    hard_junk: bool,
    #[serde(default)]
    cost_units: u64,
    #[serde(default)]
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct WindowSerde {
    #[serde(default)]
    cap: usize,
    #[serde(default)]
    buf: Vec<OutcomeSerde>,
}

#[derive(Debug, Clone, Copy, Default)]
struct SummarySerde {
    calls: u64,
    ok: u64,
    http_429: u64,
    junk: u64,
    hard_junk: u64,
    cost_units: u64,
    elapsed_ms_sum: u64,
}

impl SummarySerde {
    fn from_window(w: &WindowSerde) -> Self {
        let mut out = SummarySerde::default();
        out.calls = w.buf.len() as u64;
        for o in &w.buf {
            out.ok += o.ok as u64;
            out.http_429 += o.http_429 as u64;
            out.junk += o.junk as u64;
            out.hard_junk += o.hard_junk as u64;
            out.cost_units = out.cost_units.saturating_add(o.cost_units);
            out.elapsed_ms_sum = out.elapsed_ms_sum.saturating_add(o.elapsed_ms);
        }
        out
    }

    fn ok_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            (self.ok as f64) / (self.calls as f64)
        }
    }

    fn junk_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            (self.junk as f64) / (self.calls as f64)
        }
    }

    fn hard_junk_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            (self.hard_junk as f64) / (self.calls as f64)
        }
    }

    fn mean_elapsed_ms(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            (self.elapsed_ms_sum as f64) / (self.calls as f64)
        }
    }
}

impl BackendHistory {
    fn load(path: &PathBuf, default_window_cap: usize) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let mut h: BackendHistory = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
        if h.version == 0 {
            // Older files may not have carried a version.
            h.version = 1;
        }
        if h.window_cap == 0 {
            h.window_cap = default_window_cap.max(1);
        }
        Ok(h)
    }

    fn summaries_for(&self, arms: &[String]) -> BTreeMap<String, SummarySerde> {
        let mut out = BTreeMap::new();
        for a in arms {
            let s = self
                .windows
                .get(a)
                .map(SummarySerde::from_window)
                .unwrap_or_default();
            out.insert(a.clone(), s);
        }
        out
    }
}

fn default_history_path(perspective: MuxerPerspective) -> PathBuf {
    if let Ok(p) = std::env::var("ANNO_HISTORY_FILE") {
        return PathBuf::from(p);
    }
    if let Ok(dir) = std::env::var("ANNO_CACHE_DIR") {
        return PathBuf::from(dir).join(format!("muxer_history.{}.json", perspective.tag()));
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("anno")
        .join(format!("muxer_history.{}.json", perspective.tag()))
}

fn backend_candidates(tasks: &[Task], include_ml: bool) -> Vec<String> {
    // Start from what is actually feature-enabled in this build.
    let available: BTreeSet<String> = BackendFactory::available_backends()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    // Use the same "allowed backends per task" list as the evaluator to avoid
    // reporting "unseen" arms that will never be executed.
    let allowed: BTreeSet<&'static str> =
        tasks.iter().flat_map(|t| get_task_backends(*t)).collect();

    let mut out: Vec<String> = Vec::new();
    for b in allowed {
        // Keep the CLI behavior: ML-only controls whether we include ML backends,
        // but always include the core baselines if they are available.
        if !include_ml
            && matches!(
                b,
                "gliner"
                    | "gliner_onnx"
                    | "gliner_candle"
                    | "gliner2"
                    | "gliner_poly"
                    | "bert_onnx"
                    | "deberta_v3"
                    | "albert"
                    | "candle_ner"
                    | "burn"
                    | "nuner"
                    | "w2ner"
                    | "universal_ner"
            )
        {
            continue;
        }
        // Some coref resolvers aren't in `BackendFactory::available_backends()`, but they still
        // show up in history, so keep them even if not "available" per factory.
        if b == "coref_resolver" || b == "mention_ranking" || b == "box" {
            out.push(b.to_string());
            continue;
        }
        if available.contains(b) {
            out.push(b.to_string());
        }
    }

    // Filter to task support (defensive).
    out.retain(|b| tasks.iter().any(|t| backend_tasks(b).contains(t)));
    out.sort();
    out.dedup();
    out
}

#[cfg(feature = "eval-advanced")]
/// Execute the muxer command.
pub fn run(args: MuxerArgs) -> Result<(), String> {
    let perspective = args.perspective.unwrap_or(MuxerPerspective::Ner);
    let strategy = args.strategy.unwrap_or_else(|| {
        // Mirror harness defaults.
        match std::env::var("ANNO_SAMPLE_STRATEGY")
            .ok()
            .unwrap_or_else(|| "ml-only".to_string())
            .to_lowercase()
            .as_str()
        {
            "random" => MuxerStrategy::Random,
            "worst-first" | "worstfirst" => MuxerStrategy::WorstFirst,
            _ => MuxerStrategy::MlOnly,
        }
    });

    let history_path = args
        .history_file
        .unwrap_or_else(|| default_history_path(perspective));

    let tasks = perspective.tasks();
    let candidates = backend_candidates(&tasks, args.include_ml);

    let h = match BackendHistory::load(&history_path, 50) {
        Ok(h) => h,
        Err(e) => {
            return Err(format!(
                "{e}\nHint: run the matrix harness at least once (e.g. `ANNO_ML_IN_MATRIX=1 cargo test -p anno --features eval-advanced test_randomized_matrix_sample -- --nocapture`)."
            ));
        }
    };

    match args.action {
        MuxerAction::Stats => {
            println!("=== muxer history ===\n");
            println!("History file: {}", history_path.display());
            println!("Version: {}", h.version);
            println!("Window cap: {}", h.window_cap);
            println!("Perspective: {} ({:?})", perspective.tag(), tasks);
            println!(
                "Strategy: {} ({})",
                strategy.to_env_str(),
                strategy.to_env_str()
            );
            println!("Candidate arms: {}", candidates.len());

            let summaries = h.summaries_for(&candidates);
            let seen: Vec<String> = candidates
                .iter()
                .filter(|a| summaries.get(*a).copied().unwrap_or_default().calls > 0)
                .cloned()
                .collect();
            let unseen: Vec<String> = candidates
                .iter()
                .filter(|a| summaries.get(*a).copied().unwrap_or_default().calls == 0)
                .cloned()
                .collect();

            println!("Seen arms: {}", seen.len());
            println!("Unseen arms: {}", unseen.len());
            if !unseen.is_empty() {
                println!("\nUnseen:");
                for a in &unseen {
                    println!("  {a}");
                }
            }

            // Show a small per-arm table sorted by (calls desc, name).
            let mut rows: Vec<(u64, f64, f64, f64, f64, String)> = Vec::new();
            for a in &candidates {
                let s = summaries.get(a).copied().unwrap_or_default();
                rows.push((
                    s.calls,
                    s.ok_rate(),
                    s.junk_rate(),
                    s.hard_junk_rate(),
                    s.mean_elapsed_ms(),
                    a.clone(),
                ));
            }
            rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.5.cmp(&b.5)));

            println!("\nBy calls (windowed):");
            println!(
                "{:<16} {:>5} {:>6} {:>6} {:>6} {:>9}",
                "arm", "calls", "ok", "junk", "hard", "mean_ms"
            );
            for (calls, ok, junk, hard, mean_ms, arm) in rows.iter().take(20) {
                println!(
                    "{:<16} {:>5} {:>6.2} {:>6.2} {:>6.2} {:>9.0}",
                    arm, calls, ok, junk, hard, mean_ms
                );
            }

            println!("\nNote: muxer windows are per-arm, not per (arm, dataset, task). If you reuse one history file across multiple tasks/datasets, the objective can bleed across slices.");
        }
    }

    Ok(())
}

#[cfg(not(feature = "eval-advanced"))]
pub fn run(_args: MuxerArgs) -> Result<(), String> {
    Err("Muxer command requires --features eval-advanced".to_string())
}
