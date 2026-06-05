//! Orchestration for the local `vault` privacy workspace.

use crate::error::Result;
use crate::privacy_artifacts::{PrivacyWorkspacePaths, WorkspaceManifest};
use std::path::{Path, PathBuf};

/// Generated paths for one source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyDocumentOutputPaths {
    /// Normalized relative source path.
    pub relative_path: String,
    /// Cleartext working docx path.
    pub working_path: PathBuf,
    /// Anonymized docx path.
    pub anonymized_path: PathBuf,
}

/// Safe prepare summary returned to MCP/CLI callers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrivacyPrepareSummary {
    /// Workspace path.
    pub workspace: String,
    /// Working folder path.
    pub working_folder: String,
    /// Anonymized folder path.
    pub anonymized_folder: String,
    /// Reports folder path.
    pub reports_folder: String,
    /// Source files seen.
    pub documents_seen: usize,
    /// Documents prepared.
    pub documents_prepared: usize,
    /// Documents failed.
    pub documents_failed: usize,
}

/// Safe finalize summary returned to MCP/CLI callers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrivacyFinalizeSummary {
    /// Workspace path.
    pub workspace: String,
    /// Changed working docs.
    pub documents_changed: usize,
    /// Reindexed docs.
    pub documents_reindexed: usize,
    /// Manual mask decisions.
    pub to_mask: usize,
    /// Manual keep decisions.
    pub to_keep: usize,
    /// Anonymized folder path.
    pub anonymized_folder: String,
    /// Shareable report path.
    pub shareable_report: String,
}

/// Build mirrored working/anonymized output paths for one source path.
#[must_use]
pub fn build_relative_output_paths(
    paths: &PrivacyWorkspacePaths,
    source_path: &Path,
) -> PrivacyDocumentOutputPaths {
    let relative = source_path
        .strip_prefix(&paths.source_root)
        .unwrap_or(source_path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");

    let mut relative_docx = PathBuf::from(&relative);
    relative_docx.set_extension("docx");

    let mut relative_anon = PathBuf::from(&relative);
    let stem = relative_anon
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document")
        .to_string();
    relative_anon.set_file_name(format!("{stem}.anon.docx"));

    PrivacyDocumentOutputPaths {
        relative_path: relative,
        working_path: paths.working.join(relative_docx),
        anonymized_path: paths.anonymized.join(relative_anon),
    }
}

/// Write a manifest to disk.
///
/// # Errors
/// Returns IO or JSON serialization errors wrapped as privacy errors.
pub fn write_manifest(path: &Path, manifest: &WorkspaceManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| crate::Error::Privacy(format!("serialize manifest: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Prepare a folder privacy workspace.
///
/// # Errors
/// Returns IO or manifest serialization errors.
pub async fn prepare_folder(
    _pipeline: &crate::pipeline::Pipeline,
    source_root: &Path,
    _recursive: bool,
) -> Result<PrivacyPrepareSummary> {
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    paths.create_all()?;
    let manifest = WorkspaceManifest::new(source_root.display().to_string());
    write_manifest(&paths.manifest, &manifest)?;
    Ok(PrivacyPrepareSummary {
        workspace: paths.workspace.display().to_string(),
        working_folder: paths.working.display().to_string(),
        anonymized_folder: paths.anonymized.display().to_string(),
        reports_folder: paths.reports.display().to_string(),
        documents_seen: 0,
        documents_prepared: 0,
        documents_failed: 0,
    })
}

/// Finalize a folder privacy workspace.
///
/// # Errors
/// Reserved for future finalize implementation failures.
pub async fn finalize_folder(
    _pipeline: &crate::pipeline::Pipeline,
    workspace: &Path,
) -> Result<PrivacyFinalizeSummary> {
    Ok(PrivacyFinalizeSummary {
        workspace: workspace.display().to_string(),
        documents_changed: 0,
        documents_reindexed: 0,
        to_mask: 0,
        to_keep: 0,
        anonymized_folder: workspace
            .join(crate::privacy_artifacts::ANONYMIZED_DIR)
            .display()
            .to_string(),
        shareable_report: workspace
            .join(crate::privacy_artifacts::REPORTS_DIR)
            .join("shareable_report.docx")
            .display()
            .to_string(),
    })
}
