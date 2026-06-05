//! Local folder discovery and content-hash change detection.

use crate::error::{LocalSourceError, Result};
use anno_knowledge_core::ObjectType;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Supported file extensions for the local folder source (lowercase, no dot).
const SUPPORTED_EXTS: &[&str] = &[
    "txt", "md", "markdown", "pdf", "doc", "docx", "rtf", "odt", "html", "htm", "csv", "tsv",
    "xlsx", "xls", "pptx", "ppt", "eml", "msg", "json", "xml",
];

/// Per-run discovery budget. Bounds work so a single `knowledge_sync` call
/// returns promptly on large folders; remaining files are picked up next run.
#[derive(Debug, Clone)]
pub struct DiscoverBudget {
    /// Maximum number of files returned in one run.
    pub max_files: usize,
    /// Maximum total bytes hashed in one run.
    pub max_total_bytes: u64,
}

impl Default for DiscoverBudget {
    fn default() -> Self {
        Self {
            max_files: 200,
            max_total_bytes: 512 * 1024 * 1024,
        }
    }
}

/// One discovered file. Pure data; the indexer assigns typed IDs and pseudonymizes.
#[derive(Debug, Clone)]
pub struct DiscoveredObject {
    /// Canonical absolute path used as the stable external id.
    pub external_id: String,
    /// Path on disk (same value, typed).
    pub path: PathBuf,
    /// Object family. Always `File` for this source.
    pub object_type: ObjectType,
    /// SHA-256 of the file bytes. Content-based revision identity.
    pub content_hash: [u8; 32],
    /// Last-modified time.
    pub mtime: DateTime<Utc>,
    /// File size in bytes.
    pub byte_size: u64,
    /// File name (raw; pseudonymized downstream by the indexer).
    pub title_raw: Option<String>,
    /// Raw metadata (raw; pseudonymized downstream): path, ext, size, mtime.
    pub metadata_raw: serde_json::Value,
}

/// A local folder configured as a knowledge source.
#[derive(Debug, Clone)]
pub struct LocalFolderSource {
    root: PathBuf,
}

impl LocalFolderSource {
    /// Create a source rooted at `root`.
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Walk the folder and return discovered objects, bounded by `budget`.
    ///
    /// Files are visited in a stable sorted order so budget truncation is
    /// deterministic and `knowledge_sync` resumes predictably.
    ///
    /// # Errors
    /// Returns [`LocalSourceError`] if the root is not a directory or IO fails.
    pub fn discover(&self, budget: &DiscoverBudget) -> Result<Vec<DiscoveredObject>> {
        if !self.root.is_dir() {
            return Err(LocalSourceError::NotADirectory(
                self.root.display().to_string(),
            ));
        }

        let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_entry(|e| !is_generated_anno_dir(&self.root, e.path()))
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(walkdir::DirEntry::into_path)
            .filter(|p| is_supported(p))
            .filter(|p| !is_generated_anno_file(p))
            .collect();
        paths.sort();

        let mut out = Vec::new();
        let mut bytes_used: u64 = 0;
        for path in paths {
            if out.len() >= budget.max_files {
                break;
            }
            let meta = std::fs::metadata(&path)?;
            let size = meta.len();
            let remaining_bytes = budget.max_total_bytes.saturating_sub(bytes_used);
            if size > remaining_bytes {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            let content_hash = sha256(&bytes);
            let mtime: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            let external_id = canonical_id(&path);
            let title_raw = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string);
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let metadata_raw = serde_json::json!({
                "path": external_id,
                "ext": ext,
                "size": size,
                "mtime": mtime.to_rfc3339(),
            });

            bytes_used = bytes_used.saturating_add(size);
            out.push(DiscoveredObject {
                external_id: canonical_id(&path),
                path,
                object_type: ObjectType::File,
                content_hash,
                mtime,
                byte_size: size,
                title_raw,
                metadata_raw,
            });
        }
        Ok(out)
    }
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|e| SUPPORTED_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_generated_anno_dir(root: &Path, path: &Path) -> bool {
    if path == root {
        return false;
    }
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_generated_anno_dir_name)
    {
        return true;
    }
    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            relative.components().any(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(is_generated_anno_dir_name)
            })
        })
        .unwrap_or(false)
}

fn is_generated_anno_dir_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "anon" | "outputs" | ".anno" | ".anno-rag"
    )
}

