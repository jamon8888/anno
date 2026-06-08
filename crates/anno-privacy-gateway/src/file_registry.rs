//! Local file registry for gateway-managed uploaded documents.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use time::OffsetDateTime;

/// Gateway-managed local file id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFileId(String);

impl StoredFileId {
    /// Generate a new gateway-managed file id.
    #[must_use]
    pub fn new() -> Self {
        let id = uuid::Uuid::now_v7().simple().to_string();
        Self(format!("anno_file_{id}"))
    }

    /// Parse and validate a gateway-managed file id.
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if !value.starts_with("anno_file_") {
            return Err("file id must start with anno_file_".to_string());
        }
        let suffix = value.trim_start_matches("anno_file_");
        if suffix.len() != 32 || !suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err("file id suffix must be a 32-character hex UUID".to_string());
        }
        Ok(Self(value.to_string()))
    }

    /// Return the id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for StoredFileId {
    fn default() -> Self {
        Self::new()
    }
}

/// File registry runtime settings.
#[derive(Debug, Clone)]
pub struct FileRegistryConfig {
    /// Root directory for file metadata and derivatives.
    pub root: PathBuf,
    /// Retain raw uploaded bytes after text extraction.
    pub retain_raw: bool,
    /// Retain cleartext extracted text for DPA/local cleartext modes.
    pub retain_cleartext: bool,
}

/// Local registry storing metadata and file text derivatives.
#[derive(Debug, Clone)]
pub struct FileRegistry {
    config: FileRegistryConfig,
}

/// Stored file metadata. It intentionally contains no file text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile {
    /// Gateway-managed file id.
    pub id: StoredFileId,
    /// Original filename from the upload request.
    pub filename: String,
    /// Uploaded content type.
    pub content_type: String,
    /// Original byte size.
    pub size_bytes: usize,
    /// SHA-256 of the original uploaded bytes.
    pub sha256_hex: String,
    /// Unix timestamp for upload creation.
    pub created_at_unix: i64,
    /// Metadata path on disk.
    pub metadata_path: PathBuf,
    /// Pseudonymized text derivative path on disk.
    pub pseudonymized_text_path: PathBuf,
    /// Optional cleartext text derivative path on disk.
    pub cleartext_text_path: Option<PathBuf>,
    /// Optional raw upload path on disk.
    pub raw_path: Option<PathBuf>,
}

impl FileRegistry {
    /// Build a local registry.
    #[must_use]
    pub fn new(config: FileRegistryConfig) -> Self {
        Self { config }
    }

    /// Store metadata plus text derivatives for an uploaded file.
    pub async fn put_text_derivatives(
        &self,
        filename: &str,
        content_type: &str,
        raw_bytes: &[u8],
        cleartext_text: &str,
        pseudonymized_text: &str,
    ) -> Result<StoredFile> {
        let id = StoredFileId::new();
        let dir = self.file_dir(id.as_str());
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| Error::Privacy(format!("create file registry dir: {e}")))?;

        let metadata_path = dir.join("metadata.json");
        let pseudonymized_text_path = dir.join("pseudonymized.txt");
        let cleartext_text_path = self
            .config
            .retain_cleartext
            .then(|| dir.join("cleartext.txt"));
        let raw_path = self.config.retain_raw.then(|| dir.join("raw.bin"));

        tokio::fs::write(&pseudonymized_text_path, pseudonymized_text)
            .await
            .map_err(|e| Error::Privacy(format!("write pseudonymized file text: {e}")))?;
        if let Some(path) = &cleartext_text_path {
            tokio::fs::write(path, cleartext_text)
                .await
                .map_err(|e| Error::Privacy(format!("write cleartext file text: {e}")))?;
        }
        if let Some(path) = &raw_path {
            tokio::fs::write(path, raw_bytes)
                .await
                .map_err(|e| Error::Privacy(format!("write raw uploaded file: {e}")))?;
        }

        let sha256_hex = hex::encode(Sha256::digest(raw_bytes));
        let metadata = StoredFile {
            id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size_bytes: raw_bytes.len(),
            sha256_hex,
            created_at_unix: OffsetDateTime::now_utc().unix_timestamp(),
            metadata_path: metadata_path.clone(),
            pseudonymized_text_path,
            cleartext_text_path,
            raw_path,
        };
        let json = serde_json::to_vec_pretty(&metadata)
            .map_err(|e| Error::Privacy(format!("serialize file metadata: {e}")))?;
        tokio::fs::write(&metadata_path, json)
            .await
            .map_err(|e| Error::Privacy(format!("write file metadata: {e}")))?;
        Ok(metadata)
    }

    /// Load stored metadata by id.
    pub async fn get(&self, id: &str) -> Result<StoredFile> {
        let parsed = StoredFileId::parse(id).map_err(Error::Privacy)?;
        let path = self.file_dir(parsed.as_str()).join("metadata.json");
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| Error::Privacy(format!("read file metadata: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Privacy(format!("parse file metadata: {e}")))
    }

    /// Read the pseudonymized text derivative by id.
    pub async fn read_pseudonymized_text(&self, id: &str) -> Result<String> {
        let stored = self.get(id).await?;
        tokio::fs::read_to_string(&stored.pseudonymized_text_path)
            .await
            .map_err(|e| Error::Privacy(format!("read pseudonymized file text: {e}")))
    }

    /// Read the optional cleartext text derivative by id.
    pub async fn read_cleartext_text(&self, id: &str) -> Result<Option<String>> {
        let stored = self.get(id).await?;
        let Some(path) = stored.cleartext_text_path else {
            return Ok(None);
        };
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Privacy(format!("read cleartext file text: {e}")))?;
        Ok(Some(text))
    }

    /// Delete all local derivatives and metadata for a gateway file id.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let parsed = StoredFileId::parse(id).map_err(Error::Privacy)?;
        let dir = self.file_dir(parsed.as_str());
        if !dir.exists() {
            return Ok(false);
        }
        tokio::fs::remove_dir_all(dir)
            .await
            .map_err(|e| Error::Privacy(format!("delete stored file: {e}")))?;
        Ok(true)
    }

    fn file_dir(&self, id: &str) -> PathBuf {
        self.config.root.join(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_has_anno_prefix() {
        let id = StoredFileId::new();
        assert!(id.as_str().starts_with("anno_file_"));
        assert!(StoredFileId::parse(id.as_str()).is_ok());
        assert!(StoredFileId::parse("file_provider_123").is_err());
    }

    #[tokio::test]
    async fn registry_persists_metadata_and_derivatives() {
        let tmp = tempfile::TempDir::new().unwrap();
        let registry = FileRegistry::new(FileRegistryConfig {
            root: tmp.path().to_path_buf(),
            retain_raw: false,
            retain_cleartext: true,
        });

        let stored = registry
            .put_text_derivatives(
                "notes.txt",
                "text/plain",
                b"Bonjour Marie Dupont",
                "Bonjour Marie Dupont",
                "Bonjour PERSON_1",
            )
            .await
            .expect("store");

        let loaded = registry.get(stored.id.as_str()).await.expect("load");
        assert_eq!(loaded.filename, "notes.txt");
        assert_eq!(loaded.content_type, "text/plain");
        assert!(registry
            .read_pseudonymized_text(stored.id.as_str())
            .await
            .unwrap()
            .contains("PERSON_1"));
        assert!(registry
            .read_cleartext_text(stored.id.as_str())
            .await
            .unwrap()
            .unwrap()
            .contains("Marie Dupont"));
        assert!(loaded.raw_path.is_none());
    }
}
