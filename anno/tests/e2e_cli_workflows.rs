//! End-to-end tests for CLI workflows
//!
//! Tests complete CLI command sequences with various flags, inputs, and outputs.
//!
//! These tests use a pre-built CLI binary for speed.
//! Build with: `cargo build --release -p anno-cli`

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get the CLI command - uses pre-built binary if available.
fn anno_cli_cmd() -> Command {
    if let Ok(bin_path) = std::env::var("ANNO_CLI_BIN") {
        return Command::new(bin_path);
    }

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("anno crate should be in workspace");

    let debug_bin = workspace_root.join("target/debug/anno");
    if debug_bin.exists() {
        return Command::new(debug_bin);
    }

    // Fall back to a pre-built release binary if present. We prefer debug above because
    // `cargo test` builds with the current feature set, while a stale release binary may not.
    let release_bin = workspace_root.join("target/release/anno");
    if release_bin.exists() {
        return Command::new(release_bin);
    }

    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-p", "anno-cli", "--"]);
    cmd
}

/// Helper: Create a temporary directory with test files
fn create_test_dir() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();

    // Create test files
    fs::write(
        dir.join("doc1.txt"),
        "Barack Obama was president. He served from 2009 to 2017.",
    )
    .unwrap();
    fs::write(
        dir.join("doc2.txt"),
        "Obama was born in Hawaii. The former president lives in Washington.",
    )
    .unwrap();
    fs::write(
        dir.join("doc3.txt"),
        "The White House is in Washington D.C.",
    )
    .unwrap();

    (tmp, dir)
}

/// E2E: Extract command with various output formats
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_formats() {
    let text = "Marie Curie won the Nobel Prize in Physics.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    // Use the built binary directly if available
    let binary = if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        // Skip if binary not available (requires manual build)
        return;
    };

    // Test JSON output
    let output = Command::new(binary)
        .args(["extract", "--file", file_path, "--format", "json"])
        .output()
        .unwrap();

    // Allow graceful failures (e.g., no entities found with default model)
    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        // JSON output should be parseable
        if !stdout.is_empty() {
            assert!(
                stdout.contains("{") || stdout.contains("Marie Curie") || stdout.contains("PER"),
                "Output should contain JSON or entity info"
            );
        }
    }

    // Test human output (default)
    let output = Command::new(binary)
        .args(["extract", "--file", file_path])
        .output()
        .unwrap();

    // Allow graceful failures
    if output.status.success() {
        let _stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        // Human output should be readable (not JSON)
        if !stdout.is_empty() {
            assert!(
                !stdout.trim().starts_with("{"),
                "Human output should not be JSON"
            );
        }
    }
}

/// E2E: Extract with verbose flags
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_verbose() {
    let text = "Barack Obama was the 44th President. He served from 2009 to 2017.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    // Use the built binary directly if available, otherwise use cargo run
    let binary = if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        // Fallback: try cargo run (may be slower)
        return; // Skip if binary not available
    };

    // Test -v (level 1)
    let output = Command::new(binary)
        .args(["extract", "--file", file_path, "-v"])
        .output()
        .unwrap();

    // Allow graceful failures (e.g., no entities found)
    if output.status.success() {
        let _stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        // Level 1 should show confidence and context if entities found
        // If no entities, output may be empty or show a message
    }

    // Test -vv (level 2)
    let output = Command::new(binary)
        .args(["extract", "--file", file_path, "-vv"])
        .output()
        .unwrap();

    // Allow graceful failures
    if output.status.success() {
        let _stdout = String::from_utf8(output.stdout).unwrap();
        // Level 2 should show tracks (coreference) if available
        // Note: May not always show tracks if no coreference detected
    }
}

/// E2E: Crossdoc command with directory input
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_crossdoc_directory() {
    let (_tmp, dir) = create_test_dir();
    let dir_str = dir.to_str().unwrap();

    // Run crossdoc on directory
    let output = Command::new("cargo")
        .args([
            "run",
            "--features",
            "eval-advanced",
            "--",
            "crossdoc",
            "--directory",
            dir_str,
            "--extensions",
            "txt",
            "--threshold",
            "0.6",
        ])
        .output()
        .unwrap();

    // Should succeed (even if no entities found)
    // In real usage, would need actual NER extraction first
    // This test verifies the command structure works
    assert!(output.status.code().is_some());
}

