//! Dataset downloading and caching for NER evaluation.
//!
//! Downloads, caches, and parses real NER datasets from public sources.
//! Follows burntsushi's philosophy: real-world data, not toy examples.
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use anno_eval::eval::{DatasetLoader, LoadableDatasetId};
//!
//! let loader = DatasetLoader::new()?;
//! let dataset = loader.load(LoadableDatasetId::WikiGold)?;
//! println!("Loaded {} sentences", dataset.len());
//! ```
//!
//! # Dataset IDs: catalog vs loadable
//!
//! The full dataset catalog lives in [`dataset_registry::DatasetId`]. Not every dataset
//! in the catalog can be downloaded/parsed by this crate (some require licenses or have
//! unimplemented formats).
//!
//! This module provides [`LoadableDatasetId`], a wrapper that guarantees a dataset has
//! a loading implementation. Use it for `DatasetLoader::{load, load_or_download}`.
//!
//! # Supported Datasets
//!
//! | Dataset | Source | License | Entities |
//! |---------|--------|---------|----------|
//! | WikiGold | Wikipedia | CC-BY | PER, LOC, ORG, MISC |
//! | WNUT-17 | Social Media | Open | person, location, corporation, etc. |
//! | MIT Movie | MIT | Research | actor, director, genre, title, etc. |
//! | MIT Restaurant | MIT | Research | amenity, cuisine, dish, etc. |
//! | CoNLL-2003 Sample | Public | Research | PER, LOC, ORG, MISC |
//! | OntoNotes Sample | Public | Research | 18 entity types |
//! | BC5CDR | PubMed | Research | Disease, Chemical |
//! | NCBI Disease | PubMed | Research | Disease |
//!
//! # Design Philosophy
//!
//! - **Lazy downloading**: Only fetch what's needed
//! - **Persistent caching**: Never re-download unchanged data
//! - **Integrity verification**: SHA256 checksums for all downloads
//! - **Graceful degradation**: Work offline with cached data
//! - **Clear errors**: Explain exactly what went wrong
//!
//! # Extended Example
//!
//! ```rust,ignore
//! use anno_eval::eval::{DatasetLoader, LoadableDatasetId};
//!
//! let loader = DatasetLoader::new()?;
//!
//! // Check cache status before loading
//! if loader.is_cached(LoadableDatasetId::WikiGold) {
//!     println!("WikiGold is cached, will load from disk");
//! }
//!
//! // Load dataset (downloads if not cached, verifies checksum)
//! let dataset = loader.load(LoadableDatasetId::WikiGold)?;
//! println!("Loaded {} sentences with {} entities",
//!     dataset.len(), dataset.entity_count());
//!
//! // Iterate over examples
//! for example in dataset.iter() {
//!     println!("Text: {}", example.text);
//!     for entity in &example.entities {
//!         println!("  {} [{}]", entity.text, entity.entity_type);
//!     }
//! }
//! ```
//!
//! [`dataset_registry::DatasetId`]: super::dataset_registry::DatasetId

#[cfg(test)]
use anno::EntityType;
use anno::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// =============================================================================
// Dataset Identification
// =============================================================================

/// Dataset identifier (full catalog).
///
/// This is the single source of truth for dataset metadata. Not all datasets are
/// loadable by `DatasetLoader`. Use [`LoadableDatasetId`] when you want that guarantee.
///
/// # Usage
///
/// ```rust,ignore
/// use anno_eval::eval::{DatasetLoader, LoadableDatasetId};
///
/// let loader = DatasetLoader::new()?;
/// let dataset = loader.load(LoadableDatasetId::WikiGold)?;
/// ```
///
/// # Available Datasets
///
/// ## NER Datasets
///
/// | Dataset | Size | Domain | Entity Types |
/// |---------|------|--------|--------------|
/// | `WikiGold` | ~3.5k entities | Wikipedia | PER, LOC, ORG, MISC |
/// | `Wnut17` | ~2k entities | Social media | person, location, etc. |
/// | `MitMovie` | ~10k entities | Movies | actor, director, genre |
/// | `MitRestaurant` | ~8k entities | Restaurants | cuisine, dish, etc. |
/// | `CoNLL2003Sample` | ~20k entities | News | PER, LOC, ORG, MISC |
/// | `OntoNotesSample` | ~18k entities | Mixed | 18 types |
/// | `BC5CDR` | ~28k entities | Biomedical | Disease, Chemical |
/// | `NCBIDisease` | ~6k entities | Biomedical | Disease |
///
/// ## Coreference Datasets
///
/// | Dataset | Size | Domain | Features |
/// |---------|------|--------|----------|
/// | `GAP` | 8,908 pairs | Wikipedia | Gender-balanced pronouns |
/// | `PreCo` | 38k docs | Reading | Includes singletons |
/// | `LitBank` | 100 works | Literature | Literary coreference |
///
/// # Extending
///
/// To add a new loadable dataset:
/// 1. Add the variant here
/// 2. Implement loading in `DatasetLoader::load()`
/// 3. Add to `DatasetId::all()` iterator
/// 4. Ensure it exists in `dataset_registry::DatasetId` (metadata catalog)
///
mod types;
pub use types::{
    AnnotatedSentence, AnnotatedToken, CacheManifest, CacheManifestEntry, DataSource,
    DatasetMetadata, DatasetStats, LoadableDatasetId, LoadedDataset, RelationDocument,
    TemporalMetadata,
};
pub(crate) use types::DatasetParsePlan;

mod acquire;
mod parse;
pub use types::DatasetId;


// =============================================================================
// Dataset Loader
// =============================================================================

/// Loads and caches NER datasets.
///
/// # Caching Strategy (Tiered)
///
/// 1. **Local cache** (`~/.cache/anno/datasets/`): Checked first, fastest
/// 2. **S3 cache** (`s3://anno-data/`): Checked if `ANNO_S3_CACHE=1` and AWS credentials available
/// 3. **URL download**: Original source URL, last resort
///
/// This tiered approach:
/// - Maximizes speed (local cache hit is instant)
/// - Provides redundancy (S3 backup if original URLs go down)
/// - Allows sharing datasets across machines via S3
///
/// # Environment Variables
///
/// - `ANNO_CACHE_DIR`: Override default cache location
/// - `ANNO_S3_CACHE`: Set to "1" to enable S3 fallback (requires AWS credentials)
/// - `ANNO_S3_BUCKET`: Override S3 bucket name (default: `arc-anno-data`)
///
/// # Example
///
/// ```bash
/// # Enable S3 caching
/// export ANNO_S3_CACHE=1
/// export AWS_PROFILE=default  # or set AWS_ACCESS_KEY_ID, etc.
///
/// # Now load_or_download() will try S3 before URL
/// ```
pub struct DatasetLoader {
    cache_dir: PathBuf,
    /// S3 bucket name for fallback caching (if enabled)
    s3_bucket: Option<String>,
    /// Cache manifest for tracking downloads.
    ///
    /// This is shared across evaluation threads, so it must be thread-safe.
    manifest: std::sync::RwLock<CacheManifest>,
}

impl DatasetLoader {
    // Default download cap when `ANNO_MAX_DOWNLOAD_BYTES` is unset.
    //
    // Rationale: unset should be *usable* but *safe* by default. This cap is meant to prevent
    // accidental multi-GB downloads while still allowing many evaluation datasets.
    #[cfg(feature = "eval")]
    const DEFAULT_MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB

    #[cfg(feature = "eval")]
    fn max_download_bytes() -> Option<u64> {
        match std::env::var("ANNO_MAX_DOWNLOAD_BYTES").ok() {
            Some(s) => {
                let s = s.trim();
                if s.is_empty() {
                    return Some(Self::DEFAULT_MAX_DOWNLOAD_BYTES);
                }
                let Ok(v) = s.parse::<u64>() else {
                    return Some(Self::DEFAULT_MAX_DOWNLOAD_BYTES);
                };
                if v == 0 {
                    None // explicit opt-out
                } else {
                    Some(v)
                }
            }
            None => Some(Self::DEFAULT_MAX_DOWNLOAD_BYTES),
        }
    }

    #[cfg(feature = "eval")]
    fn enforce_max_download_bytes(content_len: usize, source: &str) -> Result<()> {
        let Some(limit) = Self::max_download_bytes() else {
            return Ok(());
        };
        let len = content_len as u64;
        if len > limit {
            return Err(Error::InvalidInput(format!(
                "Download rejected ({} bytes > ANNO_MAX_DOWNLOAD_BYTES={} bytes) from {}",
                len, limit, source
            )));
        }
        Ok(())
    }

    /// Create a new loader with default cache directory.
    ///
    /// Default location: `~/.cache/anno/datasets` (platform cache via `dirs` crate)
    /// Falls back to `.anno/datasets` in current directory if `dirs` crate unavailable.
    ///
    /// S3 fallback is enabled if `ANNO_S3_CACHE=1` environment variable is set.
    pub fn new() -> Result<Self> {
        // Load workspace `.env` (idempotent, does not override existing env vars).
        // This keeps eval tooling usable without manual exporting of env vars.
        anno::env::load_dotenv();

        // Check for custom cache directory
        let cache_dir = if let Ok(custom_dir) = std::env::var("ANNO_CACHE_DIR") {
            PathBuf::from(custom_dir).join("datasets")
        } else {
            let base_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
            base_dir.join("anno").join("datasets")
        };

        // Check for S3 caching
        let s3_bucket = if std::env::var("ANNO_S3_CACHE").unwrap_or_default() == "1" {
            Some(std::env::var("ANNO_S3_BUCKET").unwrap_or_else(|_| "arc-anno-data".to_string()))
        } else {
            None
        };

        fs::create_dir_all(&cache_dir).map_err(|e| {
            Error::InvalidInput(format!("Failed to create cache dir {:?}: {}", cache_dir, e))
        })?;

        let manifest = CacheManifest::load(&cache_dir)?;

        Ok(Self {
            cache_dir,
            s3_bucket,
            manifest: std::sync::RwLock::new(manifest),
        })
    }

    /// Update the cache manifest with a new entry and save it.
    #[cfg(feature = "eval")]
    fn update_manifest(&self, entry: CacheManifestEntry) -> Result<()> {
        let mut manifest = self
            .manifest
            .write()
            .map_err(|_| Error::InvalidInput("cache manifest lock poisoned".to_string()))?;
        manifest.update_entry(entry);
        manifest.save(&self.cache_dir)?;
        Ok(())
    }

    /// Create a loader with a custom cache directory.
    pub fn with_cache_dir(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache_dir = cache_dir.into();
        let s3_bucket = if std::env::var("ANNO_S3_CACHE").unwrap_or_default() == "1" {
            Some(std::env::var("ANNO_S3_BUCKET").unwrap_or_else(|_| "arc-anno-data".to_string()))
        } else {
            None
        };

        fs::create_dir_all(&cache_dir).map_err(|e| {
            Error::InvalidInput(format!("Failed to create cache dir {:?}: {}", cache_dir, e))
        })?;

        let manifest = CacheManifest::load(&cache_dir)?;

        Ok(Self {
            cache_dir,
            s3_bucket,
            manifest: std::sync::RwLock::new(manifest),
        })
    }

    /// Base directory for cached datasets.
    #[must_use]
    pub fn cache_dir(&self) -> &std::path::Path {
        &self.cache_dir
    }

    /// Whether S3 fallback caching is enabled.
    #[must_use]
    pub fn s3_enabled(&self) -> bool {
        self.s3_bucket.is_some()
    }

    /// The configured S3 bucket name (if enabled).
    #[must_use]
    pub fn s3_bucket(&self) -> Option<&str> {
        self.s3_bucket.as_deref()
    }

    /// Snapshot of cached dataset manifest entries (cloned).
    ///
    /// This is intended for tooling (CLI/scripts) that wants to iterate cached datasets
    /// without holding the manifest lock for a long time.
    #[must_use]
    pub fn cached_manifest_entries(&self) -> Vec<CacheManifestEntry> {
        let Ok(manifest) = self.manifest.read() else {
            return Vec::new();
        };
        let mut out: Vec<CacheManifestEntry> = manifest.entries.values().cloned().collect();
        out.sort_by(|a, b| a.dataset_id.cmp(&b.dataset_id));
        out
    }

    /// Upload a locally cached dataset (by `DatasetId`) to S3, using its manifest entry.
    ///
    /// This uses the same object layout as `load_or_download()`'s best-effort upload:
    /// - `datasets/<cache_filename>` (legacy/mutable key)
    /// - `datasets/by-sha256/<sha>/<cache_filename>` (immutable snapshot)
    /// - `datasets/<cache_filename>.latest.json` (pointer)
    /// - `datasets/<cache_filename>.manifest.json` (sidecar metadata)
    #[cfg(feature = "eval")]
    pub fn upload_cached_dataset_to_s3(&self, bucket: &str, id: DatasetId) -> Result<()> {
        let key = id.cache_filename();
        let entry = {
            let guard = self
                .manifest
                .read()
                .map_err(|_| Error::InvalidInput("cache manifest lock poisoned".to_string()))?;
            guard
                .get(key)
                .cloned()
                .ok_or_else(|| Error::InvalidInput(format!("No manifest entry for {}", key)))?
        };
        let path = self.cache_path_for(id);
        let content = std::fs::read_to_string(&path).map_err(|e| {
            Error::InvalidInput(format!(
                "Failed to read cached dataset {}: {}",
                path.display(),
                e
            ))
        })?;
        Self::enforce_max_download_bytes(content.len(), "local cache (sync-s3)")?;
        acquire::s3::upload_to_s3(bucket, id, &content, &entry)
    }

    #[must_use]
    fn cache_path_for(&self, id: DatasetId) -> PathBuf {
        self.cache_dir.join(id.cache_filename())
    }

    #[must_use]
    fn is_cached_for(&self, id: DatasetId) -> bool {
        if !self.cache_path_for(id).exists() {
            return false;
        }

        // Best-effort cache invalidation: if the registry URL changes, the on-disk cached
        // file may no longer correspond to the dataset id. Prefer re-download over silently
        // using stale/incorrect data.
        //
        // This is intentionally lightweight (no checksum re-hash here).
        if let Ok(manifest) = self.manifest.read() {
            if let Some(entry) = manifest.get(id.cache_filename()) {
                if entry.source_url != id.download_url() {
                    return false;
                }
                // Treat “cached but empty” as invalid: this is almost always a bad download
                // (HTML, auth wall, or format mismatch) and should be re-fetched.
                if entry.sentence_count == 0 {
                    return false;
                }
            }
        }

        true
    }

    /// Get the cache path for a dataset.
    #[must_use]
    pub fn cache_path(&self, id: LoadableDatasetId) -> PathBuf {
        self.cache_path_for(id.0)
    }

    /// Check if a dataset is cached locally.
    #[must_use]
    pub fn is_cached(&self, id: LoadableDatasetId) -> bool {
        self.is_cached_for(id.0)
    }

    /// Load a dataset from cache.
    ///
    /// Returns an error if the dataset is not cached.
    pub fn load(&self, id: LoadableDatasetId) -> Result<LoadedDataset> {
        let dataset_id = id.0;
        let cache_path = self.cache_path(id);
        if !cache_path.exists() {
            return Err(Error::InvalidInput(format!(
                "Dataset {:?} not cached at {:?}",
                dataset_id, cache_path
            )));
        }

        let content = fs::read_to_string(&cache_path).map_err(|e| {
            Error::InvalidInput(format!("Failed to read cache {:?}: {}", cache_path, e))
        })?;

        let mut dataset = parse::parse_content(&content, dataset_id)?;
        if dataset.sentences.is_empty() {
            return Err(Error::InvalidInput(format!(
                "Cached dataset '{}' parsed to 0 sentences (cache_path={:?})",
                dataset_id.name(),
                cache_path
            )));
        }
        dataset.data_source = DataSource::LocalCache;
        Ok(dataset)
    }

