use anno_rag::privacy_artifacts::{
    is_privacy_generated_path, PrivacyWorkspacePaths, WorkspaceManifest,
};
use std::path::Path;

#[test]
fn workspace_paths_live_under_source_vault_folder() {
    let root = Path::new("C:/Clients/Matter X");
    let paths = PrivacyWorkspacePaths::from_source_root(root);

    assert_eq!(paths.workspace, root.join("vault"));
    assert_eq!(
        paths.working,
        root.join("vault").join("01-working-documents")
    );
    assert_eq!(
        paths.anonymized,
        root.join("vault").join("02-anonymized-documents")
    );
    assert_eq!(paths.reports, root.join("vault").join("03-reports"));
    assert_eq!(paths.cache, root.join("vault").join("04-cache"));
    assert_eq!(paths.manifest, root.join("vault").join("manifest.json"));
}

#[test]
fn generated_path_detection_excludes_vault_anywhere_under_source_root() {
    let root = Path::new("C:/Clients/Matter X");
    assert!(is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/vault/01-working-documents/a.docx")
    ));
    assert!(is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/contracts/vault/report.docx")
    ));
    assert!(!is_privacy_generated_path(
        root,
        Path::new("C:/Clients/Matter X/contracts/source.pdf")
    ));
}

#[test]
fn manifest_round_trips_without_document_values() {
    let manifest = WorkspaceManifest::new("C:/Clients/Matter X");
    let json = serde_json::to_string_pretty(&manifest).expect("serialize");
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"source_root\""));
    assert!(!json.contains("Jean Dupont"));

    let restored: WorkspaceManifest = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.version, 1);
    assert_eq!(restored.documents.len(), 0);
}
