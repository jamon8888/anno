//! Smoke test for the embedded (Tesseract) OCR pipeline.
//!
//! Requires a scanned PDF — point `ANNO_TEST_SCANNED_PDF` at one:
//!
//! ```powershell
//! $env:ANNO_TEST_SCANNED_PDF = "C:\path\to\scanned.pdf"
//! $env:ANNO_RAG_VAULT_PASSPHRASE = "test-ocr"
//! powershell -File scripts\test-local.ps1 -Package anno-rag `
//!   -Features embedded-ocr -- --include-ignored embedded_ocr
//! ```
//!
//! Without the env var the test is skipped (not failed).
#![cfg(feature = "embedded-ocr")]

use anno_rag::{config::AnnoRagConfig, pipeline::Pipeline};
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [42u8; 32];

#[tokio::test]
#[ignore = "requires ANNO_TEST_SCANNED_PDF env var + embedded-ocr feature"]
async fn embedded_ocr_extracts_text_from_scanned_pdf() {
    let pdf_path = match std::env::var("ANNO_TEST_SCANNED_PDF") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => {
            eprintln!("ANNO_TEST_SCANNED_PDF not set — skipping OCR smoke test");
            return;
        }
    };
    assert!(pdf_path.exists(), "ANNO_TEST_SCANNED_PDF does not exist: {pdf_path:?}");

    let dir = TempDir::new().expect("tempdir");
    let pdf_dir = dir.path().join("input");
    std::fs::create_dir_all(&pdf_dir).unwrap();
    std::fs::copy(&pdf_path, pdf_dir.join("scanned.pdf")).unwrap();

    let cfg = AnnoRagConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };

    let pipeline = Pipeline::new(cfg.clone(), TEST_KEY)
        .await
        .expect("pipeline init");

    let output_dir = dir.path().join("outputs");
    let n = pipeline
        .ingest_folder(&pdf_dir, false, &output_dir)
        .await
        .expect("ingest_folder with OCR");

    assert_eq!(n, 1, "should ingest 1 scanned PDF, got {n}");

    let hits = pipeline
        .search("texte", 3)
        .await
        .expect("search after OCR ingest");

    assert!(
        !hits.is_empty(),
        "should find at least one hit after OCR ingest"
    );
}

#[test]
fn ocr_mode_defaults_to_auto_embedded() {
    let cfg = AnnoRagConfig::default();
    assert!(
        matches!(cfg.ocr_mode, anno_rag::config::OcrMode::AutoEmbedded),
        "ocr_mode must default to AutoEmbedded, got {:?}",
        cfg.ocr_mode
    );
}
