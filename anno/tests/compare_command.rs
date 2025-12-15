//! Tests for the compare command functionality.
//!
//! Verifies entity comparison, diff computation, and output formats.
//!
//! NOTE: These tests are ignored because the compare command's `--format json`
//! output format and entity comparison semantics are not yet implemented.
//!
//! See: https://github.com/arclabs561/anno/issues/XXX

// Note: This test requires cli feature
#![cfg(feature = "cli")]

use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

/// Get the CLI command - uses pre-built binary if available.
fn anno_cli_cmd() -> Command {
    if let Ok(bin_path) = std::env::var("ANNO_CLI_BIN") {
        return Command::new(bin_path);
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("anno crate should be in workspace");

    let release_bin = workspace_root.join("target/release/anno");
    if release_bin.exists() {
        return Command::new(release_bin);
    }

    let debug_bin = workspace_root.join("target/debug/anno");
    if debug_bin.exists() {
        return Command::new(debug_bin);
    }

    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-p", "anno-cli", "--"]);
    cmd
}

/// Create a temporary extraction JSON file for testing
fn create_extraction_file(entities: &[(&str, &str, usize, usize, f64)]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");

    let entities_json: Vec<serde_json::Value> = entities
        .iter()
        .map(|(text, entity_type, start, end, confidence)| {
            serde_json::json!({
                "text": text,
                "type": entity_type,
                "start": start,
                "end": end,
                "confidence": confidence,
                "id": format!("e:{:x}", start ^ end),
            })
        })
        .collect();

    let json = serde_json::json!({
        "entities": entities_json,
        "provenance": {
            "tool": "test",
            "version": "1.0.0",
        }
    });

    write!(file, "{}", serde_json::to_string(&json).unwrap()).expect("Failed to write");
    file
}

#[test]
fn test_compare_identical_files() {
    let entities = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("New York", "LOC", 20, 28, 0.90),
    ];

    let file1 = create_extraction_file(&entities);
    let file2 = create_extraction_file(&entities);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Identical files should have 100% Jaccard similarity
    let similarity = json["jaccard_similarity"].as_f64().unwrap_or(0.0);
    assert!(
        similarity > 0.99,
        "Identical extractions should have ~1.0 Jaccard similarity, got {}",
        similarity
    );

    assert_eq!(json["added"].as_u64().unwrap_or(999), 0);
    assert_eq!(json["removed"].as_u64().unwrap_or(999), 0);
    assert_eq!(json["unchanged"].as_u64().unwrap_or(0), 2);
}

#[test]
fn test_compare_added_entity() {
    let entities1 = vec![("John Smith", "PER", 0, 10, 0.85)];
    let entities2 = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("New York", "LOC", 20, 28, 0.90), // Added in file2
    ];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert_eq!(
        json["added"].as_u64().unwrap_or(0),
        1,
        "Should have 1 added entity"
    );
    assert_eq!(
        json["removed"].as_u64().unwrap_or(999),
        0,
        "Should have 0 removed entities"
    );
    assert_eq!(
        json["unchanged"].as_u64().unwrap_or(0),
        1,
        "Should have 1 unchanged entity"
    );
}

#[test]
fn test_compare_removed_entity() {
    let entities1 = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("New York", "LOC", 20, 28, 0.90), // This will be removed in file2
    ];
    let entities2 = vec![("John Smith", "PER", 0, 10, 0.85)];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert_eq!(
        json["removed"].as_u64().unwrap_or(0),
        1,
        "Should have 1 removed entity"
    );
    assert_eq!(
        json["added"].as_u64().unwrap_or(999),
        0,
        "Should have 0 added entities"
    );
}

#[test]
fn test_compare_modified_confidence() {
    let entities1 = vec![("John Smith", "PER", 0, 10, 0.85)];
    let entities2 = vec![
        ("John Smith", "PER", 0, 10, 0.95), // Confidence changed
    ];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            "--confidence-epsilon",
            "0.01", // Small epsilon to detect the change
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    assert_eq!(
        json["modified"].as_u64().unwrap_or(0),
        1,
        "Should detect confidence modification"
    );
}

#[test]
fn test_compare_changes_only_flag() {
    let entities1 = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("Apple Inc", "ORG", 50, 59, 0.90),
    ];
    let entities2 = vec![
        ("John Smith", "PER", 0, 10, 0.85), // Unchanged
        ("Google", "ORG", 70, 76, 0.88),    // Added (different entity)
    ];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            "--changes-only",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let diffs = json["diffs"].as_array().expect("Should have diffs array");

    // With --changes-only, unchanged entities should not appear in diffs
    let unchanged_count = diffs
        .iter()
        .filter(|d| d["change_type"].as_str() == Some("unchanged"))
        .count();

    assert_eq!(
        unchanged_count, 0,
        "With --changes-only, no unchanged diffs should be present"
    );
}

