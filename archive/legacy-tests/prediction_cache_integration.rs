//! Integration tests for PredictionCache.
//!
//! Run with: cargo test --test prediction_cache_integration --features eval

#![cfg(feature = "eval")]

use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

use anno::eval::prediction_cache::PredictionCache;
use anno::eval::task_evaluator::TaskEvaluator;
use anno::{Entity, EntityType};

/// Helper to create test entities.
fn make_entity(text: &str, entity_type: &str, start: usize, end: usize) -> Entity {
    Entity::new(
        text.to_string(),
        EntityType::from_label(entity_type),
        start,
        end,
        0.95,
    )
}

// =============================================================================
// Basic Operations
// =============================================================================

#[test]
fn test_store_and_lookup() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "John works at Google in California.";
    let backend = "test-backend";
    let version = "1.0";
    let labels = vec!["PER", "ORG", "LOC"];

    let entities = vec![
        make_entity("John", "PER", 0, 4),
        make_entity("Google", "ORG", 14, 20),
        make_entity("California", "LOC", 24, 34),
    ];

    // Store predictions
    let key = cache
        .store(text, backend, version, &labels, &entities, 100)
        .expect("store failed");

    assert!(!key.is_empty(), "cache key should not be empty");

    // Lookup should return the entities
    let cached = cache
        .lookup(text, backend, version, &labels)
        .expect("lookup should hit");

    assert_eq!(cached.len(), 3);
    assert_eq!(cached[0].text, "John");
    assert_eq!(cached[1].text, "Google");
    assert_eq!(cached[2].text, "California");
}

#[test]
fn test_cache_miss_on_different_text() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let entities = vec![make_entity("John", "PER", 0, 4)];

    cache
        .store("Hello John", "backend", "1.0", &["PER"], &entities, 50)
        .expect("store failed");

    // Different text should miss
    let result = cache.lookup("Goodbye John", "backend", "1.0", &["PER"]);
    assert!(result.is_none(), "different text should cache miss");
}

// =============================================================================
// Cache Key Invalidation
// =============================================================================

#[test]
fn test_invalidation_on_version_change() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "Marie Curie won the Nobel Prize.";
    let entities = vec![make_entity("Marie Curie", "PER", 0, 11)];

    // Store with version 1.0
    cache
        .store(text, "backend", "1.0", &["PER"], &entities, 50)
        .expect("store failed");

    // Lookup with version 1.0 should hit
    assert!(cache.lookup(text, "backend", "1.0", &["PER"]).is_some());

    // Lookup with version 2.0 should miss (invalidated)
    assert!(
        cache.lookup(text, "backend", "2.0", &["PER"]).is_none(),
        "different version should invalidate cache"
    );
}

#[test]
fn test_invalidation_on_label_change() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "Apple is headquartered in Cupertino.";
    let entities = vec![
        make_entity("Apple", "ORG", 0, 5),
        make_entity("Cupertino", "LOC", 26, 35),
    ];

    // Store with ORG, LOC labels
    cache
        .store(text, "backend", "1.0", &["ORG", "LOC"], &entities, 50)
        .expect("store failed");

    // Same labels (different order) should hit
    assert!(cache
        .lookup(text, "backend", "1.0", &["LOC", "ORG"])
        .is_some());

    // Different labels should miss
    assert!(
        cache
            .lookup(text, "backend", "1.0", &["PER", "ORG", "LOC"])
            .is_none(),
        "different labels should invalidate cache"
    );
}

#[test]
fn test_backend_name_case_insensitive() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "Test text";
    let entities = vec![make_entity("Test", "MISC", 0, 4)];

    // Store with lowercase backend name
    cache
        .store(text, "gliner", "1.0", &["MISC"], &entities, 50)
        .expect("store failed");

    // Lookup with uppercase should hit (case-insensitive)
    assert!(
        cache.lookup(text, "GLINER", "1.0", &["MISC"]).is_some(),
        "backend name lookup should be case-insensitive"
    );
}

// =============================================================================
// Persistence
// =============================================================================