fn is_generated_anno_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().contains(".anon."))
        .unwrap_or(false)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn canonical_id(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).expect("write fixture");
        p
    }

    #[test]
    fn discovers_supported_files_and_skips_unsupported() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "a.txt", b"hello world");
        write(dir.path(), "b.md", b"# title\nbody");
        write(dir.path(), "ignore.bin", b"\x00\x01\x02");

        let src = LocalFolderSource::new(dir.path());
        let objects = src.discover(&DiscoverBudget::default()).expect("discover");

        let names: Vec<String> = objects
            .iter()
            .map(|o| {
                o.external_id
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(names.iter().any(|n| n == "a.txt"));
        assert!(names.iter().any(|n| n == "b.md"));
        assert!(!names.iter().any(|n| n == "ignore.bin"));
    }

    #[test]
    fn skips_anno_generated_outputs() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "source.md", b"# source");
        fs::create_dir_all(dir.path().join("anon")).expect("anon dir");
        fs::write(
            dir.path().join("anon").join("source.anon.md"),
            b"# generated",
        )
        .expect("generated output");
        fs::create_dir_all(dir.path().join("nested")).expect("nested dir");
        fs::write(
            dir.path().join("nested").join("report.anon.md"),
            b"# generated nested",
        )
        .expect("generated nested output");
        fs::create_dir_all(dir.path().join("nested").join("anon")).expect("nested anon dir");
        fs::write(
            dir.path()
                .join("nested")
                .join("anon")
                .join("source.md"),
            b"# generated nested anon",
        )
        .expect("generated nested anon");
        fs::create_dir_all(dir.path().join("outputs")).expect("outputs dir");
        fs::write(
            dir.path().join("outputs").join("export.md"),
            b"# generated export",
        )
        .expect("generated export");
        fs::create_dir_all(dir.path().join(".anno")).expect(".anno dir");
        fs::write(
            dir.path().join(".anno").join("state.md"),
            b"# generated state",
        )
        .expect(".anno state");
        fs::create_dir_all(dir.path().join(".anno-rag")).expect(".anno-rag dir");
        fs::write(
            dir.path().join(".anno-rag").join("state.md"),
            b"# generated rag state",
        )
        .expect(".anno-rag state");

        let src = LocalFolderSource::new(dir.path());
        let objects = src.discover(&DiscoverBudget::default()).expect("discover");

        let names: Vec<String> = objects
            .iter()
            .map(|o| {
                o.external_id
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["source.md"]);
    }

    #[test]
    fn content_hash_is_stable_for_same_bytes_and_changes_on_edit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = write(dir.path(), "a.txt", b"version one");
        let src = LocalFolderSource::new(dir.path());

        let first = src.discover(&DiscoverBudget::default()).expect("discover");
        let h1 = first[0].content_hash;

        // Touch only: rewrite identical bytes -> same hash.
        fs::write(&p, b"version one").expect("rewrite");
        let second = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_eq!(second[0].content_hash, h1);

        // Real edit -> different hash.
        fs::write(&p, b"version two!!").expect("edit");
        let third = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_ne!(third[0].content_hash, h1);
    }

    #[test]
    fn budget_caps_file_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..10 {
            write(dir.path(), &format!("f{i}.txt"), b"x");
        }
        let src = LocalFolderSource::new(dir.path());
        let budget = DiscoverBudget {
            max_files: 3,
            max_total_bytes: u64::MAX,
        };
        let objects = src.discover(&budget).expect("discover");
        assert_eq!(objects.len(), 3);
    }

    #[test]
    fn budget_skips_oversized_files_and_continues() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "a-large.txt", b"too large for this budget");
        write(dir.path(), "b-small.txt", b"x");

        let src = LocalFolderSource::new(dir.path());
        let budget = DiscoverBudget {
            max_files: 10,
            max_total_bytes: 4,
        };
        let objects = src.discover(&budget).expect("discover");

        assert_eq!(objects.len(), 1);
        assert!(objects[0].external_id.ends_with("b-small.txt"));
    }

    #[test]
    fn external_id_is_canonical_and_stable() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "a.txt", b"x");
        let src = LocalFolderSource::new(dir.path());
        let a = src.discover(&DiscoverBudget::default()).expect("discover");
        let b = src.discover(&DiscoverBudget::default()).expect("discover");
        assert_eq!(a[0].external_id, b[0].external_id);
    }
}
