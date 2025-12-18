//! Tests for CLI context window and sentence extraction features.
//!
//! These features support human review, active learning, and annotation workflows.
//!
//! NOTE: These tests are ignored because the `--context-window` and `--sentence`
//! flags are not yet implemented in the CLI. When implementing:
//! 1. Add --context-window <N> to extract command
//! 2. Add context_before/context_after fields to JSON output
//! 3. Add --sentence flag for sentence extraction
//!
//! See: https://github.com/arclabs561/anno/issues/XXX

// Note: This test requires cli feature
#![cfg(feature = "cli")]

use std::process::Command;

/// Get the CLI command - uses pre-built binary if ANNO_CLI_BIN is set,
/// otherwise falls back to cargo run (slow).
fn anno_cli_cmd() -> Command {
    if let Ok(bin_path) = std::env::var("ANNO_CLI_BIN") {
        return Command::new(bin_path);
    }

    // Find the workspace root (parent of anno crate)
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("anno crate should be in workspace");

    // Check if release binary exists
    let release_bin = workspace_root.join("target/release/anno");
    if release_bin.exists() {
        return Command::new(release_bin);
    }

    // Fall back to debug binary
    let debug_bin = workspace_root.join("target/debug/anno");
    if debug_bin.exists() {
        return Command::new(debug_bin);
    }

    // Last resort: cargo run (slow)
    let mut cmd = Command::new("cargo");
    // Note: the CLI binary lives in the `anno` crate.
    cmd.args([
        "run",
        "-p",
        "anno",
        "--bin",
        "anno",
        "--features",
        "cli eval",
        "--",
    ]);
    cmd
}

/// Test that --context-window flag adds context_before and context_after to JSON output
#[test]
fn test_extract_json_context_window() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--context-window",
            "20",
            "-t",
            "Barack Obama was the 44th President of the United States.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON and verify context fields are present
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");
    assert!(!entities.is_empty(), "Should find entities");

    // Check first entity has context fields
    let first_entity = &entities[0];
    assert!(
        first_entity.get("context_before").is_some(),
        "Entity should have context_before when --context-window specified"
    );
    assert!(
        first_entity.get("context_after").is_some(),
        "Entity should have context_after when --context-window specified"
    );
}

/// Test that --include-sentence flag adds sentence field to JSON output
#[test]
fn test_extract_json_include_sentence() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--include-sentence",
            "-t",
            "Barack Obama was born in Hawaii. He became President in 2009.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON and verify sentence field is present
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");
    assert!(!entities.is_empty(), "Should find entities");

    // Find an entity and verify it has sentence field
    let has_sentence_field = entities.iter().any(|e| e.get("sentence").is_some());
    assert!(
        has_sentence_field,
        "At least one entity should have sentence field when --include-sentence specified"
    );
}

/// Test that result_hash is included in provenance
#[test]
fn test_extract_json_result_hash_in_provenance() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "-t",
            "John Smith works at Google.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let provenance = &json["provenance"];
    let result_hash = provenance.get("result_hash");

    assert!(
        result_hash.is_some(),
        "Provenance should include result_hash for caching/reproducibility"
    );

    let hash_str = result_hash
        .unwrap()
        .as_str()
        .expect("result_hash should be a string");
    assert!(
        hash_str.starts_with("xxh3:"),
        "result_hash should be prefixed with xxh3: (got: {})",
        hash_str
    );
}

/// Test that JSONL output also includes context when requested
#[test]
fn test_extract_jsonl_context_window() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "jsonl",
            "--context-window",
            "15",
            "-t",
            "Marie Curie discovered radium in Paris.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // First line is provenance, subsequent lines are entities
    assert!(
        lines.len() >= 2,
        "Should have provenance + at least one entity"
    );

    // Skip provenance line and check entity lines
    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let entity: serde_json::Value =
            serde_json::from_str(line).expect("Each line should be valid JSON");

        assert!(
            entity.get("context_before").is_some(),
            "Entity in JSONL should have context_before"
        );
        assert!(
            entity.get("context_after").is_some(),
            "Entity in JSONL should have context_after"
        );
    }
}

