//! Demonstration of evaluation history tracking.
//!
//! This example shows how evaluation results are automatically tracked
//! in both JSONL (human-readable) and SQLite (queryable) formats.
//!
//! Run with: cargo run -p anno --example eval_history_demo --features eval

use anno::eval::history::EvalHistory;
use anno::eval::loader::DatasetId;
use anno::eval::task_evaluator::{TaskEvalConfig, TaskEvaluator};
use anno::eval::task_mapping::Task;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for this demo
    let temp_dir = tempfile::tempdir()?;
    let history_path = temp_dir.path().join("eval-results.jsonl");

    println!("=== Evaluation History Demo ===\n");
    println!("History file: {}\n", history_path.display());

    // Set environment variable so evaluator uses our history path
    std::env::set_var("ANNO_EVAL_HISTORY", history_path.to_str().unwrap());

    // Create evaluator (will auto-initialize history)
    let evaluator = TaskEvaluator::new()?;

    // Run a small evaluation
    println!("Running evaluation...");
    let config = TaskEvalConfig {
        tasks: vec![Task::NER],
        datasets: vec![DatasetId::WikiGold],
        backends: vec!["stacked".to_string()],
        max_examples: Some(5),
        seed: Some(42),
        require_cached: false,
        relation_threshold: 0.5,
        robustness: false,
        compute_familiarity: false,
        temporal_stratification: false,
        confidence_intervals: false,
        custom_coref_resolver: None,
        coref_use_gold_mentions: false,
    };

    let results = evaluator.evaluate_all(config)?;
    println!("Evaluated {} combinations\n", results.results.len());

    // Access history directly
    let history = EvalHistory::new(&history_path)?;

    // Show statistics
    println!("=== History Statistics ===");
    let stats = history.stats()?;
    println!("Total entries: {}", stats.total_entries);
    if let Some(avg_f1) = stats.avg_f1 {
        println!("Average F1: {:.2}%", avg_f1 * 100.0);
    }
    println!("\nBy backend:");
    for (backend, count) in &stats.by_backend {
        println!("  {}: {} entries", backend, count);
    }
    println!("\nBy dataset:");
    for (dataset, count) in &stats.by_dataset {
        println!("  {}: {} entries", dataset, count);
    }

    // Query recent results
    println!("\n=== Recent Results (stacked backend) ===");
    let recent = history.query_recent("stacked", 5)?;
    for (i, entry) in recent.iter().enumerate() {
        println!(
            "{}. {} on {}: F1={:.2}% ({} examples, {}ms)",
            i + 1,
            entry.backend,
            entry.dataset,
            entry.f1.unwrap_or(0.0) * 100.0,
            entry.n,
            entry.duration_ms.unwrap_or(0.0) as u64
        );
    }

    // Query best results
    println!("\n=== Best Results (by F1) ===");
    let best = history.query_best("stacked", None, 3)?;
    for (i, entry) in best.iter().enumerate() {
        if let Some(f1) = entry.f1 {
            println!(
                "{}. {} on {}: F1={:.2}%",
                i + 1,
                entry.backend,
                entry.dataset,
                f1 * 100.0
            );
        }
    }

    // Demonstrate date range query (last 24 hours)
    println!("\n=== Date Range Query (last 24 hours) ===");
    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::hours(24);
    let date_range =
        history.query_by_date_range(&yesterday.to_rfc3339(), &now.to_rfc3339(), Some("stacked"))?;
    println!("Found {} results in the last 24 hours", date_range.len());

    // Demonstrate backend comparison (if we had multiple backends)
    println!("\n=== Backend Comparison ===");
    println!("(Run with multiple backends to see comparison)");
    let comparison = history.compare_backends("stacked", "stacked", None)?;
    println!("Comparison query returned {} entries", comparison.len());

    // Show file locations
    println!("\n=== Storage Locations ===");
    println!("JSONL (source of truth): {}", history_path.display());
    if history_path.parent().is_some() {
        let sqlite_path = history_path.parent().unwrap().join("eval-history.db");
        if sqlite_path.exists() {
            println!("SQLite (queryable index): {}", sqlite_path.display());
            println!("  (Use sqlite3 to query directly)");
        }
    }

    // Demonstrate rebuild capability
    println!("\n=== Rebuilding SQLite Index ===");
    history.rebuild_index()?;
    println!("Index rebuilt successfully");

    println!("\n=== Demo Complete ===");
    println!("History persists at: {}", history_path.display());
    println!("You can query it later or use it for trend analysis.");

    Ok(())
}