#[test]
fn test_persistence_across_instances() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache_path = temp.path().to_path_buf();

    let text = "Barack Obama was the 44th President.";
    let entities = vec![make_entity("Barack Obama", "PER", 0, 12)];

    // First instance: store predictions
    {
        let cache = PredictionCache::new(&cache_path).expect("failed to create cache");
        cache
            .store(text, "backend", "1.0", &["PER"], &entities, 100)
            .expect("store failed");
        assert_eq!(cache.len(), 1);
    }

    // Second instance: should load from disk
    {
        let cache = PredictionCache::new(&cache_path).expect("failed to create cache");
        assert_eq!(cache.len(), 1, "cache should persist across instances");

        let cached = cache
            .lookup(text, "backend", "1.0", &["PER"])
            .expect("lookup should hit persisted data");
        assert_eq!(cached[0].text, "Barack Obama");
    }
}

#[test]
fn test_clear_cache() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let entities = vec![make_entity("Test", "MISC", 0, 4)];
    cache
        .store("Test text", "backend", "1.0", &["MISC"], &entities, 50)
        .expect("store failed");

    assert_eq!(cache.len(), 1);

    cache.clear().expect("clear failed");

    assert_eq!(cache.len(), 0, "cache should be empty after clear");
    assert!(cache.is_empty());
}

// =============================================================================
// Thread Safety
// =============================================================================

#[test]
fn test_concurrent_writes() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = Arc::new(PredictionCache::new(temp.path()).expect("failed to create cache"));

    let num_threads = 8;
    let entries_per_thread = 50;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..entries_per_thread {
                    let text = format!("Thread {} entry {}", thread_id, i);
                    let entities = vec![make_entity(&format!("Entity{}", i), "MISC", 0, 6)];
                    cache
                        .store(&text, "backend", "1.0", &["MISC"], &entities, 10)
                        .expect("concurrent store failed");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    let expected = num_threads * entries_per_thread;
    assert_eq!(
        cache.len(),
        expected,
        "all concurrent writes should succeed"
    );
}

#[test]
fn test_concurrent_reads_and_writes() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = Arc::new(PredictionCache::new(temp.path()).expect("failed to create cache"));

    // Pre-populate some entries
    for i in 0..20 {
        let text = format!("Prepopulated entry {}", i);
        let entities = vec![make_entity(&format!("Entity{}", i), "MISC", 0, 6)];
        cache
            .store(&text, "backend", "1.0", &["MISC"], &entities, 10)
            .expect("prepopulate failed");
    }

    let num_readers = 4;
    let num_writers = 4;

    let mut handles = vec![];

    // Spawn readers
    for _ in 0..num_readers {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let text = format!("Prepopulated entry {}", i % 20);
                let _ = cache.lookup(&text, "backend", "1.0", &["MISC"]);
            }
        }));
    }

    // Spawn writers
    for thread_id in 0..num_writers {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                let text = format!("New entry from writer {} item {}", thread_id, i);
                let entities = vec![make_entity(&format!("New{}", i), "MISC", 0, 4)];
                cache
                    .store(&text, "backend", "1.0", &["MISC"], &entities, 10)
                    .expect("concurrent write failed");
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    // Should have 20 prepopulated + 4*25 new = 120 entries
    assert_eq!(cache.len(), 120);
}

// =============================================================================
// Statistics
// =============================================================================

#[test]
fn test_cache_stats() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    // Store entries from multiple backends
    for i in 0..5 {
        let text = format!("NuNER text {}", i);
        let entities = vec![make_entity(&format!("E{}", i), "PER", 0, 2)];
        cache
            .store(&text, "nuner", "1.0", &["PER"], &entities, 50)
            .expect("store failed");
    }

    for i in 0..3 {
        let text = format!("GLiNER text {}", i);
        let entities = vec![make_entity(&format!("E{}", i), "ORG", 0, 2)];
        cache
            .store(&text, "gliner", "2.0", &["ORG"], &entities, 100)
            .expect("store failed");
    }

    let stats = cache.stats();

    assert_eq!(stats.total_entries, 8);
    assert_eq!(stats.by_backend.get("nuner"), Some(&5));
    assert_eq!(stats.by_backend.get("gliner"), Some(&3));
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_empty_entities() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "No entities here.";
    let entities: Vec<Entity> = vec![];

    cache
        .store(text, "backend", "1.0", &["PER"], &entities, 10)
        .expect("store empty entities failed");

    let cached = cache
        .lookup(text, "backend", "1.0", &["PER"])
        .expect("lookup should hit even for empty entities");

    assert!(cached.is_empty(), "cached entities should be empty");
}