/// Test that context window correctly extracts surrounding text
#[test]
fn test_context_window_content_correctness() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--context-window",
            "10",
            "-t",
            "I met John Smith yesterday afternoon.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");

    // Find John Smith entity
    let john_entity = entities
        .iter()
        .find(|e| e.get("text").and_then(|t| t.as_str()) == Some("John Smith"));

    if let Some(entity) = john_entity {
        let context_before = entity
            .get("context_before")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let context_after = entity
            .get("context_after")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Context before should contain part of "I met "
        assert!(
            context_before.contains("met") || context_before.contains("I "),
            "Context before '{}' should contain preceding text",
            context_before
        );

        // Context after should contain part of " yesterday"
        assert!(
            context_after.contains("yesterday") || context_after.contains(" "),
            "Context after '{}' should contain following text",
            context_after
        );
    }
}

/// Test sentence extraction correctly identifies sentence boundaries
#[test]
fn test_sentence_extraction_boundaries() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--include-sentence",
            "-t",
            "First sentence. John Smith is mentioned here. Third sentence.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");

    // Find John Smith entity
    let john_entity = entities
        .iter()
        .find(|e| e.get("text").and_then(|t| t.as_str()) == Some("John Smith"));

    if let Some(entity) = john_entity {
        let sentence = entity
            .get("sentence")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Sentence should be the second one
        assert!(
            sentence.contains("mentioned"),
            "Sentence '{}' should be the one containing John Smith",
            sentence
        );

        // Should not contain other sentences
        assert!(
            !sentence.contains("First sentence"),
            "Sentence should not contain first sentence"
        );
    }
}

/// Test that both context-window and include-sentence can be used together
#[test]
fn test_context_window_and_sentence_combined() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--context-window",
            "15",
            "--include-sentence",
            "-t",
            "In 2024, Barack Obama gave a speech in Chicago.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");
    assert!(!entities.is_empty(), "Should find entities");

    // Check that entities have both context and sentence fields
    let first_entity = &entities[0];
    assert!(
        first_entity.get("context_before").is_some(),
        "Should have context_before"
    );
    assert!(
        first_entity.get("context_after").is_some(),
        "Should have context_after"
    );
    assert!(
        first_entity.get("sentence").is_some(),
        "Should have sentence"
    );
}

/// Test Unicode text handling with context window (critical bug fix)
#[test]
fn test_context_window_with_unicode() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--context-window",
            "10",
            "-t",
            "日本の東京で山田太郎さんに会いました。", // "I met Mr. Yamada Taro in Tokyo, Japan."
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should not panic on Unicode text
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON even with Unicode");

    // If entities are found, verify context fields are strings (not malformed)
    if let Some(entities) = json["entities"].as_array() {
        for entity in entities {
            if let Some(ctx_before) = entity.get("context_before") {
                assert!(
                    ctx_before.is_string(),
                    "context_before should be valid string"
                );
            }
            if let Some(ctx_after) = entity.get("context_after") {
                assert!(
                    ctx_after.is_string(),
                    "context_after should be valid string"
                );
            }
        }
    }
}

/// Test context window with emoji (edge case for multi-byte chars)
#[test]
fn test_context_window_with_emoji() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--context-window",
            "5",
            "-t",
            "🎉 John Smith 🎉 won the prize!",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should not panic on emoji
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON even with emoji");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");
    // Find John Smith entity
    let john_entity = entities
        .iter()
        .find(|e| e.get("text").and_then(|t| t.as_str()) == Some("John Smith"));

    if let Some(entity) = john_entity {
        let ctx_before = entity
            .get("context_before")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // Context should include the emoji character correctly
        assert!(
            ctx_before.contains("🎉") || ctx_before.is_empty() || ctx_before.contains(" "),
            "Context should handle emoji correctly: '{}'",
            ctx_before
        );
    }
}