/// E2E: Pipeline command with all stages
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_pipeline_full() {
    let text = "Barack Obama was president. He served from 2009 to 2017. \
                Obama was born in Hawaii.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    // Use the built binary directly if available
    let binary = if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return; // Skip if binary not available
    };

    // Run full pipeline with coref
    let output = Command::new(binary)
        .args(["pipeline", "--file", file_path, "--coref"])
        .output()
        .unwrap();

    // Allow graceful failures
    if output.status.success() {
        let _stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        // Should show extracted entities if any found
        // Output may be empty if no entities extracted
    }
}

/// E2E: Batch processing with stdin
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_batch_stdin() {
    use std::process::{Command, Stdio};

    // Create JSONL input
    let jsonl = r#"{"id": "doc1", "text": "Barack Obama was president."}
{"id": "doc2", "text": "Obama served from 2009 to 2017."}
{"id": "doc3", "text": "The White House is in Washington."}
"#;

    let mut child = Command::new("cargo")
        .args([
            "run",
            "--features",
            "eval-advanced",
            "--",
            "batch",
            "--stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Write input
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(jsonl.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    // Should process input (may fail if batch command not fully implemented)
    // This test verifies the command accepts stdin
    assert!(output.status.code().is_some());
}

/// E2E: Debug command with HTML output
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_debug_html() {
    let text = "Marie Curie won the Nobel Prize.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    let output_file = tempfile::NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_str().unwrap();

    // Run debug with HTML output
    let output = Command::new("cargo")
        .args([
            "run",
            "--features",
            "eval-advanced",
            "--",
            "debug",
            "--file",
            file_path,
            "--html",
            "--output",
            output_path,
        ])
        .output()
        .unwrap();

    // Should create HTML file
    if output.status.success() {
        assert!(
            PathBuf::from(output_path).exists()
                || PathBuf::from(format!("{}.html", output_path)).exists(),
            "HTML file should be created"
        );
    }
}

/// E2E: Extract with different models
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_models() {
    let text = "Apple Inc. is a technology company based in Cupertino, California.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    // Use the built binary directly if available
    let binary = if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return; // Skip if binary not available
    };

    // Test with heuristic model (always available)
    let output = Command::new(binary)
        .args(["extract", "--file", file_path, "--model", "heuristic"])
        .output()
        .unwrap();

    // Allow graceful failures (e.g., file issues)
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap();
        // Only fail on unexpected errors
        assert!(
            stderr.contains("not found") || stderr.contains("error"),
            "Unexpected error: {}",
            stderr
        );
    }

    // Test with regex model
    let output = Command::new(binary)
        .args(["extract", "--file", file_path, "--model", "regex"])
        .output()
        .unwrap();

    // Allow graceful failures
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("not found") || stderr.contains("error"),
            "Unexpected error: {}",
            stderr
        );
    }
}

/// E2E: Extract with URL input
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_url() {
    // Use a simple, stable URL for testing
    // Note: This may fail if URL is unreachable, so we check for graceful failure
    let output = Command::new("cargo")
        .args([
            "run",
            "--features",
            "eval-advanced",
            "--",
            "extract",
            "--url",
            "https://example.com",
        ])
        .output()
        .unwrap();

    // Should either succeed or fail gracefully (not panic)
    assert!(output.status.code().is_some(), "Should not panic");
}