#[test]
fn test_compare_jsonl_format() {
    let entities1 = vec![("John Smith", "PER", 0, 10, 0.85)];
    let entities2 = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("New York", "LOC", 20, 28, 0.90),
    ];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "jsonl",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // First line should be summary
    let summary: serde_json::Value =
        serde_json::from_str(lines[0]).expect("First line should be valid JSON summary");
    assert_eq!(summary["_type"].as_str(), Some("summary"));

    // Subsequent lines should be individual diffs
    assert!(lines.len() >= 2, "Should have at least summary + diffs");

    for line in lines.iter().skip(1) {
        if line.is_empty() {
            continue;
        }
        let diff: serde_json::Value =
            serde_json::from_str(line).expect("Each diff line should be valid JSON");
        assert!(diff.get("text").is_some(), "Diff should have text field");
        assert!(
            diff.get("change_type").is_some(),
            "Diff should have change_type"
        );
    }
}

#[test]
fn test_compare_empty_files() {
    let entities1: Vec<(&str, &str, usize, usize, f64)> = vec![];
    let entities2: Vec<(&str, &str, usize, usize, f64)> = vec![];

    let file1 = create_extraction_file(&entities1);
    let file2 = create_extraction_file(&entities2);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON even for empty files");

    // Empty comparison should have 100% similarity (both empty = identical)
    let similarity = json["jaccard_similarity"].as_f64().unwrap_or(0.0);
    assert!(
        similarity > 0.99,
        "Empty files comparison should have ~1.0 Jaccard similarity, got {}",
        similarity
    );
}

/// Test that multiple entities with same text but different spans are handled correctly
#[test]
fn test_compare_duplicate_entities_different_spans() {
    // A has "John Smith" at two different positions
    let entities_a = vec![
        ("John Smith", "PER", 0, 10, 0.85),
        ("John Smith", "PER", 50, 60, 0.90),
    ];

    // B has "John Smith" at first position (same), but second moved to (55, 65)
    let entities_b = vec![
        ("John Smith", "PER", 0, 10, 0.85),  // unchanged
        ("John Smith", "PER", 55, 65, 0.90), // span shifted
    ];

    let file1 = create_extraction_file(&entities_a);
    let file2 = create_extraction_file(&entities_b);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have exactly 2 diffs
    let diffs = json["diffs"].as_array().expect("Should have diffs array");
    assert_eq!(
        diffs.len(),
        2,
        "Should have exactly 2 diffs for 2 entity pairs"
    );

    // Count change types
    let unchanged = diffs
        .iter()
        .filter(|d| d["change_type"].as_str() == Some("unchanged"))
        .count();
    let modified = diffs
        .iter()
        .filter(|d| d["change_type"].as_str() == Some("modified"))
        .count();
    let added = diffs
        .iter()
        .filter(|d| d["change_type"].as_str() == Some("added"))
        .count();
    let removed = diffs
        .iter()
        .filter(|d| d["change_type"].as_str() == Some("removed"))
        .count();

    // One should be unchanged (exact match), one should be modified (span shift)
    assert_eq!(unchanged, 1, "Should have 1 unchanged entity (0-10)");
    assert_eq!(
        modified, 1,
        "Should have 1 modified entity (50-60 -> 55-65)"
    );
    assert_eq!(added, 0, "Should have no added entities");
    assert_eq!(removed, 0, "Should have no removed entities");
}

/// Test comparison when entity counts differ with same text
#[test]
fn test_compare_different_entity_counts_same_text() {
    // A has 3 "John" entities
    let entities_a = vec![
        ("John", "PER", 0, 4, 0.85),
        ("John", "PER", 20, 24, 0.85),
        ("John", "PER", 40, 44, 0.85),
    ];

    // B has only 1 "John" entity
    let entities_b = vec![
        ("John", "PER", 0, 4, 0.85), // unchanged
    ];

    let file1 = create_extraction_file(&entities_a);
    let file2 = create_extraction_file(&entities_b);

    let output = anno_cli_cmd()
        .args([
            "compare",
            "--format",
            "json",
            file1.path().to_str().unwrap(),
            file2.path().to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Check counts
    let removed = json["removed"].as_u64().unwrap_or(0);
    let unchanged = json["unchanged"].as_u64().unwrap_or(0);

    // 1 unchanged (0-4), 2 removed (20-24 and 40-44)
    assert_eq!(unchanged, 1, "Should have 1 unchanged entity");
    assert_eq!(removed, 2, "Should have 2 removed entities");
}
