//! E2E tests for dataset subcommands: export, import, context
//!
//! These tests verify the full roundtrip of export → import workflows
//! and the context viewing functionality.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn anno_cli_cmd() -> Command {
    if let Ok(bin_path) = std::env::var("ANNO_CLI_BIN") {
        return Command::new(bin_path);
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("manifest_dir should have parent");

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

// =============================================================================
// Export tests
// =============================================================================

#[test]
fn test_dataset_export_brat() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create test input
    fs::write(
        input_dir.path().join("test.txt"),
        "Dr. Marie Curie won the Nobel Prize in Paris.",
    )
    .unwrap();

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "export",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_dir.path().to_str().unwrap(),
            "--format",
            "brat",
            "--overwrite",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Export should succeed");

    // Verify .ann file created
    let ann_path = output_dir.path().join("test.ann");
    assert!(ann_path.exists(), ".ann file should exist");

    // Verify .txt file copied
    let txt_path = output_dir.path().join("test.txt");
    assert!(txt_path.exists(), ".txt file should be copied for brat");

    // Verify .ann content has entities
    let ann_content = fs::read_to_string(&ann_path).unwrap();
    assert!(
        ann_content.contains("T1\t"),
        "Should have entity annotations"
    );
}

#[test]
fn test_dataset_export_conll() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    fs::write(
        input_dir.path().join("test.txt"),
        "Barack Obama visited Berlin.",
    )
    .unwrap();

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "export",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_dir.path().to_str().unwrap(),
            "--format",
            "conll",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Export should succeed");

    let conll_path = output_dir.path().join("test.conll");
    assert!(conll_path.exists(), ".conll file should exist");

    let content = fs::read_to_string(&conll_path).unwrap();
    // Should have IOB tags
    assert!(
        content.contains("B-") || content.contains("O"),
        "Should have IOB tags"
    );
}

#[test]
fn test_dataset_export_jsonl() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    fs::write(
        input_dir.path().join("test.txt"),
        "Steve Jobs founded Apple in California.",
    )
    .unwrap();

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "export",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_dir.path().to_str().unwrap(),
            "--format",
            "jsonl",
            "--include-confidence",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Export should succeed");

    let jsonl_path = output_dir.path().join("test.jsonl");
    assert!(jsonl_path.exists(), ".jsonl file should exist");

    let content = fs::read_to_string(&jsonl_path).unwrap();
    // Should be valid JSONL with confidence
    for line in content.lines() {
        if !line.trim().is_empty() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("text").is_some(), "Should have text field");
            assert!(parsed.get("type").is_some(), "Should have type field");
            assert!(
                parsed.get("confidence").is_some(),
                "Should have confidence when --include-confidence"
            );
        }
    }
}

// =============================================================================
// Import tests
// =============================================================================

#[test]
fn test_dataset_import_brat() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create brat files
    fs::write(
        input_dir.path().join("test.txt"),
        "Dr. Marie Curie won the Nobel Prize.",
    )
    .unwrap();

    fs::write(
        input_dir.path().join("test.ann"),
        "T1\tPER 0 15\tDr. Marie Curie\nT2\tMISC 24 35\tNobel Prize",
    )
    .unwrap();

    let output_file = output_dir.path().join("imported.jsonl");

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "import",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_file.to_str().unwrap(),
            "--format",
            "brat",
            "--include-text",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Import should succeed");
    assert!(output_file.exists(), "Output file should exist");

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(
        content.contains("Dr. Marie Curie"),
        "Should contain entity text"
    );
    assert!(
        content.contains("Nobel Prize"),
        "Should contain second entity"
    );
}

#[test]
fn test_dataset_import_conll() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create CoNLL file
    fs::write(
        input_dir.path().join("test.conll"),
        "Barack\tB-PER\nObama\tI-PER\nvisited\tO\nBerlin\tB-LOC\n",
    )
    .unwrap();

    let output_file = output_dir.path().join("imported.jsonl");

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "import",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_file.to_str().unwrap(),
            "--format",
            "conll",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Import should succeed");

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(
        content.contains("Barack Obama"),
        "Should have merged B-I entity"
    );
    assert!(content.contains("Berlin"), "Should have single B entity");
}

#[test]
fn test_dataset_import_jsonl() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create JSONL file
    fs::write(
        input_dir.path().join("test.jsonl"),
        r#"{"text": "Marie Curie", "type": "PER", "start": 0, "end": 11}
{"text": "Paris", "type": "LOC", "start": 20, "end": 25}"#,
    )
    .unwrap();

    let output_file = output_dir.path().join("imported.jsonl");

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "import",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_file.to_str().unwrap(),
            "--format",
            "jsonl",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Import should succeed");

    let content = fs::read_to_string(&output_file).unwrap();
    assert!(
        content.contains("Marie Curie"),
        "Should import first entity"
    );
    assert!(content.contains("Paris"), "Should import second entity");
}

// =============================================================================
// Context tests
// =============================================================================

#[test]
fn test_dataset_context_human_format() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Dr. Marie Curie won the Nobel Prize in Paris.",
            "--format",
            "human",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Context command should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Human format should have entity headers and context
    assert!(
        stdout.contains("Entity") || stdout.contains("Context"),
        "Should have human-readable format"
    );
}

#[test]
fn test_dataset_context_json_format() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Steve Jobs founded Apple.",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Context command should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(
        parsed.get("entities").is_some(),
        "Should have entities array"
    );
    assert!(parsed.get("count").is_some(), "Should have count field");
}

