//! Local privacy workspace artifacts and path helpers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Workspace folder name created under the user-selected source root.
pub const PRIVACY_WORKSPACE_DIR: &str = "vault";
/// Editable cleartext Word review documents.
pub const WORKING_DIR: &str = "01-working-documents";
/// PII-safe Word documents generated for sharing and indexing.
pub const ANONYMIZED_DIR: &str = "02-anonymized-documents";
/// Local reports.
pub const REPORTS_DIR: &str = "03-reports";
/// Local cache and non-user-facing support data.
pub const CACHE_DIR: &str = "04-cache";
/// Manifest file name.
pub const MANIFEST_FILE: &str = "manifest.json";

/// All generated paths for one privacy workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyWorkspacePaths {
    /// User-selected source root.
    pub source_root: PathBuf,
    /// Generated workspace root named `vault`.
    pub workspace: PathBuf,
    /// Editable cleartext normalized Word documents.
    pub working: PathBuf,
    /// Anonymized Word documents.
    pub anonymized: PathBuf,
    /// Report folder.
    pub reports: PathBuf,
    /// Cache folder.
    pub cache: PathBuf,
    /// Manifest path.
    pub manifest: PathBuf,
}

impl PrivacyWorkspacePaths {
    /// Build generated paths from a source root.
    #[must_use]
    pub fn from_source_root(source_root: impl AsRef<Path>) -> Self {
        let source_root = source_root.as_ref().to_path_buf();
        let workspace = source_root.join(PRIVACY_WORKSPACE_DIR);
        Self {
            source_root,
            working: workspace.join(WORKING_DIR),
            anonymized: workspace.join(ANONYMIZED_DIR),
            reports: workspace.join(REPORTS_DIR),
            cache: workspace.join(CACHE_DIR),
            manifest: workspace.join(MANIFEST_FILE),
            workspace,
        }
    }

    /// Create all generated directories.
    ///
    /// # Errors
    /// Returns IO errors if the workspace cannot be created.
    pub fn create_all(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.working)?;
        std::fs::create_dir_all(&self.anonymized)?;
        std::fs::create_dir_all(&self.reports)?;
        std::fs::create_dir_all(&self.cache)?;
        Ok(())
    }
}

/// One document tracked in `vault/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestDocument {
    /// Stable UUID string for this document.
    pub document_id: String,
    /// Relative path from source root.
    pub relative_path: String,
    /// Original source path. Local-only operational metadata.
    pub source_path: String,
    /// Generated working `.docx` path.
    pub working_path: String,
    /// Generated anonymized `.docx` path.
    pub anonymized_path: String,
    /// SHA-256 of source bytes.
    pub source_hash: String,
    /// SHA-256 of latest working bytes.
    pub working_hash: Option<String>,
    /// SHA-256 used for the last index write.
    pub last_indexed_hash: Option<String>,
    /// Current document status.
    pub status: String,
    /// Safe aggregate counts.
    pub counts: ManifestCounts,
}

/// Safe per-document counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestCounts {
    /// Detector findings before user decisions.
    pub detected: usize,
    /// User-forced mask instructions.
    pub forced_mask: usize,
    /// User-reviewed visible exceptions.
    pub kept_visible: usize,
}

/// Workspace manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceManifest {
    /// Manifest schema version.
    pub version: u32,
    /// Source root path string.
    pub source_root: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Documents tracked in this workspace.
    pub documents: Vec<ManifestDocument>,
}

impl WorkspaceManifest {
    /// Create an empty manifest for a source root.
    #[must_use]
    pub fn new(source_root: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            version: 1,
            source_root: source_root.into(),
            created_at: now,
            updated_at: now,
            documents: Vec::new(),
        }
    }
}

/// Return true for any generated `vault` path under the source root.
#[must_use]
pub fn is_privacy_generated_path(source_root: &Path, path: &Path) -> bool {
    if path.strip_prefix(source_root).ok().is_some_and(|relative| {
        relative.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(PRIVACY_WORKSPACE_DIR))
        })
    }) {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(PRIVACY_WORKSPACE_DIR))
}

/// SHA-256 hex of bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
