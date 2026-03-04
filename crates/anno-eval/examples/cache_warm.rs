// Warm the local dataset cache by downloading all automatable datasets.
//
// Usage: cargo run -p anno-eval --example cache_warm --features "eval"
//
// Env:
//   ANNO_WARM_PER_TASK=N     -- limit to N datasets per task (default: all)
//   ANNO_WARM_TASKS=coref    -- comma-separated task filter (default: all)

use anno_eval::eval::loader::{DatasetLoader, LoadableDatasetId};
use std::time::Instant;

fn main() {
    let per_task: Option<usize> = std::env::var("ANNO_WARM_PER_TASK")
        .ok()
        .and_then(|v| v.parse().ok());
    let task_filter: Option<Vec<String>> = std::env::var("ANNO_WARM_TASKS")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_lowercase()).collect());

    let loader = match DatasetLoader::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to create DatasetLoader: {}", e);
            std::process::exit(1);
        }
    };

    // Collect automatable datasets, optionally filtered by task
    let all_loadable = LoadableDatasetId::all();
    let mut targets: Vec<LoadableDatasetId> = Vec::new();

    for lid in &all_loadable {
        let id = lid.into_inner();
        if !id.is_automatable() {
            continue;
        }

        // Task filter
        if let Some(ref filter) = task_filter {
            let tasks = id.tasks();
            let matches = tasks.iter().any(|t| filter.contains(&t.to_lowercase()));
            if !matches {
                continue;
            }
        }

        targets.push(*lid);
    }

    // Apply per-task limit
    if let Some(limit) = per_task {
        targets.truncate(limit);
    }

    let total = targets.len();
    println!("Warming cache for {} datasets...\n", total);

    let mut ok_count = 0usize;
    let mut cached_count = 0usize;
    let mut fail_count = 0usize;
    let overall_start = Instant::now();

    for (i, lid) in targets.iter().enumerate() {
        let id = lid.into_inner();
        let name = id.name();
        let start = Instant::now();

        // Check if it's a coref dataset -- use specialized loader
        if id.is_coreference() {
            match loader.load_or_download_coref(id) {
                Ok(docs) => {
                    let elapsed = start.elapsed();
                    let doc_count = docs.len();
                    let chain_count: usize = docs.iter().map(|d| d.chain_count()).sum();
                    println!(
                        "[{:>3}/{}] {} ... OK ({} docs, {} chains, {:.1}s)",
                        i + 1,
                        total,
                        name,
                        doc_count,
                        chain_count,
                        elapsed.as_secs_f64()
                    );
                    ok_count += 1;
                }
                Err(e) => {
                    let msg = format!("{}", e);
                    if msg.contains("not cached") || msg.contains("cached") {
                        cached_count += 1;
                        println!("[{:>3}/{}] {} ... CACHED", i + 1, total, name);
                    } else {
                        fail_count += 1;
                        eprintln!("[{:>3}/{}] {} ... FAIL: {}", i + 1, total, name, e);
                    }
                }
            }
            continue;
        }

        // NER / other datasets
        match loader.load_or_download(*lid) {
            Ok(dataset) => {
                let elapsed = start.elapsed();
                let sents = dataset.len();
                let ents = dataset.entity_count();
                println!(
                    "[{:>3}/{}] {} ... OK ({} sents, {} ents, {:.1}s)",
                    i + 1,
                    total,
                    name,
                    sents,
                    ents,
                    elapsed.as_secs_f64()
                );
                ok_count += 1;
            }
            Err(e) => {
                fail_count += 1;
                eprintln!("[{:>3}/{}] {} ... FAIL: {}", i + 1, total, name, e);
            }
        }
    }

    let elapsed = overall_start.elapsed();
    println!("\n--- Summary ---");
    println!(
        "Downloaded: {}  Cached: {}  Failed: {}  Total: {}",
        ok_count, cached_count, fail_count, total
    );
    println!("Time: {:.1}s", elapsed.as_secs_f64());
}