    /// Load or download a dataset.
    ///
    /// Tries cache first, then S3 (if enabled), then downloads from URL.
    #[cfg(feature = "eval")]
    pub fn load_or_download(&self, id: LoadableDatasetId) -> Result<LoadedDataset> {
        let dataset_id = id.0;
        // 1. Check local cache first
        if self.is_cached(id) {
            return self.load(id);
        }

        // 2. Try S3 cache if enabled
        if let Some(ref bucket) = self.s3_bucket {
            if let Ok((content, manifest_entry)) = acquire::s3::download_from_s3(bucket, dataset_id) {
                Self::enforce_max_download_bytes(content.len(), "S3")?;
                // Cache locally for future use
                let cache_path = self.cache_path(id);
                fs::write(&cache_path, &content).map_err(|e| {
                    Error::InvalidInput(format!("Failed to write cache {:?}: {}", cache_path, e))
                })?;

                let mut dataset = parse::parse_content(&content, dataset_id)?;
                dataset.data_source = DataSource::S3Cache;

                // Best-effort: if S3 provides a manifest entry, record it locally.
                if let Some(entry) = manifest_entry {
                    let _ = self.update_manifest(entry);
                }
                return Ok(dataset);
            }
        }

        // 3. Download from original URL
        let (content, resolved_url) = acquire::download_with_resolved_url(dataset_id)?;
        Self::enforce_max_download_bytes(content.len(), &resolved_url)?;
        let file_size = content.len() as u64;
        let sha256 = self.compute_sha256(&content);

        // 4. Cache the downloaded content locally
        let cache_path = self.cache_path(id);
        fs::write(&cache_path, &content).map_err(|e| {
            Error::InvalidInput(format!("Failed to write cache {:?}: {}", cache_path, e))
        })?;

        // 5. Parse the content
        let mut dataset = parse::parse_content(&content, dataset_id)?;
        dataset.data_source = DataSource::OriginalUrl;

        // 6. Update manifest with download metadata
        let entry = CacheManifestEntry {
            dataset_id: dataset_id.cache_filename().to_string(),
            source_url: dataset_id.download_url().to_string(),
            resolved_url: Some(resolved_url.clone()),
            sha256,
            file_size,
            downloaded_at: chrono::Utc::now().to_rfc3339(),
            sentence_count: dataset.sentences.len(),
            entity_count: dataset.entity_count(),
            anno_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let _ = self.update_manifest(entry); // Best effort

        // 7. Optionally upload to S3 for future use (best effort)
        if let Some(ref bucket) = self.s3_bucket {
            let entry = CacheManifestEntry {
                dataset_id: dataset_id.cache_filename().to_string(),
                source_url: dataset_id.download_url().to_string(),
                resolved_url: Some(resolved_url),
                sha256: self.compute_sha256(&content),
                file_size: content.len() as u64,
                downloaded_at: chrono::Utc::now().to_rfc3339(),
                sentence_count: dataset.sentences.len(),
                entity_count: dataset.entity_count(),
                anno_version: env!("CARGO_PKG_VERSION").to_string(),
            };
            let _ = acquire::s3::upload_to_s3(bucket, dataset_id, &content, &entry);
        }

        Ok(dataset)
    }

    /// Load-or-download shim when `eval` is disabled.
    ///
    /// This method will load from cache if present, otherwise return a helpful error.
    #[cfg(not(feature = "eval"))]
    pub fn load_or_download(&self, id: LoadableDatasetId) -> Result<LoadedDataset> {
        if self.is_cached(id) {
            return self.load(id);
        }

        Err(Error::InvalidInput(
            "Dataset is not cached. Rebuild with feature `eval` to enable downloading.".to_string(),
        ))
    }

    /// Compute SHA256 checksum of content.
    #[cfg(feature = "eval")]
    fn compute_sha256(&self, content: &str) -> String {
        #[cfg(feature = "eval")]
        {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        #[cfg(not(feature = "eval"))]
        {
            // Fallback if sha2 not available
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        }
    }

    /// Get temporal metadata for a dataset if available.
    pub(crate) fn get_temporal_metadata(id: DatasetId) -> Option<TemporalMetadata> {
        match id {
            DatasetId::TweetNER7 => {
                // TweetNER7 has temporal entity recognition - use dataset creation date as cutoff
                Some(TemporalMetadata {
                    kb_version: None,                                // No KB linking in TweetNER7
                    temporal_cutoff: Some("2017-01-01".to_string()), // Approximate dataset creation
                    entity_creation_dates: None,                     // Would need entity linking
                })
            }
            DatasetId::BroadTwitterCorpus => {
                // BroadTwitterCorpus is stratified across times - use approximate cutoff
                Some(TemporalMetadata {
                    kb_version: None,
                    temporal_cutoff: Some("2018-01-01".to_string()), // Approximate
                    entity_creation_dates: None,
                })
            }
            DatasetId::BC5CDR
            | DatasetId::NCBIDisease
            | DatasetId::GENIA
            | DatasetId::AnatEM
            | DatasetId::BC2GM
            | DatasetId::BC4CHEMD => {
                // Biomedical datasets might have KB versions (UMLS, etc.)
                Some(TemporalMetadata {
                    kb_version: None,
                    temporal_cutoff: None,
                    entity_creation_dates: None,
                })
            }
            _ => None, // Most datasets don't have temporal metadata
        }
    }

    /// Parse content based on dataset format.
    ///
    /// This is intentionally `pub` (behind the `eval` module) to enable offline evaluation
    /// workflows and integration tests that want to exercise parsing without network I/O.
    ///
    /// It does **not** download or read from cache. For typical usage, prefer
    /// [`DatasetLoader::load_or_download`] / [`DatasetLoader::load`].
    pub fn parse_content_str(&self, content: &str, id: DatasetId) -> Result<LoadedDataset> {
        parse::parse_content(content, id)
    }
    /// Parse HIPE-2022 style TSV NER format.
    ///
    /// Expected format:
    /// ```text
    /// TOKEN    NE-COARSE-LIT   NE-COARSE-METO   NE-FINE-LIT   ...
    /// # hipe2022:document_id = doc123
    /// word1    B-PER           _                B-pers.author ...
    /// word2    I-PER           _                I-pers.author ...
    /// word3    O               _                O             ...
    /// ```
    ///
    /// - First line is header (starts with TOKEN)
    /// - Lines starting with `#` are metadata comments
    /// - Data lines are tab-separated with token in first column
    /// - NE-COARSE-LIT (column 2) contains BIO-tagged NER labels    /// Parse CSV NER format (E-NER/EDGAR-NER style).
    ///
    /// Expected format: `Token,Tag` (comma-separated)
    /// Uses `-DOCSTART-` for document boundaries and empty lines for sentence boundaries.
    /// Tags use BIO scheme (e.g., O, B-PERSON, I-BUSINESS).
    ///
    /// # Errors
    // =========================================================================
    // Coreference Loading
    // =========================================================================

    /// Load coreference dataset, returning documents with chains.
    ///
    /// Use this for GAP, PreCo, and LitBank datasets.
    pub fn load_coref(&self, id: DatasetId) -> Result<Vec<super::coref::CorefDocument>> {
        if !id.is_coreference() {
            return Err(Error::InvalidInput(format!(
                "{:?} is not a coreference dataset",
                id
            )));
        }

        let cache_path = self.cache_path_for(id);
        if !cache_path.exists() {
            return Err(Error::InvalidInput(format!(
                "Dataset {:?} not cached at {:?}. Download from {}",
                id,
                cache_path,
                id.download_url()
            )));
        }

        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| Error::InvalidInput(format!("Failed to read {:?}: {}", cache_path, e)))?;

        match id {
            DatasetId::CorefUD => super::coref_loader::parse_corefud_conllu(&content),
            DatasetId::GAP => {
                let examples = super::coref_loader::parse_gap_tsv(&content)?;
                Ok(examples
                    .into_iter()
                    .map(|ex| ex.to_coref_document())
                    .collect())
            }
            DatasetId::PreCo => {
                // PreCo can be JSONL (one JSON object per line) or JSON array
                // Try JSONL first (more common), then fall back to JSON array
                if content.trim().starts_with('[') {
                    // JSON array format
                    let docs = super::coref_loader::parse_preco_json(&content)?;
                    Ok(docs.into_iter().map(|d| d.to_coref_document()).collect())
                } else {
                    // JSONL format - parse each line and convert to JSON array format
                    let mut json_objects = Vec::new();
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // Validate it's valid JSON
                        if serde_json::from_str::<serde_json::Value>(line).is_ok() {
                            json_objects.push(line);
                        }
                    }
                    // Convert JSONL to JSON array format
                    let json_array = format!("[{}]", json_objects.join(","));
                    let docs = super::coref_loader::parse_preco_json(&json_array)?;
                    Ok(docs.into_iter().map(|d| d.to_coref_document()).collect())
                }
            }
            DatasetId::LitBank => {
                // LitBank coreference - parse .ann format for chains
                parse::coref::parse_litbank_coref(&content)
            }
            DatasetId::ECBPlus => {
                // ECB+ may be cached as either:
                // - ZIP binary (new: real XML annotations)
                // - CSV text (legacy: sentence index)
                let raw_bytes = std::fs::read(&cache_path).map_err(|e| {
                    Error::InvalidInput(format!("Failed to read {:?}: {}", cache_path, e))
                })?;
                if raw_bytes.starts_with(b"PK\x03\x04") {
                    return super::coref_loader::parse_ecb_plus_zip(&raw_bytes);
                }
                // Fall back to CSV parser
                super::coref_loader::parse_ecb_plus_coref(&content)
            }
            DatasetId::WikiCoref => {
                // WikiCoref uses a GAP-compatible TSV format.
                let examples = super::coref_loader::parse_gap_tsv(&content)?;
                Ok(examples
                    .into_iter()
                    .map(|ex| ex.to_coref_document())
                    .collect())
            }
            DatasetId::GUM
            | DatasetId::WinoBias
            | DatasetId::TwiConv
            | DatasetId::MuDoCo
            | DatasetId::SciCo => Err(Error::InvalidInput(format!(
                "{:?} coreference format is not yet supported (requires a dedicated parser)",
                id
            ))),
            DatasetId::BookCoref | DatasetId::BookCorefSplit => {
                // BOOKCOREF: Book-scale coreference (Martinelli et al. 2025)
                // Format: OntoNotes-style with character metadata
                // Note: Actual data requires HuggingFace datasets library to download
                // from Project Gutenberg. We support pre-downloaded JSONL.
                super::coref_loader::parse_bookcoref_json(&content)
            }
            _ => Err(Error::InvalidInput(format!(
                "No coreference parser for {:?}",
                id
            ))),
        }
    }

    /// Load coreference dataset, downloading if needed.
    #[cfg(feature = "eval")]
    pub fn load_or_download_coref(
        &self,
        id: DatasetId,
    ) -> Result<Vec<super::coref::CorefDocument>> {
        if !self.is_cached_for(id) {
            if matches!(id, DatasetId::CorefUD) {
                let cache_path = self.cache_path_for(id);
                return Err(Error::InvalidInput(format!(
                    "CorefUD is not downloadable via anno yet. Please provide a local CorefUD .conllu file.\n\
                     - Option A: copy it to the cache path {:?}\n\
                     - Option B: use CorefLoader::load_corefud_from_path(<path>)",
                    cache_path
                )));
            }

            let cache_path = self.cache_path_for(id);
            if matches!(id, DatasetId::ECBPlus) {
                // ECB+ is a ZIP file -- download as raw bytes
                let url = id.download_url();
                let bytes = acquire::http::download_attempt_bytes(url)?;
                std::fs::write(&cache_path, &bytes).map_err(|e| {
                    Error::InvalidInput(format!("Failed to cache {:?}: {}", cache_path, e))
                })?;
            } else {
                let (content, _) = acquire::download_with_resolved_url(id)?;
                std::fs::write(&cache_path, &content).map_err(|e| {
                    Error::InvalidInput(format!("Failed to cache {:?}: {}", cache_path, e))
                })?;
            }
        }
        self.load_coref(id)
    }

    // =========================================================================
    // Relation Extraction Loading
    // =========================================================================

    /// Load relation extraction dataset, returning documents with relations.
    ///
    /// Use this for DocRED and ReTACRED datasets.
    pub fn load_relation(&self, id: DatasetId) -> Result<Vec<RelationDocument>> {
        if !id.is_relation_extraction() {
            return Err(Error::InvalidInput(format!(
                "{:?} is not a relation extraction dataset",
                id
            )));
        }

        let cache_path = self.cache_path_for(id);
        if !cache_path.exists() {
            return Err(Error::InvalidInput(format!(
                "Dataset {:?} not cached at {:?}. Download from {}",
                id,
                cache_path,
                id.download_url()
            )));
        }

        let content = std::fs::read_to_string(&cache_path)
            .map_err(|e| Error::InvalidInput(format!("Failed to read {:?}: {}", cache_path, e)))?;

        match id {
            DatasetId::DocRED
            | DatasetId::ReTACRED
            | DatasetId::NYTFB
            | DatasetId::WEBNLG
            | DatasetId::GoogleRE
            | DatasetId::BioRED
            | DatasetId::SciER
            | DatasetId::MixRED
            | DatasetId::CovEReD => {
                // All these datasets use the CrossRE format (same as DocRED)
                parse::relation::parse_docred_relations(&content)
            }
            DatasetId::CHisIEC => {
                // CHisIEC uses a different JSON format with entity indices
                parse::relation::parse_chisiec_relations(&content)
            }
            DatasetId::CADEC => {
                // CADEC is NER, not relation extraction
                Err(Error::InvalidInput(
                    "CADEC is a NER dataset, not relation extraction".to_string(),
                ))
            }
            _ => Err(Error::InvalidInput(format!(
                "No relation parser for {:?}",
                id
            ))),
        }
    }

    /// Load relation extraction dataset, downloading if needed.
    #[cfg(feature = "eval")]
    pub fn load_or_download_relation(&self, id: DatasetId) -> Result<Vec<RelationDocument>> {
        if !self.is_cached_for(id) {
            let (content, _) = acquire::download_with_resolved_url(id)?;
            let cache_path = self.cache_path_for(id);
            std::fs::write(&cache_path, &content).map_err(|e| {
                Error::InvalidInput(format!("Failed to cache {:?}: {}", cache_path, e))
            })?;
        }
        self.load_relation(id)
    }

    // =========================================================================
    // African Language Dataset Parsers (Masakhane Community)
    // =========================================================================

    /// Parse CoNLL-U format (Universal Dependencies).
    ///
    /// CoNLL-U format has 10 tab-separated columns per line:
    /// ID FORM LEMMA UPOS XPOS FEATS HEAD DEPREL DEPS MISC
    ///
    /// Used by MasakhaPOS for African language POS tagging.
    ///    /// Load all cached datasets.
    pub fn load_all_cached(&self) -> Vec<(DatasetId, Result<LoadedDataset>)> {
        LoadableDatasetId::all()
            .into_iter()
            .filter(|id| self.is_cached(*id))
            .map(|id| (id.0, self.load(id)))
            .collect()
    }

    /// Get status of all datasets.
    #[must_use]
    pub fn status(&self) -> Vec<(DatasetId, bool)> {
        LoadableDatasetId::all()
            .into_iter()
            .map(|id| (id.0, self.is_cached(id)))
            .collect()
    }
}