#[test]
fn test_unicode_text() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "習近平在北京會見了普京。マリー・キュリーはパリでラジウムを発見した。";
    let entities = vec![
        make_entity("習近平", "PER", 0, 3),
        make_entity("北京", "LOC", 4, 6),
        make_entity("普京", "PER", 10, 12),
    ];

    cache
        .store(text, "backend", "1.0", &["PER", "LOC"], &entities, 100)
        .expect("store unicode failed");

    let cached = cache
        .lookup(text, "backend", "1.0", &["PER", "LOC"])
        .expect("lookup unicode should hit");

    assert_eq!(cached.len(), 3);
    assert_eq!(cached[0].text, "習近平");
}

#[test]
fn test_special_characters_in_text() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = r#"He said: "Hello, World!" and left. Path: C:\Users\test"#;
    let entities = vec![make_entity("World", "MISC", 17, 22)];

    cache
        .store(text, "backend", "1.0", &["MISC"], &entities, 50)
        .expect("store special chars failed");

    let cached = cache
        .lookup(text, "backend", "1.0", &["MISC"])
        .expect("lookup special chars should hit");

    assert_eq!(cached[0].text, "World");
}

#[test]
fn test_load_or_create_nonexistent_path() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let nonexistent = temp.path().join("does_not_exist");

    // Should create directory and return empty cache
    let cache = PredictionCache::load_or_create(&nonexistent);
    assert!(cache.is_empty());
    assert!(nonexistent.exists(), "should create the directory");
}

// =============================================================================
// TaskEvaluator Integration
// =============================================================================

#[test]
fn test_task_evaluator_with_custom_cache() {
    let temp = TempDir::new().expect("failed to create temp dir");

    // Create evaluator with custom cache dir
    let evaluator = TaskEvaluator::with_cache_dir(temp.path()).expect("failed to create evaluator");

    // Cache should start empty
    assert_eq!(evaluator.cache_len(), 0);

    // Stats should reflect empty state
    let stats = evaluator.cache_stats();
    assert_eq!(stats.total_entries, 0);
    assert!(stats.by_backend.is_empty());
}

#[test]
fn test_task_evaluator_cache_clear() {
    let temp = TempDir::new().expect("failed to create temp dir");

    // Pre-populate cache
    {
        let cache = PredictionCache::new(temp.path()).expect("failed to create cache");
        let entities = vec![make_entity("Test", "PER", 0, 4)];
        cache
            .store("Test text", "backend", "1.0", &["PER"], &entities, 50)
            .expect("store failed");
        assert_eq!(cache.len(), 1);
    }

    // Create evaluator with the same cache dir
    let evaluator = TaskEvaluator::with_cache_dir(temp.path()).expect("failed to create evaluator");

    // Should have loaded the cached entry
    assert_eq!(evaluator.cache_len(), 1);

    // Clear the cache
    evaluator.clear_cache().expect("clear failed");

    // Cache should be empty
    assert_eq!(evaluator.cache_len(), 0);
}

#[test]
fn test_task_evaluator_cache_stats_by_backend() {
    let temp = TempDir::new().expect("failed to create temp dir");

    // Pre-populate cache with entries from multiple backends
    {
        let cache = PredictionCache::new(temp.path()).expect("failed to create cache");
        for i in 0..3 {
            let text = format!("NuNER text {}", i);
            let entities = vec![make_entity(&format!("E{}", i), "PER", 0, 2)];
            cache
                .store(&text, "nuner", "1.0", &["PER"], &entities, 50)
                .expect("store failed");
        }
        for i in 0..2 {
            let text = format!("GLiNER text {}", i);
            let entities = vec![make_entity(&format!("E{}", i), "ORG", 0, 2)];
            cache
                .store(&text, "gliner", "2.0", &["ORG"], &entities, 100)
                .expect("store failed");
        }
    }

    // Create evaluator with the same cache dir
    let evaluator = TaskEvaluator::with_cache_dir(temp.path()).expect("failed to create evaluator");

    let stats = evaluator.cache_stats();
    assert_eq!(stats.total_entries, 5);
    assert_eq!(stats.by_backend.get("nuner"), Some(&3));
    assert_eq!(stats.by_backend.get("gliner"), Some(&2));
}

// =============================================================================
// Error Handling & Robustness
// =============================================================================

