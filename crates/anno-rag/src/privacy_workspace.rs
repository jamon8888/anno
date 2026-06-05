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
/// Returns privacy, extraction, detection, vault, or IO errors.
pub async fn prepare_folder(
    pipeline: &crate::pipeline::Pipeline,
    source_root: &Path,
    recursive: bool,
) -> Result<PrivacyPrepareSummary> {
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    paths.create_all()?;

    let mut manifest = WorkspaceManifest::new(source_root.display().to_string());
    let source_paths =
        crate::pipeline::privacy_candidate_paths(source_root, recursive, &paths.workspace);
    let mut prepared = 0usize;
    let mut failed = 0usize;

    for source_path in &source_paths {
        match prepare_one_document(pipeline, &paths, source_path).await {
            Ok(document) => {
                prepared += 1;
                manifest.documents.push(document);
            }
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    path = %source_path.display(),
                    error = %e,
                    "privacy prepare skipped document"
                );
            }
        }
    }

    manifest.updated_at = chrono::Utc::now();
    write_manifest(&paths.manifest, &manifest)?;
    crate::privacy_docx::write_shareable_report(
        &paths.reports.join("shareable_report.docx"),
        "Privacy Workspace Report",
        &[
            ("Documents seen".to_string(), source_paths.len().to_string()),
            ("Documents prepared".to_string(), prepared.to_string()),
            ("Documents failed".to_string(), failed.to_string()),
        ],
    )?;

    Ok(PrivacyPrepareSummary {
        workspace: paths.workspace.display().to_string(),
        working_folder: paths.working.display().to_string(),
        anonymized_folder: paths.anonymized.display().to_string(),
        reports_folder: paths.reports.display().to_string(),
        documents_seen: source_paths.len(),
        documents_prepared: prepared,
        documents_failed: failed,
    })
}

async fn prepare_one_document(
    pipeline: &crate::pipeline::Pipeline,
    paths: &PrivacyWorkspacePaths,
    source_path: &Path,
) -> Result<crate::privacy_artifacts::ManifestDocument> {
    let extracted = crate::ingest::extract(source_path, pipeline.config()).await?;
    let out_paths = build_relative_output_paths(paths, source_path);

    let working_doc = crate::privacy_docx::NormalizedDocx {
        title: out_paths.relative_path.clone(),
        metadata: vec![
            ("Source".to_string(), out_paths.relative_path.clone()),
            ("Extraction".to_string(), "Kreuzberg".to_string()),
        ],
        sections: extracted
            .chunks
            .iter()
            .map(|chunk| crate::privacy_docx::NormalizedSection {
                heading: format!("Chunk {}", chunk.idx + 1),
                body: chunk.text.clone(),
            })
            .collect(),
    };
    crate::privacy_docx::write_normalized_docx(&out_paths.working_path, &working_doc)?;

    let detector = pipeline.detector_for_privacy()?;
    let no_legal = Vec::new();
    let no_thresholds = std::collections::HashMap::new();
    let mut pseudo_sections = Vec::new();
    let mut detected_count = 0usize;
    for chunk in &extracted.chunks {
        let bundle = detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
        detected_count += bundle.pii.len();
        let report = pipeline
            .vault_for_privacy()
            .pseudonymize_with_report_map(&chunk.text, &bundle.pii)
            .await?;
        pseudo_sections.push(crate::privacy_docx::NormalizedSection {
            heading: format!("Chunk {}", chunk.idx + 1),
            body: report.text,
        });
    }
    let anonymized_doc = crate::privacy_docx::NormalizedDocx {
        title: out_paths.relative_path.clone(),
        metadata: vec![
            ("Source".to_string(), out_paths.relative_path.clone()),
            ("PII".to_string(), "Anonymized".to_string()),
        ],
        sections: pseudo_sections,
    };
    crate::privacy_docx::write_normalized_docx(&out_paths.anonymized_path, &anonymized_doc)?;

    let source_bytes = std::fs::read(source_path)?;
    let working_bytes = std::fs::read(&out_paths.working_path)?;
    Ok(crate::privacy_artifacts::ManifestDocument {
        document_id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &source_bytes).to_string(),
        relative_path: out_paths.relative_path,
        source_path: source_path.display().to_string(),
        working_path: out_paths.working_path.display().to_string(),
        anonymized_path: out_paths.anonymized_path.display().to_string(),
        source_hash: crate::privacy_artifacts::sha256_hex(&source_bytes),
        working_hash: Some(crate::privacy_artifacts::sha256_hex(&working_bytes)),
        last_indexed_hash: None,
        status: "prepared".to_string(),
        counts: crate::privacy_artifacts::ManifestCounts {
            detected: detected_count,
            forced_mask: 0,
            kept_visible: 0,
        },
    })
}

/// Test helper for manifest hash comparison.
#[doc(hidden)]
#[must_use]
pub fn finalize_manifest_only_for_test(
    manifest: &WorkspaceManifest,
    working_hashes: &[(&str, &str)],
) -> PrivacyFinalizeSummary {
    let changed = manifest
        .documents
        .iter()
        .filter(|doc| {
            let current = working_hashes
                .iter()
                .find(|(id, _)| *id == doc.document_id)
                .map(|(_, hash)| (*hash).to_string());
            current.is_some() && current != doc.working_hash
        })
        .count();

    PrivacyFinalizeSummary {
        workspace: "test".to_string(),
        documents_changed: changed,
        documents_reindexed: changed,
        to_mask: 0,
        to_keep: 0,
        anonymized_folder: "test/02-anonymized-documents".to_string(),
        shareable_report: "test/03-reports/shareable_report.docx".to_string(),
    }
}

