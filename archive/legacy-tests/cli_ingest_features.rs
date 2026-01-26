//! Tests for new ingest features: URL resolution, preprocessing, graph export

use std::process::Command;

fn anno_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_anno"))
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_extract_with_url() {
    // Test URL resolution (requires eval-advanced feature)
    // Note: This test may fail if URL is unreachable, but tests the feature exists
    let output = anno_cmd()
        .args(["extract", "--url", "https://example.com"])
        .output()
        .expect("run anno extract --url");
    assert!(output.status.success());
}

#[test]
fn test_extract_with_clean() {
    // Test text cleaning
    let output = anno_cmd()
        .args(["extract", "--clean", "Apple  Inc.   was   founded"])
        .output()
        .expect("run anno extract --clean");
    assert!(output.status.success());
}

#[test]
fn test_extract_with_normalize() {
    // Test Unicode normalization
    let output = anno_cmd()
        .args(["extract", "--normalize", "Marie Curie won the Nobel Prize"])
        .output()
        .expect("run anno extract --normalize");
    assert!(output.status.success());
}

#[test]
fn test_extract_with_detect_lang() {
    // Test language detection
    let output = anno_cmd()
        .args([
            "extract",
            "--detect-lang",
            "Marie Curie won the Nobel Prize",
        ])
        .output()
        .expect("run anno extract --detect-lang");
    assert!(output.status.success());
}

#[test]
fn test_extract_graph_export_neo4j() {
    // Test graph export to Neo4j format
    let output = anno_cmd()
        .args([
            "extract",
            "--export-graph",
            "neo4j",
            "Apple Inc. was founded by Steve Jobs",
        ])
        .output()
        .expect("run anno extract --export-graph neo4j");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CREATE") || stdout.contains("Node"));
}

#[test]
fn test_extract_graph_export_networkx() {
    // Test graph export to NetworkX format
    let output = anno_cmd()
        .args([
            "extract",
            "--export-graph",
            "networkx",
            "Apple Inc. was founded by Steve Jobs",
        ])
        .output()
        .expect("run anno extract --export-graph networkx");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nodes") || stdout.contains("edges"));
}

#[test]
fn test_debug_with_url() {
    // Test debug command with URL
    #[cfg(feature = "eval-advanced")]
    {
        let output = anno_cmd()
            .args(["debug", "--url", "https://example.com"])
            .output()
            .expect("run anno debug --url");
        assert!(output.status.success());
    }
}

#[test]
fn test_debug_with_preprocessing() {
    // Test debug command with preprocessing
    let output = anno_cmd()
        .args([
            "debug",
            "--clean",
            "--normalize",
            "Apple  Inc.   was   founded",
        ])
        .output()
        .expect("run anno debug --clean --normalize");
    assert!(output.status.success());
}

#[test]
fn test_debug_graph_export() {
    // Test debug command with graph export
    let output = anno_cmd()
        .args([
            "debug",
            "--export-graph",
            "neo4j",
            "--coref",
            "Apple Inc. was founded by Steve Jobs. The company is based in Cupertino.",
        ])
        .output()
        .expect("run anno debug --export-graph neo4j --coref");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CREATE") || stdout.contains("Node"));
}

#[test]
fn test_enhance_with_graph_export() {
    // Test enhance command can work with graph export
    let dir = tempfile::tempdir().expect("Failed to create temp directory");
    let test_doc = dir.path().join("test-doc.json");

    // First extract to create a GroundedDocument
    let output = anno_cmd()
        .args([
            "extract",
            "--export",
            test_doc.to_str().unwrap(),
            "Barack Obama met Angela Merkel",
        ])
        .output()
        .expect("run anno extract --export <file>");
    assert!(output.status.success());

    // Enhance and export to graph
    let output = anno_cmd()
        .args([
            "enhance",
            test_doc.to_str().unwrap(),
            "--coref",
            "--export-graph",
            "networkx",
        ])
        .output()
        .expect("run anno enhance --export-graph networkx");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nodes") || stdout.contains("edges"));
}