#[test]
fn test_corrupted_cache_file_handling() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache_file = temp.path().join("predictions.jsonl");

    // Create a cache file with some valid and invalid entries
    use std::fs::File;
    use std::io::Write;
    let mut file = File::create(&cache_file).expect("failed to create cache file");

    // Valid entry
    writeln!(
        file,
        r#"{{"text_hash":"abc123","backend":"test","version":"1.0","labels":["PER"],"predictions":[],"timestamp":"2024-01-01T00:00:00Z","inference_ms":50}}"#
    )
    .expect("write failed");

    // Invalid JSON (malformed)
    writeln!(file, r#"{{"invalid": json}}"#).expect("write failed");

    // Valid entry
    writeln!(
        file,
        r#"{{"text_hash":"def456","backend":"test","version":"1.0","labels":["ORG"],"predictions":[],"timestamp":"2024-01-01T00:00:01Z","inference_ms":60}}"#
    )
    .expect("write failed");

    // Empty line (should be skipped)
    writeln!(file).expect("write failed");

    // Invalid entry (missing required fields)
    writeln!(file, r#"{{"text_hash":"ghi789"}}"#).expect("write failed");

    drop(file);

    // Load cache - should skip invalid entries but load valid ones
    let cache = PredictionCache::new(temp.path()).expect("failed to load cache");
    // Should have loaded 2 valid entries
    assert_eq!(
        cache.len(),
        2,
        "should load valid entries and skip invalid ones"
    );
}

#[test]
fn test_cache_handles_missing_directory_gracefully() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let nonexistent = temp.path().join("nonexistent").join("subdir");

    // Should create directory structure and return empty cache
    let cache = PredictionCache::load_or_create(&nonexistent);
    assert!(cache.is_empty());
    assert!(
        nonexistent.exists(),
        "should create the directory structure"
    );
}

#[test]
fn test_cache_key_consistency_across_instances() {
    let temp = TempDir::new().expect("failed to create temp dir");

    let text = "Consistent key test";
    let backend = "test-backend";
    let version = "1.0";
    let labels = &["PER", "ORG"];

    // Generate key in first instance
    let key1 = PredictionCache::cache_key(text, backend, version, labels);

    // Generate key in second instance (should be identical)
    let key2 = PredictionCache::cache_key(text, backend, version, labels);

    assert_eq!(key1, key2, "cache keys should be deterministic");

    // Test that label order doesn't matter
    let key3 = PredictionCache::cache_key(text, backend, version, &["ORG", "PER"]);
    assert_eq!(
        key1, key3,
        "cache keys should be order-independent for labels"
    );
}

#[test]
fn test_cache_key_case_insensitive_backend() {
    let text = "Test text";
    let labels = &["PER"];

    let key1 = PredictionCache::cache_key(text, "GLiNER", "1.0", labels);
    let key2 = PredictionCache::cache_key(text, "gliner", "1.0", labels);
    let key3 = PredictionCache::cache_key(text, "Gliner", "1.0", labels);

    assert_eq!(key1, key2, "backend names should be case-insensitive");
    assert_eq!(key2, key3, "backend names should be case-insensitive");
}

#[test]
fn test_cache_store_idempotent() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    let text = "Idempotent test";
    let entities = vec![make_entity("Test", "PER", 0, 4)];

    // Store same prediction multiple times
    let key1 = cache
        .store(text, "backend", "1.0", &["PER"], &entities, 50)
        .expect("store failed");
    let key2 = cache
        .store(text, "backend", "1.0", &["PER"], &entities, 50)
        .expect("store failed");
    let key3 = cache
        .store(text, "backend", "1.0", &["PER"], &entities, 50)
        .expect("store failed");

    // Keys should be identical
    assert_eq!(key1, key2);
    assert_eq!(key2, key3);

    // Cache should only have one entry
    assert_eq!(
        cache.len(),
        1,
        "duplicate stores should not create duplicate entries"
    );
}

#[test]
fn test_cache_clear_preserves_directory() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let cache = PredictionCache::new(temp.path()).expect("failed to create cache");

    // Store some entries
    for i in 0..5 {
        let text = format!("Text {}", i);
        let entities = vec![make_entity(&format!("E{}", i), "PER", 0, 1)];
        cache
            .store(&text, "backend", "1.0", &["PER"], &entities, 50)
            .expect("store failed");
    }

    assert_eq!(cache.len(), 5);
    assert!(temp.path().join("predictions.jsonl").exists());

    // Clear cache
    cache.clear().expect("clear failed");

    // Directory should still exist, but file should be gone
    assert!(temp.path().exists(), "cache directory should be preserved");
    assert!(
        !temp.path().join("predictions.jsonl").exists(),
        "cache file should be removed"
    );
    assert_eq!(cache.len(), 0, "cache should be empty after clear");
}
