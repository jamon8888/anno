//! Prediction caching for incremental evaluation.
//!
//! Caches NER predictions keyed by (text_hash, backend_version, labels_hash).
//! This allows re-running evaluations against new gold data without re-inference.
//!
//! # Use Cases
//!
//! 1. **Same backend, different datasets**: Predictions cached per-text, reused
//! 2. **Same dataset, different backends**: Gold parsed once, predictions per-backend
//! 3. **Re-scoring with updated metrics**: No re-inference needed
//! 4. **CI regression detection**: Compare against cached baseline predictions
//!
//! # Cache Key Design
//!
//! ```text
//! key = sha256(text || backend_name || backend_version || sorted(labels))
//! ```
//!
//! # Storage Format
//!
//! JSONL for append-only, git-friendly storage:
//! ```json
//! {"key":"abc123","text_hash":"...","backend":"nuner","version":"1.0","predictions":[...]}
//! ```

use crate::Entity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// A cached prediction entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPrediction {
    /// Hash of the input text.
    pub text_hash: String,
    /// Backend name (e.g., "nuner", "gliner2").
    pub backend: String,
    /// Backend version or model hash for invalidation.
    pub version: String,
    /// Sorted label set used for extraction.
    pub labels: Vec<String>,
    /// Predicted entities.
    pub predictions: Vec<CachedEntity>,
    /// Timestamp of prediction.
    pub timestamp: String,
    /// Inference time in milliseconds.
    pub inference_ms: u64,
}

/// Minimal entity representation for caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntity {
    pub text: String,
    pub entity_type: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f32,
}

impl From<&Entity> for CachedEntity {
    fn from(e: &Entity) -> Self {
        Self {
            text: e.text.clone(),
            entity_type: e.entity_type.clone(),
            start: e.start,
            end: e.end,
            confidence: e.confidence,
        }
    }
}

impl From<CachedEntity> for Entity {
    fn from(c: CachedEntity) -> Self {
        Entity::new(c.text, c.entity_type, c.start, c.end, c.confidence)
    }
}

/// Prediction cache for incremental evaluation.
pub struct PredictionCache {
    cache_dir: PathBuf,
    /// In-memory index: key -> file offset (for fast lookup).
    index: HashMap<String, CachedPrediction>,
}

impl PredictionCache {
    /// Create a new prediction cache in the given directory.
    pub fn new(cache_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;

        let mut cache = Self {
            cache_dir,
            index: HashMap::new(),
        };
        cache.load_index()?;
        Ok(cache)
    }

    /// Generate cache key from components.
    pub fn cache_key(text: &str, backend: &str, version: &str, labels: &[&str]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        backend.to_lowercase().hash(&mut hasher);
        version.hash(&mut hasher);

        // Sort labels for consistent hashing
        let mut sorted_labels: Vec<&str> = labels.to_vec();
        sorted_labels.sort();
        sorted_labels.hash(&mut hasher);

        format!("{:016x}", hasher.finish())
    }

    /// Look up cached prediction.
    pub fn get(&self, key: &str) -> Option<&CachedPrediction> {
        self.index.get(key)
    }

    /// Store prediction in cache.
    pub fn put(&mut self, key: String, prediction: CachedPrediction) -> std::io::Result<()> {
        // Append to JSONL file
        let cache_file = self.cache_dir.join("predictions.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cache_file)?;

        let line = serde_json::to_string(&prediction)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{}", line)?;

        // Update in-memory index
        self.index.insert(key, prediction);
        Ok(())
    }

    /// Load index from cache file.
    fn load_index(&mut self) -> std::io::Result<()> {
        let cache_file = self.cache_dir.join("predictions.jsonl");
        if !cache_file.exists() {
            return Ok(());
        }

        let file = File::open(&cache_file)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(pred) = serde_json::from_str::<CachedPrediction>(&line) {
                let key = Self::cache_key(
                    "", // We don't store raw text, use hash
                    &pred.backend,
                    &pred.version,
                    &pred.labels.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );
                // Actually use text_hash as the lookup key
                self.index.insert(pred.text_hash.clone(), pred);
            }
        }

        Ok(())
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let by_backend: HashMap<String, usize> =
            self.index.values().fold(HashMap::new(), |mut acc, p| {
                *acc.entry(p.backend.clone()).or_insert(0) += 1;
                acc
            });

        CacheStats {
            total_entries: self.index.len(),
            by_backend,
        }
    }

    /// Clear all cached predictions.
    pub fn clear(&mut self) -> std::io::Result<()> {
        let cache_file = self.cache_dir.join("predictions.jsonl");
        if cache_file.exists() {
            fs::remove_file(&cache_file)?;
        }
        self.index.clear();
        Ok(())
    }
}

/// Cache statistics.
#[derive(Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub by_backend: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_consistency() {
        let key1 = PredictionCache::cache_key("hello world", "nuner", "1.0", &["PER", "ORG"]);
        let key2 = PredictionCache::cache_key("hello world", "nuner", "1.0", &["ORG", "PER"]);
        // Labels are sorted, so order shouldn't matter
        assert_eq!(key1, key2);

        // Different text should produce different key
        let key3 = PredictionCache::cache_key("goodbye world", "nuner", "1.0", &["PER", "ORG"]);
        assert_ne!(key1, key3);
    }
}
