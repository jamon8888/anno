//! Integration tests for dataset loading functionality.
//!
//! Tests the DatasetLoader for various dataset types and caching behavior.

#![cfg(feature = "eval-advanced")]

use anno::eval::loader::DatasetId;
use anno::eval::{DatasetLoader, LoadableDatasetId};

/// Test that DatasetLoader can be created
#[test]
fn test_dataset_loader_new() {
    let loader = DatasetLoader::new();
    assert!(loader.is_ok(), "DatasetLoader::new() should succeed");
}

/// Test that cached datasets can be queried
#[test]
fn test_list_cached_datasets() {
    let loader = DatasetLoader::new().expect("Create loader");
    let status = loader.status();

    // Should return a list of (DatasetId, bool) tuples
    assert!(!status.is_empty(), "Status should list datasets");

    // Count cached datasets
    let cached_count = status.iter().filter(|(_, cached)| *cached).count();
    eprintln!("Cached datasets: {} of {}", cached_count, status.len());
}

/// Test loading a cached dataset (if available)
#[test]
fn test_load_cached_dataset() {
    let loader = DatasetLoader::new().expect("Create loader");

    // Find first cached dataset
    let status = loader.status();
    let first_cached = status.iter().find(|(_, cached)| *cached);

    if let Some((dataset_id, _)) = first_cached {
        let loadable = LoadableDatasetId::try_from(*dataset_id)
            .expect("status() only lists loadable datasets");
        let result = loader.load(loadable);

        match result {
            Ok(data) => {
                assert!(!data.sentences.is_empty(), "Should have sentences");
                eprintln!(
                    "Loaded {} sentences from {:?}",
                    data.sentences.len(),
                    dataset_id
                );
            }
            Err(e) => {
                panic!("Failed to load cached dataset {:?}: {}", dataset_id, e);
            }
        }
    } else {
        eprintln!("No cached datasets available - skipping load test");
    }
}

/// Test checking if specific datasets are cached
#[test]
fn test_is_cached() {
    let loader = DatasetLoader::new().expect("Create loader");

    // Test is_cached method on various datasets
    let datasets = [
        DatasetId::WikiGold,
        DatasetId::MitMovie,
        DatasetId::MitRestaurant,
    ];

    for dataset_id in datasets {
        match LoadableDatasetId::try_from(dataset_id) {
            Ok(loadable) => {
                let is_cached = loader.is_cached(loadable);
                eprintln!("{:?}: cached={}", dataset_id, is_cached);
            }
            Err(e) => {
                eprintln!("{:?}: not loadable ({})", dataset_id, e);
            }
        }
    }
}

/// Test that DatasetId variants can be iterated (via debug)
#[test]
fn test_dataset_id_debug() {
    // Just ensure the enum has sensible Debug implementation
    let id = DatasetId::WikiGold;
    let debug_str = format!("{:?}", id);
    assert!(!debug_str.is_empty());
    assert!(debug_str.contains("WikiGold") || debug_str.contains("wikigold"));
}

/// Test cache_path method
#[test]
fn test_cache_path() {
    let loader = DatasetLoader::new().expect("Create loader");

    let loadable = LoadableDatasetId::try_from(DatasetId::WikiGold).expect("WikiGold loadable");
    let path = loader.cache_path(loadable);

    // Should return a valid path
    assert!(!path.as_os_str().is_empty());

    // Should contain wikigold somewhere in the path
    let path_str = path.to_string_lossy().to_lowercase();
    assert!(
        path_str.contains("wikigold") || path_str.contains("wiki"),
        "Cache path should contain dataset name: {:?}",
        path
    );
}

/// Test that env module is called for HF_TOKEN
#[test]
fn test_loader_uses_env() {
    use anno::env;

    // Load .env first
    env::load_dotenv();

    // Create loader - should pick up HF_TOKEN from env
    let loader = DatasetLoader::new();
    assert!(loader.is_ok(), "Loader should be created");

    // Report status
    if env::has_hf_token() {
        eprintln!("HF_TOKEN is set - gated datasets should be accessible");
    } else {
        eprintln!("HF_TOKEN not set - gated datasets will fail");
    }
}

/// Test loading MIT datasets (if cached)
#[test]
fn test_load_mit_if_cached() {
    let loader = DatasetLoader::new().expect("Create loader");

    for dataset_id in [DatasetId::MitMovie, DatasetId::MitRestaurant] {
        let loadable = LoadableDatasetId::try_from(dataset_id).expect("MIT datasets are loadable");

        if loader.is_cached(loadable) {
            let result = loader.load(loadable);

            match result {
                Ok(data) => {
                    assert!(!data.sentences.is_empty());
                    eprintln!(
                        "Loaded {} sentences from {:?}",
                        data.sentences.len(),
                        dataset_id
                    );

                    // Check that sentences have text
                    for sentence in &data.sentences {
                        assert!(!sentence.text().is_empty(), "Sentence should have text");
                    }
                }
                Err(e) => {
                    panic!("Failed to load cached {:?}: {}", dataset_id, e);
                }
            }
        } else {
            eprintln!("{:?} not cached - skipping", dataset_id);
        }
    }
}

/// Test cache directory getter
#[test]
fn test_cache_dir() {
    let loader = DatasetLoader::new().expect("Create loader");
    let cache_dir = loader.cache_dir();

    // Should be a valid path
    assert!(!cache_dir.as_os_str().is_empty());

    // Cache dir should exist (created by DatasetLoader::new)
    assert!(
        cache_dir.exists(),
        "Cache directory should exist: {:?}",
        cache_dir
    );
}

/// Test S3 status methods
#[test]
fn test_s3_methods() {
    let loader = DatasetLoader::new().expect("Create loader");

    // S3 should only be enabled if ANNO_S3_CACHE=1
    let s3_enabled = loader.s3_enabled();
    let s3_bucket = loader.s3_bucket();

    // These should be consistent
    if s3_enabled {
        assert!(s3_bucket.is_some(), "S3 bucket should be set if enabled");
        eprintln!("S3 caching enabled with bucket: {:?}", s3_bucket);
    } else {
        eprintln!("S3 caching not enabled (set ANNO_S3_CACHE=1 to enable)");
    }
}
