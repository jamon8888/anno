//! Filesystem allowlist for local MCP path-taking tools.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Environment variable containing semicolon-separated absolute root paths.
pub(crate) const ALLOWED_ROOTS_ENV: &str = "ANNO_RAG_ALLOWED_ROOTS";

/// Safe summary returned by status tools.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AllowedRootsSummary {
    /// Whether root enforcement is active.
    pub(crate) enforced: bool,
    /// Number of configured allowed roots. Root paths are intentionally not
    /// exposed in MCP status responses.
    pub(crate) root_count: usize,
}

/// Canonical filesystem roots that MCP path inputs may access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AllowedRoots {
    roots: Vec<PathBuf>,
    literal_roots: Vec<PathBuf>,
    deny_all: bool,
}

impl AllowedRoots {
    /// Build from `ANNO_RAG_ALLOWED_ROOTS`.
    pub(crate) fn from_env() -> Result<Self, String> {
        match std::env::var(ALLOWED_ROOTS_ENV) {
            Ok(raw) => Self::parse(Some(raw.as_str())),
            Err(std::env::VarError::NotPresent) => Self::parse(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(format!("{ALLOWED_ROOTS_ENV} must be valid Unicode"))
            }
        }
    }

    /// Build a policy that rejects every path. Used when env config is invalid.
    #[must_use]
    pub(crate) fn deny_all() -> Self {
        Self {
            roots: Vec::new(),
            literal_roots: Vec::new(),
            deny_all: true,
        }
    }

    /// Parse semicolon-separated roots. Empty or missing value is permissive.
    pub(crate) fn parse(raw: Option<&str>) -> Result<Self, String> {
        let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self::default());
        };

        let mut roots = Vec::new();
        let mut literal_roots = Vec::new();
        for part in raw
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let path = PathBuf::from(part);
            if !path.is_absolute() {
                return Err(format!(
                    "{ALLOWED_ROOTS_ENV} entry must be absolute: {part}"
                ));
            }
            let canonical = std::fs::canonicalize(&path)
                .map_err(|e| format!("canonicalize allowed root {}: {e}", path.display()))?;
            roots.push(canonical);
            literal_roots.push(path);
        }

        if roots.is_empty() {
            return Ok(Self::default());
        }

        roots.sort();
        roots.dedup();
        literal_roots.sort();
        literal_roots.dedup();
        Ok(Self {
            roots,
            literal_roots,
            deny_all: false,
        })
    }

    /// Return true when root enforcement is active.
    #[must_use]
    pub(crate) fn is_enforced(&self) -> bool {
        self.deny_all || !self.roots.is_empty()
    }

    /// Return a safe status summary with no user-provided input path.
    #[must_use]
    pub(crate) fn summary(&self) -> AllowedRootsSummary {
        AllowedRootsSummary {
            enforced: self.is_enforced(),
            root_count: self.roots.len(),
        }
    }

    /// Validate an existing input path for a read/index/finalize operation.
    pub(crate) fn validate_existing_path(
        &self,
        label: &str,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, String> {
        let path = path.as_ref();
        if !self.is_enforced() {
            return Ok(path.to_path_buf());
        }

        if !path.is_absolute() {
            return Err(format!(
                "{label} must be an absolute path: {}",
                path.display()
            ));
        }

        let canonical = std::fs::canonicalize(path)
            .map_err(|e| format!("canonicalize {label} {}: {e}", path.display()))?;

        if !self.contains(&canonical) {
            return Err(format!(
                "{label} is outside {ALLOWED_ROOTS_ENV}: {}",
                path.display()
            ));
        }

        Ok(canonical)
    }

    /// Validate a metadata-cleanup path that may already have been deleted.
    pub(crate) fn validate_existing_or_removed_path(
        &self,
        label: &str,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, String> {
        let path = path.as_ref();
        if !self.is_enforced() {
            return Ok(path.to_path_buf());
        }

        if !path.is_absolute() {
            return Err(format!(
                "{label} must be an absolute path: {}",
                path.display()
            ));
        }

        match path.try_exists() {
            Ok(true) => return self.validate_existing_path(label, path),
            Ok(false) => {}
            Err(e) => return Err(format!("stat {label} {}: {e}", path.display())),
        }

        if self.is_configured_root_literal(path) {
            return Ok(path.to_path_buf());
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| format!("{label} must have a parent directory: {}", path.display()))?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("canonicalize {label} parent {}: {e}", parent.display()))?;
        if !self.contains(&canonical_parent) {
            return Err(format!(
                "{label} is outside {ALLOWED_ROOTS_ENV}: {}",
                path.display()
            ));
        }

        let file_name = path
            .file_name()
            .ok_or_else(|| format!("{label} must include a final component: {}", path.display()))?;
        Ok(canonical_parent.join(file_name))
    }

    /// Validate a write destination whose parent directory already exists.
    pub(crate) fn validate_output_path(
        &self,
        label: &str,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf, String> {
        let path = path.as_ref();
        if !self.is_enforced() {
            return Ok(path.to_path_buf());
        }

        if !path.is_absolute() {
            return Err(format!(
                "{label} must be an absolute path: {}",
                path.display()
            ));
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| format!("{label} must have a parent directory: {}", path.display()))?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("canonicalize {label} parent {}: {e}", parent.display()))?;
        if !self.contains(&canonical_parent) {
            return Err(format!(
                "{label} is outside {ALLOWED_ROOTS_ENV}: {}",
                path.display()
            ));
        }

        let file_name = path
            .file_name()
            .ok_or_else(|| format!("{label} must include a file name: {}", path.display()))?;
        let output = canonical_parent.join(file_name);
        reject_existing_symlink(label, &output)?;
        Ok(output)
    }

    fn contains(&self, canonical: &Path) -> bool {
        if self.deny_all {
            return false;
        }

        self.roots
            .iter()
            .any(|root| canonical == root || canonical.starts_with(root))
    }

    fn is_configured_root_literal(&self, path: &Path) -> bool {
        if self.deny_all {
            return false;
        }

        self.roots.iter().any(|root| path == root)
            || self.literal_roots.iter().any(|root| path == root)
    }
}

