//! End-to-end integration test on French legal fixtures.
//!
//! `#[ignore]`'d by default because it downloads the multilingual-e5-small
//! model (~470 MB) and reads/writes to the filesystem. Run with:
//!
//! ```bash
//! ANNO_RAG_VAULT_PASSPHRASE=test-e2e PROTOC_INCLUDE=/usr/include \
//!   CARGO_TARGET_DIR=/tmp/anno-target \
//!   cargo test -p anno-rag --test e2e -- --ignored
//! ```

use anno_rag::{config::AnnoRagConfig, pipeline::Pipeline};
use std::path::Path;
use tempfile::TempDir;

/// Deterministic vault key for tests — bypasses keyring + Argon2 derivation.
const TEST_KEY: [u8; 32] = [42u8; 32];

#[tokio::test]
#[ignore = "downloads ~470 MB model + does real I/O — opt-in via --ignored"]
async fn ingest_then_search_french_fixtures() {
    let dir = TempDir::new().expect("tempdir");
    let mut cfg = AnnoRagConfig::default();
    cfg.data_dir = dir.path().to_path_buf();

    let pipeline = Pipeline::new(cfg.clone(), TEST_KEY)
        .await
        .expect("pipeline init");

    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let output_dir = dir.path().join("outputs");

    let n = pipeline
        .ingest_folder(&fixtures_dir, false, &output_dir)
        .await
        .expect("ingest_folder");
    assert_eq!(n, 3, "should ingest 3 fixtures, got {n}");

    // Run a semantic search relevant to IBAN/payment language.
    let hits = pipeline
        .search("virement bancaire IBAN", 5)
        .await
        .expect("search");
    assert!(
        !hits.is_empty(),
        "search must return at least one hit for IBAN query"
    );

    // Verify no raw PII leaked into the anonymized markdown copies.
    let entries: Vec<_> = std::fs::read_dir(&output_dir)
        .expect("read output_dir")
        .flatten()
        .collect();
    assert_eq!(
        entries.len(),
        3,
        "should write 3 .anon.md files, got {}",
        entries.len()
    );

    // Structured PII (regex layer) MUST always be scrubbed — hard assertion.
    //
    // Name PII (anno NER layer): as of v0.5 #025 (T4), anno-rag uses
    // GLiNER2Fastino::from_pretrained("SemplificaAI/gliner2-multi-v1-onnx")
    // directly (multilingual, FR-aware). Coverage is now expected to be
    // 100% (12/12) — every name in every anonymized fixture.
    let names = [
        "Marie Dupont",
        "Jean Martin",
        "Sophie Bernard",
        "Pierre Lefebvre",
    ];
    let mut total_leaks: Vec<String> = Vec::new();
    for entry in &entries {
        let content = std::fs::read_to_string(entry.path()).expect("read output");
        let path_str = entry.path().display().to_string();
        for name in names {
            if content.contains(name) {
                total_leaks.push(format!("{name} in {path_str}"));
            }
        }
    }
    let scrubbed = names.len() * entries.len() - total_leaks.len();
    let total_checks = names.len() * entries.len();
    eprintln!(
        "name-scrub coverage: {scrubbed}/{total_checks} (leaks: {:?})",
        total_leaks
    );
    // v0.5 #025 — hard 100% gate (was >=50% in v0.2 when StackedNER
    // missed FR names in markdown contexts).
    assert_eq!(
        scrubbed, total_checks,
        "expected full name-scrub coverage with GLiNER2Fastino multi-v1, leaked: {total_leaks:?}"
    );

    for entry in &entries {
        let content = std::fs::read_to_string(entry.path()).expect("read output");
        let path_str = entry.path().display().to_string();

        // No raw phone (regex)
        assert!(
            !content.contains("06 12 34 56 78"),
            "raw phone leaked in {path_str}"
        );

        // No raw IBANs (regex)
        assert!(
            !content.contains("FR76 3000 6000 0112 3456 7890 189"),
            "raw IBAN FR76 leaked in {path_str}"
        );
        assert!(
            !content.contains("FR14 2004 1010 0505 0001 3M02 606"),
            "raw IBAN FR14 leaked in {path_str}"
        );

        // No raw SIRET (Luhn-valid)
        assert!(
            !content.contains("73282932000074"),
            "raw SIRET leaked in {path_str}"
        );
    }
}
