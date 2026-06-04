use crate::ids::CorpusId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RootError {
    #[error("corpus root path is empty")]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusRoot {
    pub raw_path: String,
    pub normalized_path: String,
    pub label_pseudo: String,
}

impl CorpusRoot {
    pub fn from_raw(raw_path: impl Into<String>) -> Result<Self, RootError> {
        let raw_path = raw_path.into();
        let normalized_path = normalize_path(&raw_path)?;
        let corpus_id = CorpusId::from_normalized_root(&normalized_path);
        Ok(Self {
            raw_path,
            normalized_path,
            label_pseudo: format!("corpus_{}", &corpus_id.as_string()[..12]),
        })
    }
}

pub fn normalize_path(path: &str) -> Result<String, RootError> {
    let replaced = path.replace('\\', "/");
    let trimmed = replaced.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(RootError::Empty);
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub fn roots_overlap(a: &str, b: &str) -> bool {
    fn inside(child: &str, parent: &str) -> bool {
        child == parent
            || child
                .strip_prefix(parent)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }

    inside(a, b) || inside(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_collapses_windows_variants() {
        assert_eq!(
            normalize_path("C:\\Users\\Client\\Matter\\").unwrap(),
            "c:/users/client/matter"
        );
        assert_eq!(
            normalize_path(" c:/USERS/client/matter/// ").unwrap(),
            "c:/users/client/matter"
        );
    }

    #[test]
    fn normalize_path_rejects_empty_paths() {
        assert_eq!(normalize_path("  /  "), Err(RootError::Empty));
    }

    #[test]
    fn roots_overlap_detects_equal_and_nested_paths() {
        assert!(roots_overlap("c:/clients/acme", "c:/clients/acme"));
        assert!(roots_overlap("c:/clients/acme", "c:/clients/acme/sub"));
        assert!(roots_overlap("c:/clients/acme/sub", "c:/clients/acme"));
        assert!(!roots_overlap("c:/clients/acme", "c:/clients/acme2"));
        assert!(!roots_overlap("c:/clients/acme", "c:/clients/beta"));
    }
}