#[cfg(unix)]
fn reject_existing_symlink(label: &str, path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} must not target an existing symlink: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn reject_existing_symlink(label: &str, path: &Path) -> Result<(), String> {
    use std::os::windows::fs::FileTypeExt;

    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() || file_type.is_symlink_file() || file_type.is_symlink_dir() {
        return Err(format!(
            "{label} must not target an existing symlink: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn reject_existing_symlink(_label: &str, _path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn allowed_roots_unset_is_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        assert!(!policy.is_enforced());
        assert_eq!(policy.summary().enforced, false);
        assert_eq!(policy.summary().root_count, 0);
    }

    #[test]
    fn allowed_roots_unset_keeps_relative_paths_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        let validated = policy
            .validate_existing_path("path", "relative-folder")
            .expect("permissive when unset");
        assert_eq!(validated, std::path::PathBuf::from("relative-folder"));
    }

    #[test]
    fn allowed_roots_unset_keeps_relative_output_paths_permissive() {
        let policy = AllowedRoots::parse(None).expect("parse");
        let validated = policy
            .validate_output_path("output_path", "relative-review.xlsx")
            .expect("permissive when unset");
        assert_eq!(validated, std::path::PathBuf::from("relative-review.xlsx"));
    }

    #[test]
    fn allowed_roots_accepts_path_inside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let client = dir.path().join("client-a");
        let matter = client.join("matter-1");
        std::fs::create_dir_all(&matter).expect("matter dir");

        let raw = client.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let validated = policy
            .validate_existing_path("source_root", &matter)
            .expect("inside root");

        assert!(validated.starts_with(std::fs::canonicalize(&client).expect("canonical root")));
    }

    #[test]
    fn allowed_roots_rejects_path_outside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", &outside)
            .expect_err("outside root rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[test]
    fn allowed_roots_rejects_relative_input_paths_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_path("source_root", "relative-folder")
            .expect_err("relative path rejected");

        assert!(err.contains("absolute"));
    }

    #[test]
    fn allowed_roots_validates_output_parent_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let parent = allowed.join("exports");
        std::fs::create_dir_all(&parent).expect("export dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let output = parent.join("review.xlsx");
        let validated = policy
            .validate_output_path("output_path", &output)
            .expect("inside root");

        assert_eq!(
            validated.file_name().and_then(|s| s.to_str()),
            Some("review.xlsx")
        );
    }

    #[test]
    fn allowed_roots_validates_removed_path_parent_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let removed = allowed.join("removed-folder");
        let validated = policy
            .validate_existing_or_removed_path("target", &removed)
            .expect("removed target inside root");

        assert_eq!(
            validated.file_name().and_then(|s| s.to_str()),
            Some("removed-folder")
        );
    }

    #[test]
    fn allowed_roots_validates_removed_root_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        std::fs::create_dir_all(&allowed).expect("allowed dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        std::fs::remove_dir(&allowed).expect("remove allowed root");
        let validated = policy
            .validate_existing_or_removed_path("target", &allowed)
            .expect("removed root target");

        assert_eq!(validated, allowed);
    }

    #[test]
    fn allowed_roots_rejects_removed_path_outside_configured_root() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed dir");
        std::fs::create_dir_all(&outside).expect("outside dir");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_existing_or_removed_path("target", outside.join("removed-folder"))
            .expect_err("outside removed target rejected");

        assert!(err.contains("outside ANNO_RAG_ALLOWED_ROOTS"));
    }

    #[cfg(unix)]
    #[test]
    fn allowed_roots_rejects_existing_output_symlink_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let parent = allowed.join("exports");
        let outside = dir.path().join("outside.xlsx");
        std::fs::create_dir_all(&parent).expect("export dir");
        std::fs::write(&outside, "outside").expect("outside file");
        let output = parent.join("review.xlsx");
        std::os::unix::fs::symlink(&outside, &output).expect("symlink");

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_output_path("output_path", &output)
            .expect_err("symlink rejected");

        assert!(err.contains("symlink"));
    }

    #[cfg(windows)]
    #[test]
    fn allowed_roots_rejects_existing_output_symlink_when_enforced() {
        let dir = TempDir::new().expect("tempdir");
        let allowed = dir.path().join("allowed");
        let parent = allowed.join("exports");
        let outside = dir.path().join("outside.xlsx");
        std::fs::create_dir_all(&parent).expect("export dir");
        std::fs::write(&outside, "outside").expect("outside file");
        let output = parent.join("review.xlsx");
        if std::os::windows::fs::symlink_file(&outside, &output).is_err() {
            return;
        }

        let raw = allowed.to_string_lossy().to_string();
        let policy = AllowedRoots::parse(Some(&raw)).expect("parse");
        let err = policy
            .validate_output_path("output_path", &output)
            .expect_err("symlink rejected");

        assert!(err.contains("symlink"));
    }

    #[test]
    fn allowed_roots_deny_all_rejects_every_path_and_is_enforced() {
        let policy = AllowedRoots::deny_all();
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("note.txt");
        std::fs::write(&file, "hello").expect("write file");

        assert!(policy.is_enforced());
        assert!(policy.summary().enforced);
        assert_eq!(policy.summary().root_count, 0);
        assert!(policy.validate_existing_path("path", &file).is_err());
        assert!(policy.validate_output_path("output", &file).is_err());
    }

    #[test]
    fn allowed_roots_rejects_relative_configured_roots() {
        let err = AllowedRoots::parse(Some("relative/root")).expect_err("relative root rejected");

        assert!(err.contains(ALLOWED_ROOTS_ENV));
        assert!(err.contains("absolute"));
    }

    #[test]
    fn allowed_roots_rejects_missing_configured_roots() {
        let dir = TempDir::new().expect("tempdir");
        let missing = dir.path().join("missing");

        let err = AllowedRoots::parse(Some(&missing.display().to_string()))
            .expect_err("missing root rejected");

        assert!(err.contains("canonicalize allowed root"));
    }
}
