use anno_rag::privacy_artifacts::PrivacyWorkspacePaths;
use anno_rag::privacy_workspace::{
    build_relative_output_paths, finalize_manifest_only_for_test, PrivacyDocumentOutputPaths,
};
use anno_rag::{config::AnnoRagConfig, Pipeline};
use std::path::Path;

#[test]
fn relative_output_paths_mirror_subfolders_and_change_extension() {
    let source_root = Path::new("C:/Clients/Matter X");
    let paths = PrivacyWorkspacePaths::from_source_root(source_root);
    let source = Path::new("C:/Clients/Matter X/contracts/contract.pdf");

    let PrivacyDocumentOutputPaths {
        relative_path,
        working_path,
        anonymized_path,
    } = build_relative_output_paths(&paths, source);

    assert_eq!(relative_path, "contracts/contract.pdf");
    assert_eq!(
        working_path,
        source_root
            .join("vault")
            .join("01-working-documents")
            .join("contracts")
            .join("contract.docx")
    );
    assert_eq!(
        anonymized_path,
        source_root
            .join("vault")
            .join("02-anonymized-documents")
            .join("contracts")
            .join("contract.anon.docx")
    );
}

#[tokio::test]
async fn prepare_folder_creates_working_anonymized_reports_and_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source_root = dir.path().join("matter");
    std::fs::create_dir_all(source_root.join("contracts")).expect("mkdir");
    std::fs::write(
        source_root.join("contracts").join("contract.txt"),
        "Jean Dupont signe avec Acme.",
    )
    .expect("write source");

    let cfg = AnnoRagConfig {
        data_dir: dir.path().join("data"),
        ..AnnoRagConfig::default()
    };
    if !cfg.models_cache().exists() {
        eprintln!(
            "skipping: no local models at {}",
            cfg.models_cache().display()
        );
        return;
    }

    let pipeline = Pipeline::new(cfg, [9u8; 32]).await.expect("pipeline");
    let summary = pipeline
        .privacy_prepare_folder(&source_root, true)
        .await
        .expect("prepare");

    assert_eq!(summary.documents_seen, 1);
    assert_eq!(summary.documents_prepared, 1);
    assert_eq!(summary.documents_failed, 0);
    assert!(source_root
        .join("vault")
        .join("01-working-documents")
        .join("contracts")
        .join("contract.docx")
        .exists());
    assert!(source_root
        .join("vault")
        .join("02-anonymized-documents")
        .join("contracts")
        .join("contract.anon.docx")
        .exists());
    assert!(source_root.join("vault").join("manifest.json").exists());
}

#[test]
fn finalize_manifest_skips_unchanged_working_hashes() {
    let mut manifest = anno_rag::privacy_artifacts::WorkspaceManifest::new("C:/Matter");
    manifest
        .documents
        .push(anno_rag::privacy_artifacts::ManifestDocument {
            document_id: "doc-1".to_string(),
            relative_path: "a.txt".to_string(),
            source_path: "C:/Matter/a.txt".to_string(),
            working_path: "unused".to_string(),
            anonymized_path: "unused".to_string(),
            source_hash: "source".to_string(),
            working_hash: Some("same".to_string()),
            last_indexed_hash: Some("same".to_string()),
            status: "prepared".to_string(),
            counts: Default::default(),
        });

    let summary = finalize_manifest_only_for_test(&manifest, &[("doc-1", "same")]);

    assert_eq!(summary.documents_changed, 0);
    assert_eq!(summary.documents_reindexed, 0);
}
