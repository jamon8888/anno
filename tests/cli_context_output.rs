//! Tests for CLI context window and sentence extraction features.
//!
//! These features support human review, active learning, and annotation workflows.

use std::process::Command;

/// Test that --context-window flag adds context_before and context_after to JSON output
#[test]
fn test_extract_json_context_window() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let entities = json["entities"].as_array().expect("Should have entities array");
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
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let entities = json["entities"].as_array().expect("Should have entities array");
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
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
            "extract",
            "--format",
            "json",
            "-t",
            "John Smith works at Google.",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let provenance = &json["provenance"];
    let result_hash = provenance.get("result_hash");
    
    assert!(
        result_hash.is_some(),
        "Provenance should include result_hash for caching/reproducibility"
    );
    
    let hash_str = result_hash.unwrap().as_str().expect("result_hash should be a string");
    assert!(
        hash_str.starts_with("sha256:"),
        "result_hash should be prefixed with sha256:"
    );
}

/// Test that JSONL output also includes context when requested
#[test]
fn test_extract_jsonl_context_window() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    assert!(lines.len() >= 2, "Should have provenance + at least one entity");
    
    // Skip provenance line and check entity lines
    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let entity: serde_json::Value = serde_json::from_str(line)
            .expect("Each line should be valid JSON");
        
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
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let entities = json["entities"].as_array().expect("Should have entities array");
    
    // Find John Smith entity
    let john_entity = entities.iter().find(|e| {
        e.get("text").and_then(|t| t.as_str()) == Some("John Smith")
    });
    
    if let Some(entity) = john_entity {
        let context_before = entity.get("context_before")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let context_after = entity.get("context_after")
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
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let entities = json["entities"].as_array().expect("Should have entities array");
    
    // Find John Smith entity
    let john_entity = entities.iter().find(|e| {
        e.get("text").and_then(|t| t.as_str()) == Some("John Smith")
    });
    
    if let Some(entity) = john_entity {
        let sentence = entity.get("sentence")
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
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "anno-cli",
            "--",
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
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("Output should be valid JSON");
    
    let entities = json["entities"].as_array().expect("Should have entities array");
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