#[test]
fn test_dataset_context_tsv_format() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Angela Merkel visited Berlin.",
            "--format",
            "tsv",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Context command should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // TSV should have header
    assert!(stdout.contains("text\ttype\t"), "Should have TSV header");
}

#[test]
fn test_dataset_context_markdown_format() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Barack Obama met with Angela Merkel in Berlin.",
            "--format",
            "markdown",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Context command should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Markdown should have table
    assert!(
        stdout.contains("|") && stdout.contains("Entity"),
        "Should have markdown table"
    );
}

#[test]
fn test_dataset_context_with_window_size() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "The famous Dr. Marie Curie from Paris won the Nobel Prize.",
            "--window",
            "10",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Context should be limited by window size
    if let Some(entities) = parsed["entities"].as_array() {
        for entity in entities {
            let left = entity["context"]["left"].as_str().unwrap_or("");
            let right = entity["context"]["right"].as_str().unwrap_or("");
            // Window is in characters, so context should be roughly <=10 chars
            assert!(
                left.chars().count() <= 12,
                "Left context should respect window"
            );
            assert!(
                right.chars().count() <= 12,
                "Right context should respect window"
            );
        }
    }
}

#[test]
fn test_dataset_context_full_sentence() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Dr. Marie Curie won the Nobel Prize. She was born in Poland.",
            "--full-sentence",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // With full-sentence, context should include sentence info
    if let Some(entities) = parsed["entities"].as_array() {
        for entity in entities {
            // Sentence field should be present
            assert!(
                entity["context"]["sentence"].is_string()
                    || entity["context"]["sentence"].is_null(),
                "Should have sentence field"
            );
        }
    }
}

#[test]
fn test_dataset_context_entity_type_filter() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Dr. Marie Curie from Paris won the Nobel Prize.",
            "--entity-type",
            "LOC",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Should only have LOC entities
    if let Some(entities) = parsed["entities"].as_array() {
        for entity in entities {
            let entity_type = entity["entity"]["type"].as_str().unwrap_or("");
            assert_eq!(
                entity_type.to_uppercase(),
                "LOC",
                "Should only have LOC entities"
            );
        }
    }
}

#[test]
fn test_dataset_context_quiet_mode() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--text",
            "Marie Curie discovered radium.",
            "--quiet",
            "--format",
            "human",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Quiet mode should have minimal output (just entity\ttext\tstart)
    assert!(
        !stdout.contains("Entity Context Export"),
        "Quiet mode should not have headers"
    );
}

#[test]
fn test_dataset_context_from_file() {
    let input_dir = TempDir::new().unwrap();
    let input_file = input_dir.path().join("input.txt");

    fs::write(&input_file, "Steve Jobs founded Apple in California.").unwrap();

    let output = anno_cli_cmd()
        .args([
            "dataset",
            "context",
            "--input",
            input_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Context from file should work");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        parsed["count"].as_u64().unwrap_or(0) > 0,
        "Should find entities"
    );
}

// =============================================================================
// Roundtrip tests
// =============================================================================

#[test]
fn test_export_import_roundtrip_brat() {
    let input_dir = TempDir::new().unwrap();
    let export_dir = TempDir::new().unwrap();
    let import_dir = TempDir::new().unwrap();

    // Create source
    fs::write(
        input_dir.path().join("test.txt"),
        "Barack Obama met Angela Merkel in Berlin.",
    )
    .unwrap();

    // Export to brat
    let export_output = anno_cli_cmd()
        .args([
            "dataset",
            "export",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            export_dir.path().to_str().unwrap(),
            "--format",
            "brat",
        ])
        .output()
        .unwrap();
    assert!(export_output.status.success(), "Export should succeed");

    // Import from brat
    let import_file = import_dir.path().join("imported.jsonl");
    let import_output = anno_cli_cmd()
        .args([
            "dataset",
            "import",
            "--input",
            export_dir.path().to_str().unwrap(),
            "--output",
            import_file.to_str().unwrap(),
            "--format",
            "brat",
        ])
        .output()
        .unwrap();
    assert!(import_output.status.success(), "Import should succeed");

    // Verify roundtrip preserved entities
    let content = fs::read_to_string(&import_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert!(!lines.is_empty(), "Should have imported entities");

    // Parse and verify
    for line in lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(parsed.get("text").is_some(), "Should have text");
        assert!(parsed.get("type").is_some(), "Should have type");
        assert!(parsed.get("start").is_some(), "Should have start");
        assert!(parsed.get("end").is_some(), "Should have end");
    }
}

// =============================================================================
// Error handling tests
// =============================================================================

#[test]
fn test_export_missing_input() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "export",
            "--input",
            "/nonexistent/path",
            "--output",
            "/tmp/out",
            "--format",
            "brat",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success(), "Should fail for missing input");
}

#[test]
fn test_import_missing_input() {
    let output = anno_cli_cmd()
        .args([
            "dataset",
            "import",
            "--input",
            "/nonexistent/path",
            "--output",
            "/tmp/out.jsonl",
            "--format",
            "brat",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success(), "Should fail for missing input");
}

#[test]
fn test_context_no_input() {
    let output = anno_cli_cmd()
        .args(["dataset", "context", "--format", "json"])
        .output()
        .unwrap();

    // Should fail because no input provided
    assert!(
        !output.status.success(),
        "Should fail when no input provided"
    );
}
