use anno_rag::privacy_artifacts::PrivacyWorkspacePaths;
use anno_rag::privacy_workspace::{build_relative_output_paths, PrivacyDocumentOutputPaths};
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
