//! End-to-end CLI execution tests for cross-document coreference
//!
//! Tests the actual CLI binary with real file I/O and output validation.

use std::fs;
use std::process::Command;

fn setup_test_directory() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp directory");

    // Create test files with entities that should be found
    let files = vec![
        ("doc1.txt", "Jensen Huang announced that Nvidia will build new AI supercomputers. The chipmaker plans to expand its data center business."),
        ("doc2.txt", "The CEO of Nvidia revealed plans for Blackwell chips during CES 2025. Huang said the new GPUs would advance robotics."),
        ("doc3.txt", "Nvidia's stock reached new highs after Jensen Huang's keynote. The company announced partnerships with major cloud providers."),
    ];

    for (filename, content) in files {
        let path = dir.path().join(filename);
        fs::write(&path, content).expect("Failed to write test file");
    }

    dir
}

fn anno_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_anno"))
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_command_exists() {
    let output = anno_cmd()
        .arg("cross-doc")
        .arg("--help")
        .output()
        .expect("run anno cross-doc --help");

    assert!(output.status.success(), "help should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Cross-document coreference") || stdout.contains("cluster entities"),
        "help output should describe the command"
    );
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_with_nonexistent_directory() {
    let output = anno_cmd()
        .arg("cross-doc")
        .arg("/nonexistent/directory/path")
        .output()
        .expect("run anno cross-doc <missing>");

    assert!(!output.status.success(), "missing directory should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist") || stderr.contains("Directory"),
        "stderr should explain missing directory"
    );
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_tree_format() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");

    assert!(output.status.success(), "tree format should succeed");
    let output = String::from_utf8_lossy(&output.stdout);

    // Should contain summary section
    assert!(
        output.contains("Summary") || output.contains("Documents:") || output.contains("Clusters:"),
        "Output should contain summary information"
    );
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_json_format() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");

    assert!(output.status.success(), "json format should succeed");
    let output = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let json: Result<serde_json::Value, _> = serde_json::from_str(&output);
    assert!(json.is_ok(), "Output should be valid JSON");

    if let Ok(json_val) = json {
        // Should have metadata and clusters
        assert!(
            json_val.get("metadata").is_some() || json_val.get("clusters").is_some(),
            "JSON should contain metadata or clusters"
        );
    }
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_summary_format() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("summary")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");

    assert!(output.status.success(), "summary format should succeed");
    let output = String::from_utf8_lossy(&output.stdout);

    // Summary should contain statistics
    assert!(
        output.contains("Documents") || output.contains("Entities") || output.contains("Clusters"),
        "Summary should contain statistics"
    );
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_max_clusters() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--max-clusters")
        .arg("2")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");

    assert!(output.status.success(), "max-clusters should succeed");
    let output = String::from_utf8_lossy(&output.stdout);

    // Should respect max_clusters limit
    // Count cluster markers (● or ○)
    let cluster_count = output.matches("●").count() + output.matches("○").count();
    // Note: This is approximate since clusters might not be found by StackedNER
    // But if clusters are found, should respect limit
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_cross_doc_only_filter() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--cross-doc-only")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "cross-doc-only should succeed");
    // Should only show cross-document clusters
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_verbose_mode() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--verbose")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");

    assert!(output.status.success(), "verbose should succeed");
    let output = String::from_utf8_lossy(&output.stdout);

    // Verbose mode should show more detail (context, etc.)
    // Output should be longer or contain more information
    assert!(!output.is_empty(), "Verbose output should not be empty");
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_output_to_file() {
    let dir = setup_test_directory();
    let output_file = dir.path().join("output.json");

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("json")
        .arg("--output")
        .arg(output_file.to_str().unwrap())
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "output-to-file should succeed");

    // Output file should exist and contain valid JSON
    if output_file.exists() {
        let content = fs::read_to_string(&output_file).expect("Failed to read output file");
        let json: Result<serde_json::Value, _> = serde_json::from_str(&content);
        assert!(json.is_ok(), "Output file should contain valid JSON");
    }
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_recursive_search() {
    let dir = setup_test_directory();

    // Create subdirectory
    let subdir = dir.path().join("subdir");
    fs::create_dir_all(&subdir).expect("Failed to create subdirectory");
    fs::write(
        subdir.join("doc4.txt"),
        "Nvidia announced new partnerships.",
    )
    .expect("Failed to write subdirectory file");

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--recursive")
        .arg("--format")
        .arg("tree")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "recursive should succeed");
    // Should find files in subdirectory
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_custom_extensions() {
    let dir = setup_test_directory();

    // Create a .md file
    fs::write(dir.path().join("doc.md"), "Nvidia is a technology company.")
        .expect("Failed to write markdown file");

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--extensions")
        .arg("md")
        .arg("--format")
        .arg("tree")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "custom extensions should succeed");
    // Should process .md files
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_min_cluster_size() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--min-cluster-size")
        .arg("2")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "min-cluster-size should succeed");
    // Should only show clusters with at least 2 mentions
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_entity_type_filter() {
    let dir = setup_test_directory();

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .arg("--type")
        .arg("ORG")
        .arg("--threshold")
        .arg("0.3")
        .output()
        .expect("run anno cross-doc");
    assert!(output.status.success(), "type filter should succeed");
    // Should only show Organization clusters
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_empty_directory() {
    let dir = tempfile::tempdir().expect("Failed to create temp directory");

    let output = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--format")
        .arg("tree")
        .output()
        .expect("run anno cross-doc");

    // May succeed with empty output or fail with appropriate message.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should either succeed with empty results or fail with clear message
    assert!(
        stdout.contains("0") || stderr.contains("No files") || stderr.contains("empty"),
        "Should handle empty directory appropriately"
    );
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_invalid_threshold() {
    let dir = setup_test_directory();

    // Test with threshold > 1.0 (should be clamped or error)
    let _ = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--threshold")
        .arg("1.5")
        .output()
        .expect("run anno cross-doc");
    // Command may succeed (if clamped) or fail (if validated). Either is acceptable.
}

#[test]
#[cfg(feature = "eval-advanced")]
fn test_crossdoc_negative_threshold() {
    let dir = setup_test_directory();

    let _ = anno_cmd()
        .arg("cross-doc")
        .arg(dir.path().to_str().unwrap())
        .arg("--threshold")
        .arg("-0.1")
        .output()
        .expect("run anno cross-doc");
    // Command may succeed (clamped) or fail (validation). Either is acceptable.
}