/// Finalize a folder privacy workspace.
///
/// # Errors
/// Returns privacy, extraction, detection, vault, or IO errors.
pub async fn finalize_folder(
    pipeline: &crate::pipeline::Pipeline,
    workspace: &Path,
) -> Result<PrivacyFinalizeSummary> {
    let manifest_path = workspace.join(crate::privacy_artifacts::MANIFEST_FILE);
    let manifest_json = std::fs::read_to_string(&manifest_path)?;
    let mut manifest: WorkspaceManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| crate::Error::Privacy(format!("parse manifest: {e}")))?;

    let paths = PrivacyWorkspacePaths {
        source_root: PathBuf::from(&manifest.source_root),
        workspace: workspace.to_path_buf(),
        working: workspace.join(crate::privacy_artifacts::WORKING_DIR),
        anonymized: workspace.join(crate::privacy_artifacts::ANONYMIZED_DIR),
        reports: workspace.join(crate::privacy_artifacts::REPORTS_DIR),
        cache: workspace.join(crate::privacy_artifacts::CACHE_DIR),
        manifest: manifest_path,
    };

    let mut changed = 0usize;
    let mut reindexed = 0usize;
    let mut to_mask = 0usize;
    let mut to_keep = 0usize;

    for doc in &mut manifest.documents {
        let working_path = PathBuf::from(&doc.working_path);
        let working_bytes = std::fs::read(&working_path)?;
        let working_hash = crate::privacy_artifacts::sha256_hex(&working_bytes);
        if doc.working_hash.as_deref() == Some(working_hash.as_str()) {
            continue;
        }
        changed += 1;

        let instructions = crate::docx_instructions::read_docx_instructions(&working_path)?;
        let decisions: Vec<crate::privacy_decisions::UserDecision> = instructions
            .iter()
            .map(|instruction| crate::privacy_decisions::UserDecision {
                action: instruction.action,
                selected_text: instruction.selected_text.clone(),
            })
            .collect();
        to_mask += decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Mask)
            .count();
        to_keep += decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Keep)
            .count();

        let extracted = crate::ingest::extract(&working_path, pipeline.config()).await?;
        let detector = pipeline.detector_for_privacy()?;
        let no_legal = Vec::new();
        let no_thresholds = std::collections::HashMap::new();
        let mut pseudo_sections = Vec::new();
        let mut detected_count = 0usize;
        for chunk in &extracted.chunks {
            let bundle = detector.detect_for_ingest(&chunk.text, &no_legal, &no_thresholds)?;
            detected_count += bundle.pii.len();
            let pii =
                crate::privacy_decisions::apply_user_decisions(&chunk.text, bundle.pii, &decisions);
            let report = pipeline
                .vault_for_privacy()
                .pseudonymize_with_report_map(&chunk.text, &pii)
                .await?;
            pseudo_sections.push(crate::privacy_docx::NormalizedSection {
                heading: format!("Chunk {}", chunk.idx + 1),
                body: report.text,
            });
        }

        let anonymized_path = PathBuf::from(&doc.anonymized_path);
        let anonymized_doc = crate::privacy_docx::NormalizedDocx {
            title: doc.relative_path.clone(),
            metadata: vec![
                ("Source".to_string(), doc.relative_path.clone()),
                ("PII".to_string(), "Anonymized".to_string()),
            ],
            sections: pseudo_sections,
        };
        crate::privacy_docx::write_normalized_docx(&anonymized_path, &anonymized_doc)?;

        doc.working_hash = Some(working_hash.clone());
        doc.last_indexed_hash = Some(working_hash);
        doc.status = "finalized".to_string();
        doc.counts.detected = detected_count;
        doc.counts.forced_mask = decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Mask)
            .count();
        doc.counts.kept_visible = decisions
            .iter()
            .filter(|d| d.action == crate::docx_instructions::InstructionAction::Keep)
            .count();
        reindexed += 1;
    }

    manifest.updated_at = chrono::Utc::now();
    write_manifest(&paths.manifest, &manifest)?;
    crate::privacy_docx::write_shareable_report(
        &paths.reports.join("shareable_report.docx"),
        "Privacy Finalization Report",
        &[
            ("Documents changed".to_string(), changed.to_string()),
            ("Documents reindexed".to_string(), reindexed.to_string()),
            ("Manual mask decisions".to_string(), to_mask.to_string()),
            ("Kept visible exceptions".to_string(), to_keep.to_string()),
        ],
    )?;

    Ok(PrivacyFinalizeSummary {
        workspace: paths.workspace.display().to_string(),
        documents_changed: changed,
        documents_reindexed: reindexed,
        to_mask,
        to_keep,
        anonymized_folder: paths.anonymized.display().to_string(),
        shareable_report: paths
            .reports
            .join("shareable_report.docx")
            .display()
            .to_string(),
    })
}