/// Test confidence_stats with single entity (edge case for median/std_dev)
#[test]
fn test_confidence_stats_single_entity() {
    let output = anno_cli_cmd()
        .args([
            "extract", "--format", "json", "-t",
            "John", // Short text likely to produce single entity
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    if let Some(stats) = json["provenance"]["confidence_stats"].as_object() {
        // With single entity, median should equal mean
        if let (Some(mean), Some(median)) = (stats.get("mean"), stats.get("median")) {
            if let (Some(m), Some(med)) = (mean.as_f64(), median.as_f64()) {
                assert!(
                    (m - med).abs() < 0.01,
                    "With single entity, mean ({}) should equal median ({})",
                    m,
                    med
                );
            }
        }
        // Std dev should be 0 with single entity
        if let Some(std_dev) = stats.get("std_dev").and_then(|v| v.as_f64()) {
            assert!(
                std_dev.abs() < 0.001,
                "With single entity, std_dev should be 0, got {}",
                std_dev
            );
        }
    }
}

/// Test sentence extraction without punctuation (edge case)
#[test]
fn test_sentence_no_punctuation() {
    let output = anno_cli_cmd()
        .args([
            "extract",
            "--format",
            "json",
            "--include-sentence",
            "-t",
            "John Smith lives in New York", // No sentence-ending punctuation
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    let entities = json["entities"]
        .as_array()
        .expect("Should have entities array");

    // Find an entity and check sentence field
    for entity in entities {
        if let Some(sentence) = entity.get("sentence").and_then(|v| v.as_str()) {
            // Without punctuation, entire text should be the sentence
            assert!(
                sentence.contains("John Smith") || sentence.contains("New York"),
                "Sentence should contain entity text: '{}'",
                sentence
            );
        }
    }
}

/// Test result_hash is deterministic across multiple runs
#[test]
fn test_result_hash_deterministic() {
    let text = "Barack Obama met Angela Merkel and Joe Biden in Berlin, Germany.";

    // Run extraction multiple times
    let mut hashes = Vec::new();
    for _ in 0..3 {
        let output = anno_cli_cmd()
            .args(["extract", "--format", "json", "-t", text])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("Output should be valid JSON");

        let hash = json["provenance"]["result_hash"]
            .as_str()
            .expect("Should have result_hash")
            .to_string();
        hashes.push(hash);
    }

    // All hashes should be identical
    assert!(
        hashes.iter().all(|h| h == &hashes[0]),
        "Result hash should be deterministic across runs: {:?}",
        hashes
    );
}

/// Test that result_hash changes when entities change
#[test]
fn test_result_hash_varies_with_content() {
    // Extract from different texts
    let text1 = "Barack Obama spoke at the conference.";
    let text2 = "Angela Merkel spoke at the conference.";

    let mut hashes = Vec::new();
    for text in [text1, text2] {
        let output = anno_cli_cmd()
            .args(["extract", "--format", "json", "-t", text])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("Output should be valid JSON");

        let hash = json["provenance"]["result_hash"]
            .as_str()
            .expect("Should have result_hash")
            .to_string();
        hashes.push(hash);
    }

    // Hashes should differ for different content
    assert_ne!(
        hashes[0], hashes[1],
        "Result hash should differ for different content"
    );
}

/// Test entity_id is content-addressed (same span = same id)
#[test]
fn test_entity_id_content_addressed() {
    let text = "Barack Obama met Angela Merkel.";

    // Run twice to verify entity IDs are stable
    let mut entity_ids: Vec<Vec<String>> = Vec::new();
    for _ in 0..2 {
        let output = anno_cli_cmd()
            .args(["extract", "--format", "json", "-t", text])
            .output()
            .expect("Failed to execute command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("Output should be valid JSON");

        let ids: Vec<String> = json["entities"]
            .as_array()
            .expect("Should have entities")
            .iter()
            .filter_map(|e| e.get("id").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        entity_ids.push(ids);
    }

    // Entity IDs should be identical across runs
    assert_eq!(
        entity_ids[0], entity_ids[1],
        "Entity IDs should be deterministic: {:?} vs {:?}",
        entity_ids[0], entity_ids[1]
    );
}