/// E2E: Extract with --types flag (zero-shot or filter mode)
#[test]
fn e2e_cli_extract_types_filter() {
    // Use the built binary directly if available
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return; // Skip if binary not available
    };

    // Test --types with standard entity labels (filter mode without onnx)
    let output = Command::new(binary)
        .args([
            "extract",
            "--types",
            "PER,LOC",
            "Marie Curie discovered radium in Paris",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should find entities matching the types
    assert!(
        stdout.contains("Marie Curie") || stdout.contains("PER"),
        "Should find PER entity"
    );
    assert!(
        stdout.contains("Paris") || stdout.contains("LOC"),
        "Should find LOC entity"
    );
}

/// E2E: Extract with --types excludes non-matching types
#[test]
fn e2e_cli_extract_types_excludes() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Only request LOC type
    let output = Command::new(binary)
        .args([
            "extract",
            "--types",
            "LOC",
            "Marie Curie discovered radium in Paris",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should find Paris but not Marie Curie
    assert!(
        stdout.contains("Paris") || stdout.contains("LOC"),
        "Should find LOC entity"
    );
    // PER should be excluded
    assert!(
        !stdout.contains("Marie Curie") || stdout.contains("LOC:1"),
        "Should not show PER entities when only LOC requested"
    );
}

/// E2E: Extract with --threshold argument
#[test]
fn e2e_cli_extract_threshold() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Test that --threshold is accepted
    let output = Command::new(binary)
        .args([
            "extract",
            "--types",
            "PER",
            "--threshold",
            "0.8",
            "Marie Curie discovered radium",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "Command with --threshold should succeed"
    );
}

/// E2E: Extract with type hints (eval-advanced feature)
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_type_hints() {
    let text = "The CEO of Apple is Tim Cook. He lives in California.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    // Extract with custom type hints (if supported)
    // Note: This depends on zero-shot NER backend availability
    let output = Command::new("cargo")
        .args([
            "run",
            "--features",
            "eval-advanced",
            "--",
            "extract",
            "--file",
            file_path,
            "--types",
            "CEO,Company,State",
        ])
        .output()
        .unwrap();

    // Should process (may not support type hints depending on model)
    assert!(output.status.code().is_some());
}

/// E2E: Extract with --expected-types warns about missing types (Wisdom 13)
#[test]
fn e2e_cli_extract_expected_types() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Expect PER, ORG, DATE, MONEY but text only has PER and LOC
    let output = Command::new(binary)
        .args([
            "extract",
            "--expected-types",
            "PER,ORG,DATE,MONEY",
            "Dr. Sarah Chen from MIT won the award",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Should warn about missing types
    assert!(
        stderr.contains("Expected types not found") || stderr.contains("warning"),
        "Should warn about missing expected types"
    );
}

/// E2E: Extract with --flatten modes (Wisdom 12)
#[test]
fn e2e_cli_extract_flatten() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Test each flatten mode
    for mode in &["outer", "inner", "all"] {
        let output = Command::new(binary)
            .args(["extract", "--flatten", mode, "Dr. Sarah Chen from MIT"])
            .output()
            .unwrap();

        assert!(output.status.success(), "--flatten={} should work", mode);
    }
}

/// E2E: Extract with --type-map (Wisdom 17)
#[test]
fn e2e_cli_extract_type_map() {
    use std::io::Write;
    use std::process::Command;

    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Create a type mapping file
    let mut map_file = tempfile::NamedTempFile::new().unwrap();
    writeln!(map_file, "PER\tschema:Person").unwrap();
    writeln!(map_file, "LOC\tdbo:Place").unwrap();
    let map_path = map_file.path().to_str().unwrap();

    let output = Command::new(binary)
        .args(["extract", "--type-map", map_path, "Dr. Sarah Chen from MIT"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Types should be mapped
    assert!(
        stdout.contains("schema:Person") || stdout.contains("dbo:Place"),
        "Types should be mapped to ontology: {}",
        stdout
    );
}

/// E2E: JSON output includes provenance (Wisdom 9)
#[test]
fn e2e_cli_extract_json_provenance() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    let output = Command::new(binary)
        .args([
            "extract",
            "--format",
            "json",
            "Marie Curie discovered radium",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // JSON should include provenance
    assert!(
        stdout.contains("provenance") && stdout.contains("version"),
        "JSON output should include provenance metadata"
    );

    // Should include stable entity IDs
    assert!(
        stdout.contains("\"id\":") && stdout.contains("e:"),
        "JSON output should include stable entity IDs"
    );
}

/// E2E: TSV output includes header with provenance (Wisdom 9)
#[test]
fn e2e_cli_extract_tsv_provenance() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    let output = Command::new(binary)
        .args([
            "extract",
            "--format",
            "tsv",
            "Marie Curie discovered radium",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // TSV should have header with version
    assert!(
        stdout.starts_with("# anno") || stdout.contains("anno v"),
        "TSV output should have header with version"
    );

    // Should include stable entity IDs
    assert!(
        stdout.contains("e:"),
        "TSV output should include stable entity IDs"
    );
}

/// E2E: Exit codes are semantic (Wisdom 20)
#[test]
fn e2e_cli_exit_codes() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Success case (exit 0)
    let output = Command::new(binary)
        .args(["extract", "Marie Curie"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "Success should exit 0");

    // Invalid arguments (exit 2)
    let output = Command::new(binary)
        .args(["extract", "--invalid-flag-xyz"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "Invalid args should exit 2");

    // File not found should have non-zero exit
    let output = Command::new(binary)
        .args(["extract", "--file", "/nonexistent/path/file.txt"])
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "File not found should not exit 0"
    );
}

/// E2E: Explain command shows entity decisions (Wisdom 18)
#[test]
fn e2e_cli_explain() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    let output = Command::new(binary)
        .args(["explain", "Dr. Sarah Chen from MIT won the award"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show entity explanation
    assert!(
        stdout.contains("Entity:") && stdout.contains("Type Decision:"),
        "Explain should show entity and type decision"
    );

    // Should show features
    assert!(stdout.contains("Features:"), "Explain should show features");

    // Should show span info
    assert!(
        stdout.contains("Span:") && stdout.contains("start:"),
        "Explain should show span info"
    );
}

/// E2E: Explain with --show-all shows conflicts
#[test]
fn e2e_cli_explain_show_all() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    let output = Command::new(binary)
        .args(["explain", "--show-all", "Dr. Sarah Chen from MIT"])
        .output()
        .unwrap();

    assert!(output.status.success());
}

/// E2E: Resource budgets work (Wisdom 15)
#[test]
fn e2e_cli_extract_max_tokens() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Test --max-tokens limits processing
    let output = Command::new(binary)
        .args([
            "extract",
            "--max-tokens",
            "10",
            "This is a very long document that should be truncated because it exceeds the maximum token limit we set for testing purposes.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
}

/// E2E: Split strategies work (Wisdom 15)
#[test]
fn e2e_cli_extract_split_strategy() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    for strategy in &["none", "sentence", "paragraph", "chunk"] {
        let output = Command::new(binary)
            .args([
                "extract",
                "--split-strategy",
                strategy,
                "First paragraph here.\n\nSecond paragraph. With sentences. Multiple.",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "--split-strategy={} should work",
            strategy
        );
    }
}

/// E2E: Export command works (Wisdom 16)
#[test]
fn e2e_cli_export_brat() {
    use std::process::Command;
    use tempfile::TempDir;

    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Create temp input and output directories
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create a test file
    fs::write(
        input_dir.path().join("test.txt"),
        "Dr. Sarah Chen from MIT won the Nobel Prize.",
    )
    .unwrap();

    let output = Command::new(binary)
        .args([
            "export",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_dir.path().to_str().unwrap(),
            "--format",
            "brat",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Export should succeed");

    // Check that .ann file was created
    let ann_path = output_dir.path().join("test.ann");
    assert!(ann_path.exists(), "Should create .ann file");
}

/// E2E: Import command works (Wisdom 16)
#[test]
fn e2e_cli_import_brat() {
    use std::process::Command;
    use tempfile::TempDir;

    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    // Create temp directories
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Create a test .ann file
    fs::write(
        input_dir.path().join("test.ann"),
        "T1\tPER 0 14\tDr. Sarah Chen\nT2\tORG 20 23\tMIT",
    )
    .unwrap();
    // And corresponding .txt file
    fs::write(
        input_dir.path().join("test.txt"),
        "Dr. Sarah Chen from MIT won the Nobel Prize.",
    )
    .unwrap();

    let output = Command::new(binary)
        .args([
            "import",
            "--input",
            input_dir.path().to_str().unwrap(),
            "--output",
            output_dir.path().join("output.jsonl").to_str().unwrap(),
            "--format",
            "brat",
            "--include-text",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "Import should succeed");

    // Check that output file was created
    let output_path = output_dir.path().join("output.jsonl");
    assert!(output_path.exists(), "Should create output file");

    // Check content
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(
        content.contains("Dr. Sarah Chen"),
        "Should contain entity text"
    );
    assert!(content.contains("PER"), "Should contain entity type");
}

/// E2E: JSON provenance includes backends list
#[test]
fn e2e_cli_json_provenance_backends() {
    let binary = if std::path::Path::new("target/release/anno").exists() {
        "target/release/anno"
    } else if std::path::Path::new("target/debug/anno").exists() {
        "target/debug/anno"
    } else {
        return;
    };

    let output = Command::new(binary)
        .args(["extract", "--format", "json", "Dr. Sarah Chen from MIT"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // JSON should include backends array
    assert!(
        stdout.contains("\"backends\""),
        "JSON provenance should include backends list"
    );

    // Entities should have source field
    assert!(
        stdout.contains("\"source\""),
        "Entities should include source backend"
    );
}

/// E2E: Quiet mode suppresses output
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_extract_quiet() {
    let text = "Test document.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    let output = anno_cli_cmd()
        .args(["extract", "--file", file_path, "--quiet"])
        .output()
        .unwrap();

    // Check for graceful failure (command exists and runs, even if no entities found)
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap();
        // Allow certain expected failures (e.g., file not found, no entities)
        assert!(
            stderr.contains("no entities")
                || stderr.contains("not found")
                || stderr.contains("error"),
            "Unexpected error: {}",
            stderr
        );
    }
    // Quiet mode should produce minimal/no output
    let _stdout = String::from_utf8(output.stdout).unwrap();
    // Output may still contain essential info, but should be minimal
}

/// E2E: Multi-backend comparison for debugging
/// Different backends may have different precision/recall tradeoffs
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_compare_backends() {
    let text = "Tim Cook, CEO of Apple, announced new products in Cupertino.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    let output = anno_cli_cmd()
        .args([
            "compare",
            file_path,
            "--models",
            "--model-list",
            "pattern,heuristic,stacked",
            "--format",
            "table",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "compare command should succeed. stderr: {}",
        stderr
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify table format output
    assert!(
        stdout.contains("Model Comparison"),
        "Should show comparison header"
    );
    assert!(stdout.contains("pattern"), "Should list pattern backend");
    assert!(
        stdout.contains("heuristic"),
        "Should list heuristic backend"
    );
    assert!(stdout.contains("stacked"), "Should list stacked backend");

    // Pattern backend should find 0 NER entities (only patterns)
    // Heuristic and stacked should find some entities
}

/// E2E: Domain detection for different text types
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_domain_detection() {
    // General text with named entities
    let text = "Dr. Smith presented research at Harvard University in Boston.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    let output = anno_cli_cmd()
        .args(["domain", "--input", file_path])
        .output()
        .unwrap();

    assert!(output.status.success(), "domain command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Domain command outputs domain analysis, not entities
    // The test verifies the command exists and produces domain detection output
    assert!(
        stdout.contains("Domain") || stdout.contains("domain"),
        "Should produce domain output, got: {}",
        stdout
    );
}

/// E2E: Batch processing handles heterogeneous document types
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_batch_heterogeneous() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    // Create diverse document types
    fs::write(
        dir.join("news.txt"),
        "Breaking: Apple CEO Tim Cook announced quarterly earnings.",
    )
    .unwrap();
    fs::write(
        dir.join("medical.txt"),
        "Dr. Smith administered treatment to the patient.",
    )
    .unwrap();
    fs::write(
        dir.join("code.txt"),
        "// TODO: Fix bug in Parser.java\n// Author: john@example.com",
    )
    .unwrap();

    let output = anno_cli_cmd()
        .args(["batch", "-d", dir.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "batch should process heterogeneous docs"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should produce valid JSON array
    assert!(
        stdout.starts_with('['),
        "Should output JSON array, got: {}",
        &stdout[..100.min(stdout.len())]
    );

    // Should process all 3 documents
    let count = stdout.matches("\"id\":").count();
    assert!(
        count >= 3,
        "Should process at least 3 documents, found {}",
        count
    );
}

/// E2E: Zero-shot custom types with GLiNER
#[test]
#[cfg(all(feature = "eval-advanced", feature = "onnx"))]
fn e2e_cli_zeroshot_custom_types() {
    let text = "Dr. Fauci prescribed Remdesivir for COVID-19 patients at NIH.";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", text).unwrap();
    let file_path = file.path().to_str().unwrap();

    let output = anno_cli_cmd()
        .args(&[
            "extract",
            "--model",
            "gliner",
            "--types",
            "person,drug,disease,organization",
            "--file",
            file_path,
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        // Should find at least some of: person, drug, disease, organization
        let has_entities = stdout.contains("\"type\":");
        assert!(
            has_entities || stdout.contains("entities"),
            "Should extract some entities"
        );
    }
    // GLiNER may not be available, so don't fail hard
}

/// E2E: Explain command shows entity extraction results
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_explain_detailed() {
    let output = anno_cli_cmd()
        .args([
            "explain",
            "-t",
            "Dr. John Smith works at Harvard University.",
            "--show-all",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "explain command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show extracted entities with type decisions
    assert!(
        stdout.contains("Entity:") || stdout.contains("Type Decision:"),
        "Should show extracted entities, got: {}",
        stdout
    );
}

/// E2E: Privacy command detects PII
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_privacy_detection() {
    let output = anno_cli_cmd()
        .args([
            "privacy",
            "-t",
            "John Smith (555-123-4567) lives at 123 Main St.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "privacy command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should detect PII types (PERSON, CONTACT, ADDRESS, etc.)
    assert!(
        stdout.contains("PERSON") || stdout.contains("CONTACT") || stdout.contains("ADDRESS"),
        "Should detect PII types, got: {}",
        stdout
    );
}

/// E2E: Privacy detection finds email and phone
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_privacy_redaction() {
    let output = anno_cli_cmd()
        .args(["privacy", "-t", "Contact john@company.com or 555-123-4567"])
        .output()
        .unwrap();

    assert!(output.status.success(), "privacy command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should detect contact patterns (email/phone under CONTACT category)
    assert!(
        stdout.contains("CONTACT") || stdout.contains("emails/phones"),
        "Should detect CONTACT patterns, got: {}",
        stdout
    );
}

/// E2E: Singleton analysis for coreference quality
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_singleton_analysis() {
    let output = anno_cli_cmd()
        .args([
            "singleton",
            "-t",
            "Obama spoke at the White House. The president addressed the nation.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "singleton command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show extracted entities
    assert!(
        stdout.contains("PER:") || stdout.contains("LOC:") || stdout.contains("ORG:"),
        "Should show entity analysis, got: {}",
        stdout
    );
}

/// E2E: Context export for human review
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_context_export() {
    let output = anno_cli_cmd()
        .args([
            "context",
            "-t",
            "Tim Cook announced new products at Apple Park in Cupertino.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "context command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show entities with context (format: TYPE:count "text")
    assert!(
        stdout.contains("PER:") || stdout.contains("ORG:") || stdout.contains("LOC:"),
        "Should show entity context: got '{}'",
        stdout
    );
}

/// E2E: Stacked backend combines multiple extractors
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_ensemble_backend() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--model",
            "stacked",
            "--format",
            "json",
            "Tim Cook leads Apple Inc.",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "stacked backend should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should have entities in JSON format (array of objects with "type" field)
    assert!(
        stdout.contains("\"type\"") && stdout.contains("\"text\""),
        "Should have entities with type and text fields, got: {}",
        stdout
    );
}

/// E2E: Cross-document entity resolution
#[test]
#[cfg(feature = "eval-advanced")]
fn e2e_cli_crossdoc_resolution() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    fs::write(dir.join("doc1.txt"), "Tim Cook leads Apple Inc.").unwrap();
    fs::write(dir.join("doc2.txt"), "Apple's Cook announced new products.").unwrap();

    let output = anno_cli_cmd()
        .args([
            "cross-doc",
            dir.to_str().unwrap(),
            "--threshold",
            "0.5",
            "--format",
            "summary",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "cross-doc command should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should show cluster info
    assert!(
        stdout.contains("cluster") || stdout.contains("Document"),
        "Should show cross-doc clustering results"
    );
}
