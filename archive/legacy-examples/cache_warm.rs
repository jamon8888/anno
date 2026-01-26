//! Cache warming for evaluation datasets.
//!
//! Goal: make the randomized matrix sampler "get better over time" by intentionally
//! populating the local cache (and optionally S3 mirror via `ANNO_S3_CACHE=1`).
//!
//! This is intentionally small + bounded:
//! - choose up to N datasets per task (seeded deterministic selection)
//! - only consider datasets that are loadable and appear downloadable
//! - optionally cap bytes via `ANNO_MAX_DOWNLOAD_BYTES`
//!
//! Environment variables:
//! - `ANNO_WARM_SEED` (u64): seed for deterministic selection (default: 42)
//! - `ANNO_WARM_PER_TASK` (usize): datasets per task (default: 2)
//! - `ANNO_MAX_DOWNLOAD_BYTES` (u64): hard cap on downloaded payload size (default: 50MiB; set 0 to disable)
//! - `ANNO_S3_CACHE=1` + `ANNO_S3_BUCKET`: optionally mirror snapshots to S3

#[cfg(feature = "eval-advanced")]
use anno::eval::dataset_registry::DatasetAccessibility;
#[cfg(feature = "eval-advanced")]
use anno::eval::loader::{DatasetId, DatasetLoader, LoadableDatasetId};
#[cfg(feature = "eval-advanced")]
use anno::eval::task_mapping::Task;
#[cfg(feature = "eval-advanced")]
use xxhash_rust::xxh3::xxh3_64;

#[cfg(feature = "eval-advanced")]
fn allow_manual_datasets() -> bool {
    matches!(
        std::env::var("ANNO_DATASET_ALLOW_MANUAL").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

#[cfg(feature = "eval-advanced")]
fn env_seed() -> u64 {
    std::env::var("ANNO_WARM_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42)
}

#[cfg(feature = "eval-advanced")]
fn env_per_task() -> usize {
    std::env::var("ANNO_WARM_PER_TASK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2)
        .max(1)
}

#[cfg(feature = "eval-advanced")]
fn select_random<T: Clone>(items: &[T], count: usize, seed: u64) -> Vec<T> {
    if items.len() <= count {
        return items.to_vec();
    }

    let mut indexed: Vec<(usize, u64)> = items
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let mut data = [0u8; 16];
            data[..8].copy_from_slice(&seed.to_le_bytes());
            data[8..].copy_from_slice(&(i as u64).to_le_bytes());
            (i, xxh3_64(&data))
        })
        .collect();

    indexed.sort_by_key(|(_, h)| *h);
    indexed.truncate(count);
    indexed.iter().map(|(i, _)| items[*i].clone()).collect()
}

#[cfg(feature = "eval-advanced")]
fn is_downloadable(ds: DatasetId) -> bool {
    if !allow_manual_datasets() {
        // Skip gated HF datasets unless `HF_TOKEN` is available.
        if ds.requires_hf_token() && !anno::env::has_hf_token() {
            return false;
        }

        // Skip datasets we know we can’t automate yet (keeps default runs useful).
        if !ds.is_automatable_download() {
            return false;
        }
    }

    let access = ds.access_status();
    if !matches!(
        access,
        DatasetAccessibility::Public
            | DatasetAccessibility::HuggingFace
            | DatasetAccessibility::Local
    ) {
        return false;
    }

    let url = ds.download_url();
    if url.is_empty() {
        return false;
    }

    // Prefer things that are likely to be downloadable without manual intervention.
    // HuggingFace dataset pages are OK: the loader will rewrite them to datasets-server.
    url.contains("huggingface.co/datasets/")
        || url.contains("datasets-server.huggingface.co/")
        || url.contains("raw.githubusercontent.com")
        || url.ends_with(".jsonl")
        || url.ends_with(".json")
        || url.ends_with(".tsv")
        || url.ends_with(".csv")
        || url.ends_with(".conll")
        || url.ends_with(".txt")
}

#[cfg(feature = "eval-advanced")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    anno::env::load_dotenv();

    let seed = env_seed();
    let per_task = env_per_task();

    eprintln!("=== Cache warm ===");
    eprintln!("Seed: {}", seed);
    eprintln!("Datasets per task: {}", per_task);
    eprintln!(
        "Manual datasets: {} (set ANNO_DATASET_ALLOW_MANUAL=1 to include gated/large/unstable sources)",
        allow_manual_datasets()
    );
    eprintln!(
        "S3 enabled: {}",
        std::env::var("ANNO_S3_CACHE").unwrap_or_default() == "1"
    );
    if let Ok(limit) = std::env::var("ANNO_MAX_DOWNLOAD_BYTES") {
        eprintln!("ANNO_MAX_DOWNLOAD_BYTES={}", limit);
    }

    let loader = DatasetLoader::new()?;

    // Only warm tasks we can plausibly evaluate and that have datasets.
    let tasks = [
        Task::NER,
        Task::DiscontinuousNER,
        Task::IntraDocCoref,
        Task::InterDocCoref,
        Task::AbstractAnaphora,
        Task::RelationExtraction,
    ];

    for (ti, task) in tasks.iter().enumerate() {
        // Use registry tasks as the single source of truth (avoid the hand-maintained
        // `task_datasets()` list drifting out of sync).
        let candidates: Vec<DatasetId> = DatasetId::all()
            .iter()
            .copied()
            .filter(|ds| is_downloadable(*ds))
            .filter(|ds| ds.tasks_typed().contains(task))
            .filter(|ds| LoadableDatasetId::try_from(*ds).is_ok())
            .collect();

        if candidates.is_empty() {
            eprintln!("\nTask {:?}: no downloadable candidates", task);
            continue;
        }

        let chosen = select_random(
            &candidates,
            per_task.min(candidates.len()),
            seed ^ (ti as u64),
        );
        eprintln!("\nTask {:?}: warming {:?}", task, chosen);

        for ds in chosen {
            let Ok(loadable) = LoadableDatasetId::try_from(ds) else {
                continue;
            };
            if loader.is_cached(loadable) {
                eprintln!("  - {:?}: already cached", ds);
                continue;
            }
            match loader.load_or_download(loadable) {
                Ok(loaded) => {
                    if loaded.sentences.is_empty() {
                        eprintln!("  - {:?}: failed: parsed 0 sentences", ds);
                        continue;
                    }
                    eprintln!(
                        "  - {:?}: ok ({} sentences, source={:?})",
                        ds,
                        loaded.sentences.len(),
                        loaded.data_source
                    );
                }
                Err(e) => {
                    eprintln!("  - {:?}: failed: {}", ds, e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "eval-advanced"))]
fn main() {
    eprintln!("This example requires the `eval-advanced` feature.");
    eprintln!("Try: `cargo run -p anno --example cache_warm --features eval-advanced`");
}