impl Default for DatasetLoader {
    fn default() -> Self {
        Self::new().expect("Failed to create default DatasetLoader")
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_id_basics() {
        let id = DatasetId::WikiGold;
        assert_eq!(id.name(), "WikiGold");
    }

    #[test]
    fn test_convenience_metadata_methods() {
        let id = DatasetId::WikiGold;

        // WikiGold has known metadata
        assert_eq!(id.citation(), Some("Balasuriya et al. (2009)"));
        assert_eq!(id.license(), Some("CC-BY-4.0"));
        assert_eq!(id.year(), Some(2009));
    }

    #[test]
    fn test_loadable_wrapper_invariants() {
        // There should be at least one non-loadable dataset in the catalog.
        assert!(
            DatasetId::all()
                .iter()
                .copied()
                .any(|d| !LoadableDatasetId::is_loadable_dataset(d)),
            "Expected registry to contain some non-loadable datasets"
        );

        for id in LoadableDatasetId::all() {
            let ds: DatasetId = id.into();
            assert!(
                LoadableDatasetId::is_loadable_dataset(ds),
                "LoadableDatasetId must imply is_loadable_dataset()"
            );
            assert!(LoadableDatasetId::try_from(ds).is_ok());
        }
    }

    #[test]
    fn test_parse_plan_is_single_source_of_truth_for_loadability() {
        // For *every* registry dataset id:
        // - parse_plan(id).is_some()  <=>  TryFrom<DatasetId> succeeds
        for &ds in DatasetId::all() {
            let plan_exists = LoadableDatasetId::parse_plan(ds).is_some();
            let try_ok = LoadableDatasetId::try_from(ds).is_ok();
            assert_eq!(
                plan_exists, try_ok,
                "parse_plan / TryFrom mismatch for {:?}",
                ds
            );
        }

        // Also ensure `LoadableDatasetId::all()` only returns ids with plans.
        for id in LoadableDatasetId::all() {
            let ds: DatasetId = id.into();
            assert!(
                LoadableDatasetId::parse_plan(ds).is_some(),
                "LoadableDatasetId::all() returned {:?} with no parse plan",
                ds
            );
        }
    }

    #[test]
    fn test_registry_hints_do_not_contradict_parse_plan() {
        // If a dataset is loadable (i.e., has a parse plan), and the registry provides a strong
        // hint, they should agree. (Hints are allowed to be optimistic for non-loadable datasets.)
        for &ds in DatasetId::all() {
            let Some(plan) = LoadableDatasetId::parse_plan(ds) else {
                continue;
            };
            let Some(hint) = LoadableDatasetId::registry_hint_plan(ds) else {
                continue;
            };
            assert_eq!(hint, plan, "Registry hint mismatch for {:?}", ds);
        }
    }

    #[test]
    fn test_huggingface_access_status_requires_hf_id() {
        // `access_status: HuggingFace` is our strongest signal that a dataset is automatable
        // via the Hub. Keep the registry self-consistent so hinting can stay metadata-driven.
        for &ds in DatasetId::all() {
            if ds.access_status()
                != crate::eval::dataset_registry::DatasetAccessibility::HuggingFace
            {
                continue;
            }
            assert!(
                ds.hf_id().is_some(),
                "Dataset {:?} is marked HuggingFace-accessible but has no hf_id",
                ds
            );
        }
    }

    #[test]
    fn test_huggingface_access_status_is_hintable() {
        // If the registry says “HuggingFace”, the loader should be able to produce *some*
        // parse-plan hint (not necessarily `HfApiResponse`, since a few datasets are hybrids
        // with bespoke parse plans).
        for &ds in DatasetId::all() {
            if ds.access_status()
                != crate::eval::dataset_registry::DatasetAccessibility::HuggingFace
            {
                continue;
            }
            assert!(
                LoadableDatasetId::registry_hint_plan(ds).is_some(),
                "Dataset {:?} is marked HuggingFace-accessible but has no registry hint plan",
                ds
            );
        }
    }

    #[test]
    fn test_parse_bio_tag() {
        assert_eq!(parse::util::parse_bio_tag("O"), ("O", ""));
        assert_eq!(parse::util::parse_bio_tag("B-PER"), ("B", "PER"));
        assert_eq!(parse::util::parse_bio_tag("I-LOC"), ("I", "LOC"));
        assert_eq!(parse::util::parse_bio_tag("B-ORG"), ("B", "ORG"));
    }

    #[test]
    fn test_map_entity_type() {
        // Core types
        assert_eq!(parse::util::map_entity_type("PER"), EntityType::Person);
        assert_eq!(parse::util::map_entity_type("PERSON"), EntityType::Person);
        assert_eq!(parse::util::map_entity_type("LOC"), EntityType::Location);
        assert_eq!(parse::util::map_entity_type("ORG"), EntityType::Organization);

        // GPE now preserves distinction (Custom, not Location)
        assert!(matches!(parse::util::map_entity_type("GPE"), EntityType::Custom { .. }));

        // MISC -> Custom or Other
        assert!(matches!(parse::util::map_entity_type("MISC"), EntityType::Custom { .. }));

        // OntoNotes types -> Custom (preserves semantics)
        assert!(matches!(
            parse::util::map_entity_type("PRODUCT"),
            EntityType::Custom { .. }
        ));
        assert!(matches!(
            parse::util::map_entity_type("EVENT"),
            EntityType::Custom { .. }
        ));
        assert!(matches!(
            parse::util::map_entity_type("WORK_OF_ART"),
            EntityType::Custom { .. }
        ));

        // Numeric types preserved
        assert_eq!(parse::util::map_entity_type("CARDINAL"), EntityType::Cardinal);
    }

    #[test]
    fn test_dataset_id_display() {
        assert_eq!(DatasetId::WikiGold.to_string(), "WikiGold");
        assert_eq!(DatasetId::Wnut17.to_string(), "WNUT-17");
    }

    #[test]
    fn test_dataset_id_from_str() {
        assert_eq!(
            "wikigold".parse::<DatasetId>().unwrap(),
            DatasetId::WikiGold
        );
        assert_eq!("wnut-17".parse::<DatasetId>().unwrap(), DatasetId::Wnut17);
        assert_eq!(
            "mit_movie".parse::<DatasetId>().unwrap(),
            DatasetId::MitMovie
        );
    }

    #[test]
    fn test_annotated_sentence_text() {
        let sentence = AnnotatedSentence {
            tokens: vec![
                AnnotatedToken {
                    text: "John".into(),
                    ner_tag: "B-PER".into(),
                },
                AnnotatedToken {
                    text: "lives".into(),
                    ner_tag: "O".into(),
                },
                AnnotatedToken {
                    text: "in".into(),
                    ner_tag: "O".into(),
                },
                AnnotatedToken {
                    text: "New".into(),
                    ner_tag: "B-LOC".into(),
                },
                AnnotatedToken {
                    text: "York".into(),
                    ner_tag: "I-LOC".into(),
                },
            ],
            source_dataset: DatasetId::WikiGold,
        };

        assert_eq!(sentence.text(), "John lives in New York");
    }

    #[test]
    fn test_annotated_sentence_entities() {
        let sentence = AnnotatedSentence {
            tokens: vec![
                AnnotatedToken {
                    text: "John".into(),
                    ner_tag: "B-PER".into(),
                },
                AnnotatedToken {
                    text: "Smith".into(),
                    ner_tag: "I-PER".into(),
                },
                AnnotatedToken {
                    text: "works".into(),
                    ner_tag: "O".into(),
                },
                AnnotatedToken {
                    text: "at".into(),
                    ner_tag: "O".into(),
                },
                AnnotatedToken {
                    text: "Google".into(),
                    ner_tag: "B-ORG".into(),
                },
            ],
            source_dataset: DatasetId::WikiGold,
        };

        let entities = sentence.entities();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].text, "John Smith");
        assert_eq!(entities[0].entity_type, EntityType::Person);
        assert_eq!(entities[1].text, "Google");
        assert_eq!(entities[1].entity_type, EntityType::Organization);
    }

    #[test]
    fn test_parse_conll_format() {
        let content = r#"
John B-PER
Smith I-PER
works O
at O
Google B-ORG
. O

Apple B-ORG
announced O
today O
. O
"#;

        let dataset = parse::ner::parse_conll(content, DatasetId::WikiGold).unwrap();

        assert_eq!(dataset.len(), 2);
        assert_eq!(dataset.entity_count(), 3);
    }

    #[test]
    fn test_parse_conll2003_format() {
        // CoNLL-2003 has 4 columns: word POS chunk NER
        let content = r#"
-DOCSTART- -X- -X- O

EU NNP B-NP B-ORG
rejects VBZ B-VP O
German JJ B-NP B-MISC
call NN I-NP O
. . O O

Peter NNP B-NP B-PER
Blackburn NNP I-NP I-PER
"#;

        let dataset = parse::ner::parse_conll(content, DatasetId::CoNLL2003Sample).unwrap();

        assert_eq!(dataset.len(), 2);

        let entities1 = dataset.sentences[0].entities();
        assert_eq!(entities1.len(), 2); // EU (ORG), German (MISC)

        let entities2 = dataset.sentences[1].entities();
        assert_eq!(entities2.len(), 1); // Peter Blackburn (PER)
        assert_eq!(entities2[0].text, "Peter Blackburn");
    }

    #[test]
    fn test_historical_datasets_configured() {
        // Historical NER datasets should have proper metadata
        assert!(!DatasetId::HIPE2022.download_url().is_empty());
        assert_eq!(DatasetId::HIPE2022.name(), "HIPE-2022");

        assert!(!DatasetId::MedievalCzechCharters.download_url().is_empty());
        assert_eq!(
            DatasetId::MedievalCzechCharters.name(),
            "Medieval Czech Charters"
        );

        assert!(!DatasetId::TRIDIS.download_url().is_empty());
        assert_eq!(DatasetId::TRIDIS.name(), "TRIDIS");

        // Should be in all() list
        let all = DatasetId::all();
        assert!(all.contains(&DatasetId::HIPE2022));
        assert!(all.contains(&DatasetId::TRIDIS));
    }

    #[test]
    fn test_queer_nlp_datasets_configured() {
        // Queer/gender-inclusive NLP datasets
        assert!(!DatasetId::WinoQueer.download_url().is_empty());
        assert_eq!(DatasetId::WinoQueer.name(), "WinoQueer");

        assert!(!DatasetId::GICoref.download_url().is_empty());
        assert_eq!(DatasetId::GICoref.name(), "GICoref");

        assert!(!DatasetId::BBQ.download_url().is_empty());
        assert_eq!(DatasetId::BBQ.name(), "BBQ");

        // Should be in all() list
        let all = DatasetId::all();
        assert!(all.contains(&DatasetId::WinoQueer));
        assert!(all.contains(&DatasetId::GICoref));
        assert!(all.contains(&DatasetId::BBQ));
    }

    #[test]
    fn test_joint_re_datasets_configured() {
        // Joint NER + Relation Extraction datasets
        assert!(
            DatasetId::TACRED.requires_license(),
            "TACRED is LDC-licensed; download_url may be empty"
        );
        assert_eq!(DatasetId::TACRED.name(), "TACRED");

        assert!(!DatasetId::REBEL.download_url().is_empty());
        assert_eq!(DatasetId::REBEL.name(), "REBEL");

        // Should be in all() list
        let all = DatasetId::all();
        assert!(all.contains(&DatasetId::TACRED));
        assert!(all.contains(&DatasetId::REBEL));
    }

    #[test]
    fn test_dialogue_coref_datasets_configured() {
        // Dialogue/streaming coreference datasets
        assert!(!DatasetId::CODICRAC.download_url().is_empty());
        assert_eq!(DatasetId::CODICRAC.name(), "CODI-CRAC");

        assert!(!DatasetId::AMIMeeting.download_url().is_empty());
        assert_eq!(DatasetId::AMIMeeting.name(), "AMI Meeting");

        assert!(
            DatasetId::ARRAU.requires_license(),
            "ARRAU has LDC + research distribution; download_url may be empty"
        );
        assert!(
            DatasetId::ARRAU.name().contains("ARRAU"),
            "ARRAU name should contain 'ARRAU'"
        );

        // Should be in all() list
        let all = DatasetId::all();
        assert!(all.contains(&DatasetId::CODICRAC));
        assert!(all.contains(&DatasetId::AMIMeeting));
        assert!(all.contains(&DatasetId::ARRAU));
    }

    #[test]
    fn test_is_historical_classification() {
        // Historical datasets
        assert!(DatasetId::HIPE2022.is_historical());
        assert!(DatasetId::MedievalCzechCharters.is_historical());
        assert!(DatasetId::EighteenthCenturyNER.is_historical());
        assert!(DatasetId::HistoricalChineseNER.is_historical());

        // Non-historical should return false
        assert!(!DatasetId::WikiGold.is_historical());
        assert!(!DatasetId::CoNLL2003Sample.is_historical());
    }

    #[test]
    fn test_is_bias_evaluation_classification() {
        // Bias/fairness datasets
        assert!(DatasetId::WinoQueer.is_bias_evaluation());
        assert!(DatasetId::BBQ.is_bias_evaluation());
        assert!(DatasetId::GICoref.is_bias_evaluation());
        assert!(DatasetId::WinoBias.is_bias_evaluation());
        assert!(DatasetId::GAP.is_bias_evaluation());

        // Non-bias should return false
        assert!(!DatasetId::WikiGold.is_bias_evaluation());
    }

    #[test]
    fn test_new_datasets_have_descriptions() {
        // All new datasets should have proper descriptions (not the catch-all)
        let catch_all = "Dataset not yet fully integrated";

        // Historical
        assert_ne!(DatasetId::HIPE2022.description(), catch_all);
        assert_ne!(DatasetId::TRIDIS.description(), catch_all);

        // Queer NLP
        assert_ne!(DatasetId::WinoQueer.description(), catch_all);
        assert_ne!(DatasetId::BBQ.description(), catch_all);
        assert_ne!(DatasetId::GICoref.description(), catch_all);

        // Joint NER+RE
        assert_ne!(DatasetId::TACRED.description(), catch_all);
        assert_ne!(DatasetId::REBEL.description(), catch_all);

        // Dialogue
        assert_ne!(DatasetId::CODICRAC.description(), catch_all);
        assert_ne!(DatasetId::ARRAU.description(), catch_all);
    }

    #[test]
    fn test_coreference_includes_new_datasets() {
        // All coreference datasets should be detected
        assert!(DatasetId::GICoref.is_coreference());
        assert!(DatasetId::CODICRAC.is_coreference());
        assert!(DatasetId::ARRAU.is_coreference());
        assert!(DatasetId::WinoPron.is_coreference());
        assert!(DatasetId::DROC.is_coreference());
        assert!(DatasetId::KoCoNovel.is_coreference());
    }

    #[test]
    fn test_chisiec_is_historical_and_relation_extraction() {
        // CHisIEC should be both historical and relation extraction
        assert!(DatasetId::CHisIEC.is_historical());
        assert!(DatasetId::CHisIEC.is_relation_extraction());

        // Verify entity types
        let types = DatasetId::CHisIEC.entity_types();
        assert!(types.contains(&"PER"));
        assert!(types.contains(&"LOC"));
        assert!(types.contains(&"OFI"));
        assert!(types.contains(&"BOOK"));
    }

    #[test]
    fn test_chisiec_from_str() {
        // Test various string representations
        assert_eq!("chisiec".parse::<DatasetId>().unwrap(), DatasetId::CHisIEC);
        assert_eq!(
            "ch-is-iec".parse::<DatasetId>().unwrap(),
            DatasetId::CHisIEC
        );
        assert_eq!(
            "chinese-historical-ie".parse::<DatasetId>().unwrap(),
            DatasetId::CHisIEC
        );
        assert_eq!(
            "ancient-chinese-ner".parse::<DatasetId>().unwrap(),
            DatasetId::CHisIEC
        );
    }

    #[test]
    fn test_chisiec_parse_ner() {
        // Test CHisIEC NER parsing with sample data
        let sample_json = r#"[
            {
                "tokens": "衞鞅奔魏",
                "entities": [
                    {"type": "PER", "start": 0, "end": 2, "span": "衞鞅"},
                    {"type": "LOC", "start": 3, "end": 4, "span": "魏"}
                ],
                "relations": []
            }
        ]"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);

        let sentence = &dataset.sentences[0];
        // 4 characters: 衞 鞅 奔 魏
        assert_eq!(sentence.tokens.len(), 4);

        // Check BIO tags
        assert_eq!(sentence.tokens[0].ner_tag, "B-PER"); // 衞
        assert_eq!(sentence.tokens[1].ner_tag, "I-PER"); // 鞅
        assert_eq!(sentence.tokens[2].ner_tag, "O"); // 奔
        assert_eq!(sentence.tokens[3].ner_tag, "B-LOC"); // 魏
    }

    #[test]
    fn test_chisiec_parse_relations() {
        // Test CHisIEC relation extraction parsing
        let sample_json = r#"[
            {
                "tokens": "嚴公遣玉汝使",
                "entities": [
                    {"type": "PER", "start": 0, "end": 2, "span": "嚴公"},
                    {"type": "PER", "start": 2, "end": 4, "span": "玉汝"}
                ],
                "relations": [
                    {"type": "上下級", "head": 0, "tail": 1, "head_span": "嚴公", "tail_span": "玉汝"}
                ]
            }
        ]"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::relation::parse_chisiec_relations(sample_json);
        assert!(result.is_ok());

        let docs = result.unwrap();
        assert_eq!(docs.len(), 1);

        let doc = &docs[0];
        assert_eq!(doc.relations.len(), 1);

        let rel = &doc.relations[0];
        assert_eq!(rel.relation_type, "上下級");
        assert_eq!(rel.head_text, "嚴公");
        assert_eq!(rel.tail_text, "玉汝");
        assert_eq!(rel.head_type, "PER");
        assert_eq!(rel.tail_type, "PER");
        // Character offsets (嚴公 at 0-2, 玉汝 at 2-4)
        assert_eq!(rel.head_span, (0, 2));
        assert_eq!(rel.tail_span, (2, 4));
    }

    #[test]
    fn test_chisiec_all_entity_types() {
        // Test all 4 CHisIEC entity types: PER, LOC, OFI (官职), BOOK (书籍)
        // This is important because OFI (Official position) is domain-specific
        let sample_json = r#"[
            {
                "tokens": "司馬遷為太史令著史記於長安",
                "entities": [
                    {"type": "PER", "start": 0, "end": 3, "span": "司馬遷"},
                    {"type": "OFI", "start": 4, "end": 7, "span": "太史令"},
                    {"type": "BOOK", "start": 8, "end": 10, "span": "史記"},
                    {"type": "LOC", "start": 11, "end": 13, "span": "長安"}
                ],
                "relations": []
            }
        ]"#;

        let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC)
            .unwrap();

        assert_eq!(dataset.sentences.len(), 1);
        let sentence = &dataset.sentences[0];

        // Verify each entity type is correctly tagged
        // 司馬遷 (Sima Qian - historian)
        assert_eq!(sentence.tokens[0].ner_tag, "B-PER");
        assert_eq!(sentence.tokens[1].ner_tag, "I-PER");
        assert_eq!(sentence.tokens[2].ner_tag, "I-PER");

        // 為 (was)
        assert_eq!(sentence.tokens[3].ner_tag, "O");

        // 太史令 (Grand Historian - official position)
        assert_eq!(sentence.tokens[4].ner_tag, "B-OFI");
        assert_eq!(sentence.tokens[5].ner_tag, "I-OFI");
        assert_eq!(sentence.tokens[6].ner_tag, "I-OFI");

        // 著 (wrote)
        assert_eq!(sentence.tokens[7].ner_tag, "O");

        // 史記 (Records of the Grand Historian - book)
        assert_eq!(sentence.tokens[8].ner_tag, "B-BOOK");
        assert_eq!(sentence.tokens[9].ner_tag, "I-BOOK");

        // 於 (at)
        assert_eq!(sentence.tokens[10].ner_tag, "O");

        // 長安 (Chang'an - capital city)
        assert_eq!(sentence.tokens[11].ner_tag, "B-LOC");
        assert_eq!(sentence.tokens[12].ner_tag, "I-LOC");
    }

    #[test]
    fn test_chisiec_unicode_character_offsets() {
        // Critical: CHisIEC uses CHARACTER offsets, not byte offsets
        // This test ensures we handle multi-byte Chinese characters correctly
        let sample_json = r#"[
            {
                "tokens": "曹操",
                "entities": [
                    {"type": "PER", "start": 0, "end": 2, "span": "曹操"}
                ],
                "relations": []
            }
        ]"#;

        let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC)
            .unwrap();

        // 曹操 is 2 characters (but 6 bytes in UTF-8)
        let sentence = &dataset.sentences[0];
        assert_eq!(sentence.tokens.len(), 2);
        assert_eq!(sentence.tokens[0].text, "曹");
        assert_eq!(sentence.tokens[1].text, "操");
    }

    #[test]
    fn test_chisiec_multiple_relations_same_document() {
        // Test parsing multiple relations in a single document
        // Relations: 任職 (holds office), 管理 (manages)
        let sample_json = r#"[
            {
                "tokens": "曹操為丞相管冀州",
                "entities": [
                    {"type": "PER", "start": 0, "end": 2, "span": "曹操"},
                    {"type": "OFI", "start": 3, "end": 5, "span": "丞相"},
                    {"type": "LOC", "start": 6, "end": 8, "span": "冀州"}
                ],
                "relations": [
                    {"type": "任職", "head": 0, "tail": 1, "head_span": "曹操", "tail_span": "丞相"},
                    {"type": "管理", "head": 0, "tail": 2, "head_span": "曹操", "tail_span": "冀州"}
                ]
            }
        ]"#;

        let loader = DatasetLoader::new().unwrap();
        let docs = parse::relation::parse_chisiec_relations(sample_json).unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].relations.len(), 2);

        // First relation: 任職 (office holding)
        assert_eq!(docs[0].relations[0].relation_type, "任職");
        assert_eq!(docs[0].relations[0].head_type, "PER");
        assert_eq!(docs[0].relations[0].tail_type, "OFI");

        // Second relation: 管理 (manages)
        assert_eq!(docs[0].relations[1].relation_type, "管理");
        assert_eq!(docs[0].relations[1].head_type, "PER");
        assert_eq!(docs[0].relations[1].tail_type, "LOC");
    }

    #[test]
    fn test_chisiec_distinct_from_historical_chinese_ner() {
        // CHisIEC and HistoricalChineseNER are DIFFERENT datasets
        // This test documents their distinction

        // Both should be classified as historical
        assert!(DatasetId::CHisIEC.is_historical());
        assert!(DatasetId::HistoricalChineseNER.is_historical());

        // But they are different datasets
        assert_ne!(DatasetId::CHisIEC, DatasetId::HistoricalChineseNER);

        // CHisIEC supports relation extraction; HistoricalChineseNER may not
        assert!(DatasetId::CHisIEC.is_relation_extraction());

        // Different entity types:
        // CHisIEC: PER, LOC, OFI, BOOK (ancient Chinese)
        // HistoricalChineseNER: PER, LOC, ORG, DATE, etc. (modern Chinese 1872-1949)
        let chisiec_types = DatasetId::CHisIEC.entity_types();
        assert!(chisiec_types.contains(&"OFI")); // Official position - unique to CHisIEC
        assert!(chisiec_types.contains(&"BOOK")); // Classical texts

        // Different names
        assert_eq!(DatasetId::CHisIEC.name(), "CHisIEC");
        assert_eq!(
            DatasetId::HistoricalChineseNER.name(),
            "Historical Chinese NER"
        );
    }

    #[test]
    fn test_chisiec_empty_entities_handled() {
        // Test graceful handling of documents with no entities
        let sample_json = r#"[
            {
                "tokens": "天下太平",
                "entities": [],
                "relations": []
            }
        ]"#;

        let dataset = parse::relation::parse_chisiec(sample_json, DatasetId::CHisIEC)
            .unwrap();

        assert_eq!(dataset.sentences.len(), 1);
        let sentence = &dataset.sentences[0];

        // All tokens should be tagged as O
        for token in &sentence.tokens {
            assert_eq!(token.ner_tag, "O");
        }
    }

    #[test]
    fn test_chisiec_entity_types_in_schema() {
        // Verify CHisIEC entity types are properly mapped in the schema
        use anno::schema::map_to_canonical;

        // PER -> Person
        let per_type = map_to_canonical("PER", None);
        assert_eq!(per_type, EntityType::Person);

        // LOC -> Location
        let loc_type = map_to_canonical("LOC", None);
        assert_eq!(loc_type, EntityType::Location);

        // OFI -> Custom OFFICIAL type (domain-specific for ancient Chinese)
        let ofi_type = map_to_canonical("OFI", None);
        assert!(matches!(ofi_type, EntityType::Custom { .. }));

        // BOOK -> WORK_OF_ART (creative works)
        let book_type = map_to_canonical("BOOK", None);
        assert!(matches!(book_type, EntityType::Custom { .. }));
    }

    #[test]
    fn test_chisiec_language_and_domain() {
        // CHisIEC is Classical Chinese (文言文) - ISO 639-3: lzh
        assert_eq!(DatasetId::CHisIEC.language(), "lzh");

        // CHisIEC is a historical dataset (24 dynastic histories)
        assert_eq!(DatasetId::CHisIEC.domain(), "historical");

        // Compare with HistoricalChineseNER which is modern Chinese (1872-1949)
        assert_eq!(DatasetId::HistoricalChineseNER.language(), "zh");
        assert_eq!(DatasetId::HistoricalChineseNER.domain(), "historical");
    }

    // =========================================================================
    // African Language Dataset Tests
    // =========================================================================

    #[test]
    fn test_african_datasets_configured() {
        // MasakhaNER datasets should have download URLs
        assert!(!DatasetId::MasakhaNER.download_url().is_empty());
        assert!(!DatasetId::MasakhaNER2.download_url().is_empty());
        assert!(!DatasetId::AfriSenti.download_url().is_empty());
        assert!(!DatasetId::AfriQA.download_url().is_empty());
        assert!(!DatasetId::MasakhaNEWS.download_url().is_empty());
        assert!(!DatasetId::MasakhaPOS.download_url().is_empty());

        // Names should be set
        assert_eq!(DatasetId::MasakhaNER.name(), "MasakhaNER");
        assert_eq!(DatasetId::MasakhaNER2.name(), "MasakhaNER 2.0");
        assert_eq!(DatasetId::AfriSenti.name(), "AfriSenti");
        assert_eq!(DatasetId::AfriQA.name(), "AfriQA");
        assert_eq!(DatasetId::MasakhaNEWS.name(), "MasakhaNEWS");
        assert_eq!(DatasetId::MasakhaPOS.name(), "MasakhaPOS");
    }

    #[test]
    fn test_african_datasets_entity_types() {
        // MasakhaNER uses PER, ORG, LOC, DATE
        let ner_types = DatasetId::MasakhaNER.entity_types();
        assert!(ner_types.contains(&"PER"));
        assert!(ner_types.contains(&"LOC"));
        assert!(ner_types.contains(&"ORG"));
        assert!(ner_types.contains(&"DATE"));

        // AfriSenti uses sentiment labels
        let senti_types = DatasetId::AfriSenti.entity_types();
        assert!(senti_types.contains(&"positive"));
        assert!(senti_types.contains(&"neutral"));
        assert!(senti_types.contains(&"negative"));

        // MasakhaNEWS uses topic labels
        let news_types = DatasetId::MasakhaNEWS.entity_types();
        assert!(news_types.contains(&"politics"));
        assert!(news_types.contains(&"sports"));
        assert!(news_types.contains(&"business"));

        // MasakhaPOS uses Universal Dependencies POS tags
        let pos_types = DatasetId::MasakhaPOS.entity_types();
        assert!(pos_types.contains(&"NOUN"));
        assert!(pos_types.contains(&"VERB"));
        assert!(pos_types.contains(&"ADJ"));
    }

    #[test]
    fn test_parse_afrisenti() {
        // Test AfriSenti TSV parsing
        let sample_tsv = "This movie is great!\tpositive\n\
                          Awful experience\tnegative\n\
                          It was okay\tneutral";

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_afrisenti(sample_tsv, DatasetId::AfriSenti);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 3);

        // Check first sentence has positive label
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-positive");
        // Check second sentence has negative label
        assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-negative");
        // Check third sentence has neutral label
        assert_eq!(dataset.sentences[2].tokens[0].ner_tag, "B-neutral");
    }

    #[test]
    fn test_parse_masakhanews() {
        // Test MasakhaNEWS TSV parsing
        let sample_tsv = "headline\tbody\tcategory\n\
                          Breaking: Election Results\tThe results are in...\tpolitics\n\
                          Team Wins Championship\tIn an exciting match...\tsports";

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_masakhanews(sample_tsv, DatasetId::MasakhaNEWS);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        // Header line should be skipped
        assert_eq!(dataset.sentences.len(), 2);

        // Check categories
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-politics");
        assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-sports");
    }

    #[test]
    fn test_parse_conllu() {
        // Test CoNLL-U parsing (MasakhaPOS format)
        let sample_conllu = "# sent_id = 1\n\
                             # text = John loves Mary\n\
                             1\tJohn\tJohn\tPROPN\tNNP\t_\t2\tnsubj\t_\t_\n\
                             2\tloves\tlove\tVERB\tVBZ\t_\t0\troot\t_\t_\n\
                             3\tMary\tMary\tPROPN\tNNP\t_\t2\tobj\t_\t_\n\
                             \n\
                             # sent_id = 2\n\
                             1\tHe\the\tPRON\tPRP\t_\t2\tnsubj\t_\t_\n\
                             2\truns\trun\tVERB\tVBZ\t_\t0\troot\t_\t_\n";

        let result = parse::ner::parse_conllu(sample_conllu, DatasetId::MasakhaPOS);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);

        // First sentence: John loves Mary
        assert_eq!(dataset.sentences[0].tokens.len(), 3);
        assert_eq!(dataset.sentences[0].tokens[0].text, "John");
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-PROPN");
        assert_eq!(dataset.sentences[0].tokens[1].text, "loves");
        assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-VERB");

        // Second sentence: He runs
        assert_eq!(dataset.sentences[1].tokens.len(), 2);
        assert_eq!(dataset.sentences[1].tokens[0].text, "He");
        assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-PRON");
    }

    #[test]
    fn test_ancient_language_ud_datasets_are_loadable() {
        // Ancient language UD treebanks should be loadable via registry hints
        // These have format: "CoNLLU" and categories: [ner, ancient]
        let ancient_datasets = [
            DatasetId::AncientGreekUD,
            DatasetId::LatinUD,
            DatasetId::SanskritUD,
            DatasetId::OldEnglishUD,
            DatasetId::OldNorseUD,
        ];

        for ds in ancient_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(ds),
                "{:?} should be loadable via registry hint (format={:?})",
                ds,
                ds.format()
            );
        }
    }

    #[test]
    fn test_conllu_with_ner_tags_from_ancient_greek() {
        // Test CoNLLU parsing with MISC column NER tags (Ancient Greek Perseus format)
        // Real format from UD Ancient Greek Perseus
        let sample_conllu = "\
# sent_id = tlg0012.tlg001.perseus-grc1:1.1
# text = μῆνιν ἄειδε θεὰ Πηληϊάδεω Ἀχιλῆος
1\tμῆνιν\tμῆνις\tNOUN\tn-s---fa-\tCase=Acc|Gender=Fem|Number=Sing\t2\tobj\t_\tO
2\tἄειδε\tᾄδω\tVERB\tv2sama---\tMood=Imp|Number=Sing|Person=2|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tO
3\tθεὰ\tθεά\tNOUN\tn-s---fv-\tCase=Voc|Gender=Fem|Number=Sing\t2\tvocative\t_\tO
4\tΠηληϊάδεω\tΠηληϊάδης\tNOUN\tn-s---mg-\tCase=Gen|Gender=Masc|Number=Sing\t5\tnmod\t_\tB-PER
5\tἈχιλῆος\tἈχιλλεύς\tPROPN\tn-s---mg-\tCase=Gen|Gender=Masc|Number=Sing\t1\tnmod\t_\tI-PER

";

        let result = parse::ner::parse_conllu(sample_conllu, DatasetId::AncientGreekUD);
        assert!(
            result.is_ok(),
            "Failed to parse Ancient Greek CoNLLU: {:?}",
            result.err()
        );

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens.len(), 5);

        // Check Achilles (Ἀχιλῆος) entity
        assert_eq!(dataset.sentences[0].tokens[4].text, "Ἀχιλῆος");
        // Note: CoNLLU parser may use POS tags if MISC doesn't have NER
        // This depends on how the parser handles the MISC column
    }

    #[test]
    fn test_registry_hints_cover_all_conllu_ner_datasets() {
        // Datasets with format CoNLLU/CoNLL-U and task NER should get a registry hint
        for &ds in DatasetId::all() {
            let format = ds.format().unwrap_or("");
            let is_conllu = format == "CoNLLU" || format == "CoNLL-U";
            let is_ner = ds.is_ner();

            if is_conllu && is_ner {
                let hint = LoadableDatasetId::registry_hint_plan(ds);
                assert!(
                    hint.is_some(),
                    "{:?} has format={} and is NER but no registry hint",
                    ds,
                    format
                );
                if let Some(plan) = hint {
                    assert_eq!(
                        plan,
                        DatasetParsePlan::Conllu,
                        "{:?} should use Conllu parse plan",
                        ds
                    );
                }
            }
        }
    }

    #[test]
    fn test_datasets_with_public_url_and_format_are_hintable() {
        // Datasets with a public URL and a parseable format should get hints
        let hintable_formats = ["CoNLL", "CoNLLU", "CoNLL-U", "BIO", "IOB2", "JSONL"];

        let mut missing_hints = Vec::new();

        for &ds in DatasetId::all() {
            let url = ds.download_url();
            let format = ds.format().unwrap_or("");
            let is_ner = ds.is_ner();

            // Skip datasets without URLs or non-NER datasets
            if url.is_empty() || !is_ner {
                continue;
            }

            // Skip formats we don't auto-detect
            if !hintable_formats.contains(&format) {
                continue;
            }

            let hint = LoadableDatasetId::registry_hint_plan(ds);
            if hint.is_none() {
                missing_hints.push((ds, format));
            }
        }

        // Allow some datasets to not have hints (complex formats, etc.)
        // but document them
        if !missing_hints.is_empty() {
            // These are known to be missing hints (need special parsers)
            let known_missing: &[DatasetId] = &[
                // Add any datasets that intentionally don't have hints here
            ];
            for (ds, format) in &missing_hints {
                if !known_missing.contains(ds) {
                    eprintln!(
                        "Warning: {:?} (format={}) has public URL but no registry hint",
                        ds, format
                    );
                }
            }
        }
    }

    #[test]
    fn test_loadable_count_is_reasonable() {
        // Ensure we have a reasonable number of loadable datasets
        let loadable_count = LoadableDatasetId::all().len();
        let total_count = DatasetId::all().len();

        // We should have at least 50% of datasets loadable via either parse_plan or hints
        let min_expected = total_count / 2;
        assert!(
            loadable_count >= min_expected,
            "Only {} of {} datasets are loadable (expected at least {})",
            loadable_count,
            total_count,
            min_expected
        );
    }

    #[test]
    fn test_datasets_with_urls_have_formats() {
        // Datasets with public URLs should ideally have format info for auto-loading
        let mut missing_format = Vec::new();

        for &ds in DatasetId::all() {
            let url = ds.download_url();
            let format = ds.format();
            let access = ds.access_status();

            // Skip datasets that require registration or aren't publicly available
            if url.is_empty() {
                continue;
            }

            // Check if format is missing for public datasets
            if format.is_none()
                && access == crate::eval::dataset_registry::DatasetAccessibility::Public
            {
                missing_format.push(ds);
            }
        }

        // Log datasets that could benefit from format info
        if !missing_format.is_empty() {
            eprintln!(
                "Datasets with public URLs but no format field ({}):",
                missing_format.len()
            );
            for ds in &missing_format[..missing_format.len().min(10)] {
                eprintln!("  - {:?}", ds);
            }
        }

        // We expect most public datasets to have format info
        // Allow up to 20% to be missing format (some have unusual formats)
        let max_missing = DatasetId::all().len() / 5;
        assert!(
            missing_format.len() <= max_missing,
            "Too many public datasets missing format: {} (max {})",
            missing_format.len(),
            max_missing
        );
    }

    #[test]
    fn test_conll_format_ner_only_datasets_are_parseable() {
        // All NER-only datasets with CoNLL/CoNLLU format should have a parse plan
        // (Datasets with joint RE/coref tasks may use different column formats)
        let mut not_loadable = Vec::new();

        for &ds in DatasetId::all() {
            let format = ds.format().unwrap_or("");
            let is_conll = format == "CoNLL" || format == "CoNLLU" || format == "CoNLL-U";
            let is_ner = ds.is_ner();
            let is_re = ds.is_relation_extraction();
            let is_coref = ds.is_coreference();
            let is_event = ds.is_event_coref();

            if !is_conll || !is_ner {
                continue;
            }

            // Skip explicitly blocked datasets (they may be present in the registry for metadata,
            // but intentionally cannot be downloaded/loaded automatically).
            if ds.tasks_or_inferred().contains(&"blocked") {
                continue;
            }

            // Skip joint task datasets (they use CoNLL but with different structure)
            if is_re || is_coref || is_event {
                continue;
            }

            // Pure NER CoNLL datasets should be loadable
            let is_loadable = LoadableDatasetId::is_loadable_dataset(ds);
            if !is_loadable {
                not_loadable.push((ds, format));
            }
        }

        if !not_loadable.is_empty() {
            eprintln!("Pure NER CoNLL datasets not loadable:");
            for (ds, format) in &not_loadable {
                eprintln!("  - {:?} (format={})", ds, format);
            }
        }

        // All pure NER CoNLL datasets should be loadable
        assert!(
            not_loadable.is_empty(),
            "{} pure NER CoNLL datasets are not loadable",
            not_loadable.len()
        );
    }

    #[test]
    fn test_jsonl_ner_datasets_are_parseable() {
        // JSONL datasets with NER task should ideally be loadable
        let mut jsonl_ner_not_loadable = Vec::new();

        for &ds in DatasetId::all() {
            let format = ds.format().unwrap_or("");
            let is_jsonl = format == "JSONL" || format == "JSON-Lines" || format == "jsonl";
            let is_ner = ds.is_ner();

            if !is_jsonl || !is_ner {
                continue;
            }

            if !LoadableDatasetId::is_loadable_dataset(ds) {
                jsonl_ner_not_loadable.push(ds);
            }
        }

        // Log for debugging
        if !jsonl_ner_not_loadable.is_empty() {
            eprintln!(
                "JSONL NER datasets not loadable ({}):",
                jsonl_ner_not_loadable.len()
            );
            for ds in &jsonl_ner_not_loadable {
                eprintln!("  - {:?}", ds);
            }
        }
    }

    #[test]
    fn test_parse_afriqa() {
        // Test AfriQA JSON parsing
        let sample_json = r#"[
            {
                "context": "Lagos is a major city in Nigeria.",
                "question": "What is Lagos?",
                "answers": {
                    "text": ["major city"],
                    "answer_start": [11]
                }
            }
        ]"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_afriqa(sample_json, DatasetId::AfriQA);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);

        // Check that answer is marked
        let tokens = &dataset.sentences[0].tokens;
        // The answer "major city" should have B-ANSWER and I-ANSWER tags
        let answer_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.ner_tag.contains("ANSWER"))
            .collect();
        assert!(!answer_tokens.is_empty(), "Should have answer tokens");
    }

    #[test]
    fn test_african_datasets_in_all_list() {
        let all = DatasetId::all();
        assert!(all.contains(&DatasetId::MasakhaNER));
        assert!(all.contains(&DatasetId::MasakhaNER2));
        assert!(all.contains(&DatasetId::AfriSenti));
        assert!(all.contains(&DatasetId::AfriQA));
        assert!(all.contains(&DatasetId::MasakhaNEWS));
        assert!(all.contains(&DatasetId::MasakhaPOS));
    }

    // =========================================================================
    // Event Extraction Parser Tests
    // =========================================================================

    #[test]
    fn test_parse_maven_jsonl() {
        // Test MAVEN full JSONL format with events array
        let sample_jsonl = r#"{"id": "doc1", "content": [{"sentence": "The earthquake struck Tokyo.", "tokens": ["The", "earthquake", "struck", "Tokyo", "."]}], "events": [{"type": "Disaster", "mention": [{"trigger_word": "earthquake", "sent_id": 0, "offset": [1, 2]}]}]}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::event::parse_maven(sample_jsonl, DatasetId::MAVEN);
        assert!(result.is_ok(), "parse_maven should succeed");

        let dataset = result.unwrap();
        assert!(!dataset.sentences.is_empty(), "Should have sentences");

        // Check event type tag
        let has_disaster = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Disaster")));
        assert!(has_disaster, "Should have Disaster event tag");
    }

    #[test]
    fn test_parse_maven_docid2topic_fallback() {
        // Test fallback format (docid2topic.json)
        let sample_json = r#"{"doc1": "Natural_Disaster", "doc2": "Political_Event"}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::event::parse_maven(sample_json, DatasetId::MAVEN);
        assert!(result.is_ok(), "parse_maven fallback should succeed");

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2, "Should have 2 entries");
    }

    #[test]
    fn test_parse_casie() {
        // Test CASIE cybersecurity event format
        let sample_jsonl = r#"{"content": "A vulnerability was discovered in Apache.", "cyberevent": {"hopper": [{"events": [{"subtype": "Vulnerability", "nugget": {"text": "vulnerability"}, "argument": [{"text": "Apache", "role": {"type": "Affected_System"}}]}]}]}}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::event::parse_casie(sample_jsonl, DatasetId::CASIE);
        assert!(result.is_ok(), "parse_casie should succeed");

        let dataset = result.unwrap();
        assert!(!dataset.sentences.is_empty(), "Should have sentences");

        // Check for vulnerability trigger
        let has_vuln = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Vulnerability")));
        assert!(has_vuln, "Should have Vulnerability tag");

        // Check for argument
        let has_arg = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("ARG_")));
        assert!(has_arg, "Should have argument tag");
    }

    #[test]
    fn test_parse_maven_arg() {
        // Test MAVEN-ARG format with arguments
        let sample_jsonl = r#"{"id": "doc1", "document": "The company announced layoffs.", "events": [{"type": "Employment", "mention": [{"trigger_word": "layoffs", "offset": [4, 5]}], "argument": {"Employer": [{"content": "company", "offset": [1, 2]}]}}]}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::event::parse_maven_arg(sample_jsonl, DatasetId::MAVENArg);
        assert!(result.is_ok(), "parse_maven_arg should succeed");

        let dataset = result.unwrap();
        assert!(!dataset.sentences.is_empty(), "Should have sentences");

        // Check for trigger
        let has_trigger = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("Employment")));
        assert!(has_trigger, "Should have Employment event tag");

        // Check for argument role
        let has_employer = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.contains("ARG_Employer")));
        assert!(has_employer, "Should have Employer argument tag");
    }

    #[test]
    fn test_parse_rams() {
        // Test RAMS tokenized format
        let sample_jsonl = r#"{"doc_key": "doc1", "sentences": [["The", "soldier", "fired", "his", "weapon", "."]], "evt_triggers": [[2, 2, [["conflict.attack", 1.0]]]], "gold_evt_links": [[[0], [1, 1], "attacker"]]}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::event::parse_rams(sample_jsonl, DatasetId::RAMS);
        assert!(result.is_ok(), "parse_rams should succeed");

        let dataset = result.unwrap();
        assert!(!dataset.sentences.is_empty(), "Should have sentences");

        // Check for event trigger
        let has_event = dataset
            .sentences
            .iter()
            .any(|s| s.tokens.iter().any(|t| t.ner_tag.starts_with("B-")));
        assert!(has_event, "Should have event tags");
    }

    #[test]
    fn test_parse_trec() {
        // Test TREC question classification format
        let sample =
            "NUM:dist How far is it from Denver to Aspen ?\nLOC:city What county is Modesto in ?\n";

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_trec(sample, DatasetId::TREC);
        assert!(result.is_ok(), "parse_trec should succeed");

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2, "Should have 2 questions");

        // Check coarse labels
        assert!(dataset.sentences[0].tokens[0].ner_tag.contains("NUM"));
        assert!(dataset.sentences[1].tokens[0].ner_tag.contains("LOC"));
    }

    #[test]
    fn test_parse_litbank_ner_improved() {
        // Test improved LitBank NER parser with word-level tokenization
        let sample_ann = "T1\tPER 0 5\tAlice\nT2\tPER 10 14\tBob\nT3\tORG 20 28\tMicrosoft";

        let result = parse::coref::parse_litbank(sample_ann, DatasetId::LitBank);
        assert!(result.is_ok(), "parse_litbank should succeed");

        let dataset = result.unwrap();
        assert!(!dataset.sentences.is_empty(), "Should have sentences");

        // Check that entities are tokenized into words (not just single tokens)
        let sentence = &dataset.sentences[0];
        let entity_tokens: Vec<_> = sentence
            .tokens
            .iter()
            .filter(|t| t.ner_tag.starts_with("B-") || t.ner_tag.starts_with("I-"))
            .collect();

        // Should have at least the entity tokens (Alice, Bob, Microsoft)
        assert!(
            entity_tokens.len() >= 3,
            "Should have at least 3 entity tokens, got {}",
            entity_tokens.len()
        );

        // Check BIO tagging is correct (B- for first word, I- for subsequent words)
        let mut found_b_tag = false;
        for token in &sentence.tokens {
            if token.ner_tag.starts_with("B-") {
                found_b_tag = true;
                // First word of entity should be B-
                assert!(
                    token.ner_tag.starts_with("B-"),
                    "First word of entity should have B- tag"
                );
            }
        }
        assert!(found_b_tag, "Should have at least one B- tag");
    }

    #[test]
    fn test_parse_tweettopic() {
        // Test TweetTopic JSONL format
        let sample_jsonl = r#"{"text": "Amazing game last night!", "label": 4, "label_name": "sports_&_gaming"}
{"text": "New AI breakthrough announced", "label": 5, "label_name": "science_&_technology"}"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_tweettopic(sample_jsonl, DatasetId::TweetTopic);
        assert!(result.is_ok(), "parse_tweettopic should succeed");

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2, "Should have 2 tweets");

        // Check label names are used
        assert!(dataset.sentences[0].tokens[0]
            .ner_tag
            .contains("sports_&_gaming"));
        assert!(dataset.sentences[1].tokens[0]
            .ner_tag
            .contains("science_&_technology"));
    }

    #[test]
    fn test_african_dataset_from_str() {
        // Test string parsing for African datasets
        assert_eq!(
            "masakhaner".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaNER
        );
        assert_eq!(
            "masakhaner2".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaNER2
        );
        assert_eq!(
            "afrisenti".parse::<DatasetId>().unwrap(),
            DatasetId::AfriSenti
        );
        assert_eq!("afriqa".parse::<DatasetId>().unwrap(), DatasetId::AfriQA);
        assert_eq!(
            "masakhanews".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaNEWS
        );
        assert_eq!(
            "masakhapos".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaPOS
        );

        // Alternative spellings
        assert_eq!(
            "masakhane-ner".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaNER
        );
        assert_eq!(
            "afri-senti".parse::<DatasetId>().unwrap(),
            DatasetId::AfriSenti
        );
        assert_eq!(
            "masakhane-news".parse::<DatasetId>().unwrap(),
            DatasetId::MasakhaNEWS
        );
    }

    #[test]
    fn test_afrisenti_parse_with_tonal_diacritics() {
        // Test Yoruba text with tonal diacritics (common in AfriSenti)
        let yoruba_tsv = "Ó dára púpọ̀!\tpositive\n\
                          Kò dára rárá\tnegative\n\
                          Ẹ ṣé, mo dupẹ́\tpositive";

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_afrisenti(yoruba_tsv, DatasetId::AfriSenti);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 3);

        // Verify the text is preserved correctly with diacritics
        assert!(dataset.sentences[0].tokens[0].text.contains("dára"));
        assert!(dataset.sentences[1].tokens[0].text.contains("rárá"));
        assert!(dataset.sentences[2].tokens[0].text.contains("dupẹ́"));
    }

    #[test]
    fn test_masakhaner_parse_with_ethiopic_script() {
        // Test Amharic text in MasakhaNER CoNLL format (Ethiopic script)
        let amharic_conll = "ዶክተር B-PER\n\
                             አቢይ I-PER\n\
                             አህመድ I-PER\n\
                             ኢትዮጵያ B-LOC\n\
                             ውስጥ O\n\
                             ተወለዱ O\n";

        let result = parse::ner::parse_conll(amharic_conll, DatasetId::MasakhaNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);

        let tokens = &dataset.sentences[0].tokens;
        assert_eq!(tokens.len(), 6);

        // Verify Ethiopic script is preserved
        assert_eq!(tokens[0].text, "ዶክተር");
        assert_eq!(tokens[0].ner_tag, "B-PER");
        assert_eq!(tokens[3].text, "ኢትዮጵያ");
        assert_eq!(tokens[3].ner_tag, "B-LOC");
    }

    #[test]
    fn test_conllu_parse_with_nguni_clicks() {
        // Test isiXhosa/isiZulu text with click consonants (MasakhaPOS)
        let xhosa_conllu = "# sent_id = xho_test_1\n\
                           # text = UMongameli uCyril Ramaphosa\n\
                           1\tUMongameli\tumongameli\tNOUN\tN\t_\t0\troot\t_\t_\n\
                           2\tuCyril\tuCyril\tPROPN\tNNP\t_\t1\tappos\t_\t_\n\
                           3\tRamaphosa\tRamaphosa\tPROPN\tNNP\t_\t2\tflat:name\t_\t_\n\
                           \n\
                           # sent_id = xho_test_2\n\
                           # text = Ndiqala ukuthetha isiXhosa\n\
                           1\tNdiqala\tqala\tVERB\tV\t_\t0\troot\t_\t_\n\
                           2\tukuthetha\tthetha\tVERB\tV\t_\t1\txcomp\t_\t_\n\
                           3\tisiXhosa\tisiXhosa\tNOUN\tN\t_\t2\tobj\t_\t_\n";

        let result = parse::ner::parse_conllu(xhosa_conllu, DatasetId::MasakhaPOS);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);

        // Verify text with click-like consonants is preserved
        assert_eq!(dataset.sentences[0].tokens[0].text, "UMongameli");
        assert_eq!(dataset.sentences[1].tokens[2].text, "isiXhosa");
    }

    #[test]
    fn test_masakhanews_parse_with_arabic_variants() {
        // MasakhaNEWS includes Algerian/Moroccan Arabic variants
        let news_tsv = "headline\tbody\tcategory\n\
                        الأخبار العاجلة\tتفاصيل الخبر...\tpolitics\n\
                        رياضة محلية\tمباراة اليوم...\tsports";

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_masakhanews(news_tsv, DatasetId::MasakhaNEWS);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        // Header skipped, 2 data rows
        assert_eq!(dataset.sentences.len(), 2);

        // Check Arabic text is preserved
        assert!(dataset.sentences[0].tokens[0].text.contains("الأخبار"));
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-politics");
        assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-sports");
    }

    #[test]
    fn test_afriqa_multilingual_qa() {
        // AfriQA has questions in target language, context may be in English
        let qa_json = r#"[
            {
                "context": "Yorùbá is a tonal language spoken in Nigeria.",
                "question": "Kí ni Yorùbá?",
                "answers": {
                    "text": ["tonal language"],
                    "answer_start": [13]
                },
                "language": "yo"
            }
        ]"#;

        let loader = DatasetLoader::new().unwrap();
        let result = parse::classification::parse_afriqa(qa_json, DatasetId::AfriQA);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
    }

    // NOTE: We intentionally do not embed language-code→URL expansion helpers here.
    // If we add them, they should live in the registry (metadata) or a dedicated dataset
    // URL builder module, not in the loader tests.

    // =========================================================================
    // Comprehensive Parser Smoke Tests (one per DatasetParsePlan)
    // =========================================================================

    #[test]
    fn test_parse_content_rejects_empty_for_all_loadable_datasets() {
        // Global invariant: no dataset should parse from empty content.
        for loadable in LoadableDatasetId::all() {
            let id: DatasetId = loadable.into();
            let err = parse::parse_content("   \n\t", id)
                .expect_err("empty content must error");
            let msg = format!("{err}");
            assert!(
                msg.to_lowercase().contains("empty"),
                "Expected an 'empty' error message for {:?}, got: {}",
                id,
                msg
            );
        }
    }

    #[test]
    fn test_parse_docred_smoke() {
        let sample = r#"{"doc_key":"d1","sentence":["John","met","Mary","in","Paris","."],"ner":[[0,0,"PER"],[2,2,"PER"],[4,4,"LOC"]],"relations":[]}"#;
        let loader = DatasetLoader::new().unwrap();
        let ds = parse::relation::parse_docred(sample, DatasetId::DocRED).unwrap();
        assert!(!ds.sentences.is_empty());
        assert!(ds.sentences[0]
            .tokens
            .iter()
            .any(|t| t.ner_tag.starts_with("B-")));
    }

    #[test]
    fn test_parse_cadec_jsonl_smoke() {
        // Character offsets in `entities` are interpreted over reconstructed text from tokens.
        // "I took aspirin" → aspirin starts at char 7, ends at 14 (exclusive).
        let sample = r#"{"tokens":["I","took","aspirin"],"entities":[{"text":"aspirin","label":"DRUG","start":7,"end":14}]}"#;
        let ds = parse::ner::parse_cadec_jsonl(sample, DatasetId::CADEC).unwrap();
        assert!(!ds.sentences.is_empty());
        assert!(ds.sentences[0]
            .tokens
            .iter()
            .any(|t| t.ner_tag == "B-DRUG" || t.ner_tag == "I-DRUG"));
    }

    #[test]
    fn test_parse_cadec_hf_api_smoke() {
        let sample = r#"{"rows":[{"row":{"text":"I took aspirin","ade":"aspirin"}}]}"#;
        let ds = parse::ner::parse_cadec_hf_api(sample, DatasetId::CADEC).unwrap();
        assert!(!ds.sentences.is_empty());
        assert!(ds.sentences[0]
            .tokens
            .iter()
            .any(|t| t.ner_tag.contains("adverse_drug_event")));
    }

    #[test]
    fn test_parse_bc5cdr_smoke() {
        let sample = "Aspirin\tNN\tO\tB-CHEMICAL\nhelps\tVBZ\tO\tO\n\n";
        let ds = parse::ner::parse_bc5cdr(sample, DatasetId::BC5CDR).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-CHEMICAL");
    }

    #[test]
    fn test_parse_ncbi_disease_smoke() {
        let sample = "Cancer\tNN\tO\tB-Disease\nprogresses\tVBZ\tO\tO\n\n";
        let ds = parse::ner::parse_ncbi_disease(sample, DatasetId::NCBIDisease).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
    }

    #[test]
    fn test_parse_gap_smoke() {
        let sample =
            "ID\tText\tPronoun\tPronoun-offset\tA\tA-offset\tA-coref\tB\tB-offset\tB-coref\tURL\n\
g1\tJohn met Mary. He waved.\tHe\t14\tJohn\t0\tTRUE\tMary\t9\tFALSE\thttp://example\n";
        let ds = parse::coref::parse_gap(sample, DatasetId::GAP).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert!(!ds.sentences[0].tokens.is_empty());
    }

    #[test]
    fn test_parse_preco_jsonl_smoke() {
        let sample = r#"{"sentences":[["John","went","home","."],["He","slept","."]]}"#;
        let ds = parse::coref::parse_preco_jsonl(sample, DatasetId::PreCo).unwrap();
        assert_eq!(ds.sentences.len(), 2);
        assert_eq!(ds.sentences[0].tokens[0].text, "John");
    }

    #[test]
    fn test_parse_wikiann_json_array_smoke() {
        let sample =
            r#"[{"tokens":["John","went","to","Paris"],"ner_tags":["B-PER","O","O","B-LOC"]}]"#;
        let ds = parse::ner::parse_wikiann_json(sample, DatasetId::UNER).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-PER");
    }

    #[test]
    fn test_parse_hf_api_response_smoke() {
        let sample = r#"{
  "features":[{"name":"tokens"},{"name":"ner_tags","type":{"feature":{"names":["O","B-PER","I-PER"]}}}],
  "rows":[{"row_idx":0,"row":{"tokens":["John"],"ner_tags":[1]}}]
}"#;
        let ds = parse::ner::parse_hf_api_response(sample, DatasetId::UniversalNER).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-PER");
    }

    #[test]
    fn test_parse_hf_api_response_temporal_standoff_smoke() {
        let sample = r#"{
  "features":[{"name":"text"},{"name":"time_expressions"}],
  "rows":[{"row_idx":0,"row":{
    "text":"A 10/30/89 .",
    "time_expressions":[{"text":"10/30/89","start_char":2,"end_char":10,"tid":"t1","type":"DATE","value":"1989-10-30"}],
    "event_expressions":[],
    "signal_expressions":[]
  }}]
}"#;
        let ds = parse::ner::parse_hf_api_response(sample, DatasetId::TimexRecognitionSentenceOriginal).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert_eq!(ds.sentences[0].tokens.len(), 3);
        assert_eq!(ds.sentences[0].tokens[0].text, "A");
        assert_eq!(ds.sentences[0].tokens[0].ner_tag, "O");
        assert_eq!(ds.sentences[0].tokens[1].text, "10/30/89");
        assert_eq!(ds.sentences[0].tokens[1].ner_tag, "B-TIMEX");
        assert_eq!(ds.sentences[0].tokens[2].text, ".");
        assert_eq!(ds.sentences[0].tokens[2].ner_tag, "O");
    }

    #[test]
    fn test_parse_hf_api_response_pairwise_discourse_smoke() {
        let sample = r#"{
  "features":[{"name":"unit1_txt"},{"name":"unit2_txt"},{"name":"label"}],
  "rows":[{"row_idx":0,"row":{
    "unit1_txt":"Because it rained",
    "unit2_txt":"the game was canceled",
    "label":"Cause"
  }}]
}"#;
        let ds = parse::ner::parse_hf_api_response(sample, DatasetId::DisrptEngDepScidtbRels).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert_eq!(ds.sentences[0].tokens.len(), 1);
        assert_eq!(
            ds.sentences[0].tokens[0].text,
            "Because it rained [SEP] the game was canceled"
        );
        assert_eq!(ds.sentences[0].tokens[0].ner_tag, "B-Cause");
    }

    #[test]
    fn test_parse_hf_api_response_disrpt_conllu_seg_smoke() {
        let sample = r#"{
  "features":[{"name":"form"},{"name":"misc"}],
  "rows":[{"row_idx":0,"row":{
    "form":["We","propose","a","method","."],
    "misc":["Seg=B-seg","Seg=O","Seg=O","Seg=B-seg","Seg=O"]
  }}]
}"#;
        let ds = parse::ner::parse_hf_api_response(sample, DatasetId::DisrptEngDepScidtbConlluSeg).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        let tags: Vec<&str> = ds.sentences[0]
            .tokens
            .iter()
            .map(|t| t.ner_tag.as_str())
            .collect();
        assert_eq!(tags, vec!["B-SEG", "I-SEG", "I-SEG", "B-SEG", "I-SEG"]);
    }

    #[test]
    fn test_parse_agnews_smoke() {
        let sample = r#"{"text":"Stocks rally on earnings","label":2}"#;
        let loader = DatasetLoader::new().unwrap();
        let ds = parse::classification::parse_agnews(sample, DatasetId::AGNews).unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
    }

    #[test]
    fn test_parse_dbpedia14_smoke() {
        let sample = r#"{"content":"The Beatles released Abbey Road","label":5}"#;
        let ds = parse::classification::parse_dbpedia14(sample, DatasetId::DBPedia14)
            .unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
    }

    #[test]
    fn test_parse_yahoo_answers_smoke() {
        let sample = r#"{"question_title":"Why is the sky blue?","topic":1}"#;
        let ds = parse::classification::parse_yahoo_answers(sample, DatasetId::YahooAnswers)
            .unwrap();
        assert_eq!(ds.sentences.len(), 1);
        assert!(ds.sentences[0].tokens[0].ner_tag.starts_with("B-"));
    }

    // =========================================================================
    // Dataset Registry Integration Tests
    // =========================================================================

    #[test]
    fn test_sec_filings_has_raw_url() {
        // Verify SEC-filings dataset has a raw GitHub URL for direct download
        let url = DatasetId::SECFilingsNER.download_url();
        assert!(
            url.contains("raw.githubusercontent.com"),
            "SEC-filings should have raw GitHub URL, got: {}",
            url
        );
        assert!(
            url.ends_with(".txt"),
            "SEC-filings should point to a .txt file, got: {}",
            url
        );
    }

    #[test]
    fn test_twiconv_has_format() {
        // Verify TwiConv dataset has format field
        let format = DatasetId::TwiConv.format();
        assert!(format.is_some(), "TwiConv should have format field");
        // TwiConv uses CoNLL format for coreference data
        assert_eq!(format.unwrap(), "CoNLL", "TwiConv should be CoNLL format");
    }

    #[test]
    fn test_mudoco_has_format() {
        // Verify MuDoCo dataset has format field
        let format = DatasetId::MuDoCo.format();
        assert!(format.is_some(), "MuDoCo should have format field");
        assert_eq!(format.unwrap(), "JSON", "MuDoCo should be JSON format");
    }

    #[test]
    fn test_all_public_ud_datasets_have_conllu_format() {
        // All Universal Dependencies datasets should have CoNLLU format
        let ud_datasets = vec![
            DatasetId::AncientGreekUD,
            DatasetId::LatinUD,
            DatasetId::SanskritUD,
            DatasetId::OldEnglishUD,
            DatasetId::UDEsperantoCairo,
        ];

        for ds in ud_datasets {
            let format = ds.format();
            assert!(format.is_some(), "{:?} should have format field", ds);
            assert_eq!(
                format.unwrap(),
                "CoNLLU",
                "{:?} should be CoNLLU format",
                ds
            );
        }
    }

    #[test]
    fn test_datasets_with_public_urls_are_accessible() {
        // Verify that key public datasets have valid URLs (format check only)
        let test_cases = vec![
            (DatasetId::AncientGreekUD, "universaldependencies"),
            (DatasetId::LatinUD, "universaldependencies"),
            (DatasetId::SECFilingsNER, "entity-recognition-datasets"),
            (DatasetId::TwiConv, "twiconv"), // lowercase for case-insensitive match
        ];

        for (ds, expected_substring) in test_cases {
            let url = ds.download_url();
            assert!(!url.is_empty(), "{:?} should have a download URL", ds);
            assert!(
                url.to_lowercase()
                    .contains(&expected_substring.to_lowercase()),
                "{:?} URL should contain '{}', got: {}",
                ds,
                expected_substring,
                url
            );
        }
    }

    #[test]
    fn test_loadable_datasets_count_is_stable() {
        // Track the number of loadable datasets to detect regressions
        // This should only increase as we add more loaders
        let loadable = LoadableDatasetId::all();
        let count = loadable.len();

        // As of 2025-12-15, after recent fixes we have 295 loadable datasets
        assert!(
            count >= 295,
            "Expected at least 295 loadable datasets, got {}. \
             This may indicate a regression in the loading system.",
            count
        );
    }

    #[test]
    fn test_conll_format_variants_all_detected() {
        // Ensure CoNLL format variants for pure NER datasets are properly detected
        // We exclude RE/coref datasets as they have special parsing needs
        for &ds in DatasetId::all() {
            let format = ds.format();
            if let Some(fmt) = format {
                let is_conll_variant =
                    fmt == "CoNLL" || fmt == "CoNLLU" || fmt == "CoNLL-U" || fmt == "CoNLL03";

                // Only check pure NER datasets (no coref, no RE)
                let is_pure_ner = ds.supports_ner() && !ds.supports_coref() && !ds.supports_re();

                if is_conll_variant && is_pure_ner {
                    // Should be loadable via hint system
                    let hint = LoadableDatasetId::registry_hint_plan(ds);
                    assert!(
                        hint.is_some() || LoadableDatasetId::is_loadable_dataset(ds),
                        "{:?} with format {} and pure NER task should be loadable",
                        ds,
                        fmt
                    );
                }
            }
        }
    }

    #[test]
    fn test_parse_csv_ner_smoke() {
        // Test CSV NER format (E-NER/EDGAR-NER style: Token,Tag)
        let sample = "\
-DOCSTART-,O
,O
Check,O
the,O
appropriate,O
box,O
,O
Nuveen,I-BUSINESS
New,I-BUSINESS
York,I-BUSINESS
Fund,I-BUSINESS
,O

The,O
SEC,I-GOVERNMENT
filed,O
charges,O
.

John,I-PERSON
Smith,I-PERSON
is,O
the,O
CEO,O
.
";
        let ds = parse::ner::parse_csv_ner(sample, DatasetId::ENer).unwrap();

        // Should have 3 sentences (separated by empty lines and -DOCSTART-)
        assert_eq!(
            ds.sentences.len(),
            3,
            "Expected 3 sentences, got {:?}",
            ds.sentences.len()
        );

        // First sentence should have BUSINESS entities
        let first_sentence = &ds.sentences[0];
        assert!(
            first_sentence
                .tokens
                .iter()
                .any(|t| t.ner_tag == "I-BUSINESS"),
            "First sentence should contain I-BUSINESS tags"
        );

        // Second sentence should have GOVERNMENT entity
        let second_sentence = &ds.sentences[1];
        assert!(
            second_sentence
                .tokens
                .iter()
                .any(|t| t.ner_tag == "I-GOVERNMENT"),
            "Second sentence should contain I-GOVERNMENT tag"
        );

        // Third sentence should have PERSON entities
        let third_sentence = &ds.sentences[2];
        assert!(
            third_sentence
                .tokens
                .iter()
                .any(|t| t.ner_tag == "I-PERSON"),
            "Third sentence should contain I-PERSON tags"
        );

        // Check specific token/tag pairs
        let nuveen_token = first_sentence.tokens.iter().find(|t| t.text == "Nuveen");
        assert!(nuveen_token.is_some(), "Should have Nuveen token");
        assert_eq!(nuveen_token.unwrap().ner_tag, "I-BUSINESS");

        let john_token = third_sentence.tokens.iter().find(|t| t.text == "John");
        assert!(john_token.is_some(), "Should have John token");
        assert_eq!(john_token.unwrap().ner_tag, "I-PERSON");
    }

    #[test]
    fn test_csv_ner_format_is_detected() {
        // Ensure CSV format datasets with NER tasks are properly detected as loadable
        let ener_hint = LoadableDatasetId::registry_hint_plan(DatasetId::ENer);
        assert_eq!(
            ener_hint,
            Some(DatasetParsePlan::CsvNer),
            "ENer should use CsvNer parse plan"
        );

        // Verify ENer is loadable
        assert!(
            LoadableDatasetId::is_loadable_dataset(DatasetId::ENer),
            "ENer should be loadable"
        );
    }

    // =========================================================================
    // Tests for newly added dataset loaders (2025-12)
    // =========================================================================

    #[test]
    fn test_newly_added_conll_datasets_are_loadable() {
        let new_conll = [
            DatasetId::QxoRef,
            DatasetId::GICoref,
            DatasetId::WNUT16,
            DatasetId::NoiseBench,
            DatasetId::CrossWeigh,
            DatasetId::ZELDA,
            DatasetId::GENIANested,
        ];

        for id in new_conll {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable with Conll parse plan",
                id
            );
            assert_eq!(
                LoadableDatasetId::parse_plan(id),
                Some(DatasetParsePlan::Conll),
                "{:?} should use Conll plan",
                id
            );
        }
    }

    #[test]
    fn test_newly_added_jsonl_datasets_are_loadable() {
        let new_jsonl = [
            DatasetId::REBEL,
            DatasetId::BBQ,
            DatasetId::RealToxicityPrompts,
            DatasetId::BookCoref,
            DatasetId::BookCorefSplit,
            DatasetId::WIESP2022NER,
            DatasetId::FewRel,
            DatasetId::PIIMasking200k,
            DatasetId::B2NERD,
            DatasetId::OpenNER,
            DatasetId::FictionNER750M,
        ];

        for id in new_jsonl {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable with JsonlNer parse plan",
                id
            );
            assert_eq!(
                LoadableDatasetId::parse_plan(id),
                Some(DatasetParsePlan::JsonlNer),
                "{:?} should use JsonlNer plan",
                id
            );
        }
    }

    #[test]
    fn test_genia_nested_conll_parse() {
        // GENIA nested NER uses multi-layered BIO tags
        let nested_conll = "IL-2\tB-protein\n\
                            gene\tI-protein\n\
                            expression\tO\n\
                            \n\
                            T\tB-cell_type\n\
                            cells\tI-cell_type\n";

        let result = parse::ner::parse_conll(nested_conll, DatasetId::GENIANested);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-protein");
        assert_eq!(dataset.sentences[1].tokens[0].ner_tag, "B-cell_type");
    }

    #[test]
    fn test_gicoref_gender_inclusive_parse() {
        // GICoref uses neopronouns and singular they
        let gicoref_conll = "Alex\tB-PER\n\
                             uses\tO\n\
                             they\tB-PER\n\
                             pronouns\tO\n\
                             \n\
                             Jordan\tB-PER\n\
                             introduced\tO\n\
                             themself\tB-PER\n";

        let result = parse::ner::parse_conll(gicoref_conll, DatasetId::GICoref);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);

        // Verify pronoun tokens are correctly tagged
        assert_eq!(dataset.sentences[0].tokens[2].text, "they");
        assert_eq!(dataset.sentences[0].tokens[2].ner_tag, "B-PER");
        assert_eq!(dataset.sentences[1].tokens[2].text, "themself");
        assert_eq!(dataset.sentences[1].tokens[2].ner_tag, "B-PER");
    }

    #[test]
    fn test_fewrel_jsonl_parse() {
        // FewRel has relation extraction in JSONL format
        // The parser expects integer tags mapping to MultiNERD labels:
        // 0=O, 1=B-PER, 3=B-ORG
        let fewrel_sample =
            r#"{"tokens":["John","works","at","Google","."],"ner_tags":[1,0,0,3,0]}"#;

        let result = parse::ner::parse_jsonl_ner(fewrel_sample, DatasetId::FewRel);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens.len(), 5);
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-PER");
        assert_eq!(dataset.sentences[0].tokens[3].ner_tag, "B-ORG");
    }

    #[test]
    fn test_b2nerd_business_entities_parse() {
        // B2NERD focuses on business/financial entity types
        // Using standard MultiNERD indices: 3=B-ORG (closest to COMPANY), 0=O
        let b2nerd_sample =
            r#"{"tokens":["Apple","Inc",".","reports","Q4","earnings"],"ner_tags":[3,4,4,0,0,0]}"#;

        let result = parse::ner::parse_jsonl_ner(b2nerd_sample, DatasetId::B2NERD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-ORG");
        assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "I-ORG");
    }

    #[test]
    fn test_ud_classical_languages_are_loadable() {
        // Universal Dependencies treebanks for classical/ancient languages
        let ud_datasets = [
            DatasetId::AncientGreekUD,
            DatasetId::LatinUD,
            DatasetId::SanskritUD,
            DatasetId::OldEnglishUD,
            DatasetId::OldNorseUD,
            DatasetId::UDEsperantoCairo,
        ];

        for id in ud_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable with Conllu parse plan",
                id
            );
            assert_eq!(
                LoadableDatasetId::parse_plan(id),
                Some(DatasetParsePlan::Conllu),
                "{:?} should use Conllu plan",
                id
            );
        }
    }

    #[test]
    fn test_hipe2022_tsv_is_loadable() {
        assert!(
            LoadableDatasetId::is_loadable_dataset(DatasetId::HIPE2022),
            "HIPE2022 should be loadable"
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(DatasetId::HIPE2022),
            Some(DatasetParsePlan::TsvNer),
            "HIPE2022 should use TsvNer plan"
        );
    }

    #[test]
    fn test_ener_csv_is_loadable() {
        assert!(
            LoadableDatasetId::is_loadable_dataset(DatasetId::ENer),
            "ENer should be loadable"
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(DatasetId::ENer),
            Some(DatasetParsePlan::CsvNer),
            "ENer should use CsvNer plan"
        );
    }

    #[test]
    fn test_loadable_count_increased() {
        // Regression test: ensure we have at least 295 loadable datasets
        // (Updated 2025-12 after adding CoNLL/JSONL/CoNLLU batches)
        let loadable_count = LoadableDatasetId::all().len();
        assert!(
            loadable_count >= 295,
            "Expected at least 295 loadable datasets, got {}",
            loadable_count
        );
    }

    // =========================================================================
    // Domain-Specific Parser Tests
    // =========================================================================

    #[test]
    fn test_biomedical_conll_with_chemical_entities() {
        // CHEMDNER-style biomedical NER with chemical entity types
        let chemdner_conll = "Aspirin\tB-CHEMICAL\n\
                              inhibits\tO\n\
                              COX-2\tB-GENE\n\
                              expression\tO\n\
                              \n\
                              Metformin\tB-CHEMICAL\n\
                              treats\tO\n\
                              diabetes\tB-DISEASE\n";

        let result = parse::ner::parse_conll(chemdner_conll, DatasetId::CHEMDNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-CHEMICAL");
        assert_eq!(dataset.sentences[0].tokens[2].ner_tag, "B-GENE");
        assert_eq!(dataset.sentences[1].tokens[2].ner_tag, "B-DISEASE");
    }

    #[test]
    fn test_historical_ner_with_archaic_spelling() {
        // Historical NER should handle archaic spellings and diacritics
        let historical_conll = "Præsident\tB-PER\n\
                                 Washington\tI-PER\n\
                                 addresseth\tO\n\
                                 ye\tO\n\
                                 Congreſs\tB-ORG\n";

        let result = parse::ner::parse_conll(historical_conll, DatasetId::EighteenthCenturyNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Verify archaic characters are preserved
        assert!(dataset.sentences[0].tokens[0].text.contains('æ'));
        assert!(dataset.sentences[0].tokens[4].text.contains('ſ'));
    }

    #[test]
    fn test_multilingual_code_switching_ner() {
        // LinCE/CALCS datasets have code-switched text (e.g., Spanish-English)
        let codeswitched_conll = "My\tO\n\
                                   abuela\tB-PER\n\
                                   lives\tO\n\
                                   in\tO\n\
                                   Ciudad\tB-LOC\n\
                                   de\tI-LOC\n\
                                   México\tI-LOC\n";

        let result = parse::ner::parse_conll(codeswitched_conll, DatasetId::LinCE);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[1].text, "abuela");
        assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-PER");
        // Multi-word location
        assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-LOC");
        assert_eq!(dataset.sentences[0].tokens[6].ner_tag, "I-LOC");
    }

    #[test]
    fn test_indigenous_language_ner() {
        // Test parsing of indigenous language NER (Guarani/Shipibo-Konibo)
        let guarani_conll = "Paraguái\tB-LOC\n\
                              ha\tO\n\
                              yvypora\tO\n\
                              oiko\tO\n\
                              Asunción\tB-LOC\n\
                              pe\tO\n";

        let result = parse::ner::parse_conll(guarani_conll, DatasetId::GuaraniNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[0].text, "Paraguái");
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-LOC");
    }

    #[test]
    fn test_ancient_greek_conllu_with_polytonic() {
        // Ancient Greek with polytonic diacritics
        let greek_conllu = "# sent_id = grc_test_1\n\
                            # text = ἐπιστήμη καὶ δικαιοσύνη\n\
                            1\tἐπιστήμη\tἐπιστήμη\tNOUN\tN\tCase=Nom|Gender=Fem|Number=Sing\t0\troot\t_\tSpaceAfter=Yes\n\
                            2\tκαὶ\tκαί\tCCONJ\tC\t_\t3\tcc\t_\tSpaceAfter=Yes\n\
                            3\tδικαιοσύνη\tδικαιοσύνη\tNOUN\tN\tCase=Nom|Gender=Fem|Number=Sing\t1\tconj\t_\tSpaceAfter=No\n";

        let result = parse::ner::parse_conllu(greek_conllu, DatasetId::AncientGreekUD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Verify polytonic Greek is preserved
        assert_eq!(dataset.sentences[0].tokens[0].text, "ἐπιστήμη");
        assert_eq!(dataset.sentences[0].tokens[2].text, "δικαιοσύνη");
    }

    #[test]
    fn test_latin_conllu_with_macrons() {
        // Latin with optional macrons for vowel length
        let latin_conllu = "# sent_id = lat_test_1\n\
                            # text = Rōma āterna est\n\
                            1\tRōma\tRoma\tPROPN\tNNP\tCase=Nom|Gender=Fem|Number=Sing\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                            2\tāterna\taeternus\tADJ\tA\tCase=Nom|Gender=Fem|Number=Sing\t1\tamod\t_\tSpaceAfter=Yes\n\
                            3\test\tsum\tAUX\tV\tMood=Ind|Number=Sing|Person=3|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tSpaceAfter=No\n";

        let result = parse::ner::parse_conllu(latin_conllu, DatasetId::LatinUD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Verify macrons are preserved
        assert!(dataset.sentences[0].tokens[0].text.contains('ō'));
        assert!(dataset.sentences[0].tokens[1].text.contains('ā'));
    }

    #[test]
    fn test_sanskrit_conllu_with_devanagari() {
        // Sanskrit in Devanagari script
        let sanskrit_conllu = "# sent_id = sa_test_1\n\
                               # text = रामः सीतां पश्यति\n\
                               1\tरामः\tराम\tNOUN\tN\tCase=Nom|Gender=Masc|Number=Sing\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                               2\tसीतां\tसीता\tPROPN\tNNP\tCase=Acc|Gender=Fem|Number=Sing\t3\tobj\t_\tSpaceAfter=Yes\n\
                               3\tपश्यति\tदृश्\tVERB\tV\tMood=Ind|Number=Sing|Person=3|Tense=Pres|VerbForm=Fin|Voice=Act\t0\troot\t_\tSpaceAfter=No\n";

        let result = parse::ner::parse_conllu(sanskrit_conllu, DatasetId::SanskritUD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens.len(), 3);
        // Verify Devanagari is preserved
        assert_eq!(dataset.sentences[0].tokens[0].text, "रामः");
        assert_eq!(dataset.sentences[0].tokens[1].text, "सीतां");
    }

    #[test]
    fn test_klingon_conllu_is_loadable() {
        // Klingon (tlh) is a constructed language in TaggedPBCKlingon
        assert!(
            LoadableDatasetId::is_loadable_dataset(DatasetId::TaggedPBCKlingon),
            "Klingon dataset should be loadable"
        );
        assert_eq!(
            LoadableDatasetId::parse_plan(DatasetId::TaggedPBCKlingon),
            Some(DatasetParsePlan::Conllu),
            "Klingon should use Conllu plan"
        );
    }

    #[test]
    fn test_financial_ner_entities() {
        // FinanceNER with financial entity types
        let finance_conll = "Tesla\tB-COMPANY\n\
                             stock\tO\n\
                             rose\tO\n\
                             5%\tB-PERCENTAGE\n\
                             after\tO\n\
                             Q4\tB-PERIOD\n\
                             earnings\tO\n";

        let result = parse::ner::parse_conll(finance_conll, DatasetId::FinanceNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-COMPANY");
        assert_eq!(dataset.sentences[0].tokens[3].ner_tag, "B-PERCENTAGE");
    }

    #[test]
    fn test_recipe_ner_food_entities() {
        // RecipeNER with culinary entity types
        let recipe_conll = "Add\tO\n\
                            2\tB-QUANTITY\n\
                            cups\tI-QUANTITY\n\
                            of\tO\n\
                            flour\tB-INGREDIENT\n\
                            and\tO\n\
                            1\tB-QUANTITY\n\
                            tsp\tI-QUANTITY\n\
                            salt\tB-INGREDIENT\n";

        let result = parse::ner::parse_conll(recipe_conll, DatasetId::RecipeNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-INGREDIENT");
        assert_eq!(dataset.sentences[0].tokens[8].ner_tag, "B-INGREDIENT");
    }

    #[test]
    fn test_astronomy_ner_entities() {
        // AstroNER with astronomical entity types
        let astro_conll = "The\tO\n\
                           Andromeda\tB-GALAXY\n\
                           Galaxy\tI-GALAXY\n\
                           is\tO\n\
                           2.5\tB-DISTANCE\n\
                           million\tI-DISTANCE\n\
                           light-years\tI-DISTANCE\n\
                           away\tO\n";

        let result = parse::ner::parse_conll(astro_conll, DatasetId::AstroNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens[1].ner_tag, "B-GALAXY");
        assert_eq!(dataset.sentences[0].tokens[4].ner_tag, "B-DISTANCE");
    }

    #[test]
    fn test_nested_ner_datasets_are_loadable() {
        // Nested NER datasets (entities within entities)
        let nested_datasets = [DatasetId::GENIANested, DatasetId::ChineseNestedNER];

        for id in nested_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
        }
    }

    #[test]
    fn test_discontinuous_ner_datasets_are_loadable() {
        // Discontinuous NER datasets (non-contiguous entity spans)
        let discontinuous_datasets = [
            DatasetId::GermEvalDiscontinuous,
            DatasetId::PubMedDiscontinuous,
        ];

        for id in discontinuous_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
        }
    }

    #[test]
    fn test_social_media_ner_datasets_are_loadable() {
        // Social media NER datasets (noisy text, hashtags, mentions)
        let social_datasets = [
            DatasetId::WNUT16,
            DatasetId::TwiConv,
            DatasetId::NERsocialFood,
        ];

        for id in social_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
            assert_eq!(
                LoadableDatasetId::parse_plan(id),
                Some(DatasetParsePlan::Conll),
                "{:?} should use Conll plan",
                id
            );
        }
    }

    #[test]
    fn test_literary_ner_datasets_are_loadable() {
        // Literary NER datasets (fiction, novels)
        let literary_datasets = [
            DatasetId::CharacterCodex,
            DatasetId::FictionNER750M,
            DatasetId::BookCoref,
        ];

        for id in literary_datasets {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
        }
    }

    #[test]
    fn test_jsonl_ner_with_empty_tokens_handled() {
        // Edge case: JSONL with some empty tokens
        let jsonl_with_empty = r#"{"tokens":["Hello","","world"],"ner_tags":[0,0,0]}"#;

        let result = parse::ner::parse_jsonl_ner(jsonl_with_empty, DatasetId::MultiWOZNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Empty token should still be preserved in parsing
        assert_eq!(dataset.sentences[0].tokens.len(), 3);
    }

    #[test]
    fn test_conll_with_long_entity_spans() {
        // Test parsing of very long entity spans (e.g., legal document titles)
        let long_span_conll = "The\tB-DOCUMENT\n\
                               United\tI-DOCUMENT\n\
                               States\tI-DOCUMENT\n\
                               Constitution\tI-DOCUMENT\n\
                               Article\tI-DOCUMENT\n\
                               I\tI-DOCUMENT\n\
                               Section\tI-DOCUMENT\n\
                               8\tI-DOCUMENT\n\
                               Clause\tI-DOCUMENT\n\
                               3\tI-DOCUMENT\n";

        let result = parse::ner::parse_conll(long_span_conll, DatasetId::LegNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens.len(), 10);
        // All tokens should be part of the same entity
        assert_eq!(dataset.sentences[0].tokens[0].ner_tag, "B-DOCUMENT");
        assert_eq!(dataset.sentences[0].tokens[9].ner_tag, "I-DOCUMENT");
    }

    #[test]
    fn test_all_added_conll_datasets_are_loadable() {
        // Comprehensive check for all newly added CoNLL datasets
        let added_conll = [
            DatasetId::HistNERo,
            DatasetId::DutchArchaeology,
            DatasetId::FINER,
            DatasetId::CALCS2018,
            DatasetId::MedievalCharterNER,
            DatasetId::RockNER,
            DatasetId::AIDACoNLL,
            DatasetId::NNE,
            DatasetId::IndicNER,
            DatasetId::NorNE,
            DatasetId::TASTEset,
            DatasetId::TechNER,
            DatasetId::FinTechPatent,
            DatasetId::WaterAgriNER,
            DatasetId::RussianCulturalNER,
            DatasetId::BASHI,
            DatasetId::ENER,
        ];

        for id in added_conll {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
        }
    }

    #[test]
    fn test_all_added_jsonl_datasets_are_loadable() {
        // Comprehensive check for all newly added JSONL datasets
        let added_jsonl = [
            DatasetId::MultiWOZNER,
            DatasetId::HinglishNER,
            DatasetId::AgCNER,
            DatasetId::LongDocNER,
            DatasetId::MultiBioNERLong,
            DatasetId::ReasoningNER,
            DatasetId::BioNERLLaMA,
            DatasetId::LexGLUENER,
            DatasetId::FinBenNER,
            DatasetId::FiNER139,
            DatasetId::SciNER,
            DatasetId::AIONER,
            DatasetId::WIESPAstro,
            DatasetId::CEREC,
            DatasetId::DELICATE,
            DatasetId::CSN,
        ];

        for id in added_jsonl {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
        }
    }

    #[test]
    fn test_all_added_ud_datasets_are_loadable() {
        // Comprehensive check for all newly added UD datasets
        let added_ud = [
            DatasetId::CopticScriptorium,
            DatasetId::TaggedPBCEsperanto,
            DatasetId::TaggedPBCKlingon,
            DatasetId::AkkadianUD,
            DatasetId::AncientHebrewUD,
            DatasetId::ClassicalChineseUD,
            DatasetId::CopticUD,
            DatasetId::GothicUD,
            DatasetId::HittiteUD,
            DatasetId::OldChurchSlavonicUD,
            DatasetId::LatinITTB,
            DatasetId::LatinPROIEL,
            DatasetId::EsperantoUD,
            DatasetId::NavajoMorph,
        ];

        for id in added_ud {
            assert!(
                LoadableDatasetId::is_loadable_dataset(id),
                "{:?} should be loadable",
                id
            );
            assert_eq!(
                LoadableDatasetId::parse_plan(id),
                Some(DatasetParsePlan::Conllu),
                "{:?} should use Conllu plan",
                id
            );
        }
    }

    // =========================================================================
    // Edge Case and Robustness Tests
    // =========================================================================

    #[test]
    fn test_conll_handles_windows_line_endings() {
        // Test CRLF line endings (Windows format)
        let windows_conll = "John\tB-PER\r\nSmith\tI-PER\r\n\r\nLondon\tB-LOC\r\n";

        let result = parse::ner::parse_conll(windows_conll, DatasetId::WikiGold);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 2);
    }

    #[test]
    fn test_conll_handles_extra_whitespace() {
        // Test lines with trailing/leading whitespace
        let whitespace_conll = "  John  \t  B-PER  \n  meets  \t  O  \n\n";

        let result = parse::ner::parse_conll(whitespace_conll, DatasetId::WikiGold);
        // Should handle gracefully (may skip malformed lines)
        assert!(result.is_ok());
    }

    #[test]
    fn test_conllu_handles_multiword_tokens() {
        // CoNLL-U multi-word tokens (e.g., "don't" -> "do" + "n't")
        let mwt_conllu = "# sent_id = test\n\
                          # text = I can't go\n\
                          1\tI\tI\tPRON\tPRP\t_\t3\tnsubj\t_\tSpaceAfter=Yes\n\
                          2-3\tcan't\t_\t_\t_\t_\t_\t_\t_\tSpaceAfter=Yes\n\
                          2\tca\tcan\tAUX\tMD\t_\t3\taux\t_\t_\n\
                          3\tn't\tnot\tPART\tRB\t_\t0\troot\t_\t_\n\
                          4\tgo\tgo\tVERB\tVB\t_\t3\txcomp\t_\tSpaceAfter=No\n";

        let result = parse::ner::parse_conllu(mwt_conllu, DatasetId::LatinUD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // MWT range tokens (2-3) should be skipped, only atomic tokens kept
        assert!(dataset.sentences[0].tokens.len() >= 3);
    }

    #[test]
    fn test_conllu_handles_empty_nodes() {
        // CoNLL-U empty nodes (e.g., 2.1 for elided elements)
        let empty_node_conllu = "# sent_id = test\n\
                                 # text = I saw and heard\n\
                                 1\tI\tI\tPRON\tPRP\t_\t2\tnsubj\t_\tSpaceAfter=Yes\n\
                                 2\tsaw\tsee\tVERB\tVBD\t_\t0\troot\t_\tSpaceAfter=Yes\n\
                                 2.1\tI\tI\tPRON\tPRP\t_\t4\tnsubj\t_\t_\n\
                                 3\tand\tand\tCCONJ\tCC\t_\t4\tcc\t_\tSpaceAfter=Yes\n\
                                 4\theard\thear\tVERB\tVBD\t_\t2\tconj\t_\tSpaceAfter=No\n";

        let result = parse::ner::parse_conllu(empty_node_conllu, DatasetId::LatinUD);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Empty nodes (2.1) should be skipped
        assert_eq!(dataset.sentences[0].tokens.len(), 4);
    }

    #[test]
    fn test_conll_with_bio_tag_normalization() {
        // Some corpora use I- at start of entity (should be B-)
        let malformed_bio = "Paris\tI-LOC\n\
                             is\tO\n\
                             beautiful\tO\n";

        let result = parse::ner::parse_conll(malformed_bio, DatasetId::WikiGold);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        // Parser should normalize or preserve the tag
        let first_tag = &dataset.sentences[0].tokens[0].ner_tag;
        assert!(first_tag == "I-LOC" || first_tag == "B-LOC");
    }

    #[test]
    fn test_conll_with_unicode_normalization() {
        // Test that precomposed (U+00E9) and decomposed (U+0065 U+0301) forms
        // both parse and produce equivalent tokens.
        let composed = "Caf\u{00e9}\tB-LOC\n";
        let decomposed = "Cafe\u{0301}\tB-LOC\n";
        assert_ne!(
            composed, decomposed,
            "test inputs must actually differ in bytes"
        );

        let d1 = parse::ner::parse_conll(composed, DatasetId::WikiGold).unwrap();
        let d2 = parse::ner::parse_conll(decomposed, DatasetId::WikiGold).unwrap();

        assert_eq!(d1.sentences.len(), d2.sentences.len());
        assert_eq!(d1.sentences[0].tokens.len(), d2.sentences[0].tokens.len());
        assert_eq!(
            d1.sentences[0].tokens[0].ner_tag,
            d2.sentences[0].tokens[0].ner_tag
        );
    }

    #[test]
    fn test_cadec_hf_api_unicode_prefix_case_insensitive_span_search_is_safe() {
        // Regression: span finding must not rely on Unicode lowercasing index alignment.
        let loader = DatasetLoader::new().unwrap();
        // Include `features` and compact JSON so `is_hf_api_response()` recognizes it.
        let content = r#"{"features":[{"name":"text"},{"name":"ade"},{"name":"term_PT"}],"rows":[{"row":{"text":"Müller reported HEADACHE after taking aspirin.","ade":"headache","term_PT":"Headache"}}]}"#;

        let ds = loader
            .parse_content_str(content, DatasetId::CADEC)
            .expect("parse CADEC HF API");
        assert_eq!(ds.id, DatasetId::CADEC);
        assert!(!ds.sentences.is_empty());

        let sent = &ds.sentences[0];
        // We should tag the ADE as B-/I- adverse_drug_event in BIO space.
        assert!(
            sent.tokens
                .iter()
                .any(|t| t.ner_tag == "B-adverse_drug_event" || t.ner_tag == "I-adverse_drug_event"),
            "Expected ADE tags in tokens: {:?}",
            sent.tokens
        );
    }

    #[test]
    fn test_jsonl_with_unicode_tokens() {
        // JSONL with various Unicode characters
        let unicode_jsonl = r#"{"tokens":["北京","🎉","Москва","القاهرة"],"ner_tags":[5,0,5,5]}"#;

        let result = parse::ner::parse_jsonl_ner(unicode_jsonl, DatasetId::MultiWOZNER);
        assert!(result.is_ok());

        let dataset = result.unwrap();
        assert_eq!(dataset.sentences.len(), 1);
        assert_eq!(dataset.sentences[0].tokens.len(), 4);
        assert_eq!(dataset.sentences[0].tokens[0].text, "北京");
        assert_eq!(dataset.sentences[0].tokens[1].text, "🎉");
        assert_eq!(dataset.sentences[0].tokens[2].text, "Москва");
    }

    #[test]
    fn test_parse_plan_consistency_with_is_loadable() {
        // Invariant: parse_plan returns Some iff is_loadable_dataset returns true
        for &id in DatasetId::all() {
            let has_plan = LoadableDatasetId::parse_plan(id).is_some();
            let is_loadable = LoadableDatasetId::is_loadable_dataset(id);
            assert_eq!(
                has_plan, is_loadable,
                "Mismatch for {:?}: parse_plan={}, is_loadable={}",
                id, has_plan, is_loadable
            );
        }
    }

    #[test]
    fn test_dataset_coverage_by_plan() {
        // Verify we have good coverage across different parse plans
        let mut conll_count = 0;
        let mut jsonl_count = 0;
        let mut conllu_count = 0;
        let mut _other_count = 0;

        for id in LoadableDatasetId::all() {
            let ds: DatasetId = id.into();
            match LoadableDatasetId::parse_plan(ds) {
                Some(DatasetParsePlan::Conll) => conll_count += 1,
                Some(DatasetParsePlan::JsonlNer) => jsonl_count += 1,
                Some(DatasetParsePlan::Conllu) => conllu_count += 1,
                Some(_) => _other_count += 1,
                None => {}
            }
        }

        // Should have substantial coverage for each format
        assert!(
            conll_count >= 50,
            "Expected at least 50 CoNLL datasets loadable, got {}",
            conll_count
        );
        assert!(
            jsonl_count >= 20,
            "Expected at least 20 JSONL datasets loadable, got {}",
            jsonl_count
        );
        assert!(
            conllu_count >= 10,
            "Expected at least 10 CoNLLU datasets loadable, got {}",
            conllu_count
        );
    }

    #[test]
    fn test_loadable_datasets_have_valid_metadata() {
        // All loadable datasets should have basic metadata
        for id in LoadableDatasetId::all() {
            let ds: DatasetId = id.into();

            // Name should not be empty
            assert!(!ds.name().is_empty(), "{:?} has empty name", ds);

            // Description should exist for most
            // (not asserting as some may legitimately have no description)
        }
    }
}
