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
pub(crate) use types::DatasetParsePlan;
pub use types::{
    AnnotatedSentence, AnnotatedToken, CacheManifest, CacheManifestEntry, DataSource,
    DatasetMetadata, DatasetStats, LoadableDatasetId, LoadedDataset, RelationDocument,
    TemporalMetadata,
};

mod acquire;
mod cache;
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
        cache::enforce_max_download_bytes(content.len(), "local cache (sync-s3)")?;
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
            if let Ok((content, manifest_entry)) = acquire::s3::download_from_s3(bucket, dataset_id)
            {
                cache::enforce_max_download_bytes(content.len(), "S3")?;
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
        cache::enforce_max_download_bytes(content.len(), &resolved_url)?;
        let file_size = content.len() as u64;
        let sha256 = cache::compute_sha256(&content);

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
        let _ = self.update_manifest(entry.clone()); // Best effort

        // 7. Optionally upload to S3 for future use (best effort)
        if let Some(ref bucket) = self.s3_bucket {
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

#[cfg(test)]
mod tests_a;
#[cfg(test)]
mod tests_b;
